//! The media engine: a LiveKit room, a microphone and everyone else's audio.
//!
//! Built only with the `voice` feature, because under `livekit` sits
//! `libwebrtc` — roughly 200MB of prebuilt C++ that needs clang 21 or newer
//! to link. Everything else in this client, the whole control plane
//! included, builds without it.
//!
//! Two threads and no more. The room runs on a small tokio runtime of its
//! own, owned here and kept alive between calls so that leaving a channel
//! never blocks the UI thread waiting for a socket to close. The UI thread
//! only ever sends a command down a channel or reads a mutex that the room
//! task writes.
//!
//! Capture and playback are libwebrtc's own audio device module, which
//! `PlatformAudio` switches on: WASAPI on Windows, CoreAudio on macOS,
//! PulseAudio or ALSA on Linux. Pushing frames in by hand through
//! `NativeAudioSource` was the alternative, and it would mean reimplementing
//! echo cancellation badly — acoustic echo cancellation needs the render
//! stream as its reference, which a client that only sees its own capture
//! buffer does not have. The platform module has both sides and uses the
//! hardware canceller where the machine has one.

use std::sync::{Arc, Mutex};

use livekit::options::TrackPublishOptions;
use livekit::prelude::*;
use livekit::track::{LocalAudioTrack, LocalTrack, TrackSource};
use livekit::{PlatformAudio, Room, RoomEvent, RoomOptions};
use tokio::sync::mpsc;

use super::{Engine, Grant, Media};

/// What the UI thread asks the room task to do.
#[derive(Debug)]
enum Command {
    Muted(bool),
    Deafened(bool),
    Leave,
}

/// What the room task tells the UI thread, without it having to ask.
#[derive(Debug, Default)]
struct Shared {
    media: Mutex<Media>,
    speaking: Mutex<Vec<String>>,
}

impl Shared {
    fn set(&self, media: Media) {
        if let Ok(mut slot) = self.media.lock() {
            *slot = media;
        }
    }
}

/// The `voice` feature's engine.
#[derive(Default)]
pub struct LiveKit {
    /// Built on the first join and kept for the life of the process. A
    /// runtime that is torn down on every hang-up has to be waited for on the
    /// UI thread, and a client that freezes for a moment when you leave a
    /// call feels broken even though nothing is wrong.
    runtime: Option<tokio::runtime::Runtime>,
    commands: Option<mpsc::UnboundedSender<Command>>,
    shared: Arc<Shared>,
}

impl LiveKit {
    fn send(&self, command: Command) {
        if let Some(commands) = self.commands.as_ref() {
            // A closed channel means the room task has already ended, which
            // is the state the command was asking for anyway.
            let _ = commands.send(command);
        }
    }
}

impl Engine for LiveKit {
    fn connect(&mut self, grant: &Grant, muted: bool) -> Result<(), String> {
        // Any previous room goes first: joining a second channel while still
        // in the first is how you end up publishing to both.
        self.send(Command::Leave);

        if self.runtime.is_none() {
            self.runtime = Some(
                tokio::runtime::Builder::new_multi_thread()
                    // Two: one for the signalling socket, one for the media
                    // callbacks. libwebrtc runs its own threads underneath.
                    .worker_threads(2)
                    .thread_name("minichat-voice")
                    .enable_all()
                    .build()
                    .map_err(|error| format!("Could not start the voice runtime: {error}"))?,
            );
        }
        let runtime = self
            .runtime
            .as_ref()
            .expect("the runtime was just built or already existed");

        let (commands, inbox) = mpsc::unbounded_channel();
        self.commands = Some(commands);
        self.shared = Arc::new(Shared::default());
        self.shared.set(Media::Connecting);

        runtime.spawn(run(grant.clone(), muted, self.shared.clone(), inbox));
        Ok(())
    }

    fn media(&self) -> Media {
        self.shared
            .media
            .lock()
            .map(|media| media.clone())
            .unwrap_or(Media::Absent)
    }

    fn set_muted(&mut self, muted: bool) {
        self.send(Command::Muted(muted));
    }

    fn set_deafened(&mut self, deafened: bool) {
        self.send(Command::Deafened(deafened));
    }

    fn speaking(&self) -> Vec<String> {
        self.shared
            .speaking
            .lock()
            .map(|speaking| speaking.clone())
            .unwrap_or_default()
    }

    fn disconnect(&mut self) {
        self.send(Command::Leave);
        self.commands = None;
        self.shared = Arc::new(Shared::default());
    }
}

