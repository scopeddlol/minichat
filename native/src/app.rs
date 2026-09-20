//! Wiring.
//!
//! Slint's event loop owns the main thread and its state is not `Send`, so
//! everything that waits — HTTP, the gateway, image fetches — runs on a Tokio
//! runtime on another thread and posts results back with
//! `invoke_from_event_loop`. The rule this file follows throughout: the store
//! is only ever touched from the UI thread, and the network is only ever
//! touched from the runtime. Nothing is shared but the channels between them.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, ModelRc, SharedString, VecModel, Weak};
use tokio::sync::mpsc;

use crate::api::{self, types::*};
use crate::gateway;
use crate::images;
use crate::settings::{Settings, ThemeChoice};
use crate::store::Store;
use crate::text::layout;
use crate::theme::{self, Branding, Palette, Rgb};
use crate::ui;
use crate::view;

/// Work for the runtime thread.
enum Task {
    Meta(String),
    SignIn {
        origin: String,
        username: String,
        password: String,
    },
    Connect {
        origin: String,
        token: String,
    },
    LoadMessages {
        channel: String,
    },
    Send {
        channel: String,
        temp_id: String,
        content: String,
        reply_to: Option<String>,
        attachments: Vec<Attachment>,
    },
    React {
        message: String,
        emoji: String,
        on: bool,
    },
    Ack {
        channel: String,
        message: String,
    },
    DeleteMessage(String),
    PinMessage {
        message: String,
        pinned: bool,
    },
    EditMessage {
        message: String,
        content: String,
    },
    /// Pick files and upload them. The picker runs on the runtime thread so
    /// the UI is not blocked while it is open.
    Attach {
        channel: String,
        max_mb: i64,
    },
    FetchImages(Vec<String>),
    /// The same image again, at lightbox resolution.
    FetchFullImage(String),
    Search {
        query: String,
    },
    LoadPins {
        channel: String,
    },
    UpdateStatus(String),
    Typing(String),
    JoinVoice {
        channel: String,
    },
    LoadConversations,
    LoadRelationships,
    LoadAdmin(&'static str),
    AdminAction(AdminAction),
    StartCall {
        conversation: String,
    },
    AnswerCall {
        call: String,
        action: &'static str,
    },
    Relate {
        user: String,
        action: Relate,
    },
    ReorderChannels(Vec<crate::reorder::Placement>),
    ReorderCategories(Vec<(String, i64)>),
    OpenConversation {
        peer: String,
    },
    LoadDirect {
        conversation: String,
    },
    SendDirect {
        conversation: String,
        temp_id: String,
        content: String,
    },
    AckDirect {
        conversation: String,
        message: String,
    },
    LoadOlder {
        channel: String,
        before: String,
    },
    Disconnect,
}

/// Results for the UI thread.
enum Event {
    Meta(Box<Instance>),
    MetaFailed(String),
    SignedIn(String),
    SignInFailed(String),
    Ready(Box<ReadyPayload>),
    Gateway(gateway::Frame),
    Status(gateway::Status),
    SessionInvalid,
    Messages {
        channel: String,
        messages: Vec<Message>,
    },
    MessagesFailed {
        channel: String,
        error: String,
    },
    Sent {
        channel: String,
        temp_id: String,
        saved: Box<Message>,
    },
    SendFailed {
        channel: String,
        temp_id: String,
    },
    ImageLoaded {
        url: String,
        bytes: Vec<u8>,
        max_side: u32,
    },
    ImageFailed(String),
    FullImageLoaded {
        url: String,
        bytes: Vec<u8>,
    },
    FullImageFailed(String),
    SearchResults(Vec<Message>),
    Pins(Vec<Message>),
    OlderMessages {
        channel: String,
        messages: Vec<Message>,
    },
    VoiceGrant {
        channel: String,
        grant: Box<crate::voice::Grant>,
    },
    VoiceFailed {
        channel: String,
        error: String,
    },
    Attached {
        channel: String,
        files: Vec<Attachment>,
    },
    AttachFailed(String),
    Reordered,
    CallFailed(String),
    AdminStats(Box<Stats>),
    AdminInstance(Box<Instance>),
    AdminBans(Vec<BanEntry>),
    AdminInvites(Vec<Invite>),
    AdminAudit(Vec<AuditEntry>),
    AdminDone(String),
    Relationships(Vec<Relationship>),
    Conversations(Vec<Conversation>),
    ConversationOpened(Box<Conversation>),
    DirectHistory {
        conversation: String,
        messages: Vec<DirectMessage>,
    },
    DirectSent {
        conversation: String,
        temp_id: String,
        saved: Box<DirectMessage>,
    },
    DirectSendFailed {
        conversation: String,
        temp_id: String,
    },
}

/// Everything the UI thread owns.
struct AppState {
    store: Store,
    settings: Settings,
    images: images::Cache,
    measurer: layout::Measurer,
    palette: Palette,
    /// The column a message body wraps into, updated when the pane resizes.
    body_width: f32,
    connection: gateway::Status,
    replying_to: Option<String>,
    /// Counter behind the optimistic-message IDs.
    next_temp: u64,

    /// What a side panel is currently showing.
    panel_hits: Vec<Message>,
    panel_loading: bool,
    /// The member whose card is open.
    profile: Option<String>,
    /// What the open confirmation will do if confirmed.
    pending_confirm: Option<Confirm>,
    /// Whether the window has focus. A notification for something already
    /// on screen is noise.
    window_focused: bool,
    /// Files uploaded and waiting to go with the next message.
    pending_attachments: Vec<Attachment>,
    /// Why the last attachment did not upload.
    attach_notice: String,
    // --- administration ----------------------------------------------------
    admin_section: String,
    stats: Option<Stats>,
    bans: Vec<BanEntry>,
    invites: Vec<Invite>,
    audit: Vec<AuditEntry>,
    /// The instance as last fetched, which the form edits.
    admin_form: Option<Instance>,
    admin_saving: bool,
    admin_notice: String,
    /// The member whose roles are being edited.
    admin_picked: Option<String>,
    /// Why the last call attempt failed.
    call_notice: String,
    /// When the current call was answered, for the duration readout.
    call_started: Option<i64>,
    /// Which tab the inbox is on: 0 conversations, 1 friends.
    inbox_tab: i32,
    /// The channel the pointer went down on, which becomes the dragged one
    /// once the pointer has moved far enough to mean it.
    pressed_channel: Option<String>,
    /// The channel being dragged, and how far it has moved.
    dragging: Option<String>,
    drag_offset: f32,
    /// The attachment open in the lightbox.
    lightbox: Option<Attachment>,
    /// The message being forwarded, and where to.
    forwarding: Option<Message>,
    forward_target: String,
    forward_busy: bool,
    /// The message being edited in place, and the text so far.
    editing: Option<String>,
    /// The message a reaction is being picked for, if the picker was opened
    /// from a message rather than the composer.
    reacting_to: Option<String>,
    /// What the open context menu is about.
    menu_target: Option<MenuTarget>,

    /// How far the newest message is below the fold, as the pane last
    /// reported it. Zero or less means the bottom is on screen.
    scroll_distance: f32,
    /// Bumped to ask the pane to jump to the newest message.
    scroll_token: i32,
    /// Bumped to put the caret back in the composer.
    focus_token: i32,
    /// The instance origin, for turning a relative upload URL into one the
    /// desktop can open.
    origin: String,
    /// A page of older messages is in flight.
    loading_older: bool,
    /// Channels known to have nothing older left, so scrolling to the top
    /// stops asking for a page that will come back empty.
    exhausted: Vec<String>,
    /// The message list's content and viewport heights, as the pane last
    /// reported them. Needed to turn a distance-from-bottom into "near the
    /// top", which is what triggers a page back.
    content_height: f32,
    viewport_height: f32,
    /// The member's own voice connection.
    voice: crate::voice::Session,
    /// The media engine, or the no-media one in a build without the feature.
    engine: Box<dyn crate::voice::Engine>,
    /// Who the engine last reported as speaking.
    speaking: Vec<String>,
    /// When typing was last announced, per channel. The gateway notice is
    /// throttled: one keystroke per frame would be a frame per keystroke.
    typing_sent: std::collections::HashMap<String, i64>,
}

/// How close to the bottom counts as "reading the newest messages".
///
/// A reader a line or two up is still following along and expects an
/// arriving message to scroll into view; one who has scrolled up to read
/// something does not, and moving the view under them is the single most
/// irritating thing a chat client does.
const PINNED_WITHIN: f32 = 90.0;

/// How close to the top the reader gets before the next page is fetched.
/// Far enough ahead that the page usually arrives before they reach the end.
const PREFETCH_WITHIN: f32 = 400.0;

/// Something an administrator asked for.
#[derive(Clone, Debug)]
enum AdminAction {
    Kick(String),
    Ban {
        user: String,
        reason: String,
    },
    Unban(String),
    SetRole {
        user: String,
        role: String,
        granted: bool,
    },
    CreateInvite,
    RevokeInvite(String),
    SaveInstance(serde_json::Value),
}

/// A change to how I relate to another member.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Relate {
    Add,
    Remove,
    Block,
    Unblock,
    Favourite(bool),
}

/// An action held behind a confirmation.
#[derive(Clone, Debug, PartialEq)]
enum Confirm {
    DeleteMessage(String),
    SignOut,
    Kick(String),
    Ban(String),
}

#[derive(Clone, Debug, PartialEq)]
enum MenuTarget {
    Message(String),
    Channel(String),
    Category(String),
    Member(String),
}

impl AppState {
    fn new(settings: Settings) -> Self {
        Self {
            store: Store::default(),
            settings,
            images: images::Cache::new(),
            measurer: layout::Measurer::new(),
            palette: Palette::resolve(&Branding::default()),
            body_width: 566.0,
            connection: gateway::Status::Closed,
            replying_to: None,
            next_temp: 0,
            panel_hits: Vec::new(),
            panel_loading: false,
            profile: None,
            pending_confirm: None,
            window_focused: true,
            pending_attachments: Vec::new(),
            attach_notice: String::new(),
            admin_section: "overview".into(),
            stats: None,
            bans: Vec::new(),
            invites: Vec::new(),
            audit: Vec::new(),
            admin_form: None,
            admin_saving: false,
            admin_notice: String::new(),
            admin_picked: None,
            call_notice: String::new(),
            call_started: None,
            inbox_tab: 0,
            pressed_channel: None,
            dragging: None,
            drag_offset: 0.0,
            lightbox: None,
            forwarding: None,
            forward_target: String::new(),
            forward_busy: false,
            editing: None,
            reacting_to: None,
            menu_target: None,
            scroll_distance: 0.0,
            scroll_token: 0,
            focus_token: 0,
            origin: String::new(),
            loading_older: false,
            exhausted: Vec::new(),
            content_height: 0.0,
            viewport_height: 0.0,
            voice: crate::voice::Session::default(),
            engine: crate::voice::engine(),
            speaking: Vec::new(),
            typing_sent: std::collections::HashMap::new(),
        }
    }

    /// Whether the reader is at the bottom of the channel.
    fn pinned_to_bottom(&self) -> bool {
        self.scroll_distance <= PINNED_WITHIN
    }

    /// Whether the reader has scrolled within a screen of the top, which is
    /// when the next page back is worth fetching.
    ///
    /// `distance` is measured from the bottom, so this compares it against
    /// the whole scrollable height less one screen. The pane reports the
    /// distance rather than the position, so the comparison is done with
    /// what the last refresh knew the content height to be.
    fn scroll_distance_at_top(&self, distance: f32) -> bool {
        distance >= self.content_height - self.viewport_height - PREFETCH_WITHIN
    }

    /// An upload URL the desktop can open. Uploads come through relative.
    fn origin_url(&self, url: &str) -> String {
        if url.starts_with("http://") || url.starts_with("https://") {
            return url.to_string();
        }
        format!(
            "{}/{}",
            self.origin.trim_end_matches('/'),
            url.trim_start_matches('/')
        )
    }

    fn temp_id(&mut self) -> String {
        self.next_temp += 1;
        format!("pending-{}", self.next_temp)
    }

    /// The branding the palette is built from, honouring the member's own
    /// theme choice over the instance's default.
    fn branding(&self) -> Branding {
        let instance = &self.store.instance;
        let light = match self.settings.theme {
            ThemeChoice::Light => true,
            ThemeChoice::Dark => false,
            // "System" is what the instance asked for; without a portal to
            // ask the desktop, the instance's own default is the answer.
            ThemeChoice::Instance => instance.theme_mode == ThemeMode::Light,
        };
        Branding {
            accent: Rgb::parse(&instance.accent_color).unwrap_or(Branding::default().accent),
            tint: instance.surface_tint.as_deref().and_then(Rgb::parse),
            corner_radius: instance.corner_radius as f32,
            light,
        }
    }
}

type Shared = Rc<RefCell<AppState>>;

pub fn run(settings: Settings) -> Result<(), Box<dyn std::error::Error>> {
    let app = ui::App::new()?;
    let state: Shared = Rc::new(RefCell::new(AppState::new(settings.clone())));

    let (task_tx, task_rx) = mpsc::unbounded_channel::<Task>();
    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<Event>();

    // The runtime thread. Slint owns the main thread, so nothing that waits
    // may run on it.
    let runtime = std::thread::Builder::new()
        .name("minichat-net".into())
        .spawn({
            let event_tx = event_tx.clone();
            move || {
                let runtime = match tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(2)
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime,
                    Err(error) => {
                        eprintln!("minichat: could not start the network runtime: {error}");
                        return;
                    }
                };
                runtime.block_on(serve(task_rx, event_tx));
            }
        })?;

    // The tray, on the platforms that have one. Moved into the pump, which
    // holds it for the life of the app — dropping it takes the icon out of
    // the tray.
    let tray = crate::tray::Tray::new(state.borrow().settings.close_to_tray);

    // Global voice hotkeys, registered with the desktop so push-to-talk
    // works with the window in the background. Moved into the pump, which
    // holds them for the life of the app.
    // Shared with the settings callback rather than owned by the pump alone,
    // so turning hotkeys off re-registers at once instead of at next launch.
    let hotkeys = Rc::new(RefCell::new(crate::hotkeys::Hotkeys::new()));
    if let Some(hotkeys) = hotkeys.borrow_mut().as_mut() {
        hotkeys.apply(&state.borrow().settings.hotkeys);
    }

    // Events come back on the runtime thread and are drained here, on the UI
    // thread, where the store lives.
    let pump = {
        let weak = app.as_weak();
        let state = state.clone();
        let tasks = task_tx.clone();
        let tray_handle = tray;
        let hotkey_handle = hotkeys.clone();
        move || {
            let Some(app) = weak.upgrade() else { return };
            let mut dirty = false;
            while let Ok(event) = event_rx.try_recv() {
                dirty |= handle_event(event, &app, &state, &tasks);
            }

            // Who is speaking comes from the media engine rather than the
            // gateway — LiveKit knows within a frame or two, and the server
            // is not told at all. Polled here because it changes constantly
            // and pushing every change would be a repaint per frame.
            {
                let mut state_ref = state.borrow_mut();
                if state_ref.voice.is_live() {
                    let speaking = state_ref.engine.speaking();
                    if speaking != state_ref.speaking {
                        state_ref.speaking = speaking;
                        dirty = true;
                    }
                } else if !state_ref.speaking.is_empty() {
                    state_ref.speaking.clear();
                    dirty = true;
                }

                // Where the media engine got to. Polled with the speakers
                // rather than pushed, because the engine runs on its own
                // runtime and this is the one place per frame that the store
                // is already borrowed.
                if state_ref.voice.status != crate::voice::Status::Disconnected {
                    dirty |= follow_media(&mut state_ref);
                }
            }

            for action in tray_handle.as_ref().map(|t| t.poll()).unwrap_or_default() {
                dirty |= handle_tray(action, &app, &state, &tasks, tray_handle.as_ref());
            }

            let pressed = hotkey_handle
                .borrow()
                .as_ref()
                .map(|h| h.poll())
                .unwrap_or_default();
            for action in pressed {
                dirty |= handle_hotkey(action, &state);
            }

            if dirty {
                refresh(&app, &state, &tasks);
            }
        }
    };

    // A 16ms tick is the simplest correct way to drain a channel into Slint
    // without a second wake-up path; at rest it costs one empty try_recv.
    let timer = slint::Timer::default();
    timer.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_millis(16),
        pump,
    );

    wire_callbacks(&app, &state, &task_tx, &hotkeys);

    // Open straight into the saved instance when there is a saved session,
    // otherwise show the connect screen.
    {
        let state_ref = state.borrow();
        let saved = state_ref.settings.instance_url.clone();
        let token = state_ref.settings.token.clone();
        drop(state_ref);

        theme::apply(&app, &state.borrow().palette);
        if let Some(origin) = state.borrow().settings.instance_url.clone() {
            state.borrow_mut().origin = origin;
        }
        match (saved, token) {
            (Some(origin), Some(token)) => {
                app.set_instance_name("MiniChat".into());
                let _ = task_tx.send(Task::Meta(origin.clone()));
                let _ = task_tx.send(Task::Connect { origin, token });
            }
            (Some(origin), None) => {
                app.set_address(origin.clone().into());
                app.set_screen(ui::Screen::Signin);
                let _ = task_tx.send(Task::Meta(origin));
            }
            _ => {
                app.set_screen(ui::Screen::Connect);
                if let Some(last) = settings.instance_url {
                    app.set_last_instance(last.into());
                }
            }
        }
    }

    app.run()?;

    let _ = task_tx.send(Task::Disconnect);
    drop(task_tx);
    let _ = runtime.join();
    Ok(())
}

