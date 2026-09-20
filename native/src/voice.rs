//! Voice: the control plane, and where the media engine attaches.
//!
//! Joining a MiniChat voice channel is two halves. The first is ordinary:
//! ask the instance for a LiveKit grant, tell the instance you have joined,
//! and track who else is in the room from the gateway's `VOICE_STATE_*`
//! events. That half is here in full and does not need a media stack at all —
//! the member list, the sidebar's voice rows, the speaking indicators and the
//! mute and deafen controls are all driven by it.
//!
//! The second half is the media: capturing a microphone, encoding it, and
//! playing everyone else back. That is `livekit` and, under it, `libwebrtc` —
//! roughly 200MB of prebuilt C++ that needs clang 21 or newer to link on
//! Linux. It sits behind the `voice` feature so the rest of the client builds
//! and is testable everywhere, and so the day the media lands it is one
//! module that changes rather than the whole app.
//!
//! See `Engine` at the bottom for exactly what that module has to provide.

use serde::Deserialize;

/// What the instance hands back for a voice channel.
///
/// The permissions come from the server rather than being worked out here:
/// a channel can deny SPEAK to a role that has it instance-wide, and the
/// grant is the authoritative answer.
// `token`, `url` and `room` are what the media engine connects with, so they
// are unread in a build without the `voice` feature. They are still part of
// the grant, and dropping them would mean parsing the server's answer
// differently depending on a cargo feature.
#[allow(dead_code)]
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Grant {
    pub token: String,
    pub url: String,
    #[serde(default)]
    pub room: String,
    #[serde(default)]
    pub can_speak: bool,
    #[serde(default)]
    pub can_video: bool,
    #[serde(default)]
    pub can_screen_share: bool,
}

/// Where a voice connection is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Status {
    #[default]
    Disconnected,
    Connecting,
    Connected,
    /// The control plane joined but the media engine is not present, which is
    /// what a build without the `voice` feature does. Distinct from an error:
    /// nothing went wrong, the capability simply is not there.
    ControlOnly,
    Failed,
}

/// The client's own voice state.
#[derive(Clone, Debug, Default)]
pub struct Session {
    pub status: Status,
    pub channel: String,
    pub muted: bool,
    pub deafened: bool,
    pub can_speak: bool,
    pub can_video: bool,
    pub can_screen_share: bool,
    pub camera_on: bool,
    pub screen_sharing: bool,
    /// Why the last attempt failed, or why media is unavailable.
    pub notice: String,
}

impl Session {
    pub fn is_live(&self) -> bool {
        matches!(self.status, Status::Connected | Status::ControlOnly)
    }

    /// Joining a channel resets everything that was true of the last one,
    /// while keeping the member's own mute and deafen choices — those are
    /// preferences, not properties of a room.
    pub fn joining(&mut self, channel: &str, grant: &Grant) {
        self.status = Status::Connecting;
        self.channel = channel.to_string();
        self.can_speak = grant.can_speak;
        self.can_video = grant.can_video;
        self.can_screen_share = grant.can_screen_share;
        self.camera_on = false;
        self.screen_sharing = false;
        self.notice = String::new();
        // Someone who may not speak is muted as a fact, not a choice.
        if !grant.can_speak {
            self.muted = true;
        }
    }

    pub fn left(&mut self) {
        let notice = std::mem::take(&mut self.notice);
        *self = Session {
            // Mute and deafen survive leaving a channel.
            muted: self.muted,
            deafened: self.deafened,
            notice,
            ..Default::default()
        };
    }

    /// Deafening implies muting: you cannot be heard while you are not
    /// listening, which is what every voice client does and what members
    /// expect when they press one button in a hurry.
    pub fn set_deafened(&mut self, deafened: bool) {
        self.deafened = deafened;
        if deafened {
            self.muted = true;
        }
    }

    pub fn set_muted(&mut self, muted: bool) {
        if !self.can_speak && !muted {
            return;
        }
        self.muted = muted;
        // Unmuting implies undeafening, for the same reason.
        if !muted {
            self.deafened = false;
        }
    }

    /// What the sidebar's voice panel says under the channel name.
    pub fn summary(&self) -> &str {
        match self.status {
            Status::Disconnected => "Not connected",
            Status::Connecting => "Connecting…",
            Status::Connected => "Voice connected",
            Status::ControlOnly => "Connected — no audio in this build",
            Status::Failed => "Could not connect",
        }
    }
}

