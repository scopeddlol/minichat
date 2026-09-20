//! End-to-end check against a running instance.
//!
//! `--live <origin> <username> <password> <out.png>` signs in for real,
//! connects the gateway, waits for READY, loads the first channel and renders
//! what came back. No window, no display.
//!
//! This exists because "it compiles" and "it renders demo data" are both a
//! long way from "it talks to the server": every wire type, the gateway
//! handshake, the READY payload's shape and the permission bits are only
//! actually checked by a real instance answering.

use std::time::Duration;

use tokio::sync::mpsc;

use crate::api;
use crate::api::types::ReadyPayload;
use crate::gateway;
use crate::store::Store;

pub struct Report {
    pub instance: String,
    pub me: String,
    pub channels: usize,
    pub categories: usize,
    pub members: usize,
    pub roles: usize,
    pub messages: usize,
    pub permissions: u64,
    pub channel_name: String,
}

/// Sign in, connect, and fill a store from what the server sends.
pub async fn fetch(
    origin: &str,
    username: &str,
    password: &str,
) -> Result<(Store, Report), String> {
    let client = api::Client::new(origin).map_err(|e| e.to_string())?;

    let meta = client.meta().await.map_err(|e| format!("meta: {e}"))?;
    let auth = client
        .login(username, password)
        .await
        .map_err(|e| format!("login: {e}"))?;
    client.set_token(Some(auth.token.clone()));

    let (update_tx, mut update_rx) = mpsc::unbounded_channel();
    let (command_tx, command_rx) = mpsc::unbounded_channel();
    tokio::spawn(gateway::run(
        api::gateway_url(origin),
        auth.token,
        update_tx,
        command_rx,
    ));

    // Wait for READY, but not forever: a gateway that never identifies is a
    // failure to report, not a hang to sit in.
    let ready: ReadyPayload = tokio::time::timeout(Duration::from_secs(20), async {
        while let Some(update) = update_rx.recv().await {
            if let gateway::Update::Event(frame) = update {
                if frame.event == "READY" {
                    return serde_json::from_value(frame.data)
                        .map_err(|e| format!("READY did not parse: {e}"));
                }
            }
        }
        Err("the gateway closed before READY".to_string())
    })
    .await
    .map_err(|_| "timed out waiting for READY".to_string())??;

    let mut store = Store::default();
    store.apply_ready(ready);

    let channel = store
        .first_text_channel()
        .ok_or("the instance has no text channel this account can see")?;
    store.selected_channel = channel.clone();

    let messages = client
        .messages(&channel, None, None, 50)
        .await
        .map_err(|e| format!("messages: {e}"))?;
    store.set_messages(&channel, messages);

    let report = Report {
        instance: meta.name.clone(),
        me: store.me.member.name().to_string(),
        channels: store.channels.len(),
        categories: store.categories.len(),
        members: store.members.len(),
        roles: store.roles.len(),
        messages: store.messages_in(&channel).len(),
        permissions: store.permissions,
        channel_name: store
            .channel(&channel)
            .map(|c| c.name.clone())
            .unwrap_or_default(),
    };

    let _ = command_tx.send(gateway::Command::Close);
    Ok((store, report))
}

/// Render a store into an `App`, the way `refresh` does at run time.
///
/// Deliberately a separate, smaller path than `app::refresh`: this one has no
/// settings, no image cache to prime and no callbacks, so it shows what the
/// server's own data looks like rather than what a configured client does
/// with it.
pub fn populate(app: &crate::ui::App, store: &Store, palette: &crate::theme::Palette) {
    use crate::ui;
    use crate::view;
    use slint::{ModelRc, SharedString, VecModel};

    let accent = palette.accent;
    let images = crate::images::Cache::new();
    let measurer = crate::text::layout::Measurer::new();
    let selected = store.selected_channel.clone();

    app.set_screen(ui::Screen::Chat);
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
    app.set_my_name(store.me.member.name().into());
    app.set_my_initials(crate::format::initials(store.me.member.name()).into());
    app.set_my_avatar_fallback(
        crate::format::avatar_colour(&store.me.member.id, accent).to_slint(),
    );
    app.set_my_presence("online".into());
    app.set_can_admin(crate::perms::can_see_admin_panel(store.permissions));
    app.set_can_invite(crate::perms::can(
        store.permissions,
        crate::perms::CREATE_INVITES,
    ));
    app.set_selected_channel(selected.clone().into());
    app.set_members_open(true);
    app.set_member_count(store.members.len() as i32);
    app.set_loading_messages(false);

    if let Some(channel) = store.channel(&selected) {
        app.set_channel_name(channel.name.clone().into());
        app.set_channel_topic(channel.topic.clone().into());
    }
    app.set_can_send(store.can_send_in(&selected));

    let to_channel = |channel: &crate::api::types::Channel| ui::ChannelRow {
        id: channel.id.clone().into(),
        name: channel.name.clone().into(),
        kind: match channel.kind {
            crate::api::types::ChannelKind::Voice => "voice",
            crate::api::types::ChannelKind::Announcement => "announcement",
            crate::api::types::ChannelKind::Text => "text",
        }
        .into(),
        description: channel.description.clone().into(),
        emoji: channel.emoji.clone().into(),
        private: channel.is_private,
        unread: store.unread.get(&channel.id).copied().unwrap_or(0) as i32,
        mentions: store.mentions.get(&channel.id).copied().unwrap_or(0) as i32,
        voice_count: 0,
        voice_members: ModelRc::new(VecModel::from(Vec::<ui::VoiceMember>::new())),
    };

    app.set_loose_channels(ModelRc::new(VecModel::from(
        store
            .channels
            .iter()
            .filter(|c| c.category_id.is_none())
            .map(to_channel)
            .collect::<Vec<_>>(),
    )));
    app.set_categories(ModelRc::new(VecModel::from(
        store
            .categories
            .iter()
            .map(|category| ui::CategoryRow {
                id: category.id.clone().into(),
                name: category.name.to_uppercase().into(),
                collapsed: false,
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
            .collect::<Vec<_>>(),
    )));

    let members: Vec<ui::MemberRow> = store
        .members
        .iter()
        .map(|member| ui::MemberRow {
            id: member.id.clone().into(),
            name: member.name().into(),
            status: member.custom_status.clone().into(),
            presence: format!("{:?}", member.presence).to_lowercase().into(),
            avatar: slint::Image::default(),
            avatar_fallback: crate::format::avatar_colour(&member.id, accent).to_slint(),
            initials: crate::format::initials(member.name()).into(),
            role_colour: view::role_colour(&member.roles, &store.roles, palette.text).to_slint(),
            badge: SharedString::new(),
            operator: member.is_operator,
            speaking: false,
            muted: false,
            deafened: false,
        })
        .collect();
    app.set_member_groups(ModelRc::new(VecModel::from(vec![ui::MemberGroup {
        name: format!("MEMBERS — {}", members.len()).into(),
        members: ModelRc::new(VecModel::from(members)),
    }])));

    app.set_messages(ModelRc::new(VecModel::from(
        view::build_messages_with_roles(
            store.messages_in(&selected),
            &store.members,
            &store.emojis,
            &store.roles,
            &store.me.member.id,
            view::MessageContext {
                width: 566.0,
                accent,
                text: palette.text,
                measurer: &measurer,
                first_unread: None,
                images: &images,
            },
        ),
    )));
}