// --- the runtime side ---------------------------------------------------

async fn serve(mut tasks: mpsc::UnboundedReceiver<Task>, events: mpsc::UnboundedSender<Event>) {
    let mut client: Option<api::Client> = None;
    let mut gateway_tx: Option<mpsc::UnboundedSender<gateway::Command>> = None;

    while let Some(task) = tasks.recv().await {
        match task {
            Task::Disconnect => {
                if let Some(tx) = gateway_tx.take() {
                    let _ = tx.send(gateway::Command::Close);
                }
                return;
            }

            Task::Meta(origin) => {
                let events = events.clone();
                match api::Client::new(&origin) {
                    Ok(fresh) => {
                        client.get_or_insert(fresh.clone());
                        tokio::spawn(async move {
                            let _ = match fresh.meta().await {
                                Ok(meta) => events.send(Event::Meta(Box::new(meta))),
                                Err(error) => events.send(Event::MetaFailed(error.to_string())),
                            };
                        });
                    }
                    Err(error) => {
                        let _ = events.send(Event::MetaFailed(error.to_string()));
                    }
                }
            }

            Task::SignIn {
                origin,
                username,
                password,
            } => {
                let events = events.clone();
                match api::Client::new(&origin) {
                    Ok(fresh) => {
                        client = Some(fresh.clone());
                        tokio::spawn(async move {
                            let _ = match fresh.login(&username, &password).await {
                                Ok(auth) => events.send(Event::SignedIn(auth.token)),
                                Err(error) => events.send(Event::SignInFailed(error.to_string())),
                            };
                        });
                    }
                    Err(error) => {
                        let _ = events.send(Event::SignInFailed(error.to_string()));
                    }
                }
            }

            Task::Connect { origin, token } => {
                let Ok(fresh) = api::Client::new(&origin) else {
                    let _ = events.send(Event::SignInFailed(
                        "That instance address is not valid.".into(),
                    ));
                    continue;
                };
                fresh.set_token(Some(token.clone()));
                client = Some(fresh);

                if let Some(previous) = gateway_tx.take() {
                    let _ = previous.send(gateway::Command::Close);
                }
                let (command_tx, command_rx) = mpsc::unbounded_channel();
                gateway_tx = Some(command_tx);

                let (update_tx, mut update_rx) = mpsc::unbounded_channel();
                tokio::spawn(gateway::run(
                    api::gateway_url(&origin),
                    token,
                    update_tx,
                    command_rx,
                ));

                let events = events.clone();
                tokio::spawn(async move {
                    while let Some(update) = update_rx.recv().await {
                        let sent = match update {
                            gateway::Update::Status(status) => events.send(Event::Status(status)),
                            gateway::Update::SessionInvalid => events.send(Event::SessionInvalid),
                            gateway::Update::Event(frame) => {
                                if frame.event == "READY" {
                                    match serde_json::from_value::<ReadyPayload>(frame.data.clone())
                                    {
                                        Ok(ready) => events.send(Event::Ready(Box::new(ready))),
                                        Err(_) => events.send(Event::Gateway(frame)),
                                    }
                                } else {
                                    events.send(Event::Gateway(frame))
                                }
                            }
                        };
                        if sent.is_err() {
                            return;
                        }
                    }
                });
            }

            Task::LoadMessages { channel } => {
                let (Some(client), events) = (client.clone(), events.clone()) else {
                    continue;
                };
                tokio::spawn(async move {
                    let _ = match client.messages(&channel, None, None, 50).await {
                        Ok(messages) => events.send(Event::Messages { channel, messages }),
                        Err(error) => events.send(Event::MessagesFailed {
                            channel,
                            error: error.to_string(),
                        }),
                    };
                });
            }

            Task::Send {
                channel,
                temp_id,
                content,
                reply_to,
                attachments,
            } => {
                let (Some(client), events) = (client.clone(), events.clone()) else {
                    continue;
                };
                tokio::spawn(async move {
                    let _ = match client
                        .send_message_with(&channel, &content, reply_to.as_deref(), &attachments)
                        .await
                    {
                        Ok(saved) => events.send(Event::Sent {
                            channel,
                            temp_id,
                            saved: Box::new(saved),
                        }),
                        Err(_) => events.send(Event::SendFailed { channel, temp_id }),
                    };
                });
            }

            Task::React { message, emoji, on } => {
                let Some(client) = client.clone() else {
                    continue;
                };
                tokio::spawn(async move {
                    // The gateway echoes the new counts, so nothing is sent
                    // back from here.
                    let _ = client.react(&message, &emoji, on).await;
                });
            }

            Task::Ack { channel, message } => {
                let Some(client) = client.clone() else {
                    continue;
                };
                tokio::spawn(async move {
                    let _ = client.ack(&channel, &message).await;
                });
            }

            Task::DeleteMessage(id) => {
                let Some(client) = client.clone() else {
                    continue;
                };
                tokio::spawn(async move {
                    let _ = client.delete_message(&id).await;
                });
            }

            Task::EditMessage { message, content } => {
                let Some(client) = client.clone() else {
                    continue;
                };
                tokio::spawn(async move {
                    // The gateway echoes MESSAGE_UPDATE, so the edit lands in
                    // the store by the same path as anyone else's.
                    let _ = client.edit_message(&message, &content).await;
                });
            }

            Task::Attach { channel, max_mb } => {
                let (Some(client), events) = (client.clone(), events.clone()) else {
                    continue;
                };
                tokio::spawn(async move {
                    let Some(paths) = rfd::AsyncFileDialog::new()
                        .set_title("Attach files")
                        .pick_files()
                        .await
                    else {
                        // Cancelled. Not an error, and not worth a notice.
                        return;
                    };

                    let mut uploaded = Vec::new();
                    for handle in paths {
                        match client.upload(handle.path(), max_mb).await {
                            Ok(file) => uploaded.push(file),
                            Err(error) => {
                                // One bad file does not throw away the rest.
                                let _ = events.send(Event::AttachFailed(error.to_string()));
                            }
                        }
                    }
                    if !uploaded.is_empty() {
                        let _ = events.send(Event::Attached {
                            channel,
                            files: uploaded,
                        });
                    }
                });
            }

            Task::PinMessage { message, pinned } => {
                let Some(client) = client.clone() else {
                    continue;
                };
                tokio::spawn(async move {
                    let _ = client.pin_message(&message, pinned).await;
                });
            }

            Task::FetchFullImage(url) => {
                let (Some(client), events) = (client.clone(), events.clone()) else {
                    continue;
                };
                tokio::spawn(async move {
                    let _ = match client.fetch_bytes(&url, images::MAX_BYTES).await {
                        Ok(bytes) => events.send(Event::FullImageLoaded { url, bytes }),
                        Err(_) => events.send(Event::FullImageFailed(url)),
                    };
                });
            }

            Task::Search { query } => {
                let (Some(client), events) = (client.clone(), events.clone()) else {
                    continue;
                };
                tokio::spawn(async move {
                    let hits = client.search(&query, None).await.unwrap_or_default();
                    let _ = events.send(Event::SearchResults(hits));
                });
            }

            Task::LoadPins { channel } => {
                let (Some(client), events) = (client.clone(), events.clone()) else {
                    continue;
                };
                tokio::spawn(async move {
                    let pins = client.pins(&channel).await.unwrap_or_default();
                    let _ = events.send(Event::Pins(pins));
                });
            }

            Task::Typing(channel) => {
                if let Some(tx) = gateway_tx.as_ref() {
                    let _ = tx.send(gateway::Command::Typing(channel));
                }
            }

            Task::JoinVoice { channel } => {
                let (Some(client), events) = (client.clone(), events.clone()) else {
                    continue;
                };
                tokio::spawn(async move {
                    // The instance decides what this member may do in this
                    // channel, so the grant is asked for rather than
                    // assembled from the instance-wide permissions.
                    let _ = match client.voice_grant(&channel).await {
                        Ok(grant) => events.send(Event::VoiceGrant {
                            channel,
                            grant: Box::new(grant),
                        }),
                        Err(error) => events.send(Event::VoiceFailed {
                            channel,
                            error: error.to_string(),
                        }),
                    };
                });
            }

            Task::ReorderChannels(order) => {
                let (Some(client), events) = (client.clone(), events.clone()) else {
                    continue;
                };
                tokio::spawn(async move {
                    // The gateway echoes CHANNEL_UPDATE for each move, so the
                    // store is brought up to date by the same path as anyone
                    // else's reorder. This only reports that it landed.
                    if client.reorder_channels(&order).await.is_ok() {
                        let _ = events.send(Event::Reordered);
                    }
                });
            }

            Task::ReorderCategories(order) => {
                let Some(client) = client.clone() else {
                    continue;
                };
                tokio::spawn(async move {
                    for (id, position) in order {
                        let _ = client
                            .update_category(&id, serde_json::json!({ "position": position }))
                            .await;
                    }
                });
            }

            Task::StartCall { conversation } => {
                let (Some(client), events) = (client.clone(), events.clone()) else {
                    continue;
                };
                tokio::spawn(async move {
                    // The gateway announces the call to both sides, so the
                    // store learns about it by the same path either way.
                    if let Err(error) = client.start_call(&conversation).await {
                        let _ = events.send(Event::CallFailed(error.to_string()));
                    }
                });
            }

            Task::AnswerCall { call, action } => {
                let (Some(client), events) = (client.clone(), events.clone()) else {
                    continue;
                };
                tokio::spawn(async move {
                    if let Err(error) = client.call_action(&call, action).await {
                        let _ = events.send(Event::CallFailed(error.to_string()));
                    }
                });
            }

            Task::LoadAdmin(section) => {
                let (Some(client), events) = (client.clone(), events.clone()) else {
                    continue;
                };
                tokio::spawn(async move {
                    // Each section fetches only what it shows: an admin who
                    // opens the overview should not pull the whole audit log.
                    match section {
                        "overview" => {
                            if let Ok(stats) = client.stats().await {
                                let _ = events.send(Event::AdminStats(Box::new(stats)));
                            }
                        }
                        "instance" => {
                            if let Ok(instance) = client.admin_instance().await {
                                let _ = events.send(Event::AdminInstance(Box::new(instance)));
                            }
                        }
                        "bans" => {
                            let _ = events
                                .send(Event::AdminBans(client.bans().await.unwrap_or_default()));
                        }
                        "invites" => {
                            let _ = events.send(Event::AdminInvites(
                                client.invites().await.unwrap_or_default(),
                            ));
                        }
                        "audit" => {
                            let _ = events.send(Event::AdminAudit(
                                client.audit(None).await.unwrap_or_default(),
                            ));
                        }
                        _ => {}
                    }
                });
            }

            Task::AdminAction(action) => {
                let (Some(client), events) = (client.clone(), events.clone()) else {
                    continue;
                };
                tokio::spawn(async move {
                    let (outcome, reload) = match action {
                        AdminAction::Kick(user) => {
                            (client.kick_member(&user).await.map(|_| ()), "")
                        }
                        AdminAction::Ban { user, reason } => {
                            (client.ban_member(&user, &reason).await.map(|_| ()), "bans")
                        }
                        AdminAction::Unban(user) => {
                            (client.unban_member(&user).await.map(|_| ()), "bans")
                        }
                        AdminAction::SetRole {
                            user,
                            role,
                            granted,
                        } => (
                            client
                                .set_member_role(&user, &role, granted)
                                .await
                                .map(|_| ()),
                            "",
                        ),
                        AdminAction::CreateInvite => (
                            // A week, ten uses: the defaults the web client
                            // offers, so an invite made here behaves the same.
                            client.create_invite("", 10, 24 * 7).await.map(|_| ()),
                            "invites",
                        ),
                        AdminAction::RevokeInvite(code) => {
                            (client.revoke_invite(&code).await.map(|_| ()), "invites")
                        }
                        AdminAction::SaveInstance(payload) => (
                            client.update_instance(payload).await.map(|_| ()),
                            "instance",
                        ),
                    };

                    let _ = events.send(Event::AdminDone(match outcome {
                        Ok(()) => String::new(),
                        Err(error) => error.to_string(),
                    }));

                    // Whatever changed, the list it changed is refetched:
                    // the server is the record, not the local guess.
                    match reload {
                        "bans" => {
                            let _ = events
                                .send(Event::AdminBans(client.bans().await.unwrap_or_default()));
                        }
                        "invites" => {
                            let _ = events.send(Event::AdminInvites(
                                client.invites().await.unwrap_or_default(),
                            ));
                        }
                        "instance" => {
                            if let Ok(instance) = client.admin_instance().await {
                                let _ = events.send(Event::AdminInstance(Box::new(instance)));
                            }
                        }
                        _ => {}
                    }
                });
            }

            Task::LoadRelationships => {
                let (Some(client), events) = (client.clone(), events.clone()) else {
                    continue;
                };
                tokio::spawn(async move {
                    let list = client.relationships().await.unwrap_or_default();
                    let _ = events.send(Event::Relationships(list));
                });
            }

            Task::Relate { user, action } => {
                let (Some(client), events) = (client.clone(), events.clone()) else {
                    continue;
                };
                tokio::spawn(async move {
                    let outcome = match action {
                        Relate::Add => client.add_friend(&user).await.map(|_| ()),
                        Relate::Remove => client.remove_friend(&user).await.map(|_| ()),
                        Relate::Block => client.block_user(&user).await.map(|_| ()),
                        Relate::Unblock => client.unblock_user(&user).await.map(|_| ()),
                        Relate::Favourite(on) => client.set_favourite(&user, on).await.map(|_| ()),
                    };
                    // Refetched either way: the local guess is there so the
                    // button answers at once, and the server's answer is
                    // what the client ends up holding.
                    let _ = outcome;
                    let list = client.relationships().await.unwrap_or_default();
                    let _ = events.send(Event::Relationships(list));
                });
            }

            Task::LoadConversations => {
                let (Some(client), events) = (client.clone(), events.clone()) else {
                    continue;
                };
                tokio::spawn(async move {
                    let list = client.conversations().await.unwrap_or_default();
                    let _ = events.send(Event::Conversations(list));
                });
            }

            Task::OpenConversation { peer } => {
                let (Some(client), events) = (client.clone(), events.clone()) else {
                    continue;
                };
                tokio::spawn(async move {
                    if let Ok(conversation) = client.open_conversation(&peer).await {
                        let _ = events.send(Event::ConversationOpened(Box::new(conversation)));
                    }
                });
            }

            Task::LoadDirect { conversation } => {
                let (Some(client), events) = (client.clone(), events.clone()) else {
                    continue;
                };
                tokio::spawn(async move {
                    let messages = client
                        .direct_history(&conversation, None)
                        .await
                        .unwrap_or_default();
                    let _ = events.send(Event::DirectHistory {
                        conversation,
                        messages,
                    });
                });
            }

            Task::SendDirect {
                conversation,
                temp_id,
                content,
            } => {
                let (Some(client), events) = (client.clone(), events.clone()) else {
                    continue;
                };
                tokio::spawn(async move {
                    let _ = match client.send_direct(&conversation, &content).await {
                        Ok(saved) => events.send(Event::DirectSent {
                            conversation,
                            temp_id,
                            saved: Box::new(saved),
                        }),
                        Err(_) => events.send(Event::DirectSendFailed {
                            conversation,
                            temp_id,
                        }),
                    };
                });
            }

            Task::AckDirect {
                conversation,
                message,
            } => {
                let Some(client) = client.clone() else {
                    continue;
                };
                tokio::spawn(async move {
                    let _ = client.ack_direct(&conversation, &message).await;
                });
            }

            Task::LoadOlder { channel, before } => {
                let (Some(client), events) = (client.clone(), events.clone()) else {
                    continue;
                };
                tokio::spawn(async move {
                    let messages = client
                        .messages(&channel, Some(&before), None, 50)
                        .await
                        .unwrap_or_default();
                    let _ = events.send(Event::OlderMessages { channel, messages });
                });
            }

            Task::UpdateStatus(status) => {
                let Some(client) = client.clone() else {
                    continue;
                };
                tokio::spawn(async move {
                    // The gateway echoes a MEMBER_UPDATE, so the store is
                    // brought up to date by the same path as anyone else's.
                    let _ = client
                        .update_me(serde_json::json!({ "custom_status": status }))
                        .await;
                });
            }

            Task::FetchImages(urls) => {
                let (Some(client), events) = (client.clone(), events.clone()) else {
                    continue;
                };
                for url in urls {
                    let client = client.clone();
                    let events = events.clone();
                    tokio::spawn(async move {
                        let _ = match client.fetch_bytes(&url, images::MAX_BYTES).await {
                            Ok(bytes) => events.send(Event::ImageLoaded {
                                url,
                                bytes,
                                max_side: images::MAX_PREVIEW,
                            }),
                            Err(_) => events.send(Event::ImageFailed(url)),
                        };
                    });
                }
            }
        }
    }
}

// --- the UI side ---------------------------------------------------------

