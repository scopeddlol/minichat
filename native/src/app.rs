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
    FetchImages(Vec<String>),
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
        }
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

    // Events come back on the runtime thread and are drained here, on the UI
    // thread, where the store lives.
    let pump = {
        let weak = app.as_weak();
        let state = state.clone();
        let tasks = task_tx.clone();
        move || {
            let Some(app) = weak.upgrade() else { return };
            let mut dirty = false;
            while let Ok(event) = event_rx.try_recv() {
                dirty |= handle_event(event, &app, &state, &tasks);
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

    wire_callbacks(&app, &state, &task_tx);

    // Open straight into the saved instance when there is a saved session,
    // otherwise show the connect screen.
    {
        let state_ref = state.borrow();
        let saved = state_ref.settings.instance_url.clone();
        let token = state_ref.settings.token.clone();
        drop(state_ref);

        theme::apply(&app, &state.borrow().palette);
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
            } => {
                let (Some(client), events) = (client.clone(), events.clone()) else {
                    continue;
                };
                tokio::spawn(async move {
                    let _ = match client
                        .send_message(&channel, &content, reply_to.as_deref())
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

            Task::PinMessage { message, pinned } => {
                let Some(client) = client.clone() else {
                    continue;
                };
                tokio::spawn(async move {
                    let _ = client.pin_message(&message, pinned).await;
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
            true
        }

        Event::Gateway(frame) => state.borrow_mut().store.apply_event(&frame, now),

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
    app.set_direct_unread(store.total_unread() as i32);

    let rows = view::build_messages_with_roles(
        store.messages_in(&selected),
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
    let to_member = |member: &Member| {
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

    queue_images(&state_ref, tasks);
}

fn wire_callbacks(app: &ui::App, state: &Shared, tasks: &mpsc::UnboundedSender<Task>) {
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

            if !cached {
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
            if content.is_empty() {
                return;
            }

            let mut state_ref = state.borrow_mut();
            let channel = state_ref.store.selected_channel.clone();
            if channel.is_empty() || !state_ref.store.can_send_in(&channel) {
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
                created_at: time::OffsetDateTime::now_utc()
                    .format(&time::format_description::well_known::Rfc3339)
                    .unwrap_or_default(),
                pending: true,
                ..Default::default()
            });
            drop(state_ref);

            app.set_draft(SharedString::new());
            let _ = tasks.send(Task::Send {
                channel,
                temp_id,
                content,
                reply_to,
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
        let state = state.clone();
        let tasks = tasks.clone();
        move |message, emoji| {
            if emoji.is_empty() {
                // The picker is not built yet; an empty emoji means the
                // button was pressed with nothing chosen.
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

    app.on_quit(move || {
        let _ = slint::quit_event_loop();
    });
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