/// The room, from connect to hang-up.
async fn run(
    grant: Grant,
    muted: bool,
    shared: Arc<Shared>,
    mut commands: mpsc::UnboundedReceiver<Command>,
) {
    // The platform audio module, which is what makes a published track
    // capture the microphone and a subscribed one reach the speakers.
    // Without it there is no audio at all, so a failure here is a failure to
    // join with media — the control plane above will still have the member
    // in the channel.
    let audio = match PlatformAudio::new() {
        Ok(audio) => Some(audio),
        Err(error) => {
            shared.set(Media::Failed(format!("No audio devices: {error}")));
            return;
        }
    };

    // Field by field because `RoomOptions` is non-exhaustive and cannot be
    // written as a struct literal from outside the crate.
    let mut options = RoomOptions::default();
    options.auto_subscribe = true;
    // Both of the next two are for video: adaptive stream drops layers for
    // renderers that are not on screen, dynacast stops publishing what nobody
    // is watching. An audio-only client wants neither.
    options.adaptive_stream = false;
    options.dynacast = false;
    // The SDK's own defaults are 5 seconds and three attempts, which on a
    // voice server that is simply not there adds up to most of a minute of
    // "Connecting…". A member needs to be told sooner than that, and the
    // instance itself answered a moment ago, so a long retry is unlikely to
    // be the thing that saves the join.
    options.connect_timeout = std::time::Duration::from_secs(8);
    options.join_retries = 2;

    let (room, mut events) = match Room::connect(&grant.url, &grant.token, options).await {
        Ok(joined) => joined,
        Err(error) => {
            shared.set(Media::Failed(describe(&error)));
            return;
        }
    };

    // Published rather than assumed: a member with SPEAK denied on this
    // channel gets no microphone track at all, so nothing can go out even if
    // the client is wrong about the permission.
    let mut track: Option<LocalAudioTrack> = None;
    if grant.can_speak {
        if let Some(audio) = audio.as_ref() {
            let published = LocalAudioTrack::create_audio_track("microphone", audio.rtc_source());
            let options = TrackPublishOptions {
                source: TrackSource::Microphone,
                // Discontinuous transmission: send nothing during silence.
                // Speech is mostly silence, and this is most of the bandwidth
                // saving in a busy channel.
                dtx: true,
                // Redundant encoding, which is what makes a dropped packet
                // inaudible rather than a gap on a lossy connection.
                red: true,
                ..Default::default()
            };
            match room
                .local_participant()
                .publish_track(LocalTrack::Audio(published.clone()), options)
                .await
            {
                Ok(_) => track = Some(published),
                // Listening is better than failing: the member is in the
                // channel and can hear everyone, which is most of the point.
                Err(error) => shared.set(Media::Degraded(format!(
                    "Could not publish the microphone: {}",
                    describe(&error)
                ))),
            }
        }
    }

    let mut state = State {
        muted,
        deafened: false,
        track,
        audio,
    };
    // Whatever the member had already chosen before the room existed.
    state.apply_mute(&room).await;

    if !matches!(shared.media.lock().as_deref(), Ok(Media::Degraded(_))) {
        shared.set(Media::Connected);
    }

    loop {
        tokio::select! {
            command = commands.recv() => match command {
                Some(Command::Muted(muted)) => {
                    state.muted = muted;
                    state.apply_mute(&room).await;
                }
                Some(Command::Deafened(deafened)) => {
                    state.deafened = deafened;
                    apply_deafen(&room, deafened);
                }
                // A dropped sender means the engine was dropped, which is a
                // hang-up by another name.
                Some(Command::Leave) | None => break,
            },
            event = events.recv() => match event {
                Some(event) => {
                    if handle(event, &shared, &state).is_break() {
                        break;
                    }
                }
                None => break,
            },
        }
    }

    // Stop the microphone before the room, so the recording indicator goes
    // out as the call ends rather than a moment after it.
    state.stop_recording();
    let _ = room.close().await;
    if let Ok(mut speaking) = shared.speaking.lock() {
        speaking.clear();
    }
    shared.set(Media::Absent);
}

/// What the room task is holding on to.
struct State {
    muted: bool,
    deafened: bool,
    track: Option<LocalAudioTrack>,
    audio: Option<PlatformAudio>,
}