/// Fold one event in. Returns true when the interface needs rebuilding.
fn handle_event(
    event: Event,
    app: &ui::App,
    state: &Shared,
    tasks: &mpsc::UnboundedSender<Task>,
) -> bool {
    let now = time::OffsetDateTime::now_utc().unix_timestamp();

    match event {
        Event::Meta(meta) => {
            let mut state = state.borrow_mut();
            // Before sign-in the instance record is only the public subset,
            // but it carries the branding, which is the point: the sign-in
            // screen is already the instance's own colours.
            state.store.instance = *meta;
            state.palette = Palette::resolve(&state.branding());
            let palette = state.palette.clone();
            drop(state);
            theme::apply(app, &palette);
            true
        }

        Event::MetaFailed(error) => {
            app.set_connecting(false);
            app.set_connect_error(error.into());
            app.set_screen(ui::Screen::Connect);
            false
        }

        Event::SignedIn(token) => {
            let mut state = state.borrow_mut();
            state.settings.token = Some(token.clone());
            let origin = state.settings.instance_url.clone().unwrap_or_default();
            state.origin = origin.clone();
            if let Err(error) = state.settings.store() {
                eprintln!("minichat: {error}");
            }
            drop(state);

            app.set_authenticating(false);
            app.set_auth_error(SharedString::new());
            app.set_password(SharedString::new());
            let _ = tasks.send(Task::Connect { origin, token });
            false
        }

        Event::SignInFailed(error) => {
            app.set_authenticating(false);
            app.set_auth_error(error.into());
            app.set_screen(ui::Screen::Signin);
            false
        }

        Event::Ready(ready) => {
            let mut state = state.borrow_mut();
            let wanted = state.settings.last_channel.clone();
            state.store.apply_ready(*ready);
            if let Some(channel) = wanted {
                if state.store.channel(&channel).is_some() {
                    state.store.selected_channel = channel;
                }
            }
            state.palette = Palette::resolve(&state.branding());
            state.store.collapsed_categories = state.settings.collapsed_categories.clone();

            let palette = state.palette.clone();
            let channel = state.store.selected_channel.clone();
            let avatars = view::referenced_images(&[], &state.store.members, &state.store.emojis);
            drop(state);

            theme::apply(app, &palette);
            app.set_screen(ui::Screen::Chat);
            if !channel.is_empty() {
                let _ = tasks.send(Task::LoadMessages { channel });
            }
            if !avatars.is_empty() {
                let _ = tasks.send(Task::FetchImages(avatars));
            }
            let _ = tasks.send(Task::LoadRelationships);
            let _ = tasks.send(Task::LoadConversations);
            true
        }

        Event::Gateway(frame) => {
            if frame.event == "RELATIONSHIPS_STALE" {
                let _ = tasks.send(Task::LoadRelationships);
            }
            let mut state_ref = state.borrow_mut();
            let changed = state_ref.store.apply_event(&frame, now);

            // Notifications are decided from the message rather than from the
            // server's MENTION_ADD: the two can arrive in either order, and a
            // notification that fires on the wrong one is late or doubled.
            if frame.event == "MESSAGE_CREATE" {
                if let Ok(message) = serde_json::from_value::<Message>(frame.data.clone()) {
                    let focused = app.window().is_visible()
                        && state_ref.window_focused
                        && !state_ref.store.inbox_open
                        && message.channel_id == state_ref.store.selected_channel;

                    if crate::notify::should_notify(
                        &message,
                        &state_ref.store.me.member.id,
                        &state_ref.store.members,
                        &state_ref.store.notifications,
                        focused,
                    ) {
                        let channel = state_ref
                            .store
                            .channel(&message.channel_id)
                            .map(|c| format!("#{}", c.name))
                            .unwrap_or_default();
                        let title = if channel.is_empty() {
                            message.author_name().to_string()
                        } else {
                            format!("{} in {channel}", message.author_name())
                        };
                        // The body is the message as written, minus the
                        // markup, so a notification is readable rather than
                        // full of asterisks.
                        let context = crate::text::markdown::Context {
                            members: &state_ref.store.members,
                            emojis: &state_ref.store.emojis,
                            me_id: &state_ref.store.me.member.id,
                        };
                        let body = crate::text::markdown::to_plain(&crate::text::markdown::parse(
                            &message.content,
                            &context,
                        ));
                        crate::notify::show(&title, &body);
                    }
                }
            }

            // An accepted call is a voice room like any other, reached with
            // the `direct:` conversation ID. Joined once, on the transition
            // to accepted, rather than on every event about the call.
            if frame.event == "DIRECT_CALL" {
                let live = state_ref
                    .store
                    .my_call()
                    .filter(|c| c.status == CallStatus::Accepted)
                    .map(|c| c.conversation_id.clone());
                match live {
                    Some(conversation) if state_ref.call_started.is_none() => {
                        state_ref.call_started = Some(now);
                        state_ref.call_notice.clear();
                        drop(state_ref);
                        let _ = tasks.send(Task::JoinVoice {
                            channel: format!("direct:{conversation}"),
                        });
                        return true;
                    }
                    None => {
                        // The call ended, from either side.
                        if state_ref.call_started.take().is_some() {
                            state_ref.engine.disconnect();
                            state_ref.voice.left();
                        }
                    }
                    _ => {}
                }
                return changed;
            }

            if frame.event == "DIRECT_MESSAGE" {
                if let Ok(direct) = serde_json::from_value::<DirectMessage>(frame.data.clone()) {
                    let reading_it = state_ref.window_focused
                        && state_ref.store.inbox_open
                        && state_ref.store.selected_conversation == direct.conversation_id;
                    let mine = direct.author_id == state_ref.store.me.member.id;
                    if !mine && !reading_it {
                        let from = state_ref
                            .store
                            .member(&direct.author_id)
                            .map(|m| m.name().to_string())
                            .unwrap_or_else(|| "Someone".into());
                        crate::notify::show(&from, &direct.content);
                    }
                }
            }

            // An arriving message in the open channel follows the reader
            // down only if they were already at the bottom.
            if frame.event == "MESSAGE_CREATE" && state_ref.pinned_to_bottom() {
                let landed_here = frame.data["channel_id"].as_str()
                    == Some(state_ref.store.selected_channel.as_str());
                if landed_here {
                    state_ref.scroll_token += 1;
                }
            }
            changed
        }

        Event::Status(status) => {
            state.borrow_mut().connection = status;
            app.set_connection(
                match status {
                    gateway::Status::Connecting => "connecting",
                    gateway::Status::Ready => "ready",
                    gateway::Status::Reconnecting => "reconnecting",
                    gateway::Status::Closed => "closed",
                }
                .into(),
            );
            false
        }

        Event::SessionInvalid => {
            let mut state = state.borrow_mut();
            state.settings.sign_out();
            let _ = state.settings.store();
            drop(state);
            app.set_screen(ui::Screen::Signin);
            app.set_auth_error("Your session expired. Sign in again.".into());
            false
        }

        Event::Messages { channel, messages } => {
            let mut state = state.borrow_mut();
            state.store.set_messages(&channel, messages);
            if channel == state.store.selected_channel {
                // Opening a channel puts you at the newest message, the way
                // every chat client does.
                state.scroll_token += 1;
                state.scroll_distance = 0.0;
            }
            let wanted = view::referenced_images(
                state.store.messages_in(&channel),
                &[],
                &state.store.emojis,
            );
            drop(state);
            if !wanted.is_empty() {
                let _ = tasks.send(Task::FetchImages(wanted));
            }
            true
        }

        Event::MessagesFailed { channel, error } => {
            eprintln!("minichat: could not load #{channel}: {error}");
            true
        }

        Event::Sent {
            channel,
            temp_id,
            saved,
        } => {
            state
                .borrow_mut()
                .store
                .confirm_message(&channel, &temp_id, *saved);
            true
        }

        Event::SendFailed { channel, temp_id } => {
            state.borrow_mut().store.fail_message(&channel, &temp_id);
            true
        }

        Event::ImageLoaded {
            url,
            bytes,
            max_side,
        } => {
            let state = state.borrow();
            state.images.insert(&url, &bytes, max_side)
        }

        Event::ImageFailed(url) => {
            state.borrow().images.fail(&url);
            false
        }

        Event::FullImageLoaded { url, bytes } => {
            let state = state.borrow();
            state.images.insert_full(&url, &bytes)
        }

        Event::FullImageFailed(url) => {
            state.borrow().images.fail_full(&url);
            false
        }

        Event::OlderMessages { channel, messages } => {
            if messages.is_empty() {
                // Nothing older exists; stop asking.
                state.borrow_mut().exhausted.push(channel);
                return false;
            }
            let mut state = state.borrow_mut();
            state.store.prepend_messages(&channel, messages);
            state.loading_older = false;
            true
        }

        Event::VoiceGrant { channel, grant } => {
            let mut state_ref = state.borrow_mut();
            state_ref.voice.joining(&channel, &grant);
            let muted = state_ref.voice.muted;

            // Returns at once; the room connects on its own thread and the
            // pump below follows it through `engine.media()`.
            let outcome = state_ref.engine.connect(&grant, muted);
            state_ref.voice.status = match outcome {
                Ok(()) if crate::voice::has_media() => crate::voice::Status::Connecting,
                // The control plane joined; this build has no media. Not an
                // error — the member is in the channel and visible to
                // everyone else.
                Ok(()) => crate::voice::Status::ControlOnly,
                Err(error) => {
                    state_ref.voice.notice = error;
                    crate::voice::Status::Failed
                }
            };
            if state_ref.voice.status == crate::voice::Status::ControlOnly {
                state_ref.voice.notice =
                    "This build carries no audio. See native/README.md.".into();
            }
            true
        }

        Event::VoiceFailed { channel, error } => {
            let mut state_ref = state.borrow_mut();
            state_ref.voice.left();
            state_ref.voice.status = crate::voice::Status::Failed;
            state_ref.voice.channel = channel;
            state_ref.voice.notice = error;
            true
        }

        Event::Attached { channel, files } => {
            let mut state_ref = state.borrow_mut();
            if state_ref.store.selected_channel != channel {
                return false;
            }
            let previews: Vec<String> = files
                .iter()
                .filter(|f| f.is_image())
                .map(|f| f.url.clone())
                .filter(|url| state_ref.images.claim(url))
                .collect();
            state_ref.pending_attachments.extend(files);
            state_ref.attach_notice.clear();
            drop(state_ref);

            if !previews.is_empty() {
                let _ = tasks.send(Task::FetchImages(previews));
            }
            true
        }

        Event::AttachFailed(error) => {
            let mut state_ref = state.borrow_mut();
            state_ref.attach_notice = error;
            true
        }

        // The server accepted the move; the gateway's CHANNEL_UPDATEs carry
        // the new positions, so there is nothing to apply here.
        Event::Reordered => false,

        Event::CallFailed(error) => {
            state.borrow_mut().call_notice = error;
            true
        }

        Event::AdminStats(stats) => {
            state.borrow_mut().stats = Some(*stats);
            true
        }

        Event::AdminInstance(instance) => {
            let mut state_ref = state.borrow_mut();
            // `/admin/instance` is the only place the public URL comes from,
            // and the forward link needs it.
            state_ref.store.public_url = instance.public_url.clone();
            let (name, tagline, accent) = (
                instance.name.clone(),
                instance.tagline.clone(),
                instance.accent_color.clone(),
            );
            state_ref.admin_form = Some(*instance);
            state_ref.admin_saving = false;
            drop(state_ref);

            // The form is filled from the server's answer rather than left
            // showing whatever was typed before the save.
            app.set_form_name(name.into());
            app.set_form_tagline(tagline.into());
            app.set_form_accent(accent.into());
            true
        }

        Event::AdminBans(list) => {
            state.borrow_mut().bans = list;
            true
        }

        Event::AdminInvites(list) => {
            state.borrow_mut().invites = list;
            true
        }

        Event::AdminAudit(list) => {
            state.borrow_mut().audit = list;
            true
        }

        Event::AdminDone(error) => {
            let mut state_ref = state.borrow_mut();
            state_ref.admin_saving = false;
            state_ref.admin_notice = if error.is_empty() {
                "Saved.".into()
            } else {
                error
            };
            true
        }

        Event::Relationships(list) => {
            state.borrow_mut().store.relationships = list;
            true
        }

        Event::Conversations(list) => {
            state.borrow_mut().store.conversations = list;
            true
        }

        Event::ConversationOpened(conversation) => {
            let mut state_ref = state.borrow_mut();
            let id = conversation.id.clone();
            if state_ref.store.conversation(&id).is_none() {
                state_ref.store.conversations.push(*conversation);
            }
            state_ref.store.inbox_open = true;
            state_ref.store.selected_conversation = id.clone();
            drop(state_ref);
            app.set_overlay(ui::Overlay::None);
            let _ = tasks.send(Task::LoadDirect { conversation: id });
            true
        }

        Event::DirectHistory {
            conversation,
            messages,
        } => {
            let mut state_ref = state.borrow_mut();
            let last = messages.last().map(|m| m.id.clone());
            state_ref.store.set_direct(&conversation, messages);
            if conversation == state_ref.store.selected_conversation {
                state_ref.scroll_token += 1;
                state_ref.store.mark_conversation_read(&conversation);
            }
            drop(state_ref);
            if let Some(message) = last {
                let _ = tasks.send(Task::AckDirect {
                    conversation,
                    message,
                });
            }
            true
        }

        Event::DirectSent {
            conversation,
            temp_id,
            saved,
        } => {
            state
                .borrow_mut()
                .store
                .confirm_direct(&conversation, &temp_id, *saved);
            true
        }

        Event::DirectSendFailed {
            conversation,
            temp_id,
        } => {
            state
                .borrow_mut()
                .store
                .fail_direct(&conversation, &temp_id);
            true
        }

        Event::SearchResults(hits) | Event::Pins(hits) => {
            let mut state = state.borrow_mut();
            state.panel_hits = hits;
            state.panel_loading = false;
            true
        }
    }
}

/// The picker's grid width, in cells.
///
/// The picker is 328px wide with 12px padding and 36px cells, which leaves
/// room for eight across.
const EMOJI_COLUMNS: usize = 8;

/// Chunk entries into rows. Slint has no wrapping layout, so the grid is
/// built here — the same reason message bodies arrive as positioned runs.
fn emoji_rows(entries: Vec<ui::EmojiEntry>) -> Vec<ui::EmojiRow> {
    entries
        .chunks(EMOJI_COLUMNS)
        .map(|chunk| ui::EmojiRow {
            entries: ModelRc::new(VecModel::from(chunk.to_vec())),
        })
        .collect()
}

/// Fold the media engine's state into the voice session.
///
/// Returns true when the UI needs rebuilding.
fn follow_media(state: &mut AppState) -> bool {
    // `None` is a build without media, which the join path has already put
    // in ControlOnly; leaving that alone is what keeps it honest.
    let Some((status, notice)) = state.engine.media().resolve() else {
        return false;
    };

    if state.voice.status == status && state.voice.notice == notice {
        return false;
    }
    state.voice.status = status;
    state.voice.notice = notice;
    true
}

/// Act on a global hotkey. Returns true when the UI needs rebuilding.
///
/// Nothing happens when there is no voice connection: a push-to-talk key
/// pressed while not in a call should do nothing, not silently arm something
/// for the next one.
fn handle_hotkey(action: crate::hotkeys::Action, state: &Shared) -> bool {
    let mut state_ref = state.borrow_mut();
    if !state_ref.voice.is_live() {
        return false;
    }

    match action {
        crate::hotkeys::Action::PushToTalk(held) => {
            if !state_ref.voice.can_speak {
                return false;
            }
            // Held open, closed on release — the opposite of a toggle, and
            // the reason it is a hold at all.
            state_ref.voice.set_muted(!held);
            let muted = state_ref.voice.muted;
            state_ref.engine.set_muted(muted);
        }
        crate::hotkeys::Action::ToggleMute => {
            let next = !state_ref.voice.muted;
            state_ref.voice.set_muted(next);
            let muted = state_ref.voice.muted;
            let deafened = state_ref.voice.deafened;
            state_ref.engine.set_muted(muted);
            state_ref.engine.set_deafened(deafened);
        }
        crate::hotkeys::Action::ToggleDeafen => {
            let next = !state_ref.voice.deafened;
            state_ref.voice.set_deafened(next);
            let muted = state_ref.voice.muted;
            state_ref.engine.set_deafened(next);
            state_ref.engine.set_muted(muted);
        }
    }
    true
}

/// Act on a tray menu choice. Returns true when the UI needs rebuilding.
fn handle_tray(
    action: crate::tray::Action,
    app: &ui::App,
    state: &Shared,
    tasks: &mpsc::UnboundedSender<Task>,
    tray: Option<&crate::tray::Tray>,
) -> bool {
    match action {
        crate::tray::Action::Open => {
            // Back from the tray, or from being minimised.
            app.window().set_minimized(false);
            app.show().ok();
            false
        }

        crate::tray::Action::SwitchInstance => {
            app.invoke_forget_instance();
            false
        }

        crate::tray::Action::ToggleCloseToTray => {
            let mut state_ref = state.borrow_mut();
            state_ref.settings.close_to_tray = !state_ref.settings.close_to_tray;
            let enabled = state_ref.settings.close_to_tray;
            let _ = state_ref.settings.store();
            drop(state_ref);
            if let Some(tray) = tray {
                tray.set_close_to_tray(enabled);
            }
            false
        }

        crate::tray::Action::Quit => {
            let _ = tasks.send(Task::Disconnect);
            let _ = slint::quit_event_loop();
            false
        }
    }
}

/// Which images the current view wants that are not loaded yet.
fn queue_images(state: &AppState, tasks: &mpsc::UnboundedSender<Task>) {
    let channel = state.store.selected_channel.clone();
    let wanted = view::referenced_images(
        state.store.messages_in(&channel),
        &state.store.members,
        &state.store.emojis,
    );
    let missing: Vec<String> = wanted
        .into_iter()
        .filter(|url| state.images.claim(url))
        .collect();
    if !missing.is_empty() {
        let _ = tasks.send(Task::FetchImages(missing));
    }
}

