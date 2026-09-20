//! Turning stored data into what the UI draws.
//!
//! Everything expensive happens here, once per change, rather than in a
//! binding that reruns on every repaint: parsing a message body, laying it
//! out, resolving a member's colour, picking an avatar out of the cache.

use slint::{ModelRc, SharedString, VecModel};

use crate::api::types::{self, Attachment, Emoji, Member, Message, Role};
use crate::images::Cache;
use crate::text::{layout, markdown};
use crate::theme::Rgb;
use crate::ui;

/// The widths the body is laid out into, and what it needs to resolve.
pub struct MessageContext<'a> {
    /// The column available for the body, in pixels.
    pub width: f32,
    pub accent: Rgb,
    pub measurer: &'a layout::Measurer,
    /// The first message that arrived since the channel was last read, which
    /// gets the "NEW" rule above it.
    pub first_unread: Option<&'a str>,
    pub images: &'a Cache,
}

/// The kind codes the markup switches on. Kept next to the enum they mirror
/// so the two cannot drift apart unnoticed.
fn kind_code(kind: markdown::Kind) -> i32 {
    match kind {
        markdown::Kind::Plain => 0,
        markdown::Kind::Link => 1,
        markdown::Kind::Mention => 2,
        markdown::Kind::MentionSelf => 3,
        markdown::Kind::Emoji => 4,
    }
}

fn decoration_code(kind: layout::DecorationKind) -> i32 {
    match kind {
        layout::DecorationKind::CodeBlock => 0,
        layout::DecorationKind::CodeChip => 1,
        layout::DecorationKind::QuoteBar => 2,
        layout::DecorationKind::Mention => 3,
        layout::DecorationKind::MentionSelf => 4,
        layout::DecorationKind::Spoiler => 5,
    }
}

/// The colour a member's name renders in: their highest hoisted role's
/// colour, or the default text colour when they have none.
pub fn role_colour(member_roles: &[String], roles: &[Role], fallback: Rgb) -> Rgb {
    roles
        .iter()
        .filter(|role| member_roles.iter().any(|id| id == &role.id))
        .filter(|role| role.color.is_some())
        // Highest position wins, which is how the hierarchy is ordered.
        .max_by_key(|role| role.position)
        .and_then(|role| role.color.as_deref())
        .and_then(Rgb::parse)
        .unwrap_or(fallback)
}

fn attachment_kind(file: &Attachment) -> &'static str {
    if file.is_image() {
        "image"
    } else if file.is_video() {
        "video"
    } else if file.is_audio() {
        "audio"
    } else {
        "file"
    }
}

/// How large an image attachment is drawn.
///
/// The web client caps a preview at 400×300 and keeps the aspect ratio; an
/// image without stated dimensions gets the cap, because guessing a shape and
/// being wrong reflows the whole list once it loads.
fn preview_size(file: &Attachment) -> (f32, f32) {
    const MAX_W: f32 = 400.0;
    const MAX_H: f32 = 300.0;
    match (file.width, file.height) {
        (Some(w), Some(h)) if w > 0 && h > 0 => {
            let scale = (MAX_W / w as f32).min(MAX_H / h as f32).min(1.0);
            ((w as f32 * scale).round(), (h as f32 * scale).round())
        }
        _ => (MAX_W, MAX_H),
    }
}

fn build_attachments(files: &[Attachment], images: &Cache) -> (Vec<ui::AttachmentRow>, f32) {
    let mut rows = Vec::with_capacity(files.len());
    let mut height = 0.0f32;

    for file in files {
        let kind = attachment_kind(file);
        let (width, preview_height) = preview_size(file);
        let picture = images.get_or_blank(&file.url);
        let loaded = picture.size().width > 0;

        // A file card is a fixed height; an image takes its preview's.
        let row_height = if kind == "image" && loaded { preview_height } else { 58.0 };
        height += row_height;
        if rows.len() + 1 < files.len() {
            height += 8.0;
        }

        rows.push(ui::AttachmentRow {
            id: file.id.clone().into(),
            filename: file.filename.clone().into(),
            size: crate::format::bytes(file.size).into(),
            url: file.url.clone().into(),
            kind: kind.into(),
            picture,
            preview_width: width,
            preview_height,
        });
    }

    (rows, height)
}

