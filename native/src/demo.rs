//! Representative data for looking at the interface without a server.
//!
//! `--screenshot` renders this, so a layout change can be reviewed the way a
//! member would see it. It is also what the layout tests assert against, so
//! the awkward cases live here on purpose: a long wrapping paragraph, a code
//! block, a quote, a mention, a reply, a grouped run of messages, a member
//! with a role colour and a badge.

use slint::{ModelRc, SharedString, VecModel};

use crate::api::types::{Emoji, Member, Message, MessageAuthor, Presence};
use crate::text::layout;
use crate::theme::{Palette, Rgb};
use crate::ui;

fn member(id: &str, username: &str, display: &str, presence: Presence) -> Member {
    Member {
        id: id.into(),
        username: username.into(),
        display_name: display.into(),
        presence,
        ..Default::default()
    }
}

fn message(id: &str, author: &Member, content: &str, at: &str) -> Message {
    Message {
        id: id.into(),
        channel_id: "general".into(),
        author: Some(MessageAuthor {
            id: author.id.clone(),
            username: author.username.clone(),
            display_name: author.display_name.clone(),
            ..Default::default()
        }),
        content: content.into(),
        created_at: at.into(),
        ..Default::default()
    }
}

pub struct Demo {
    pub members: Vec<Member>,
    pub messages: Vec<Message>,
    pub emojis: Vec<Emoji>,
}

pub fn data() -> Demo {
    let ada = member("u1", "ada", "Ada Lovelace", Presence::Online);
    let grace = member("u2", "grace", "Grace Hopper", Presence::Online);
    let alan = member("u3", "alan", "Alan Turing", Presence::Idle);
    let katherine = member("u4", "katherine", "Katherine Johnson", Presence::Dnd);
    let radia = member("u5", "radia", "Radia Perlman", Presence::Offline);

    let messages = vec![
        message(
            "m1",
            &grace,
            "Morning all. I've pushed the **flow layout** branch — message \
             bodies now wrap properly with inline formatting, which was the \
             last thing blocking the native client.",
            "2026-09-19 09:12:00",
        ),
        message(
            "m2",
            &grace,
            "The measurement runs against the bundled font files, so what we \
             measure is what gets painted.",
            "2026-09-19 09:12:30",
        ),
        message(
            "m3",
            &ada,
            "Nice. Does it handle `inline code` and links like \
             https://example.com/a-fairly-long-url-that-gets-elided-eventually ?",
            "2026-09-19 09:15:00",
        ),
        message(
            "m4",
            &grace,
            "Both. Here's the shape of it:\n```rust\nlet laid = layout(&blocks, width, &measurer);\nfor run in laid.runs {\n    draw(run);\n}\n```",
            "2026-09-19 09:16:00",
        ),
        Message {
            reply_to_id: Some("m3".into()),
            ..message(
                "m5",
                &alan,
                "@ada it also does spoilers — ||like this one|| — and quotes.",
                "2026-09-19 09:18:00",
            )
        },
        message(
            "m6",
            &alan,
            "> the measurement runs against the bundled font files\n> so what we measure is what gets painted\n\nThat's the part I was worried about. Good.",
            "2026-09-19 09:19:00",
        ),
        Message {
            reactions: vec![
                crate::api::types::ReactionGroup { emoji: "🎉".into(), count: 4, me: true },
                crate::api::types::ReactionGroup { emoji: "🚀".into(), count: 2, me: false },
            ],
            ..message("m7", &katherine, "Shipping it. 🎉", "2026-09-19 09:41:00")
        },
    ];

    Demo {
        members: vec![ada, grace, alan, katherine, radia],
        messages,
        emojis: Vec::new(),
    }
}