/// Rebuild every derived view from the store.
fn refresh(app: &ui::App, state: &Shared, tasks: &mpsc::UnboundedSender<Task>) {
    let state_ref = state.borrow();
    let store = &state_ref.store;
    let accent = state_ref.palette.accent;
    let now = time::OffsetDateTime::now_utc().unix_timestamp();

    // --- instance and me ------------------------------------------------
    app.set_instance_name(store.instance.name.clone().into());
    app.set_instance_tagline(store.instance.tagline.clone().into());
    app.set_instance_initial(
        store
            .instance
            .name
            .chars()
            .next()
            .map(|c| c.to_uppercase().to_string())
            .unwrap_or_else(|| "M".into())
            .into(),
    );
    app.set_instance_icon(
        store
            .instance
            .icon_url
            .as_deref()
            .map(|url| state_ref.images.get_or_blank(url))
            .unwrap_or_default(),
    );

    let me = &store.me.member;
    app.set_my_name(me.name().into());
    app.set_my_status(me.custom_status.clone().into());
    app.set_my_initials(crate::format::initials(me.name()).into());
    app.set_my_avatar_fallback(crate::format::avatar_colour(&me.id, accent).to_slint());
    app.set_my_avatar(
        me.avatar_url
            .as_deref()
            .map(|url| state_ref.images.get_or_blank(url))
            .unwrap_or_default(),
    );
    app.set_my_presence(format!("{:?}", me.presence).to_lowercase().into());
    app.set_can_admin(crate::perms::can_see_admin_panel(store.permissions));
    app.set_can_invite(crate::perms::can(
        store.permissions,
        crate::perms::CREATE_INVITES,
    ));

    // --- channels --------------------------------------------------------
    // Who is sitting in a voice channel, with what LiveKit last reported
    // about who is talking.
    let voice_members = |channel_id: &str| -> Vec<ui::VoiceMember> {
        store
            .voice_states
            .iter()
            .filter(|s| s.channel_id == channel_id)
            .filter_map(|voice| {
                let member = store.member(&voice.user_id)?;
                Some(ui::VoiceMember {
                    id: member.id.clone().into(),
                    name: member.name().into(),
                    avatar: member
                        .avatar_url
                        .as_deref()
                        .map(|url| state_ref.images.get_or_blank(url))
                        .unwrap_or_default(),
                    avatar_fallback: crate::format::avatar_colour(&member.id, accent).to_slint(),
                    initials: crate::format::initials(member.name()).into(),
                    muted: voice.muted,
                    deafened: voice.deafened,
                    streaming: voice.streaming,
                    video: voice.video,
                    speaking: state_ref.speaking.iter().any(|id| id == &member.id),
                })
            })
            .collect()
    };

    let to_channel = |channel: &Channel| ui::ChannelRow {
        id: channel.id.clone().into(),
        name: channel.name.clone().into(),
        kind: match channel.kind {
            ChannelKind::Voice => "voice",
            ChannelKind::Announcement => "announcement",
            ChannelKind::Text => "text",
        }
        .into(),
        description: channel.description.clone().into(),
        emoji: channel.emoji.clone().into(),
        private: channel.is_private,
        unread: store.unread.get(&channel.id).copied().unwrap_or(0) as i32,
        mentions: store.mentions.get(&channel.id).copied().unwrap_or(0) as i32,
        voice_count: store
            .voice_states
            .iter()
            .filter(|s| s.channel_id == channel.id)
            .count() as i32,
        voice_members: ModelRc::new(VecModel::from(voice_members(&channel.id))),
    };

    let loose: Vec<ui::ChannelRow> = store
        .channels
        .iter()
        .filter(|c| c.category_id.is_none())
        .map(to_channel)
        .collect();
    app.set_loose_channels(ModelRc::new(VecModel::from(loose)));

    let categories: Vec<ui::CategoryRow> = store
        .categories
        .iter()
        .map(|category| ui::CategoryRow {
            id: category.id.clone().into(),
            name: category.name.to_uppercase().into(),
            collapsed: store.collapsed_categories.contains(&category.id),
            private: category.is_private,
            channels: ModelRc::new(VecModel::from(
                store
                    .channels
                    .iter()
                    .filter(|c| c.category_id.as_deref() == Some(category.id.as_str()))
                    .map(to_channel)
                    .collect::<Vec<_>>(),
            )),
        })
        .collect();
    app.set_categories(ModelRc::new(VecModel::from(categories)));

    // --- the open channel -------------------------------------------------
    let selected = store.selected_channel.clone();
    app.set_selected_channel(selected.clone().into());
    if let Some(channel) = store.channel(&selected) {
        app.set_channel_name(channel.name.clone().into());
        app.set_channel_topic(channel.topic.clone().into());
        app.set_channel_private(channel.is_private);
        app.set_channel_kind(
            match channel.kind {
                ChannelKind::Voice => "voice",
                ChannelKind::Announcement => "announcement",
                ChannelKind::Text => "text",
            }
            .into(),
        );
        app.set_compose_notice(if channel.slowmode > 0 {
            format!(
                "Slow mode is on — one message every {}.",
                crate::format::slowmode(channel.slowmode)
            )
            .into()
        } else {
            SharedString::new()
        });
    }
    app.set_can_send(store.can_send_in(&selected));

    let visible = store.visible_messages_in(&selected);
    let rows = view::build_messages_with_roles(
        &visible,
        &store.members,
        &store.emojis,
        &store.roles,
        &store.me.member.id,
        view::MessageContext {
            width: state_ref.body_width,
            accent,
            text: state_ref.palette.text,
            measurer: &state_ref.measurer,
            first_unread: store.first_unread(&selected),
            images: &state_ref.images,
        },
    );
    app.set_messages(ModelRc::new(VecModel::from(rows)));
    app.set_loading_messages(false);

    app.set_replying_to(
        state_ref
            .replying_to
            .as_deref()
            .and_then(|id| store.messages_in(&selected).iter().find(|m| m.id == *id))
            .map(|m| SharedString::from(m.author_name()))
            .unwrap_or_default(),
    );

    // --- who is typing ----------------------------------------------------
    let typing = store.typing_names(&selected, now);
    app.set_typing_line(
        match typing.len() {
            0 => String::new(),
            1 => format!("{} is typing…", typing[0]),
            2 => format!("{} and {} are typing…", typing[0], typing[1]),
            _ => "Several people are typing…".into(),
        }
        .into(),
    );

    // --- members ----------------------------------------------------------
    let default_name_colour = state_ref.palette.text;
    let to_member = |member: &Member| -> ui::MemberRow {
        let voice = store.voice_states.iter().find(|s| s.user_id == member.id);
        let top_role = store
            .roles
            .iter()
            .filter(|r| member.roles.contains(&r.id) && r.hoist)
            .max_by_key(|r| r.position);
        ui::MemberRow {
            id: member.id.clone().into(),
            name: member.name().into(),
            status: member.custom_status.clone().into(),
            presence: format!("{:?}", member.presence).to_lowercase().into(),
            avatar: member
                .avatar_url
                .as_deref()
                .map(|url| state_ref.images.get_or_blank(url))
                .unwrap_or_default(),
            avatar_fallback: crate::format::avatar_colour(&member.id, accent).to_slint(),
            initials: crate::format::initials(member.name()).into(),
            role_colour: view::role_colour(&member.roles, &store.roles, default_name_colour)
                .to_slint(),
            badge: top_role.map(|r| r.badge.clone()).unwrap_or_default().into(),
            operator: member.is_operator,
            speaking: false,
            muted: voice.is_some_and(|v| v.muted),
            deafened: voice.is_some_and(|v| v.deafened),
        }
    };

    // Hoisted roles get their own heading, highest first; everyone else falls
    // into online and offline. Same grouping as the web client's member list.
    let mut groups: Vec<ui::MemberGroup> = Vec::new();
    // Borrowed from the store, which outlives this function; a member placed
    // under a hoisted role is not listed again under ONLINE.
    let mut placed: Vec<&str> = Vec::new();

    for role in store.roles.iter().filter(|r| r.hoist) {
        let hoisted: Vec<&Member> = store
            .members
            .iter()
            .filter(|m| m.presence != Presence::Offline)
            .filter(|m| m.roles.contains(&role.id) && !placed.contains(&m.id.as_str()))
            .collect();
        if hoisted.is_empty() {
            continue;
        }
        placed.extend(hoisted.iter().map(|m| m.id.as_str()));
        let members: Vec<ui::MemberRow> = hoisted.into_iter().map(to_member).collect();
        groups.push(ui::MemberGroup {
            name: format!("{} — {}", role.name.to_uppercase(), members.len()).into(),
            members: ModelRc::new(VecModel::from(members)),
        });
    }

    let online: Vec<ui::MemberRow> = store
        .members
        .iter()
        .filter(|m| m.presence != Presence::Offline && !placed.contains(&m.id.as_str()))
        .map(to_member)
        .collect();
    if !online.is_empty() {
        groups.push(ui::MemberGroup {
            name: format!("ONLINE — {}", online.len()).into(),
            members: ModelRc::new(VecModel::from(online)),
        });
    }

    let offline: Vec<ui::MemberRow> = store
        .members
        .iter()
        .filter(|m| m.presence == Presence::Offline)
        .map(to_member)
        .collect();
    if !offline.is_empty() {
        groups.push(ui::MemberGroup {
            name: format!("OFFLINE — {}", offline.len()).into(),
            members: ModelRc::new(VecModel::from(offline)),
        });
    }

    app.set_member_groups(ModelRc::new(VecModel::from(groups)));
    app.set_member_count(store.members.len() as i32);
    app.set_members_open(state_ref.settings.members_open);

    // --- side panel ------------------------------------------------------
    let hits: Vec<ui::SearchHit> = state_ref
        .panel_hits
        .iter()
        .map(|message| {
            let author_id = message.author_id().unwrap_or_default().to_string();
            ui::SearchHit {
                id: message.id.clone().into(),
                channel: store
                    .channel(&message.channel_id)
                    .map(|c| SharedString::from(c.name.as_str()))
                    .unwrap_or_default(),
                author: message.author_name().into(),
                excerpt: view::excerpt(message, 140).into(),
                timestamp: crate::format::relative(&message.created_at).into(),
                avatar: message
                    .author
                    .as_ref()
                    .and_then(|a| a.avatar_url.as_deref())
                    .map(|url| state_ref.images.get_or_blank(url))
                    .unwrap_or_default(),
                avatar_fallback: crate::format::avatar_colour(&author_id, accent).to_slint(),
                initials: crate::format::initials(message.author_name()).into(),
            }
        })
        .collect();
    app.set_panel_hits(ModelRc::new(VecModel::from(hits)));
    app.set_panel_loading(state_ref.panel_loading);

    // --- the open profile --------------------------------------------------
    if let Some(member) = state_ref.profile.as_ref().and_then(|id| store.member(id)) {
        app.set_profile_member(to_member(member));
        app.set_profile_pronouns(member.pronouns.clone().into());
        app.set_profile_bio(member.bio.clone().into());
        app.set_profile_game(member.favorite_game.clone().into());
        app.set_profile_joined(if member.created_at.is_empty() {
            SharedString::new()
        } else {
            crate::format::full_date(&member.created_at).into()
        });
        app.set_profile_banner(
            member
                .banner_url
                .as_deref()
                .map(|url| state_ref.images.get_or_blank(url))
                .unwrap_or_default(),
        );
        app.set_profile_is_me(member.id == store.me.member.id);
        app.set_profile_relationship(
            match store.relationship(&member.id) {
                RelationshipKind::Friend => "friend",
                RelationshipKind::Outgoing => "outgoing",
                RelationshipKind::Incoming => "incoming",
                RelationshipKind::Blocked => "blocked",
                RelationshipKind::None => "none",
            }
            .into(),
        );
        app.set_profile_favourite(store.is_favourite(&member.id));
    }

    // --- the emoji picker ---------------------------------------------------
    let filter = app.get_emoji_filter().to_string();
    let unicode: Vec<ui::EmojiEntry> = crate::emoji::search(&filter)
        .into_iter()
        .map(|(glyph, name)| ui::EmojiEntry {
            name: name.into(),
            url: SharedString::new(),
            picture: slint::Image::default(),
            glyph: glyph.into(),
        })
        .collect();
    let needle = filter.trim().to_lowercase();
    let custom: Vec<ui::EmojiEntry> = store
        .emojis
        .iter()
        .filter(|e| needle.is_empty() || e.name.contains(&needle))
        .map(|e| ui::EmojiEntry {
            name: e.name.clone().into(),
            url: e.url.clone().into(),
            picture: state_ref.images.get_or_blank(&e.url),
            glyph: SharedString::new(),
        })
        .collect();
    app.set_emoji_unicode(ModelRc::new(VecModel::from(emoji_rows(unicode))));
    app.set_emoji_custom(ModelRc::new(VecModel::from(emoji_rows(custom))));

    // --- settings ------------------------------------------------------------
    app.set_my_username(store.me.member.username.clone().into());
    app.set_instance_version(store.instance.version.clone().into());
    app.set_hotkeys_available(crate::hotkeys::available());
    app.set_hotkeys_enabled(state_ref.settings.hotkeys.enabled);
    app.set_hotkey_summary(
        format!(
            "Push to talk {} \u{b7} Mute {} \u{b7} Deafen {}",
            state_ref.settings.hotkeys.push_to_talk,
            state_ref.settings.hotkeys.mute,
            state_ref.settings.hotkeys.deafen,
        )
        .into(),
    );
    app.set_theme_choice(match state_ref.settings.theme {
        ThemeChoice::Dark => 0,
        ThemeChoice::Light => 1,
        ThemeChoice::Instance => 2,
    });

    // --- direct messages ---------------------------------------------------
    app.set_inbox_open(store.inbox_open);
    // Friend requests are counted with unread conversations: both are
    // things waiting on you behind that one button, and a request is
    // otherwise invisible until you happen to open the inbox.
    app.set_direct_unread((store.direct_unread() + store.incoming_requests() as i64) as i32);
    app.set_total_unread((store.total_unread() + store.direct_unread()) as i32);
    app.set_voice_enabled(store.voice_enabled);
    app.set_selected_conversation(store.selected_conversation.clone().into());

    let conversations: Vec<ui::ConversationRow> = store
        .conversations
        .iter()
        .map(|conversation| {
            let peer = store.peer(&conversation.id);
            let name = peer.map(|m| m.name().to_string()).unwrap_or_else(|| {
                // A conversation whose peer is not in the member list any
                // more still has to render as something.
                "Former member".to_string()
            });
            let newest = store.direct_in(&conversation.id).last();
            ui::ConversationRow {
                id: conversation.id.clone().into(),
                name: name.clone().into(),
                excerpt: conversation
                    .last_content
                    .as_deref()
                    .map(|text| crate::format::elide(text.trim(), 60))
                    .unwrap_or_default()
                    .into(),
                timestamp: newest
                    .map(|m| SharedString::from(crate::format::relative(&m.created_at)))
                    .unwrap_or_default(),
                avatar: peer
                    .and_then(|m| m.avatar_url.as_deref())
                    .map(|url| state_ref.images.get_or_blank(url))
                    .unwrap_or_default(),
                avatar_fallback: crate::format::avatar_colour(&conversation.peer_id, accent)
                    .to_slint(),
                initials: crate::format::initials(&name).into(),
                presence: peer
                    .map(|m| SharedString::from(format!("{:?}", m.presence).to_lowercase()))
                    .unwrap_or_default(),
                unread: conversation.unread as i32,
            }
        })
        .collect();
    app.set_conversations(ModelRc::new(VecModel::from(conversations)));
    app.set_inbox_tab(state_ref.inbox_tab);

    let people = |kind: RelationshipKind| -> Vec<ui::MemberRow> {
        store.related(kind).into_iter().map(to_member).collect()
    };
    app.set_friend_requests(ModelRc::new(VecModel::from(people(
        RelationshipKind::Incoming,
    ))));
    app.set_friends(ModelRc::new(VecModel::from(people(
        RelationshipKind::Friend,
    ))));
    app.set_blocked_members(ModelRc::new(VecModel::from(people(
        RelationshipKind::Blocked,
    ))));

    if let Some(peer) = store.peer(&store.selected_conversation) {
        app.set_peer_name(peer.name().into());
        app.set_peer_presence(format!("{:?}", peer.presence).to_lowercase().into());
        app.set_peer_initials(crate::format::initials(peer.name()).into());
        app.set_peer_avatar_fallback(crate::format::avatar_colour(&peer.id, accent).to_slint());
        app.set_peer_avatar(
            peer.avatar_url
                .as_deref()
                .map(|url| state_ref.images.get_or_blank(url))
                .unwrap_or_default(),
        );
    }

    // A direct message renders through the same message list as a channel
    // one, so it is converted rather than given a second renderer.
    let direct: Vec<Message> = store
        .direct_in(&store.selected_conversation)
        .iter()
        .map(|message| message.as_message(store.member(&message.author_id)))
        .collect();
    app.set_direct_messages(ModelRc::new(VecModel::from(
        view::build_messages_with_roles(
            &direct,
            &store.members,
            &store.emojis,
            &store.roles,
            &store.me.member.id,
            view::MessageContext {
                width: state_ref.body_width,
                accent,
                text: state_ref.palette.text,
                measurer: &state_ref.measurer,
                first_unread: None,
                images: &state_ref.images,
            },
        ),
    )));

    // --- direct calls -------------------------------------------------------
    //
    // One card whether the call is ringing, being placed or connected; only
    // the phase differs, so answering one does not move anything.
    let call = store.incoming_call().or_else(|| store.my_call());
    match call {
        Some(call) => {
            let phase = if call.status == CallStatus::Accepted {
                "connected"
            } else if call.caller_id == store.me.member.id {
                "calling"
            } else {
                "ringing"
            };
            app.set_call_phase(phase.into());
            app.set_call_peer(store.call_peer(call).map(to_member).unwrap_or_default());
            app.set_call_duration(
                state_ref
                    .call_started
                    .map(|started| crate::format::duration((now - started).max(0)))
                    .unwrap_or_default()
                    .into(),
            );
            app.set_call_notice(state_ref.call_notice.clone().into());
        }
        None => {
            app.set_call_phase(SharedString::new());
            app.set_call_duration(SharedString::new());
        }
    }

    // --- voice ------------------------------------------------------------
    let voice = &state_ref.voice;
    app.set_voice_live(voice.is_live() || voice.status == crate::voice::Status::Connecting);
    app.set_voice_channel(
        store
            .channel(&voice.channel)
            .map(|c| SharedString::from(c.name.as_str()))
            .unwrap_or_default(),
    );
    app.set_voice_summary(voice.summary().into());
    app.set_voice_muted(voice.muted);
    app.set_voice_deafened(voice.deafened);
    app.set_voice_can_speak(voice.can_speak);
    app.set_voice_can_screen_share(voice.can_screen_share);
    app.set_voice_screen_sharing(voice.screen_sharing);
    app.set_voice_notice(voice.notice.clone().into());

    // --- pending attachments ----------------------------------------------
    let pending: Vec<ui::PendingAttachment> = state_ref
        .pending_attachments
        .iter()
        .map(|file| ui::PendingAttachment {
            id: file.id.clone().into(),
            filename: file.filename.clone().into(),
            size: crate::format::bytes(file.size).into(),
            kind: if file.is_image() {
                "image"
            } else if file.is_video() {
                "video"
            } else if file.is_audio() {
                "audio"
            } else {
                "file"
            }
            .into(),
            picture: state_ref.images.get_or_blank(&file.url),
        })
        .collect();
    app.set_pending_attachments(ModelRc::new(VecModel::from(pending)));

    // The composer's notice doubles as where an upload failure is said.
    if !state_ref.attach_notice.is_empty() {
        app.set_compose_notice(state_ref.attach_notice.clone().into());
    }

    // --- lightbox and forwarding -------------------------------------------
    if let Some(file) = state_ref.lightbox.as_ref() {
        app.set_lightbox_picture(state_ref.images.get_full_or_preview(&file.url));
        app.set_lightbox_filename(file.filename.clone().into());
        app.set_lightbox_caption(
            match (file.width, file.height) {
                (Some(w), Some(h)) => {
                    format!("{w} × {h} · {}", crate::format::bytes(file.size))
                }
                _ => crate::format::bytes(file.size),
            }
            .into(),
        );
    }

    if let Some(message) = state_ref.forwarding.as_ref() {
        app.set_forward_author(message.author_name().into());
        app.set_forward_body(view::excerpt(message, 160).into());
        app.set_forward_target(state_ref.forward_target.clone().into());
        app.set_forwarding(state_ref.forward_busy);
        // Only channels this member may actually post in, so the dialog
        // cannot offer a destination the server would refuse.
        let targets: Vec<ui::ChannelRow> = store
            .channels
            .iter()
            .filter(|c| c.kind != ChannelKind::Voice)
            .filter(|c| store.can_send_in(&c.id))
            .map(to_channel)
            .collect();
        app.set_forward_targets(ModelRc::new(VecModel::from(targets)));
    }

    app.set_editing_id(
        state_ref
            .editing
            .as_deref()
            .map(SharedString::from)
            .unwrap_or_default(),
    );

    app.set_scroll_to_newest(state_ref.scroll_token);
    app.set_focus_composer(state_ref.focus_token);
    app.set_scroll_distance(state_ref.scroll_distance);
    app.set_can_reorder(crate::perms::can(
        store.permissions,
        crate::perms::MANAGE_CHANNELS,
    ));
    app.set_dragging_channel(
        state_ref
            .dragging
            .as_deref()
            .map(SharedString::from)
            .unwrap_or_default(),
    );
    app.set_drag_offset(state_ref.drag_offset);

    refresh_admin(app, &state_ref, accent);

    queue_images(&state_ref, tasks);
}