impl State {
    /// Mute at both ends.
    ///
    /// The track mute is what tells the server and everyone else, and it is
    /// what puts the crossed-out microphone next to your name for them. The
    /// recording stop is what turns off the operating system's own recording
    /// indicator, which is the part a member actually trusts: a client that
    /// says muted while the system says the microphone is live is a client
    /// nobody believes.
    async fn apply_mute(&self, room: &Room) {
        let Some(track) = self.track.as_ref() else {
            return;
        };
        if self.muted {
            track.mute();
        } else {
            track.unmute();
        }

        if let Some(audio) = self.audio.as_ref() {
            let _ = if self.muted {
                audio.stop_recording()
            } else {
                audio.start_recording()
            };
        }
        let _ = room;
    }

    fn stop_recording(&self) {
        if let Some(audio) = self.audio.as_ref() {
            let _ = audio.stop_recording();
        }
    }
}

/// Deafening unsubscribes rather than turning the volume down.
///
/// Turning playback off would still pull every stream over the network and
/// decode it, which on a laptop is the difference between a quiet call and a
/// warm one. Unsubscribing tells the server to stop sending, and the audio
/// comes back on the next subscribe.
fn apply_deafen(room: &Room, deafened: bool) {
    for participant in room.remote_participants().values() {
        for publication in participant.track_publications().values() {
            if publication.kind() == TrackKind::Audio {
                publication.set_subscribed(!deafened);
            }
        }
    }
}

/// Fold one room event. Breaking ends the call.
fn handle(event: RoomEvent, shared: &Arc<Shared>, state: &State) -> std::ops::ControlFlow<()> {
    use std::ops::ControlFlow;

    match event {
        RoomEvent::ActiveSpeakersChanged { speakers } => {
            let speaking = speakers
                .iter()
                .map(|speaker| speaker.identity().to_string())
                .collect();
            if let Ok(mut slot) = shared.speaking.lock() {
                *slot = speaking;
            }
        }

        // Someone who joins while we are deafened must not start playing.
        RoomEvent::TrackSubscribed { publication, .. } if state.deafened => {
            if publication.kind() == TrackKind::Audio {
                publication.set_subscribed(false);
            }
        }

        RoomEvent::ParticipantDisconnected(participant) => {
            if let Ok(mut speaking) = shared.speaking.lock() {
                let identity = participant.identity().to_string();
                speaking.retain(|speaker| speaker != &identity);
            }
        }

        // The SDK reconnects by itself; saying so beats a silent gap where
        // the member cannot tell whether anyone can hear them.
        RoomEvent::Reconnecting => shared.set(Media::Connecting),
        RoomEvent::Reconnected => shared.set(Media::Connected),

        RoomEvent::Disconnected { reason } => {
            shared.set(Media::Failed(format!("Voice disconnected: {reason:?}")));
            return ControlFlow::Break(());
        }

        _ => {}
    }
    ControlFlow::Continue(())
}