/// Fill an `App` with the demo, so a screenshot shows a populated window.
pub fn populate(app: &ui::App, palette: &Palette) {
    let demo = data();
    let accent = palette.accent;

    app.set_screen(ui::Screen::Chat);
    app.set_instance_name("Lovelace Works".into());
    app.set_instance_tagline("a small workshop".into());
    app.set_instance_initial("L".into());
    app.set_channel_name("general".into());
    app.set_channel_topic("Anything and everything".into());
    app.set_member_count(demo.members.len() as i32);
    app.set_members_open(true);
    app.set_can_admin(true);
    app.set_can_invite(true);
    app.set_selected_channel("general".into());
    app.set_my_name("Ada Lovelace".into());
    app.set_my_status("building the native client".into());
    app.set_my_initials("AL".into());
    app.set_my_avatar_fallback(crate::format::avatar_colour("u1", accent).to_slint());
    app.set_typing_line("Grace Hopper is typing\u{2026}".into());
    app.set_draft("".into());

    // --- channels ------------------------------------------------------
    let channel = |id: &str, name: &str, kind: &str, unread: i32, mentions: i32| ui::ChannelRow {
        id: id.into(),
        name: name.into(),
        kind: kind.into(),
        description: SharedString::new(),
        emoji: SharedString::new(),
        private: false,
        unread,
        mentions,
        voice_count: 0,
    };

    let general = ui::CategoryRow {
        id: "c1".into(),
        name: "TEXT CHANNELS".into(),
        collapsed: false,
        private: false,
        channels: ModelRc::new(VecModel::from(vec![
            channel("general", "general", "text", 0, 0),
            channel("design", "design", "text", 3, 0),
            channel("releases", "releases", "announcement", 1, 1),
            ui::ChannelRow {
                private: true,
                ..channel("ops", "ops", "text", 0, 0)
            },
        ])),
    };
    let voice = ui::CategoryRow {
        id: "c2".into(),
        name: "VOICE".into(),
        collapsed: false,
        private: false,
        channels: ModelRc::new(VecModel::from(vec![
            ui::ChannelRow {
                voice_count: 2,
                ..channel("lounge", "Lounge", "voice", 0, 0)
            },
            channel("focus", "Focus room", "voice", 0, 0),
        ])),
    };
    app.set_categories(ModelRc::new(VecModel::from(vec![general, voice])));
    app.set_loose_channels(ModelRc::new(VecModel::from(vec![channel(
        "welcome", "welcome", "text", 0, 0,
    )])));
    app.set_direct_unread(2);

    // --- members -------------------------------------------------------
    let to_row = |m: &Member, role: Rgb, badge: &str, operator: bool| ui::MemberRow {
        id: m.id.clone().into(),
        name: m.name().into(),
        status: m.custom_status.clone().into(),
        presence: format!("{:?}", m.presence).to_lowercase().into(),
        avatar: slint::Image::default(),
        avatar_fallback: crate::format::avatar_colour(&m.id, accent).to_slint(),
        initials: crate::format::initials(m.name()).into(),
        role_colour: role.to_slint(),
        badge: badge.into(),
        operator,
        speaking: false,
        muted: false,
        deafened: false,
    };

    let online: Vec<ui::MemberRow> = demo
        .members
        .iter()
        .filter(|m| m.presence != Presence::Offline)
        .enumerate()
        .map(|(index, m)| {
            let (colour, badge, operator) = match index {
                0 => (accent, "OWNER", true),
                1 => (Rgb::new(0x34, 0xd3, 0x99), "DEV", false),
                _ => (palette.text, "", false),
            };
            to_row(m, colour, badge, operator)
        })
        .collect();
    let offline: Vec<ui::MemberRow> = demo
        .members
        .iter()
        .filter(|m| m.presence == Presence::Offline)
        .map(|m| to_row(m, palette.text_faint, "", false))
        .collect();

    let mut groups = vec![ui::MemberGroup {
        name: format!("ONLINE — {}", online.len()).into(),
        members: ModelRc::new(VecModel::from(online)),
    }];
    if !offline.is_empty() {
        groups.push(ui::MemberGroup {
            name: format!("OFFLINE — {}", offline.len()).into(),
            members: ModelRc::new(VecModel::from(offline)),
        });
    }
    app.set_member_groups(ModelRc::new(VecModel::from(groups)));

    // --- messages ------------------------------------------------------
    let measurer = layout::Measurer::new();
    let rows = crate::view::build_messages(
        &demo.messages,
        &demo.members,
        &demo.emojis,
        "u1",
        crate::view::MessageContext {
            // 1180 window − 264 sidebar − 236 members − 64 padding − 50 gutter.
            width: 566.0,
            accent,
            text: palette.text,
            measurer: &measurer,
            first_unread: Some("m7"),
            images: &crate::images::Cache::new(),
        },
    );
    app.set_messages(ModelRc::new(VecModel::from(rows)));
    app.set_loading_messages(false);
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_demo_covers_the_cases_the_layout_has_to_get_right() {
        let demo = super::data();
        let joined: String = demo.messages.iter().map(|m| m.content.as_str()).collect();
        assert!(joined.contains("**"), "no bold");
        assert!(joined.contains('`'), "no code");
        assert!(joined.contains("```"), "no code block");
        assert!(joined.contains("||"), "no spoiler");
        assert!(joined.contains("> "), "no quote");
        assert!(joined.contains("@ada"), "no mention");
        assert!(joined.contains("https://"), "no link");
        assert!(
            demo.messages.iter().any(|m| m.reply_to_id.is_some()),
            "no reply"
        );
        assert!(
            demo.messages.iter().any(|m| !m.reactions.is_empty()),
            "no reactions"
        );
        // Two consecutive messages from one author, close in time, so the
        // grouping path is exercised.
        assert!(crate::api::types::should_group(
            Some(&demo.messages[0]),
            &demo.messages[1]
        ));
    }
}