/// Everything the admin panel shows.
///
/// Split out because it is a screen of its own and rebuilding it with every
/// message that arrives would be pure waste — it only runs while the panel
/// is open.
fn refresh_admin(app: &ui::App, state: &AppState, accent: Rgb) {
    if app.get_screen() != ui::Screen::Admin {
        return;
    }
    let store = &state.store;
    let bits = store.permissions;
    let me = &store.me.member;

    app.set_admin_section(state.admin_section.clone().into());
    app.set_admin_saving(state.admin_saving);
    app.set_admin_notice(state.admin_notice.clone().into());

    app.set_admin_sections(ModelRc::new(VecModel::from(
        crate::admin::sections_for(bits)
            .into_iter()
            .map(|section| ui::AdminSection {
                id: section.id.into(),
                label: section.label.into(),
            })
            .collect::<Vec<_>>(),
    )));

    // --- overview ---------------------------------------------------------
    let tile = |label: &str, value: String, detail: String| ui::StatTile {
        label: label.into(),
        value: value.into(),
        detail: detail.into(),
    };
    let tiles = match state.stats.as_ref() {
        Some(stats) => vec![
            tile(
                "MEMBERS",
                stats.members.to_string(),
                format!("{} online", stats.online),
            ),
            tile(
                "MESSAGES",
                stats.messages.to_string(),
                format!("{} this week", stats.messages_last_week),
            ),
            tile("CHANNELS", stats.channels.to_string(), String::new()),
            tile(
                "STORAGE",
                crate::format::bytes(stats.storage_bytes),
                String::new(),
            ),
            tile(
                "JOINED",
                stats.joined_last_week.to_string(),
                "in the last week".into(),
            ),
            tile(
                "IN VOICE",
                stats.in_voice.to_string(),
                if stats.voice_enabled {
                    String::new()
                } else {
                    "voice is off".into()
                },
            ),
        ],
        None => Vec::new(),
    };
    // Four across fits the narrowest pane the panel is usable at.
    const TILES_PER_ROW: usize = 4;
    app.set_admin_tiles(ModelRc::new(VecModel::from(
        tiles
            .chunks(TILES_PER_ROW)
            .map(|chunk| ui::StatRow {
                tiles: ModelRc::new(VecModel::from(chunk.to_vec())),
            })
            .collect::<Vec<_>>(),
    )));

    // --- members -----------------------------------------------------------
    let members: Vec<ui::AdminMemberRow> = store
        .members
        .iter()
        .map(|member| ui::AdminMemberRow {
            id: member.id.clone().into(),
            name: member.name().into(),
            username: member.username.clone().into(),
            avatar: member
                .avatar_url
                .as_deref()
                .map(|url| state.images.get_or_blank(url))
                .unwrap_or_default(),
            avatar_fallback: crate::format::avatar_colour(&member.id, accent).to_slint(),
            initials: crate::format::initials(member.name()).into(),
            presence: format!("{:?}", member.presence).to_lowercase().into(),
            roles: crate::admin::role_summary(member, &store.roles).into(),
            joined: crate::format::short_date(&member.created_at).into(),
            operator: member.is_operator,
            can_kick: crate::admin::can_kick(me, member, &store.roles, bits),
            can_ban: crate::admin::can_ban(me, member, &store.roles, bits),
        })
        .collect();
    app.set_admin_members(ModelRc::new(VecModel::from(members)));

    // --- roles, for whichever member is picked ------------------------------
    let picked = state
        .admin_picked
        .as_deref()
        .and_then(|id| store.member(id));
    app.set_admin_has_picked(picked.is_some());
    if let Some(member) = picked {
        app.set_admin_picked(ui::AdminMemberRow {
            id: member.id.clone().into(),
            name: member.name().into(),
            ..Default::default()
        });
    }

    let roles: Vec<ui::RoleRow> = store
        .roles
        .iter()
        .filter(|role| !role.is_default)
        .map(|role| {
            let colour = role.color.as_deref().and_then(Rgb::parse);
            ui::RoleRow {
                id: role.id.clone().into(),
                name: role.name.clone().into(),
                colour: colour.unwrap_or(state.palette.text).to_slint(),
                coloured: colour.is_some(),
                badge: role.badge.clone().into(),
                members: store
                    .members
                    .iter()
                    .filter(|m| m.roles.contains(&role.id))
                    .count() as i32,
                assignable: crate::admin::can_assign(me, role, &store.roles, bits),
                granted: picked.is_some_and(|m| m.roles.contains(&role.id)),
            }
        })
        .collect();
    app.set_admin_roles(ModelRc::new(VecModel::from(roles)));

    // --- invites, bans, audit -------------------------------------------------
    app.set_admin_invites(ModelRc::new(VecModel::from(
        state
            .invites
            .iter()
            .map(|invite| ui::InviteRow {
                code: invite.code.clone().into(),
                url: invite.url.clone().into(),
                note: invite.note.clone().into(),
                uses: crate::admin::invite_uses(invite.uses, invite.max_uses).into(),
                expires: invite
                    .expires_at
                    .as_deref()
                    .map(|at| {
                        SharedString::from(format!("expires {}", crate::format::relative(at)))
                    })
                    .unwrap_or_else(|| "never expires".into()),
                revoked: invite.revoked,
            })
            .collect::<Vec<_>>(),
    )));

    app.set_admin_bans(ModelRc::new(VecModel::from(
        state
            .bans
            .iter()
            .map(|ban| ui::BanRow {
                user_id: ban.user_id.clone().into(),
                name: if ban.display_name.is_empty() {
                    ban.username.clone()
                } else {
                    ban.display_name.clone()
                }
                .into(),
                reason: ban.reason.clone().into(),
                when: crate::format::relative(&ban.created_at).into(),
            })
            .collect::<Vec<_>>(),
    )));

    app.set_admin_audit(ModelRc::new(VecModel::from(
        state
            .audit
            .iter()
            .map(|entry| ui::AuditRow {
                id: entry.id.clone().into(),
                actor: entry
                    .actor_name
                    .clone()
                    .unwrap_or_else(|| "The system".into())
                    .into(),
                action: entry.action.replace('_', " ").into(),
                detail: entry.detail.clone().into(),
                when: crate::format::relative(&entry.created_at).into(),
            })
            .collect::<Vec<_>>(),
    )));
}