/// A message worth putting in front of a member.
///
/// LiveKit's errors are precise and unreadable — an expired token arrives as
/// a signalling close code. What a member can act on is whether to try again
/// or to tell whoever runs the instance.
fn describe(error: &impl std::fmt::Display) -> String {
    let text = error.to_string();
    let lowered = text.to_lowercase();
    if lowered.contains("401") || lowered.contains("unauthorized") || lowered.contains("token") {
        "The voice server refused this join. Try again, or ask an admin to check the voice \
         configuration."
            .into()
    } else if lowered.contains("timed out") || lowered.contains("timeout") {
        "The voice server did not answer. Check your connection and try again.".into()
    } else {
        format!("Voice failed: {text}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Joining must never block the UI thread, and a room that cannot be
    /// reached has to end up somewhere a member can see rather than in
    /// `Connecting` forever.
    #[test]
    fn an_unreachable_room_fails_without_blocking() {
        let grant = Grant {
            // Reserved for documentation, so it resolves nowhere.
            url: "wss://192.0.2.1:7880".into(),
            token: "not-a-token".into(),
            can_speak: true,
            ..Default::default()
        };

        let mut engine = LiveKit::default();
        let started = std::time::Instant::now();
        assert!(engine.connect(&grant, false).is_ok());
        assert!(
            started.elapsed() < std::time::Duration::from_millis(250),
            "connect blocked for {:?}",
            started.elapsed()
        );

        // Two attempts at eight seconds, plus the backoff between them.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(45);
        loop {
            match engine.media() {
                // Either answer is the contract: a machine with no audio
                // devices — a CI runner, usually — fails before it dials.
                Media::Failed(notice) => {
                    assert!(!notice.is_empty(), "a failure the member cannot read");
                    break;
                }
                Media::Connected | Media::Degraded(_) => {
                    panic!("connected to a reserved address")
                }
                _ if std::time::Instant::now() > deadline => {
                    panic!("still connecting after 45s")
                }
                _ => std::thread::sleep(std::time::Duration::from_millis(100)),
            }
        }

        engine.disconnect();
        assert_eq!(engine.media(), Media::Absent);
    }

    /// Everything the control plane cannot tell you, against a real server.
    ///
    /// Ignored by default because it needs one: `native/tests/voice.sh`
    /// fetches `livekit-server`, runs it in dev mode and calls this. A test
    /// that quietly passed when there was no server would be worse than no
    /// test, which is why this is ignored rather than self-skipping.
    #[test]
    #[ignore = "needs a LiveKit server: run native/tests/voice.sh"]
    fn a_real_room_hears_the_microphone() {
        use livekit_api::access_token::{AccessToken, VideoGrants};

        let url = std::env::var("LIVEKIT_URL").unwrap_or_else(|_| "ws://127.0.0.1:7880".into());
        let key = std::env::var("LIVEKIT_API_KEY").unwrap_or_else(|_| "devkey".into());
        let secret = std::env::var("LIVEKIT_API_SECRET").unwrap_or_else(|_| "secret".into());
        let room = "minichat-native-test";

        let token = |identity: &str| {
            AccessToken::with_api_key(&key, &secret)
                .with_identity(identity)
                .with_name(identity)
                .with_grants(VideoGrants {
                    room_join: true,
                    room: room.to_string(),
                    can_publish: true,
                    can_subscribe: true,
                    ..Default::default()
                })
                .to_jwt()
                .expect("minting a token")
        };

        // A second member of the room, standing in for everyone else. It
        // watches what the engine does from the outside, which is the only
        // way to know the microphone is really going out.
        let watcher = tokio::runtime::Runtime::new().expect("a runtime for the watcher");
        let (_room, mut events) = watcher
            .block_on(Room::connect(
                &url,
                &token("watcher"),
                RoomOptions::default(),
            ))
            .expect("the watcher joins");

        let mut engine = LiveKit::default();
        engine
            .connect(
                &Grant {
                    url: url.clone(),
                    token: token("speaker"),
                    room: room.into(),
                    can_speak: true,
                    ..Default::default()
                },
                false,
            )
            .expect("the engine starts connecting");

        // What the watcher must see, in order.
        let mut joined = false;
        let mut subscribed = false;
        let mut muted = false;
        let mut left = false;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);

        while std::time::Instant::now() < deadline && !left {
            let event = watcher.block_on(async {
                tokio::time::timeout(std::time::Duration::from_secs(5), events.recv())
                    .await
                    .ok()
                    .flatten()
            });
            let Some(event) = event else { continue };

            match event {
                RoomEvent::ParticipantConnected(participant) => {
                    assert_eq!(participant.identity().to_string(), "speaker");
                    joined = true;
                }
                // The microphone, published and picked up by someone else:
                // the one thing a control plane can never prove.
                RoomEvent::TrackSubscribed {
                    publication,
                    participant,
                    ..
                } => {
                    assert_eq!(participant.identity().to_string(), "speaker");
                    assert_eq!(publication.kind(), TrackKind::Audio);
                    assert_eq!(publication.source(), TrackSource::Microphone);
                    subscribed = true;
                    assert_eq!(engine.media(), Media::Connected);
                    // Muting has to be visible to everyone else, not just in
                    // this client's own toolbar.
                    engine.set_muted(true);
                }
                RoomEvent::TrackMuted { participant, .. } if subscribed => {
                    assert_eq!(participant.identity().to_string(), "speaker");
                    muted = true;
                    engine.disconnect();
                }
                RoomEvent::ParticipantDisconnected(participant) if muted => {
                    assert_eq!(participant.identity().to_string(), "speaker");
                    left = true;
                }
                _ => {}
            }
        }

        assert!(joined, "the room never saw the engine join");
        assert!(subscribed, "nobody ever heard the microphone");
        assert!(muted, "muting never reached the room");
        assert!(left, "leaving never reached the room");
        // Cleared rather than left behind, or the next join starts with the
        // last call's speakers still ringing.
        assert!(engine.speaking().is_empty());
    }
}