fn build_reactions(message: &Message, emojis: &[Emoji], images: &Cache) -> Vec<ui::ReactionChip> {
    message
        .reactions
        .iter()
        .map(|group| {
            // A reaction is either a unicode emoji or the name of a custom
            // one; only the latter has a picture.
            let picture = emojis
                .iter()
                .find(|e| e.name == group.emoji)
                .map(|e| images.get_or_blank(&e.url))
                .unwrap_or_default();
            ui::ReactionChip {
                emoji: group.emoji.clone().into(),
                count: group.count as i32,
                me: group.me,
                picture,
            }
        })
        .collect()
}

/// A short, single-line version of a message, for a reply preview.
pub fn excerpt(message: &Message, limit: usize) -> String {
    let flattened: String = message
        .content
        .split('\n')
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    if flattened.is_empty() && !message.attachments.is_empty() {
        return format!("{} attachment(s)", message.attachments.len());
    }
    crate::format::elide(&flattened, limit)
}

/// Build the rows for a channel's messages.
pub fn build_messages(
    messages: &[Message],
    members: &[Member],
    emojis: &[Emoji],
    me_id: &str,
    context: MessageContext<'_>,
) -> Vec<ui::MessageRow> {
    build_messages_with_roles(messages, members, emojis, &[], me_id, context)
}

pub fn build_messages_with_roles(
    messages: &[Message],
    members: &[Member],
    emojis: &[Emoji],
    roles: &[Role],
    me_id: &str,
    context: MessageContext<'_>,
) -> Vec<ui::MessageRow> {
    let markdown_context = markdown::Context {
        members,
        emojis,
        me_id,
    };
    let default_name_colour = Rgb::new(0xed, 0xed, 0xee);

    messages
        .iter()
        .enumerate()
        .map(|(index, message)| {
            let previous = index.checked_sub(1).and_then(|i| messages.get(i));
            let grouped = types::should_group(previous, message);

            // A divider goes above the first message of each day.
            let day_divider = match previous {
                Some(previous)
                    if crate::format::same_day(&previous.created_at, &message.created_at) =>
                {
                    String::new()
                }
                _ => crate::format::day_divider(&message.created_at),
            };

            let blocks = markdown::parse(&message.content, &markdown_context);
            let laid = layout::layout(&blocks, context.width, context.measurer);

            let runs: Vec<ui::MessageRun> = laid
                .runs
                .iter()
                .map(|run| ui::MessageRun {
                    text: run.text.clone().into(),
                    x: run.x,
                    y: run.y,
                    width: run.width,
                    height: run.height,
                    bold: run.style.bold,
                    italic: run.style.italic,
                    underline: run.style.underline,
                    strike: run.style.strike,
                    code: run.style.code,
                    spoiler: run.style.spoiler,
                    size: run.size,
                    kind: kind_code(run.style.kind),
                    target: run.target.clone().into(),
                    picture: if run.emoji_url.is_empty() {
                        slint::Image::default()
                    } else {
                        context.images.get_or_blank(&run.emoji_url)
                    },
                })
                .collect();

            let decorations: Vec<ui::MessageDecoration> = laid
                .decorations
                .iter()
                .map(|spec| ui::MessageDecoration {
                    x: spec.x,
                    y: spec.y,
                    width: spec.width,
                    height: spec.height,
                    kind: decoration_code(spec.kind),
                })
                .collect();

            let author = message.author.as_ref();
            let author_id = author.map(|a| a.id.clone()).unwrap_or_default();
            let member = members.iter().find(|m| Some(&m.id) == author.map(|a| &a.id));

            let name_colour = member
                .map(|m| role_colour(&m.roles, roles, default_name_colour))
                .unwrap_or(default_name_colour);

            let avatar_url = author
                .and_then(|a| a.avatar_url.clone())
                .or_else(|| member.and_then(|m| m.avatar_url.clone()))
                .unwrap_or_default();

            let reply = message.reply_to_id.as_ref().and_then(|id| {
                messages.iter().find(|m| &m.id == id)
            });

            let (attachments, attachments_height) =
                build_attachments(&message.attachments, context.images);

            ui::MessageRow {
                id: message.id.clone().into(),
                author_id: author_id.clone().into(),
                author_name: message.author_name().into(),
                author_colour: name_colour.to_slint(),
                timestamp: crate::format::timestamp(&message.created_at).into(),
                avatar: context.images.get_or_blank(&avatar_url),
                avatar_fallback: crate::format::avatar_colour(&author_id, context.accent)
                    .to_slint(),
                initials: crate::format::initials(message.author_name()).into(),
                grouped,
                pinned: message.pinned,
                edited: message.edited_at.is_some(),
                pending: message.pending,
                failed: message.failed,
                system: message.is_system(),
                system_text: message.content.clone().into(),
                day_divider: day_divider.into(),
                unread_marker: context.first_unread == Some(message.id.as_str()),
                runs: ModelRc::new(VecModel::from(runs)),
                decorations: ModelRc::new(VecModel::from(decorations)),
                body_height: laid.height,
                reactions: ModelRc::new(VecModel::from(build_reactions(
                    message,
                    emojis,
                    context.images,
                ))),
                attachments: ModelRc::new(VecModel::from(attachments)),
                attachments_height,
                reply_author: reply
                    .map(|m| SharedString::from(m.author_name()))
                    .unwrap_or_default(),
                reply_excerpt: reply
                    .map(|m| SharedString::from(excerpt(m, 80)))
                    .unwrap_or_default(),
                jumbo: markdown::is_jumbo(&message.content),
            }
        })
        .collect()
}