fn wire_callbacks(
    app: &ui::App,
    state: &Shared,
    tasks: &mpsc::UnboundedSender<Task>,
    hotkeys: &Rc<RefCell<Option<crate::hotkeys::Hotkeys>>>,
) {
    macro_rules! handler {
        ($app:expr, $state:expr, $tasks:expr, |$a:ident, $s:ident, $t:ident| $body:block) => {{
            let weak: Weak<ui::App> = $app.as_weak();
            let $s = $state.clone();
            let $t = $tasks.clone();
            move || {
                let Some($a) = weak.upgrade() else { return };
                $body
            }
        }};
    }

    // --- connect ---------------------------------------------------------
    app.on_connect({
        let weak = app.as_weak();
        let state = state.clone();
        let tasks = tasks.clone();
        move |address| {
            let Some(app) = weak.upgrade() else { return };
            match api::normalise_origin(&address) {
                Ok(origin) => {
                    let mut state = state.borrow_mut();
                    state.settings.instance_url = Some(origin.clone());
                    let _ = state.settings.store();
                    drop(state);

                    app.set_connect_error(SharedString::new());
                    app.set_connecting(true);
                    app.set_screen(ui::Screen::Signin);
                    let _ = tasks.send(Task::Meta(origin));
                    app.set_connecting(false);
                }
                Err(error) => app.set_connect_error(error.to_string().into()),
            }
        }
    });

    app.on_sign_in({
        let weak = app.as_weak();
        let state = state.clone();
        let tasks = tasks.clone();
        move || {
            let Some(app) = weak.upgrade() else { return };
            let origin = state
                .borrow()
                .settings
                .instance_url
                .clone()
                .unwrap_or_default();
            if origin.is_empty() {
                app.set_screen(ui::Screen::Connect);
                return;
            }
            app.set_authenticating(true);
            app.set_auth_error(SharedString::new());
            let _ = tasks.send(Task::SignIn {
                origin,
                username: app.get_username().to_string(),
                password: app.get_password().to_string(),
            });
        }
    });

    app.on_forget_instance(handler!(app, state, tasks, |app, state, tasks| {
        let mut state_ref = state.borrow_mut();
        state_ref.settings.forget_instance();
        let _ = state_ref.settings.store();
        drop(state_ref);
        let _ = tasks.send(Task::Disconnect);
        app.set_screen(ui::Screen::Connect);
        app.set_password(SharedString::new());
        app.set_auth_error(SharedString::new());
    }));

    // --- navigation -------------------------------------------------------
    app.on_select_channel({
        let weak = app.as_weak();
        let state = state.clone();
        let tasks = tasks.clone();
        move |id| {
            let Some(app) = weak.upgrade() else { return };
            let id = id.to_string();
            let mut state_ref = state.borrow_mut();

            // A voice channel is joined, not opened: it has no message list,
            // and clicking it while already in it does nothing rather than
            // reconnecting.
            if state_ref.store.channel(&id).map(|c| c.kind) == Some(ChannelKind::Voice) {
                if state_ref.voice.channel == id && state_ref.voice.is_live() {
                    return;
                }
                state_ref.engine.disconnect();
                state_ref.voice.left();
                state_ref.voice.status = crate::voice::Status::Connecting;
                state_ref.voice.channel = id.clone();
                drop(state_ref);
                let _ = tasks.send(Task::JoinVoice { channel: id });
                refresh(&app, &state, &tasks);
                return;
            }

            if state_ref.store.selected_channel == id {
                return;
            }
            state_ref.store.selected_channel = id.clone();
            state_ref.store.touch(&id);
            state_ref.replying_to = None;
            state_ref.settings.last_channel = Some(id.clone());
            let _ = state_ref.settings.store();

            // A channel whose history is already held is shown at once; only
            // a cold one waits on the network.
            let cached = !state_ref.store.messages_in(&id).is_empty();
            if let Some(last) = state_ref
                .store
                .messages_in(&id)
                .last()
                .map(|m| m.id.clone())
            {
                state_ref.store.mark_read(&id);
                let _ = tasks.send(Task::Ack {
                    channel: id.clone(),
                    message: last,
                });
            }
            drop(state_ref);

            state.borrow_mut().focus_token += 1;
            if cached {
                let mut state_ref = state.borrow_mut();
                state_ref.scroll_token += 1;
                state_ref.scroll_distance = 0.0;
            } else {
                app.set_loading_messages(true);
                let _ = tasks.send(Task::LoadMessages { channel: id });
            }
            refresh(&app, &state, &tasks);
        }
    });

    app.on_toggle_category({
        let weak = app.as_weak();
        let state = state.clone();
        let tasks = tasks.clone();
        move |id| {
            let Some(app) = weak.upgrade() else { return };
            let mut state_ref = state.borrow_mut();
            state_ref.store.toggle_category(&id);
            state_ref.settings.collapsed_categories = state_ref.store.collapsed_categories.clone();
            let _ = state_ref.settings.store();
            drop(state_ref);
            refresh(&app, &state, &tasks);
        }
    });

    app.on_focus_changed({
        let state = state.clone();
        move |focused| {
            state.borrow_mut().window_focused = focused;
        }
    });

    // --- reordering ------------------------------------------------------
    //
    // A drag only becomes a drag past a few pixels, so an ordinary click on
    // a channel still selects it rather than nudging it one row.
    const DRAG_THRESHOLD: f32 = 5.0;
    /// The height of a channel row, which is what a drag distance is
    /// measured in. Matches `ChannelButton` in ui/sidebar.slint.
    const ROW_HEIGHT: f32 = 38.0;

    app.on_drag_start({
        let state = state.clone();
        move |id| {
            // Nothing moves yet: the press might be a click. The drag begins
            // on the first move past the threshold.
            let mut state_ref = state.borrow_mut();
            state_ref.pressed_channel = Some(id.to_string());
            state_ref.drag_offset = 0.0;
        }
    });

    app.on_drag_move({
        let weak = app.as_weak();
        let state = state.clone();
        let tasks = tasks.clone();
        move |dy| {
            let Some(app) = weak.upgrade() else { return };
            let mut state_ref = state.borrow_mut();
            if !crate::perms::can(state_ref.store.permissions, crate::perms::MANAGE_CHANNELS) {
                return;
            }
            if dy.abs() < DRAG_THRESHOLD && state_ref.dragging.is_none() {
                return;
            }
            if state_ref.dragging.is_none() {
                // The row being dragged is the one under the pointer, which
                // is the selected one only by coincidence — so it is taken
                // from the press rather than from the selection.
                state_ref.dragging = state_ref.pressed_channel.clone();
            }
            state_ref.drag_offset = dy;
            drop(state_ref);
            refresh(&app, &state, &tasks);
        }
    });

    app.on_drag_end({
        let weak = app.as_weak();
        let state = state.clone();
        let tasks = tasks.clone();
        move || {
            let Some(app) = weak.upgrade() else { return };
            let mut state_ref = state.borrow_mut();
            let Some(id) = state_ref.dragging.take() else {
                state_ref.drag_offset = 0.0;
                return;
            };
            let offset = crate::reorder::rows_moved(state_ref.drag_offset, ROW_HEIGHT);
            state_ref.drag_offset = 0.0;

            let order = crate::reorder::move_channel(
                &state_ref.store.channels,
                &state_ref.store.categories,
                &id,
                offset,
            );
            drop(state_ref);

            if let Some(order) = order {
                let _ = tasks.send(Task::ReorderChannels(order));
            }
            refresh(&app, &state, &tasks);
        }
    });

    app.on_category_menu({
        let weak = app.as_weak();
        let state = state.clone();
        move |id| {
            let Some(app) = weak.upgrade() else { return };
            let mut state_ref = state.borrow_mut();
            if !crate::perms::can(state_ref.store.permissions, crate::perms::MANAGE_CHANNELS) {
                return;
            }
            state_ref.menu_target = Some(MenuTarget::Category(id.to_string()));
            drop(state_ref);

            app.set_menu_items(ModelRc::new(VecModel::from(vec![
                menu_item("category-up", "Move up", "", false, false),
                menu_item("category-down", "Move down", "", false, false),
            ])));
            app.set_overlay(ui::Overlay::Menu);
        }
    });

    app.on_step_channel({
        let weak = app.as_weak();
        let state = state.clone();
        move |direction| {
            let Some(app) = weak.upgrade() else { return };
            let state_ref = state.borrow();
            // Only the channels you can actually open, in the order the
            // sidebar shows them — so Alt+Down does not land on a voice
            // channel and silently try to join it.
            let openable: Vec<String> = state_ref
                .store
                .channels
                .iter()
                .filter(|c| c.kind != ChannelKind::Voice)
                .map(|c| c.id.clone())
                .collect();
            if openable.is_empty() {
                return;
            }
            let current = openable
                .iter()
                .position(|id| id == &state_ref.store.selected_channel)
                .unwrap_or(0) as i32;
            // Wraps, so the list has no dead ends at either end.
            let count = openable.len() as i32;
            let next = (current + direction).rem_euclid(count) as usize;
            let target = openable[next].clone();
            drop(state_ref);

            app.invoke_select_channel(target.into());
        }
    });

    app.on_toggle_members({
        let weak = app.as_weak();
        let state = state.clone();
        move || {
            let Some(app) = weak.upgrade() else { return };
            let mut state_ref = state.borrow_mut();
            state_ref.settings.members_open = !state_ref.settings.members_open;
            let open = state_ref.settings.members_open;
            let _ = state_ref.settings.store();
            drop(state_ref);
            app.set_members_open(open);
        }
    });

    // --- sending -----------------------------------------------------------
    app.on_send({
        let weak = app.as_weak();
        let state = state.clone();
        let tasks = tasks.clone();
        move |text| {
            let Some(app) = weak.upgrade() else { return };
            let content = text.trim().to_string();

            let mut state_ref = state.borrow_mut();
            let channel = state_ref.store.selected_channel.clone();
            if channel.is_empty() || !state_ref.store.can_send_in(&channel) {
                return;
            }
            let attachments = std::mem::take(&mut state_ref.pending_attachments);
            // A message with nothing in it and nothing attached is not a
            // message; one with only attachments is.
            if content.is_empty() && attachments.is_empty() {
                return;
            }
            let temp_id = state_ref.temp_id();
            let reply_to = state_ref.replying_to.take();
            let me = state_ref.store.me.member.clone();

            // Shown immediately, dimmed, and replaced by the stored message
            // when the server answers.
            state_ref.store.upsert_message(Message {
                id: temp_id.clone(),
                channel_id: channel.clone(),
                author: Some(MessageAuthor {
                    id: me.id.clone(),
                    username: me.username.clone(),
                    display_name: me.display_name.clone(),
                    avatar_url: me.avatar_url.clone(),
                    ..Default::default()
                }),
                content: content.clone(),
                reply_to_id: reply_to.clone(),
                attachments: attachments.clone(),
                created_at: time::OffsetDateTime::now_utc()
                    .format(&time::format_description::well_known::Rfc3339)
                    .unwrap_or_default(),
                pending: true,
                ..Default::default()
            });
            drop(state_ref);

            let mut state_ref = state.borrow_mut();
            state_ref.scroll_token += 1;
            state_ref.scroll_distance = 0.0;
            drop(state_ref);

            app.set_draft(SharedString::new());
            let _ = tasks.send(Task::Send {
                channel,
                temp_id,
                content,
                reply_to,
                attachments,
            });
            refresh(&app, &state, &tasks);
        }
    });

    app.on_reply_to({
        let weak = app.as_weak();
        let state = state.clone();
        let tasks = tasks.clone();
        move |id| {
            let Some(app) = weak.upgrade() else { return };
            state.borrow_mut().replying_to = Some(id.to_string());
            refresh(&app, &state, &tasks);
        }
    });

    app.on_cancel_reply({
        let weak = app.as_weak();
        let state = state.clone();
        let tasks = tasks.clone();
        move || {
            let Some(app) = weak.upgrade() else { return };
            state.borrow_mut().replying_to = None;
            refresh(&app, &state, &tasks);
        }
    });

    app.on_react({
        let weak = app.as_weak();
        let state = state.clone();
        let tasks = tasks.clone();
        move |message, emoji| {
            // An empty emoji means the message's react button was pressed:
            // open the picker rather than toggling anything.
            if emoji.is_empty() {
                let Some(app) = weak.upgrade() else { return };
                let mut state_ref = state.borrow_mut();
                state_ref.reacting_to = Some(message.to_string());
                drop(state_ref);
                let size = app.window().size().to_logical(app.window().scale_factor());
                app.set_pointer_x(size.width / 2.0);
                app.set_pointer_y(size.height / 2.0 + 180.0);
                app.set_emoji_filter(SharedString::new());
                refresh(&app, &state, &tasks);
                app.set_overlay(ui::Overlay::Emoji);
                return;
            }
            let state_ref = state.borrow();
            let channel = state_ref.store.selected_channel.clone();
            let mine = state_ref
                .store
                .messages_in(&channel)
                .iter()
                .find(|m| m.id == message.as_str())
                .and_then(|m| m.reactions.iter().find(|r| r.emoji == emoji.as_str()))
                .is_some_and(|r| r.me);
            drop(state_ref);

            let _ = tasks.send(Task::React {
                message: message.to_string(),
                emoji: emoji.to_string(),
                on: !mine,
            });
        }
    });

    app.on_delete_message({
        let tasks = tasks.clone();
        move |id| {
            let _ = tasks.send(Task::DeleteMessage(id.to_string()));
        }
    });

    app.on_edit_message({
        let weak = app.as_weak();
        let state = state.clone();
        let tasks = tasks.clone();
        move |id| {
            let Some(app) = weak.upgrade() else { return };
            let mut state_ref = state.borrow_mut();
            let channel = state_ref.store.selected_channel.clone();
            let Some(message) = state_ref
                .store
                .messages_in(&channel)
                .iter()
                .find(|m| m.id == id.as_str())
                .cloned()
            else {
                return;
            };
            // Only your own, and never one that has not been stored yet.
            if message.author_id() != Some(state_ref.store.me.member.id.as_str()) || message.pending
            {
                return;
            }
            state_ref.editing = Some(id.to_string());
            drop(state_ref);

            // The editor opens with the message as written, not as rendered:
            // you edit the markdown, not the result of it.
            app.set_edit_draft(message.content.clone().into());
            refresh(&app, &state, &tasks);
        }
    });

    app.on_cancel_edit({
        let weak = app.as_weak();
        let state = state.clone();
        let tasks = tasks.clone();
        move || {
            let Some(app) = weak.upgrade() else { return };
            state.borrow_mut().editing = None;
            app.set_edit_draft(SharedString::new());
            refresh(&app, &state, &tasks);
        }
    });

    app.on_save_edit({
        let weak = app.as_weak();
        let state = state.clone();
        let tasks = tasks.clone();
        move |text| {
            let Some(app) = weak.upgrade() else { return };
            let content = text.trim().to_string();
            let mut state_ref = state.borrow_mut();
            let Some(id) = state_ref.editing.take() else {
                return;
            };
            let channel = state_ref.store.selected_channel.clone();
            let unchanged = state_ref
                .store
                .messages_in(&channel)
                .iter()
                .find(|m| m.id == id)
                .is_some_and(|m| m.content == content);
            drop(state_ref);

            app.set_edit_draft(SharedString::new());
            // An empty edit is a delete in the web client; here it is simply
            // refused, because the delete path has a confirmation and losing
            // a message to a stray backspace should not be possible.
            if !content.is_empty() && !unchanged {
                let _ = tasks.send(Task::EditMessage {
                    message: id,
                    content,
                });
            }
            refresh(&app, &state, &tasks);
        }
    });

    app.on_pin_message({
        let state = state.clone();
        let tasks = tasks.clone();
        move |id| {
            let state_ref = state.borrow();
            let channel = state_ref.store.selected_channel.clone();
            let pinned = state_ref
                .store
                .messages_in(&channel)
                .iter()
                .find(|m| m.id == id.as_str())
                .is_some_and(|m| m.pinned);
            drop(state_ref);
            let _ = tasks.send(Task::PinMessage {
                message: id.to_string(),
                pinned: !pinned,
            });
        }
    });

    // --- direct messages -------------------------------------------------------
    app.on_open_inbox({
        let weak = app.as_weak();
        let state = state.clone();
        let tasks = tasks.clone();
        move || {
            let Some(app) = weak.upgrade() else { return };
            state.borrow_mut().store.inbox_open = true;
            let _ = tasks.send(Task::LoadConversations);
            refresh(&app, &state, &tasks);
        }
    });

    // --- administration ----------------------------------------------------
    app.on_open_admin({
        let weak = app.as_weak();
        let state = state.clone();
        let tasks = tasks.clone();
        move || {
            let Some(app) = weak.upgrade() else { return };
            let mut state_ref = state.borrow_mut();
            if !crate::perms::can_see_admin_panel(state_ref.store.permissions) {
                return;
            }
            state_ref.admin_notice.clear();
            let section = state_ref.admin_section.clone();
            drop(state_ref);

            app.set_screen(ui::Screen::Admin);
            // Leak the section name for the task, which outlives this call.
            for known in crate::admin::SECTIONS {
                if known.id == section {
                    let _ = tasks.send(Task::LoadAdmin(known.id));
                }
            }
            refresh(&app, &state, &tasks);
        }
    });

    app.on_close_admin({
        let weak = app.as_weak();
        let state = state.clone();
        let tasks = tasks.clone();
        move || {
            let Some(app) = weak.upgrade() else { return };
            app.set_screen(ui::Screen::Chat);
            refresh(&app, &state, &tasks);
        }
    });

    app.on_choose_admin_section({
        let weak = app.as_weak();
        let state = state.clone();
        let tasks = tasks.clone();
        move |id| {
            let Some(app) = weak.upgrade() else { return };
            let mut state_ref = state.borrow_mut();
            state_ref.admin_section = id.to_string();
            state_ref.admin_notice.clear();
            drop(state_ref);

            for known in crate::admin::SECTIONS {
                if known.id == id.as_str() {
                    let _ = tasks.send(Task::LoadAdmin(known.id));
                }
            }
            refresh(&app, &state, &tasks);
        }
    });

    app.on_pick_admin_member({
        let weak = app.as_weak();
        let state = state.clone();
        let tasks = tasks.clone();
        move |id| {
            let Some(app) = weak.upgrade() else { return };
            let mut state_ref = state.borrow_mut();
            state_ref.admin_picked = Some(id.to_string());
            // Picking someone moves to the roles section, which is where
            // the thing you picked them for actually is.
            state_ref.admin_section = "roles".into();
            drop(state_ref);
            refresh(&app, &state, &tasks);
        }
    });

    app.on_toggle_role({
        let weak = app.as_weak();
        let state = state.clone();
        let tasks = tasks.clone();
        move |user, role, granted| {
            let Some(app) = weak.upgrade() else { return };
            // Applied locally so the button answers at once; the gateway's
            // MEMBER_UPDATE is what the client ends up holding.
            {
                let mut state_ref = state.borrow_mut();
                if let Some(member) = state_ref
                    .store
                    .members
                    .iter_mut()
                    .find(|m| m.id == user.as_str())
                {
                    member.roles.retain(|r| r != role.as_str());
                    if granted {
                        member.roles.push(role.to_string());
                    }
                }
            }
            let _ = tasks.send(Task::AdminAction(AdminAction::SetRole {
                user: user.to_string(),
                role: role.to_string(),
                granted,
            }));
            refresh(&app, &state, &tasks);
        }
    });

    app.on_kick_member({
        let weak = app.as_weak();
        let state = state.clone();
        move |id| {
            let Some(app) = weak.upgrade() else { return };
            let name = state
                .borrow()
                .store
                .member(&id)
                .map(|m| m.name().to_string())
                .unwrap_or_default();
            state.borrow_mut().pending_confirm = Some(Confirm::Kick(id.to_string()));
            app.set_confirm_title("Kick member?".into());
            app.set_confirm_body(
                format!("{name} will be removed but can rejoin with an invite.").into(),
            );
            app.set_confirm_label("Kick".into());
            app.set_overlay(ui::Overlay::Confirm);
        }
    });

    app.on_ban_member({
        let weak = app.as_weak();
        let state = state.clone();
        move |id| {
            let Some(app) = weak.upgrade() else { return };
            let name = state
                .borrow()
                .store
                .member(&id)
                .map(|m| m.name().to_string())
                .unwrap_or_default();
            state.borrow_mut().pending_confirm = Some(Confirm::Ban(id.to_string()));
            app.set_confirm_title("Ban member?".into());
            app.set_confirm_body(
                format!("{name} will be removed and cannot rejoin until unbanned.").into(),
            );
            app.set_confirm_label("Ban".into());
            app.set_overlay(ui::Overlay::Confirm);
        }
    });

    app.on_unban_member({
        let tasks = tasks.clone();
        move |id| {
            let _ = tasks.send(Task::AdminAction(AdminAction::Unban(id.to_string())));
        }
    });

    app.on_create_invite({
        let tasks = tasks.clone();
        move || {
            let _ = tasks.send(Task::AdminAction(AdminAction::CreateInvite));
        }
    });

    app.on_revoke_invite({
        let tasks = tasks.clone();
        move |code| {
            let _ = tasks.send(Task::AdminAction(AdminAction::RevokeInvite(
                code.to_string(),
            )));
        }
    });

    app.on_copy_invite({
        move |url| {
            copy_to_clipboard(&url);
        }
    });

    app.on_save_instance({
        let weak = app.as_weak();
        let state = state.clone();
        let tasks = tasks.clone();
        move || {
            let Some(app) = weak.upgrade() else { return };
            let accent = app.get_form_accent().trim().to_string();
            // A malformed accent would retint the whole app to nothing, so
            // it is refused here rather than sent and applied.
            if !accent.is_empty() && Rgb::parse(&accent).is_none() {
                state.borrow_mut().admin_notice = "That accent is not a #rrggbb colour.".into();
                refresh(&app, &state, &tasks);
                return;
            }

            let mut payload = serde_json::json!({
                "name": app.get_form_name().trim(),
                "tagline": app.get_form_tagline().trim(),
            });
            if !accent.is_empty() {
                payload["accent_color"] = serde_json::Value::String(accent);
            }

            let mut state_ref = state.borrow_mut();
            state_ref.admin_saving = true;
            state_ref.admin_notice.clear();
            drop(state_ref);

            let _ = tasks.send(Task::AdminAction(AdminAction::SaveInstance(payload)));
            refresh(&app, &state, &tasks);
        }
    });

    app.on_choose_inbox_tab({
        let weak = app.as_weak();
        let state = state.clone();
        let tasks = tasks.clone();
        move |index| {
            let Some(app) = weak.upgrade() else { return };
            state.borrow_mut().inbox_tab = index;
            if index == 1 {
                // The friends tab is the one place a stale list is visible,
                // so it is refreshed on the way in.
                let _ = tasks.send(Task::LoadRelationships);
            }
            refresh(&app, &state, &tasks);
        }
    });

    app.on_close_inbox({
        let weak = app.as_weak();
        let state = state.clone();
        let tasks = tasks.clone();
        move || {
            let Some(app) = weak.upgrade() else { return };
            state.borrow_mut().store.inbox_open = false;
            refresh(&app, &state, &tasks);
        }
    });

    app.on_open_conversation({
        let weak = app.as_weak();
        let state = state.clone();
        let tasks = tasks.clone();
        move |id| {
            let Some(app) = weak.upgrade() else { return };
            let id = id.to_string();
            let mut state_ref = state.borrow_mut();
            state_ref.store.selected_conversation = id.clone();
            state_ref.store.mark_conversation_read(&id);
            let cached = !state_ref.store.direct_in(&id).is_empty();
            state_ref.scroll_token += 1;
            drop(state_ref);

            if !cached {
                let _ = tasks.send(Task::LoadDirect { conversation: id });
            }
            refresh(&app, &state, &tasks);
        }
    });

    app.on_send_direct({
        let weak = app.as_weak();
        let state = state.clone();
        let tasks = tasks.clone();
        move |text| {
            let Some(app) = weak.upgrade() else { return };
            let content = text.trim().to_string();
            if content.is_empty() {
                return;
            }
            let mut state_ref = state.borrow_mut();
            let conversation = state_ref.store.selected_conversation.clone();
            if conversation.is_empty() {
                return;
            }
            let temp_id = state_ref.temp_id();
            let me = state_ref.store.me.member.id.clone();

            state_ref.store.upsert_direct(DirectMessage {
                id: temp_id.clone(),
                conversation_id: conversation.clone(),
                author_id: me,
                content: content.clone(),
                created_at: time::OffsetDateTime::now_utc()
                    .format(&time::format_description::well_known::Rfc3339)
                    .unwrap_or_default(),
                pending: true,
                failed: false,
            });
            state_ref.scroll_token += 1;
            drop(state_ref);

            app.set_direct_draft(SharedString::new());
            let _ = tasks.send(Task::SendDirect {
                conversation,
                temp_id,
                content,
            });
            refresh(&app, &state, &tasks);
        }
    });

    app.on_message_member({
        let tasks = tasks.clone();
        move |id| {
            if id.is_empty() {
                return;
            }
            // The server creates the conversation if there is not one yet.
            let _ = tasks.send(Task::OpenConversation {
                peer: id.to_string(),
            });
        }
    });

    // --- friends and blocking --------------------------------------------
    //
    // Each of these guesses the outcome locally so the button answers at
    // once, then refetches: the server decides, and a request that crossed
    // with theirs becomes a friendship rather than a second request.
    fn relate(
        app: &ui::App,
        state: &Shared,
        tasks: &mpsc::UnboundedSender<Task>,
        user: &str,
        action: Relate,
    ) {
        if user.is_empty() {
            return;
        }
        {
            let mut state_ref = state.borrow_mut();
            match action {
                Relate::Add => {
                    // Accepting an incoming request and sending a new one
                    // are the same call; the guess differs.
                    let kind = match state_ref.store.relationship(user) {
                        RelationshipKind::Incoming => RelationshipKind::Friend,
                        _ => RelationshipKind::Outgoing,
                    };
                    state_ref.store.set_relationship(user, kind);
                }
                Relate::Remove => state_ref
                    .store
                    .set_relationship(user, RelationshipKind::None),
                Relate::Block => state_ref
                    .store
                    .set_relationship(user, RelationshipKind::Blocked),
                Relate::Unblock => state_ref
                    .store
                    .set_relationship(user, RelationshipKind::None),
                Relate::Favourite(on) => state_ref.store.set_favourite(user, on),
            }
        }
        let _ = tasks.send(Task::Relate {
            user: user.to_string(),
            action,
        });
        refresh(app, state, tasks);
    }

    macro_rules! relationship_handler {
        ($setter:ident, $action:expr) => {{
            let weak = app.as_weak();
            let state = state.clone();
            let tasks = tasks.clone();
            app.$setter(move |id| {
                let Some(app) = weak.upgrade() else { return };
                relate(&app, &state, &tasks, &id, $action);
            });
        }};
    }

    relationship_handler!(on_add_friend, Relate::Add);
    relationship_handler!(on_remove_friend, Relate::Remove);
    relationship_handler!(on_block_member, Relate::Block);
    relationship_handler!(on_unblock_member, Relate::Unblock);

    app.on_toggle_favourite({
        let weak = app.as_weak();
        let state = state.clone();
        let tasks = tasks.clone();
        move |id| {
            let Some(app) = weak.upgrade() else { return };
            let now_favourite = !state.borrow().store.is_favourite(&id);
            relate(&app, &state, &tasks, &id, Relate::Favourite(now_favourite));
        }
    });

    app.on_start_call({
        let weak = app.as_weak();
        let state = state.clone();
        let tasks = tasks.clone();
        move || {
            let Some(app) = weak.upgrade() else { return };
            let mut state_ref = state.borrow_mut();
            let conversation = state_ref.store.selected_conversation.clone();
            if conversation.is_empty() || !state_ref.store.voice_enabled {
                return;
            }
            state_ref.call_notice.clear();
            drop(state_ref);
            let _ = tasks.send(Task::StartCall { conversation });
            refresh(&app, &state, &tasks);
        }
    });

    app.on_accept_call({
        let weak = app.as_weak();
        let state = state.clone();
        let tasks = tasks.clone();
        move || {
            let Some(app) = weak.upgrade() else { return };
            let state_ref = state.borrow();
            let Some(call) = state_ref.store.incoming_call().map(|c| c.id.clone()) else {
                return;
            };
            drop(state_ref);
            let _ = tasks.send(Task::AnswerCall {
                call,
                action: "accept",
            });
            refresh(&app, &state, &tasks);
        }
    });

    // Declining and hanging up are the same call to the server: a call that
    // is over is over, whoever ended it and whenever.
    let end_call = {
        let weak = app.as_weak();
        let state = state.clone();
        let tasks = tasks.clone();
        move || {
            let Some(app) = weak.upgrade() else { return };
            let mut state_ref = state.borrow_mut();
            let call = state_ref
                .store
                .incoming_call()
                .or_else(|| state_ref.store.my_call())
                .map(|c| c.id.clone());
            state_ref.engine.disconnect();
            state_ref.voice.left();
            state_ref.call_started = None;
            drop(state_ref);

            if let Some(call) = call {
                let _ = tasks.send(Task::AnswerCall {
                    call,
                    action: "end",
                });
            }
            refresh(&app, &state, &tasks);
        }
    };
    app.on_decline_call(end_call.clone());
    app.on_hang_up(end_call);

    // --- voice ---------------------------------------------------------------
    app.on_toggle_mute({
        let weak = app.as_weak();
        let state = state.clone();
        let tasks = tasks.clone();
        move || {
            let Some(app) = weak.upgrade() else { return };
            let mut state_ref = state.borrow_mut();
            let next = !state_ref.voice.muted;
            state_ref.voice.set_muted(next);
            let muted = state_ref.voice.muted;
            let deafened = state_ref.voice.deafened;
            state_ref.engine.set_muted(muted);
            state_ref.engine.set_deafened(deafened);
            drop(state_ref);
            refresh(&app, &state, &tasks);
        }
    });

    app.on_toggle_deafen({
        let weak = app.as_weak();
        let state = state.clone();
        let tasks = tasks.clone();
        move || {
            let Some(app) = weak.upgrade() else { return };
            let mut state_ref = state.borrow_mut();
            let next = !state_ref.voice.deafened;
            state_ref.voice.set_deafened(next);
            let muted = state_ref.voice.muted;
            state_ref.engine.set_deafened(next);
            state_ref.engine.set_muted(muted);
            drop(state_ref);
            refresh(&app, &state, &tasks);
        }
    });

    app.on_toggle_screen_share({
        move || {
            // The capture half already exists, in Rust, in
            // desktop/src/capture.rs; what is missing is the media engine to
            // publish it through. See src/voice.rs.
            eprintln!("minichat: screen sharing needs the voice feature");
        }
    });

    app.on_leave_voice({
        let weak = app.as_weak();
        let state = state.clone();
        let tasks = tasks.clone();
        move || {
            let Some(app) = weak.upgrade() else { return };
            let mut state_ref = state.borrow_mut();
            state_ref.engine.disconnect();
            state_ref.voice.left();
            state_ref.speaking.clear();
            drop(state_ref);
            refresh(&app, &state, &tasks);
        }
    });

    app.on_open_attachment({
        let weak = app.as_weak();
        let state = state.clone();
        let tasks = tasks.clone();
        move |id| {
            let Some(app) = weak.upgrade() else { return };
            let mut state_ref = state.borrow_mut();
            let channel = state_ref.store.selected_channel.clone();
            let file = state_ref
                .store
                .messages_in(&channel)
                .iter()
                .flat_map(|m| m.attachments.iter())
                .find(|f| f.id == id.as_str())
                .cloned();
            let Some(file) = file else { return };

            // An image opens in place; anything else is handed to the
            // desktop, which knows what to do with a PDF or a zip and this
            // client does not.
            if !file.is_image() {
                let url = state_ref.origin_url(&file.url);
                drop(state_ref);
                open_externally(&url);
                return;
            }

            // The preview in the message list is scaled down, so the full
            // image is fetched before it is shown at full size.
            let url = file.url.clone();
            let fetch = state_ref.images.claim_full(&url);
            state_ref.lightbox = Some(file);
            drop(state_ref);

            if fetch {
                let _ = tasks.send(Task::FetchFullImage(url));
            }
            refresh(&app, &state, &tasks);
            app.set_overlay(ui::Overlay::Lightbox);
        }
    });

    app.on_lightbox_download({
        let state = state.clone();
        move || {
            let state_ref = state.borrow();
            if let Some(file) = state_ref.lightbox.as_ref() {
                let url = state_ref.origin_url(&file.url);
                drop(state_ref);
                open_externally(&url);
            }
        }
    });

    // --- forwarding ------------------------------------------------------
    app.on_forward_message({
        let weak = app.as_weak();
        let state = state.clone();
        let tasks = tasks.clone();
        move |id| {
            let Some(app) = weak.upgrade() else { return };
            let mut state_ref = state.borrow_mut();
            let channel = state_ref.store.selected_channel.clone();
            let message = state_ref
                .store
                .messages_in(&channel)
                .iter()
                .find(|m| m.id == id.as_str())
                .cloned();
            let Some(message) = message else { return };
            state_ref.forwarding = Some(message);
            state_ref.forward_target.clear();
            state_ref.forward_busy = false;
            drop(state_ref);

            app.set_forward_note(SharedString::new());
            refresh(&app, &state, &tasks);
            app.set_overlay(ui::Overlay::Forward);
        }
    });

    app.on_choose_forward_target({
        let weak = app.as_weak();
        let state = state.clone();
        let tasks = tasks.clone();
        move |id| {
            let Some(app) = weak.upgrade() else { return };
            state.borrow_mut().forward_target = id.to_string();
            refresh(&app, &state, &tasks);
        }
    });

    app.on_confirm_forward({
        let weak = app.as_weak();
        let state = state.clone();
        let tasks = tasks.clone();
        move || {
            let Some(app) = weak.upgrade() else { return };
            let note = app.get_forward_note().trim().to_string();
            let mut state_ref = state.borrow_mut();
            let target = state_ref.forward_target.clone();
            let Some(message) = state_ref.forwarding.clone() else {
                return;
            };
            if target.is_empty() {
                return;
            }
            let origin = state_ref
                .store
                .channel(&message.channel_id)
                .map(|c| c.name.clone());
            let public = state_ref.store.instance_public_url();
            let body = crate::forward::compose(&message, origin.as_deref(), &note, &public);
            state_ref.forward_busy = true;
            let temp_id = state_ref.temp_id();
            drop(state_ref);

            let _ = tasks.send(Task::Send {
                channel: target.clone(),
                temp_id,
                content: body,
                reply_to: None,
                attachments: Vec::new(),
            });

            // Follow the forward to where it landed, the way the web client
            // does: you nearly always want to see it arrive.
            app.set_overlay(ui::Overlay::None);
            state.borrow_mut().forwarding = None;
            app.invoke_select_channel(target.into());
            refresh(&app, &state, &tasks);
        }
    });

    app.on_remove_attachment({
        let weak = app.as_weak();
        let state = state.clone();
        let tasks = tasks.clone();
        move |id| {
            let Some(app) = weak.upgrade() else { return };
            let mut state_ref = state.borrow_mut();
            state_ref
                .pending_attachments
                .retain(|f| f.id != id.as_str());
            state_ref.attach_notice.clear();
            drop(state_ref);
            refresh(&app, &state, &tasks);
        }
    });

    app.on_attach({
        let state = state.clone();
        let tasks = tasks.clone();
        move || {
            let state_ref = state.borrow();
            let channel = state_ref.store.selected_channel.clone();
            if channel.is_empty()
                || !crate::perms::can(
                    state_ref.store.permissions_in(&channel),
                    crate::perms::ATTACH_FILES,
                )
            {
                return;
            }
            let max_mb = state_ref.store.instance.max_upload_mb.max(1);
            drop(state_ref);
            let _ = tasks.send(Task::Attach { channel, max_mb });
        }
    });

    app.on_typing({
        let state = state.clone();
        let tasks = tasks.clone();
        move || {
            let now = time::OffsetDateTime::now_utc().unix_timestamp();
            let mut state_ref = state.borrow_mut();
            let channel = state_ref.store.selected_channel.clone();
            if channel.is_empty() {
                return;
            }
            // The server broadcasts each notice to the whole channel, so
            // sending one per keystroke would be a frame per keystroke for
            // everyone. Four seconds is under the eight the indicator
            // expires at, so a continuous typist never flickers.
            const THROTTLE: i64 = 4;
            let last = state_ref.typing_sent.get(&channel).copied().unwrap_or(0);
            if now - last < THROTTLE {
                return;
            }
            state_ref.typing_sent.insert(channel.clone(), now);
            drop(state_ref);
            let _ = tasks.send(Task::Typing(channel));
        }
    });

    app.on_scrolled({
        let weak = app.as_weak();
        let state = state.clone();
        let tasks = tasks.clone();
        move |distance, content, viewport| {
            let Some(app) = weak.upgrade() else { return };
            let mut state_ref = state.borrow_mut();
            state_ref.scroll_distance = distance;
            state_ref.content_height = content;
            state_ref.viewport_height = viewport;

            // `distance` counts from the bottom, so a large value means the
            // reader is near the top and the next page should be fetched.
            let near_top = distance > 0.0 && state_ref.scroll_distance_at_top(distance);
            let channel = state_ref.store.selected_channel.clone();
            let oldest = state_ref
                .store
                .messages_in(&channel)
                .first()
                .map(|m| m.id.clone());

            if near_top && !state_ref.loading_older && !state_ref.exhausted.contains(&channel) {
                if let Some(before) = oldest {
                    state_ref.loading_older = true;
                    drop(state_ref);
                    let _ = tasks.send(Task::LoadOlder { channel, before });
                    let _ = &app;
                }
            }
        }
    });

    // --- layout -------------------------------------------------------------
    app.on_body_width_changed({
        let weak = app.as_weak();
        let state = state.clone();
        let tasks = tasks.clone();
        move |width| {
            let Some(app) = weak.upgrade() else { return };
            let width = width.max(120.0);
            let mut state_ref = state.borrow_mut();
            // A resize relays every visible message, so it is only done when
            // the column actually changed by more than a rounding error.
            if (state_ref.body_width - width).abs() < 0.5 {
                return;
            }
            state_ref.body_width = width;
            drop(state_ref);
            refresh(&app, &state, &tasks);
        }
    });

    // --- links and mentions --------------------------------------------------
    app.on_run_activated({
        move |target, kind| {
            // 1 is a link. Mentions and spoilers are handled in-app and do
            // not need the browser.
            if kind == 1 && (target.starts_with("https://") || target.starts_with("http://")) {
                open_externally(&target);
            }
        }
    });

    // --- window --------------------------------------------------------------
    app.on_minimise({
        let weak = app.as_weak();
        move || {
            if let Some(app) = weak.upgrade() {
                app.window().set_minimized(true);
            }
        }
    });

    app.on_toggle_maximise({
        let weak = app.as_weak();
        move || {
            if let Some(app) = weak.upgrade() {
                let maximised = app.window().is_maximized();
                app.window().set_maximized(!maximised);
                app.set_maximised(!maximised);
            }
        }
    });

    app.on_quit({
        let weak = app.as_weak();
        let state = state.clone();
        let tasks = tasks.clone();
        move || {
            let Some(app) = weak.upgrade() else { return };
            let state_ref = state.borrow();
            // Hiding is only offered where there is a tray to restore from,
            // and only once signed in: a first-run member who hides the
            // connect screen is left with an app that appears not to have
            // started.
            let hide = state_ref.settings.close_to_tray
                && crate::tray::available()
                && state_ref.store.ready;
            drop(state_ref);

            if hide {
                let _ = app.hide();
                return;
            }
            let _ = tasks.send(Task::Disconnect);
            let _ = slint::quit_event_loop();
        }
    });

    // --- overlays ------------------------------------------------------------
    app.on_dismiss_overlay({
        let weak = app.as_weak();
        let state = state.clone();
        let tasks = tasks.clone();
        move || {
            let Some(app) = weak.upgrade() else { return };
            let mut state_ref = state.borrow_mut();
            state_ref.profile = None;
            state_ref.pending_confirm = None;
            state_ref.reacting_to = None;
            state_ref.menu_target = None;
            state_ref.lightbox = None;
            state_ref.forwarding = None;
            state_ref.focus_token += 1;
            drop(state_ref);
            app.set_overlay(ui::Overlay::None);
            app.set_emoji_filter(SharedString::new());
            refresh(&app, &state, &tasks);
        }
    });

    app.on_open_settings({
        let weak = app.as_weak();
        let state = state.clone();
        move || {
            let Some(app) = weak.upgrade() else { return };
            app.set_status_draft(state.borrow().store.me.member.custom_status.clone().into());
            app.set_overlay(ui::Overlay::Settings);
        }
    });

    app.on_choose_theme({
        let weak = app.as_weak();
        let state = state.clone();
        let tasks = tasks.clone();
        move |index| {
            let Some(app) = weak.upgrade() else { return };
            let mut state_ref = state.borrow_mut();
            state_ref.settings.theme = match index {
                0 => ThemeChoice::Dark,
                1 => ThemeChoice::Light,
                _ => ThemeChoice::Instance,
            };
            let _ = state_ref.settings.store();
            state_ref.palette = Palette::resolve(&state_ref.branding());
            let palette = state_ref.palette.clone();
            drop(state_ref);

            theme::apply(&app, &palette);
            // Author names and the fallback avatar colours are derived from
            // the palette, so the whole view is rebuilt rather than retinted.
            refresh(&app, &state, &tasks);
        }
    });

    app.on_set_hotkeys({
        let weak = app.as_weak();
        let state = state.clone();
        let tasks = tasks.clone();
        let hotkeys = hotkeys.clone();
        move |enabled| {
            let Some(app) = weak.upgrade() else { return };
            let mut state_ref = state.borrow_mut();
            state_ref.settings.hotkeys.enabled = enabled;
            let _ = state_ref.settings.store();
            let bindings = state_ref.settings.hotkeys.clone();
            drop(state_ref);

            // Re-registered at once rather than at the next launch: a
            // setting that only takes effect after a restart is a setting
            // people assume is broken.
            if let Some(control) = hotkeys.borrow_mut().as_mut() {
                control.apply(&bindings);
            }
            refresh(&app, &state, &tasks);
        }
    });

    app.on_save_status({
        let state = state.clone();
        let tasks = tasks.clone();
        move |text| {
            let _ = tasks.send(Task::UpdateStatus(text.to_string()));
            let _ = &state;
        }
    });

    app.on_sign_out({
        let weak = app.as_weak();
        let state = state.clone();
        move || {
            let Some(app) = weak.upgrade() else { return };
            state.borrow_mut().pending_confirm = Some(Confirm::SignOut);
            app.set_confirm_title("Sign out?".into());
            app.set_confirm_body(
                "You will need your username and password to sign back in.".into(),
            );
            app.set_confirm_label("Sign out".into());
            app.set_overlay(ui::Overlay::Confirm);
            let _ = &state;
        }
    });

    app.on_confirm_action({
        let weak = app.as_weak();
        let state = state.clone();
        let tasks = tasks.clone();
        move || {
            let Some(app) = weak.upgrade() else { return };
            let pending = state.borrow_mut().pending_confirm.take();
            app.set_overlay(ui::Overlay::None);

            match pending {
                Some(Confirm::DeleteMessage(id)) => {
                    let _ = tasks.send(Task::DeleteMessage(id));
                }
                Some(Confirm::Kick(user)) => {
                    let _ = tasks.send(Task::AdminAction(AdminAction::Kick(user)));
                }
                Some(Confirm::Ban(user)) => {
                    // No reason field yet; the server accepts an empty one
                    // and the audit log records who did it either way.
                    let _ = tasks.send(Task::AdminAction(AdminAction::Ban {
                        user,
                        reason: String::new(),
                    }));
                }
                // The sign-out confirmation is the only one raised without a
                // stored action, because the button that raises it is itself
                // the action.
                Some(Confirm::SignOut) | None => {
                    let mut state_ref = state.borrow_mut();
                    state_ref.settings.sign_out();
                    let _ = state_ref.settings.store();
                    drop(state_ref);
                    let _ = tasks.send(Task::Disconnect);
                    app.set_screen(ui::Screen::Signin);
                    app.set_password(SharedString::new());
                }
            }
        }
    });

    // Delete goes through a confirmation; nothing else does.
    app.on_delete_message({
        let weak = app.as_weak();
        let state = state.clone();
        move |id| {
            let Some(app) = weak.upgrade() else { return };
            state.borrow_mut().pending_confirm = Some(Confirm::DeleteMessage(id.to_string()));
            app.set_confirm_title("Delete message?".into());
            app.set_confirm_body("This cannot be undone.".into());
            app.set_confirm_label("Delete".into());
            app.set_overlay(ui::Overlay::Confirm);
        }
    });

    // --- profiles --------------------------------------------------------------
    let open_profile = {
        let weak = app.as_weak();
        let state = state.clone();
        let tasks = tasks.clone();
        move |id: SharedString| {
            let Some(app) = weak.upgrade() else { return };
            if id.is_empty() {
                return;
            }
            state.borrow_mut().profile = Some(id.to_string());
            refresh(&app, &state, &tasks);
            app.set_overlay(ui::Overlay::Profile);
        }
    };
    app.on_open_author(open_profile.clone());
    app.on_open_member(open_profile);

    // --- side panels ------------------------------------------------------------
    app.on_open_search({
        let weak = app.as_weak();
        let state = state.clone();
        let tasks = tasks.clone();
        move || {
            let Some(app) = weak.upgrade() else { return };
            state.borrow_mut().panel_hits.clear();
            app.set_panel_query(SharedString::new());
            app.set_side_panel(ui::SidePanelKind::Search);
            refresh(&app, &state, &tasks);
        }
    });

    app.on_open_pins({
        let weak = app.as_weak();
        let state = state.clone();
        let tasks = tasks.clone();
        move || {
            let Some(app) = weak.upgrade() else { return };
            let mut state_ref = state.borrow_mut();
            let channel = state_ref.store.selected_channel.clone();
            state_ref.panel_hits.clear();
            state_ref.panel_loading = true;
            drop(state_ref);

            app.set_side_panel(ui::SidePanelKind::Pins);
            let _ = tasks.send(Task::LoadPins { channel });
            refresh(&app, &state, &tasks);
        }
    });

    app.on_close_panel({
        let weak = app.as_weak();
        let state = state.clone();
        move || {
            let Some(app) = weak.upgrade() else { return };
            state.borrow_mut().panel_hits.clear();
            app.set_side_panel(ui::SidePanelKind::None);
        }
    });

    app.on_run_search({
        let weak = app.as_weak();
        let state = state.clone();
        let tasks = tasks.clone();
        move |query| {
            let Some(app) = weak.upgrade() else { return };
            let query = query.trim().to_string();
            if query.is_empty() {
                return;
            }
            state.borrow_mut().panel_loading = true;
            refresh(&app, &state, &tasks);
            let _ = tasks.send(Task::Search { query });
        }
    });

    app.on_open_hit({
        let weak = app.as_weak();
        let state = state.clone();
        let tasks = tasks.clone();
        move |id| {
            let Some(app) = weak.upgrade() else { return };
            // Jump to the hit's channel; the message itself is scrolled to
            // once its page is loaded.
            let channel = state
                .borrow()
                .panel_hits
                .iter()
                .find(|m| m.id == id.as_str())
                .map(|m| m.channel_id.clone());
            if let Some(channel) = channel {
                let mut state_ref = state.borrow_mut();
                if state_ref.store.selected_channel != channel {
                    state_ref.store.selected_channel = channel.clone();
                    drop(state_ref);
                    let _ = tasks.send(Task::LoadMessages { channel });
                } else {
                    drop(state_ref);
                }
                refresh(&app, &state, &tasks);
            }
        }
    });

    // --- the emoji picker ----------------------------------------------------------
    app.on_emoji({
        let weak = app.as_weak();
        let state = state.clone();
        let tasks = tasks.clone();
        move || {
            let Some(app) = weak.upgrade() else { return };
            state.borrow_mut().reacting_to = None;
            // Anchored above the composer, which is where it was opened from.
            let size = app.window().size().to_logical(app.window().scale_factor());
            app.set_pointer_x(size.width / 2.0);
            app.set_pointer_y(size.height - 90.0);
            app.set_emoji_filter(SharedString::new());
            refresh(&app, &state, &tasks);
            app.set_overlay(ui::Overlay::Emoji);
        }
    });

    // --- context menus --------------------------------------------------------
    //
    // The menu is built from what the member may actually do here, so it
    // never offers an action the server would refuse.
    fn menu_item(id: &str, label: &str, icon: &str, danger: bool, gap: bool) -> ui::MenuItem {
        ui::MenuItem {
            id: id.into(),
            label: label.into(),
            icon: icon.into(),
            danger,
            separator_before: gap,
        }
    }

    app.on_channel_menu({
        let weak = app.as_weak();
        let state = state.clone();
        move |id| {
            let Some(app) = weak.upgrade() else { return };
            let state_ref = state.borrow();
            let bits = state_ref.store.permissions;
            let mut items = vec![menu_item("mark-read", "Mark as read", "", false, false)];
            if crate::perms::can(bits, crate::perms::MANAGE_CHANNELS) {
                items.push(menu_item("edit-channel", "Edit channel", "", false, true));
                items.push(menu_item(
                    "delete-channel",
                    "Delete channel",
                    "",
                    true,
                    false,
                ));
            }
            drop(state_ref);

            state.borrow_mut().menu_target = Some(MenuTarget::Channel(id.to_string()));
            app.set_menu_items(ModelRc::new(VecModel::from(items)));
            app.set_overlay(ui::Overlay::Menu);
        }
    });

    app.on_message_menu({
        let weak = app.as_weak();
        let state = state.clone();
        move |id| {
            let Some(app) = weak.upgrade() else { return };
            let state_ref = state.borrow();
            let channel = state_ref.store.selected_channel.clone();
            let bits = state_ref.store.permissions_in(&channel);
            let message = state_ref
                .store
                .messages_in(&channel)
                .iter()
                .find(|m| m.id == id.as_str())
                .cloned();
            let mine = message
                .as_ref()
                .and_then(|m| m.author_id().map(|a| a == state_ref.store.me.member.id))
                .unwrap_or(false);
            let pinned = message.as_ref().is_some_and(|m| m.pinned);
            drop(state_ref);

            let mut items = vec![
                menu_item("react", "Add reaction", "", false, false),
                menu_item("reply", "Reply", "", false, false),
                menu_item("copy", "Copy text", "", false, false),
            ];
            if crate::perms::can(bits, crate::perms::PIN_MESSAGES) {
                items.push(menu_item(
                    "pin",
                    if pinned {
                        "Unpin message"
                    } else {
                        "Pin message"
                    },
                    "",
                    false,
                    true,
                ));
            }
            // Anyone may edit their own; deleting also needs the permission
            // when the message is someone else's.
            if mine {
                items.push(menu_item("edit", "Edit", "", false, true));
            }
            if mine || crate::perms::can(bits, crate::perms::MANAGE_MESSAGES) {
                items.push(menu_item("delete", "Delete", "", true, !mine));
            }

            state.borrow_mut().menu_target = Some(MenuTarget::Message(id.to_string()));
            app.set_menu_items(ModelRc::new(VecModel::from(items)));
            app.set_overlay(ui::Overlay::Menu);
        }
    });

    app.on_member_menu({
        let weak = app.as_weak();
        let state = state.clone();
        move |id| {
            let Some(app) = weak.upgrade() else { return };
            let state_ref = state.borrow();
            let bits = state_ref.store.permissions;
            let is_me = id.as_str() == state_ref.store.me.member.id;
            let relationship = state_ref.store.relationship(&id);
            let favourite = state_ref.store.is_favourite(&id);
            drop(state_ref);

            let mut items = vec![menu_item("profile", "View profile", "", false, false)];
            if !is_me {
                items.push(menu_item("dm", "Send a message", "", false, false));

                // What the friend entry says depends on where you already
                // are, so the menu never offers a step that does nothing.
                match relationship {
                    RelationshipKind::Blocked => {
                        items.push(menu_item("unblock", "Unblock", "", false, true));
                    }
                    kind => {
                        items.push(menu_item(
                            "friend",
                            match kind {
                                RelationshipKind::Friend => "Remove friend",
                                RelationshipKind::Outgoing => "Cancel friend request",
                                RelationshipKind::Incoming => "Accept friend request",
                                _ => "Add friend",
                            },
                            "",
                            false,
                            true,
                        ));
                        items.push(menu_item(
                            "favourite",
                            if favourite {
                                "Remove favourite"
                            } else {
                                "Favourite"
                            },
                            "",
                            false,
                            false,
                        ));
                        items.push(menu_item("block", "Block", "", true, true));
                    }
                }

                // Moderation actions only appear to someone who has them.
                if crate::perms::can(bits, crate::perms::KICK_MEMBERS) {
                    items.push(menu_item("kick", "Kick from community", "", true, true));
                }
                if crate::perms::can(bits, crate::perms::BAN_MEMBERS) {
                    items.push(menu_item("ban", "Ban", "", true, false));
                }
            }

            state.borrow_mut().menu_target = Some(MenuTarget::Member(id.to_string()));
            app.set_menu_items(ModelRc::new(VecModel::from(items)));
            app.set_overlay(ui::Overlay::Menu);
        }
    });

    app.on_menu_chose({
        let weak = app.as_weak();
        let state = state.clone();
        let tasks = tasks.clone();
        move |choice| {
            let Some(app) = weak.upgrade() else { return };
            let target = state.borrow_mut().menu_target.take();
            app.set_overlay(ui::Overlay::None);

            match (choice.as_str(), target) {
                ("category-up", Some(MenuTarget::Category(id)))
                | ("category-down", Some(MenuTarget::Category(id))) => {
                    let offset = if choice == "category-up" { -1 } else { 1 };
                    let order = crate::reorder::move_category(
                        &state.borrow().store.categories,
                        &id,
                        offset,
                    );
                    if let Some(order) = order {
                        let _ = tasks.send(Task::ReorderCategories(order));
                    }
                }

                ("mark-read", Some(MenuTarget::Channel(id))) => {
                    let mut state_ref = state.borrow_mut();
                    let last = state_ref
                        .store
                        .messages_in(&id)
                        .last()
                        .map(|m| m.id.clone());
                    state_ref.store.mark_read(&id);
                    drop(state_ref);
                    if let Some(message) = last {
                        let _ = tasks.send(Task::Ack {
                            channel: id,
                            message,
                        });
                    }
                    refresh(&app, &state, &tasks);
                }
                ("profile", Some(MenuTarget::Member(id))) => {
                    state.borrow_mut().profile = Some(id);
                    refresh(&app, &state, &tasks);
                    app.set_overlay(ui::Overlay::Profile);
                }
                ("dm", Some(MenuTarget::Member(id))) => {
                    let _ = tasks.send(Task::OpenConversation { peer: id });
                }
                ("friend", Some(MenuTarget::Member(id))) => {
                    let action = match state.borrow().store.relationship(&id) {
                        RelationshipKind::Friend | RelationshipKind::Outgoing => Relate::Remove,
                        _ => Relate::Add,
                    };
                    relate(&app, &state, &tasks, &id, action);
                }
                ("favourite", Some(MenuTarget::Member(id))) => {
                    let on = !state.borrow().store.is_favourite(&id);
                    relate(&app, &state, &tasks, &id, Relate::Favourite(on));
                }
                ("block", Some(MenuTarget::Member(id))) => {
                    relate(&app, &state, &tasks, &id, Relate::Block);
                }
                ("unblock", Some(MenuTarget::Member(id))) => {
                    relate(&app, &state, &tasks, &id, Relate::Unblock);
                }
                ("reply", Some(MenuTarget::Message(id))) => {
                    state.borrow_mut().replying_to = Some(id);
                    refresh(&app, &state, &tasks);
                }
                ("react", Some(MenuTarget::Message(id))) => {
                    state.borrow_mut().reacting_to = Some(id);
                    let size = app.window().size().to_logical(app.window().scale_factor());
                    app.set_pointer_x(size.width / 2.0);
                    app.set_pointer_y(size.height / 2.0 + 180.0);
                    refresh(&app, &state, &tasks);
                    app.set_overlay(ui::Overlay::Emoji);
                }
                ("copy", Some(MenuTarget::Message(id))) => {
                    let state_ref = state.borrow();
                    let channel = state_ref.store.selected_channel.clone();
                    let text = state_ref
                        .store
                        .messages_in(&channel)
                        .iter()
                        .find(|m| m.id == id)
                        .map(|m| m.content.clone());
                    drop(state_ref);
                    if let Some(text) = text {
                        copy_to_clipboard(&text);
                    }
                }
                ("pin", Some(MenuTarget::Message(id))) => {
                    let state_ref = state.borrow();
                    let channel = state_ref.store.selected_channel.clone();
                    let pinned = state_ref
                        .store
                        .messages_in(&channel)
                        .iter()
                        .find(|m| m.id == id)
                        .is_some_and(|m| m.pinned);
                    drop(state_ref);
                    let _ = tasks.send(Task::PinMessage {
                        message: id,
                        pinned: !pinned,
                    });
                }
                ("delete", Some(MenuTarget::Message(id))) => {
                    state.borrow_mut().pending_confirm = Some(Confirm::DeleteMessage(id));
                    app.set_confirm_title("Delete message?".into());
                    app.set_confirm_body("This cannot be undone.".into());
                    app.set_confirm_label("Delete".into());
                    app.set_overlay(ui::Overlay::Confirm);
                }
                ("edit", Some(MenuTarget::Message(id))) => {
                    app.invoke_edit_message(id.into());
                }
                // Channel editing and moderation are admin surfaces this
                // client does not carry yet. The entries only appear to
                // someone who could use them, so saying so beats silently
                // doing nothing.
                ("edit-channel", _) | ("delete-channel", _) | ("kick", _) | ("ban", _) => {
                    eprintln!("minichat: {choice} is not available in this client yet");
                }
                _ => {}
            }
        }
    });

    app.on_choose_emoji({
        let weak = app.as_weak();
        let state = state.clone();
        let tasks = tasks.clone();
        move |choice| {
            let Some(app) = weak.upgrade() else { return };
            let mut state_ref = state.borrow_mut();
            let target = state_ref.reacting_to.take();
            drop(state_ref);
            app.set_overlay(ui::Overlay::None);
            app.set_emoji_filter(SharedString::new());

            match target {
                // Opened from a message: it is a reaction.
                Some(message) => {
                    let _ = tasks.send(Task::React {
                        message,
                        emoji: choice.to_string(),
                        on: true,
                    });
                }
                // Opened from the composer: it is text.
                None => {
                    let draft = app.get_draft();
                    app.set_draft(format!("{draft}{choice}").into());
                }
            }
        }
    });
}

/// Put text on the system clipboard.
///
/// A failure here is worth a line on stderr and nothing more: the member
/// tried to copy a message and it did not arrive, which is annoying rather
/// than a reason to interrupt them with a dialog.
fn copy_to_clipboard(text: &str) {
    match arboard::Clipboard::new().and_then(|mut board| board.set_text(text.to_owned())) {
        Ok(()) => {}
        Err(error) => eprintln!("minichat: could not copy to the clipboard: {error}"),
    }
}

/// Hand a URL to the desktop.
///
/// Only http(s) ever reaches here — the caller checks — so this cannot be
/// turned into "run this command" by a message that contains a crafted URL.
fn open_externally(url: &str) {
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return;
    }
    #[cfg(target_os = "windows")]
    let result = std::process::Command::new("cmd")
        .args(["/C", "start", "", url])
        .spawn();
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open").arg(url).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let result = std::process::Command::new("xdg-open").arg(url).spawn();

    if let Err(error) = result {
        eprintln!("minichat: could not open {url}: {error}");
    }
}