/// The media engine.
///
/// Implemented once, behind the `voice` feature, by a module that owns a
/// `livekit::Room`. Everything above this line already works without it.
///
/// What an implementation has to do, in the order the join path calls it:
///
/// 1. `connect` — open the room with the grant's URL and token, publishing
///    a microphone track when `can_speak`, with echo cancellation, automatic
///    gain and noise suppression on. A member with no microphone, or who
///    denies permission, stays connected as a listener rather than failing.
/// 2. `set_muted` / `set_deafened` — the local track's enabled state, and
///    the subscription state for everyone else's.
/// 3. `speaking` — the identities LiveKit reports as active speakers, polled
///    by the UI each frame. Used for the ring around an avatar.
/// 4. `disconnect` — tear the room down; safe to call when not connected.
///
/// Camera and screen share are deliberately not in this trait yet. Both need
/// frame rendering (I420 to a GPU texture) that the UI has no path for, and
/// the screen-capture half already exists in `desktop/src/capture.rs` waiting
/// to be moved across.
pub trait Engine {
    fn connect(&mut self, grant: &Grant, muted: bool) -> Result<(), String>;
    fn set_muted(&mut self, muted: bool);
    fn set_deafened(&mut self, deafened: bool);
    /// Identities currently speaking.
    fn speaking(&self) -> Vec<String>;
    fn disconnect(&mut self);
}

/// The engine used when the `voice` feature is off.
///
/// It succeeds rather than failing: the control plane is genuinely working —
/// you appear in the channel, you can see who else is there, and they can see
/// you — and saying so is more honest than pretending the join failed.
#[derive(Default)]
pub struct NoMedia;

impl Engine for NoMedia {
    fn connect(&mut self, _grant: &Grant, _muted: bool) -> Result<(), String> {
        Ok(())
    }
    fn set_muted(&mut self, _muted: bool) {}
    fn set_deafened(&mut self, _deafened: bool) {}
    fn speaking(&self) -> Vec<String> {
        Vec::new()
    }
    fn disconnect(&mut self) {}
}

/// The engine this build has.
pub fn engine() -> Box<dyn Engine> {
    #[cfg(feature = "voice")]
    {
        Box::new(livekit_engine::LiveKit::default())
    }
    #[cfg(not(feature = "voice"))]
    {
        Box::new(NoMedia)
    }
}

/// Whether this build can carry audio.
pub fn has_media() -> bool {
    cfg!(feature = "voice")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grant(can_speak: bool) -> Grant {
        Grant {
            token: "t".into(),
            url: "wss://livekit.example.com".into(),
            can_speak,
            can_video: can_speak,
            can_screen_share: can_speak,
            ..Default::default()
        }
    }

    #[test]
    fn joining_takes_the_grants_permissions() {
        let mut session = Session::default();
        session.joining("lounge", &grant(true));
        assert_eq!(session.channel, "lounge");
        assert_eq!(session.status, Status::Connecting);
        assert!(session.can_speak && session.can_video && session.can_screen_share);
    }

    #[test]
    fn someone_who_may_not_speak_joins_muted() {
        let mut session = Session::default();
        session.joining("lounge", &grant(false));
        assert!(session.muted);
        // And cannot unmute themselves out of it.
        session.set_muted(false);
        assert!(session.muted);
    }

    #[test]
    fn deafening_mutes_and_unmuting_undeafens() {
        let mut session = Session::default();
        session.joining("lounge", &grant(true));
        session.set_muted(false);

        session.set_deafened(true);
        assert!(session.muted, "you cannot be heard while not listening");

        session.set_muted(false);
        assert!(!session.deafened, "unmuting has to undeafen");
    }

    #[test]
    fn leaving_keeps_the_members_own_choices() {
        let mut session = Session::default();
        session.joining("lounge", &grant(true));
        session.set_deafened(true);
        session.left();

        assert_eq!(session.status, Status::Disconnected);
        assert!(session.channel.is_empty());
        // Mute and deafen are preferences, not properties of the room.
        assert!(session.muted && session.deafened);
    }

    #[test]
    fn a_build_without_media_still_joins() {
        // The control plane works: you appear in the channel and can see who
        // else is there. Saying so beats reporting a failure that did not
        // happen.
        let mut engine = NoMedia;
        assert!(engine.connect(&grant(true), false).is_ok());
        assert!(engine.speaking().is_empty());
        engine.disconnect();
    }

    #[test]
    fn the_summary_says_which_kind_of_connected_this_is() {
        let mut session = Session::default();
        assert_eq!(session.summary(), "Not connected");
        session.status = Status::ControlOnly;
        assert!(session.summary().contains("no audio"));
        session.status = Status::Connected;
        assert_eq!(session.summary(), "Voice connected");
    }
}