/// Every image URL a set of messages will want, so they can be fetched in one
/// pass rather than discovered one repaint at a time.
pub fn referenced_images(messages: &[Message], members: &[Member], emojis: &[Emoji]) -> Vec<String> {
    let mut urls = Vec::new();

    for message in messages {
        if let Some(url) = message.author.as_ref().and_then(|a| a.avatar_url.as_ref()) {
            urls.push(url.clone());
        }
        for file in &message.attachments {
            if file.is_image() {
                urls.push(file.url.clone());
            }
        }
        for reaction in &message.reactions {
            if let Some(emoji) = emojis.iter().find(|e| e.name == reaction.emoji) {
                urls.push(emoji.url.clone());
            }
        }
    }
    for member in members {
        if let Some(url) = &member.avatar_url {
            urls.push(url.clone());
        }
    }

    urls.sort();
    urls.dedup();
    urls.retain(|url| !url.is_empty());
    urls
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::types::{MessageAuthor, ReactionGroup};

    fn message(id: &str, author: &str, content: &str, at: &str) -> Message {
        Message {
            id: id.into(),
            author: Some(MessageAuthor {
                id: author.into(),
                username: author.into(),
                display_name: author.into(),
                ..Default::default()
            }),
            content: content.into(),
            created_at: at.into(),
            ..Default::default()
        }
    }

    fn build(messages: &[Message]) -> Vec<ui::MessageRow> {
        let measurer = layout::Measurer::new();
        let images = Cache::new();
        build_messages(
            messages,
            &[],
            &[],
            "me",
            MessageContext {
                width: 600.0,
                accent: Rgb::new(0x5b, 0x6e, 0xe8),
                measurer: &measurer,
                first_unread: None,
                images: &images,
            },
        )
    }

    #[test]
    fn consecutive_messages_from_one_author_group() {
        let rows = build(&[
            message("1", "ada", "first", "2026-03-04 10:00:00"),
            message("2", "ada", "second", "2026-03-04 10:00:30"),
            message("3", "grace", "third", "2026-03-04 10:01:00"),
        ]);
        assert!(!rows[0].grouped, "the first can never be grouped");
        assert!(rows[1].grouped);
        assert!(!rows[2].grouped, "a different author breaks the group");
    }

    #[test]
    fn a_day_divider_appears_once_per_day() {
        let rows = build(&[
            message("1", "ada", "a", "2026-03-04 10:00:00"),
            message("2", "ada", "b", "2026-03-04 10:00:10"),
            message("3", "ada", "c", "2026-03-05 09:00:00"),
        ]);
        assert!(!rows[0].day_divider.is_empty(), "the first starts a day");
        assert!(rows[1].day_divider.is_empty());
        assert!(!rows[2].day_divider.is_empty(), "a new day needs a divider");
    }

    #[test]
    fn a_reply_carries_the_quoted_author_and_an_excerpt() {
        let mut second = message("2", "grace", "answering", "2026-03-04 10:05:00");
        second.reply_to_id = Some("1".into());
        let rows = build(&[
            message("1", "ada", "the original question", "2026-03-04 10:00:00"),
            second,
        ]);
        assert_eq!(rows[1].reply_author, "ada");
        assert_eq!(rows[1].reply_excerpt, "the original question");
        // A reply always starts its own group, even from the same author.
        assert!(!rows[1].grouped);
    }

    #[test]
    fn the_highest_hoisted_role_colours_the_name() {
        let roles = vec![
            Role {
                id: "r1".into(),
                color: Some("#34d399".into()),
                position: 1,
                ..Default::default()
            },
            Role {
                id: "r2".into(),
                color: Some("#f87171".into()),
                position: 5,
                ..Default::default()
            },
        ];
        let member_roles = vec!["r1".to_string(), "r2".to_string()];
        assert_eq!(
            role_colour(&member_roles, &roles, Rgb::new(0, 0, 0)),
            Rgb::new(0xf8, 0x71, 0x71)
        );
        // No roles at all falls back rather than rendering invisible.
        assert_eq!(
            role_colour(&[], &roles, Rgb::new(1, 2, 3)),
            Rgb::new(1, 2, 3)
        );
    }

    #[test]
    fn a_preview_keeps_its_aspect_ratio_within_the_cap() {
        let wide = Attachment {
            width: Some(1600),
            height: Some(900),
            content_type: "image/png".into(),
            ..Default::default()
        };
        let (w, h) = preview_size(&wide);
        assert!(w <= 400.0 && h <= 300.0);
        assert!((w / h - 1600.0 / 900.0).abs() < 0.02, "{w}x{h}");

        // Unknown dimensions take the cap rather than guessing.
        let unknown = Attachment {
            content_type: "image/png".into(),
            ..Default::default()
        };
        assert_eq!(preview_size(&unknown), (400.0, 300.0));
    }

    #[test]
    fn every_referenced_image_is_listed_once() {
        let mut message = message("1", "ada", "hi", "2026-03-04 10:00:00");
        message.author.as_mut().unwrap().avatar_url = Some("/a.png".into());
        message.attachments = vec![Attachment {
            url: "/b.png".into(),
            content_type: "image/png".into(),
            ..Default::default()
        }];
        message.reactions = vec![ReactionGroup {
            emoji: "party".into(),
            count: 1,
            me: false,
        }];
        let emojis = vec![Emoji {
            name: "party".into(),
            url: "/c.png".into(),
            ..Default::default()
        }];

        let urls = referenced_images(&[message.clone(), message], &[], &emojis);
        assert_eq!(urls, vec!["/a.png", "/b.png", "/c.png"]);
    }

    #[test]
    fn an_excerpt_flattens_newlines_and_notes_bare_attachments() {
        let multi = Message {
            content: "first line\n\nsecond line".into(),
            ..Default::default()
        };
        assert_eq!(excerpt(&multi, 80), "first line second line");

        let bare = Message {
            attachments: vec![Attachment::default()],
            ..Default::default()
        };
        assert_eq!(excerpt(&bare, 80), "1 attachment(s)");
    }
}
