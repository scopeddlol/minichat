//! Representative data for looking at the interface without a server.
//!
//! `--screenshot` renders this, so a layout change can be reviewed the way a
//! member would see it. It is also what the layout tests assert against, so
//! the awkward cases live here on purpose: a long wrapping paragraph, a code
//! block, a quote, a mention, a reply, a grouped run of messages, a member
//! with a role colour and a badge.

use slint::{Model, ModelRc, SharedString, VecModel};

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
        voice_members: ModelRc::new(VecModel::from(Vec::<ui::VoiceMember>::new())),
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
    // Two people sitting in the lounge, one of them talking, so the nested
    // voice rows and the speaking ring are both exercised.
    let seat =
        |id: &str, name: &str, muted: bool, deafened: bool, speaking: bool| ui::VoiceMember {
            id: id.into(),
            name: name.into(),
            avatar: slint::Image::default(),
            avatar_fallback: crate::format::avatar_colour(id, accent).to_slint(),
            initials: crate::format::initials(name).into(),
            muted,
            deafened,
            streaming: false,
            video: false,
            speaking,
        };
    let lounge_seats = vec![
        seat("u2", "Grace Hopper", false, false, true),
        seat("u3", "Alan Turing", true, false, false),
    ];

    let voice = ui::CategoryRow {
        id: "c2".into(),
        name: "VOICE".into(),
        collapsed: false,
        private: false,
        channels: ModelRc::new(VecModel::from(vec![
            ui::ChannelRow {
                voice_count: 2,
                voice_members: ModelRc::new(VecModel::from(lounge_seats)),
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

    // Connected to voice, so the sidebar's panel is in the frame.
    app.set_voice_live(true);
    app.set_voice_channel("Lounge".into());
    app.set_voice_summary("Voice connected".into());
    app.set_voice_can_speak(true);
    app.set_voice_can_screen_share(true);
}

/// Open one of the dialogs over the demo, for reviewing it headlessly.
pub fn show_overlay(app: &ui::App, which: &str) {
    match which {
        "settings" => {
            app.set_status_draft("building the native client".into());
            app.set_my_username("ada".into());
            app.set_instance_version("0.5.0".into());
            app.set_theme_choice(0);
            // The demo shows the control a Windows or macOS build gets; a
            // Linux build reports the truth through `hotkeys::available()`.
            app.set_hotkeys_available(true);
            app.set_hotkeys_enabled(true);
            app.set_hotkey_summary("Push to talk F8 \u{b7} Mute F9 \u{b7} Deafen F10".into());
            app.set_overlay(ui::Overlay::Settings);
        }
        "profile" => {
            let members = app.get_member_groups();
            if let Some(group) = members.row_data(0) {
                if let Some(member) = group.members.row_data(1) {
                    app.set_profile_member(member);
                }
            }
            app.set_profile_pronouns("she/her".into());
            app.set_profile_bio(
                "Compiler person. Wrote the first one, more or less. Ask me about \
                 COBOL, or about why a nanosecond is 30cm of wire."
                    .into(),
            );
            app.set_profile_game("Zachtronics, mostly".into());
            app.set_profile_joined("9 December 1906".into());
            app.set_overlay(ui::Overlay::Profile);
        }
        "emoji" => {
            let entries: Vec<ui::EmojiEntry> = crate::emoji::COMMON
                .iter()
                .map(|(glyph, name)| ui::EmojiEntry {
                    name: (*name).into(),
                    url: SharedString::new(),
                    picture: slint::Image::default(),
                    glyph: (*glyph).into(),
                })
                .collect();
            let rows: Vec<ui::EmojiRow> = entries
                .chunks(8)
                .map(|chunk| ui::EmojiRow {
                    entries: ModelRc::new(VecModel::from(chunk.to_vec())),
                })
                .collect();
            app.set_emoji_unicode(ModelRc::new(VecModel::from(rows)));
            app.set_pointer_x(620.0);
            app.set_pointer_y(660.0);
            app.set_overlay(ui::Overlay::Emoji);
        }
        "menu" => {
            let item = |id: &str, label: &str, danger: bool, gap: bool| ui::MenuItem {
                id: id.into(),
                label: label.into(),
                icon: SharedString::new(),
                danger,
                separator_before: gap,
            };
            app.set_menu_items(ModelRc::new(VecModel::from(vec![
                item("react", "Add reaction", false, false),
                item("reply", "Reply", false, false),
                item("copy", "Copy text", false, false),
                item("pin", "Pin message", false, true),
                item("edit", "Edit", false, true),
                item("delete", "Delete", true, false),
            ])));
            app.set_pointer_x(520.0);
            app.set_pointer_y(300.0);
            app.set_overlay(ui::Overlay::Menu);
        }
        "confirm" => {
            app.set_confirm_title("Delete message?".into());
            app.set_confirm_body("This cannot be undone.".into());
            app.set_confirm_label("Delete".into());
            app.set_overlay(ui::Overlay::Confirm);
        }
        "inbox" => {
            let peer = |id: &str, name: &str, excerpt: &str, when: &str, unread: i32| {
                ui::ConversationRow {
                    id: id.into(),
                    name: name.into(),
                    excerpt: excerpt.into(),
                    timestamp: when.into(),
                    avatar: slint::Image::default(),
                    avatar_fallback: crate::format::avatar_colour(
                        id,
                        crate::theme::Rgb::new(0x5b, 0x6e, 0xe8),
                    )
                    .to_slint(),
                    initials: crate::format::initials(name).into(),
                    presence: "online".into(),
                    unread,
                }
            };
            app.set_inbox_open(true);
            app.set_conversations(ModelRc::new(VecModel::from(vec![
                peer(
                    "c1",
                    "Grace Hopper",
                    "so what we measure is what gets painted",
                    "2h ago",
                    2,
                ),
                peer(
                    "c2",
                    "Alan Turing",
                    "that is the part I was worried about",
                    "yesterday",
                    0,
                ),
                peer("c3", "Katherine Johnson", "shipping it", "Tuesday", 0),
            ])));
            app.set_selected_conversation("c1".into());
            app.set_peer_name("Grace Hopper".into());
            app.set_peer_initials("GH".into());
            app.set_peer_presence("online".into());
            app.set_peer_avatar_fallback(
                crate::format::avatar_colour("c1", crate::theme::Rgb::new(0x5b, 0x6e, 0xe8))
                    .to_slint(),
            );
            app.set_direct_messages(app.get_messages());
            app.set_loading_direct(false);
        }
        "admin" => {
            app.set_screen(ui::Screen::Admin);
            app.set_admin_section("overview".into());
            let section = |id: &str, label: &str| ui::AdminSection {
                id: id.into(),
                label: label.into(),
            };
            app.set_admin_sections(ModelRc::new(VecModel::from(vec![
                section("overview", "Overview"),
                section("instance", "Instance"),
                section("members", "Members"),
                section("roles", "Roles"),
                section("invites", "Invites"),
                section("bans", "Bans"),
                section("audit", "Audit log"),
            ])));
            let tile = |label: &str, value: &str, detail: &str| ui::StatTile {
                label: label.into(),
                value: value.into(),
                detail: detail.into(),
            };
            let tiles = [
                tile("MEMBERS", "5", "4 online"),
                tile("MESSAGES", "1,284", "96 this week"),
                tile("CHANNELS", "6", ""),
                tile("STORAGE", "41 MB", ""),
                tile("JOINED", "2", "in the last week"),
                tile("IN VOICE", "2", ""),
            ];
            app.set_admin_tiles(ModelRc::new(VecModel::from(
                tiles
                    .chunks(4)
                    .map(|chunk| ui::StatRow {
                        tiles: ModelRc::new(VecModel::from(chunk.to_vec())),
                    })
                    .collect::<Vec<_>>(),
            )));
        }
        "admin-members" => {
            show_overlay(app, "admin");
            app.set_admin_section("members".into());
            let groups = app.get_member_groups();
            let mut rows: Vec<ui::AdminMemberRow> = Vec::new();
            for g in 0..groups.row_count() {
                let Some(group) = groups.row_data(g) else {
                    continue;
                };
                for m in 0..group.members.row_count() {
                    let Some(member) = group.members.row_data(m) else {
                        continue;
                    };
                    rows.push(ui::AdminMemberRow {
                        id: member.id.clone(),
                        name: member.name.clone(),
                        username: member.name.to_lowercase().replace(' ', "").into(),
                        avatar: member.avatar.clone(),
                        avatar_fallback: member.avatar_fallback,
                        initials: member.initials.clone(),
                        presence: member.presence.clone(),
                        roles: if m == 0 {
                            "Maintainer".into()
                        } else {
                            SharedString::new()
                        },
                        joined: "4 Mar".into(),
                        operator: member.operator,
                        can_kick: !member.operator,
                        can_ban: !member.operator,
                    });
                }
            }
            app.set_admin_members(ModelRc::new(VecModel::from(rows)));
        }
        "ringing" => {
            let groups = app.get_member_groups();
            if let Some(member) = groups.row_data(0).and_then(|g| g.members.row_data(1)) {
                app.set_call_peer(member);
            }
            app.set_call_phase("ringing".into());
        }
        "in-call" => {
            let groups = app.get_member_groups();
            if let Some(member) = groups.row_data(0).and_then(|g| g.members.row_data(1)) {
                app.set_call_peer(member);
            }
            app.set_call_phase("connected".into());
            app.set_call_duration("3:41".into());
            app.set_voice_can_speak(true);
        }
        "friends" => {
            let groups = app.get_member_groups();
            let person = |group: usize, row: usize| {
                groups
                    .row_data(group)
                    .and_then(|g| g.members.row_data(row))
                    .unwrap_or_default()
            };
            app.set_inbox_open(true);
            app.set_inbox_tab(1);
            app.set_friend_requests(ModelRc::new(VecModel::from(vec![person(0, 3)])));
            app.set_friends(ModelRc::new(VecModel::from(vec![
                person(0, 1),
                person(0, 2),
            ])));
            app.set_blocked_members(ModelRc::new(VecModel::from(vec![person(1, 0)])));
        }
        // Rendered from a gradient rather than a file: what is being checked
        // is that the lightbox lays out over the window at all, and a demo
        // that needed an attachment on disk would not run in CI.
        "lightbox" => {
            let (width, height) = (960u32, 600u32);
            let mut pixels = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(width, height);
            let stride = width as usize;
            for (index, pixel) in pixels.make_mut_slice().iter_mut().enumerate() {
                let x = (index % stride) as f32 / width as f32;
                let y = (index / stride) as f32 / height as f32;
                *pixel = slint::Rgba8Pixel {
                    r: (40.0 + 150.0 * x) as u8,
                    g: (60.0 + 90.0 * y) as u8,
                    b: (120.0 + 110.0 * (1.0 - x)) as u8,
                    a: 255,
                };
            }
            app.set_lightbox_picture(slint::Image::from_rgba8(pixels));
            app.set_lightbox_filename("flow-layout.png".into());
            app.set_lightbox_caption("960 \u{00d7} 600 \u{00b7} 214 KB".into());
            app.set_overlay(ui::Overlay::Lightbox);
        }

        "forward" => {
            let rows = app.get_messages();
            if let Some(row) = rows.row_data(3) {
                app.set_forward_author(row.author_name.clone());
            }
            app.set_forward_body(
                "Both. Here's the shape of it: let laid = layout(&blocks, width, &measurer);"
                    .into(),
            );
            app.set_forward_targets(app.get_loose_channels());
            app.set_forward_target("welcome".into());
            app.set_overlay(ui::Overlay::Forward);
        }
        "scrolled" => {
            // Scrolled back, so the jump-to-present button is in the frame.
            app.set_scroll_distance(420.0);
        }
        "attaching" => {
            let file = |id: &str, name: &str, size: &str, kind: &str| ui::PendingAttachment {
                id: id.into(),
                filename: name.into(),
                size: size.into(),
                kind: kind.into(),
                picture: slint::Image::default(),
            };
            app.set_pending_attachments(ModelRc::new(VecModel::from(vec![
                file("a1", "flow-layout.png", "184 KB", "image"),
                file("a2", "profile.json", "2.1 KB", "file"),
            ])));
            app.set_draft("Here's what it looks like now".into());
        }
        "editing" => {
            // The third message, opened for editing with its source text.
            let rows = app.get_messages();
            if let Some(row) = rows.row_data(2) {
                app.set_editing_id(row.id.clone());
                app.set_edit_draft(
                    "Nice. Does it handle `inline code` and links like \
                     https://example.com/a-fairly-long-url-that-gets-elided-eventually ?"
                        .into(),
                );
            }
        }
        "search" => {
            app.set_side_panel(ui::SidePanelKind::Search);
            app.set_panel_query("flow layout".into());
            app.set_panel_hits(ModelRc::new(VecModel::from(vec![ui::SearchHit {
                id: "m1".into(),
                channel: "general".into(),
                author: "Grace Hopper".into(),
                excerpt: "Morning all. I've pushed the flow layout branch — message bodies \
                          now wrap properly with inline formatting."
                    .into(),
                timestamp: "2h ago".into(),
                avatar: slint::Image::default(),
                avatar_fallback: crate::format::avatar_colour(
                    "u2",
                    crate::theme::Rgb::new(0x5b, 0x6e, 0xe8),
                )
                .to_slint(),
                initials: "GH".into(),
            }])));
        }
        _ => {}
    }
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
