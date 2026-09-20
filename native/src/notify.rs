//! Desktop notifications.
//!
//! The web client routes these through Web Push and a service worker, which
//! is what a page has to do. A native client talks to the desktop directly:
//! no VAPID keys, no subscription, and it works while the window is hidden.
//!
//! What gets shown follows the member's own preferences, which the server
//! already computes and sends in READY — so the rule here is a filter, not a
//! second implementation of the policy.

use crate::api::types::{Member, Message, NotificationMode, NotificationPreferences};

/// Whether a message should raise a notification.
///
/// `focused` is whether the window has focus *and* the message landed in what
/// the member is looking at; a notification for a message already on screen
/// is noise.
pub fn should_notify(
    message: &Message,
    me_id: &str,
    members: &[Member],
    preferences: &NotificationPreferences,
    focused: bool,
) -> bool {
    // Never for your own, and never for a system notice.
    if message.author_id() == Some(me_id) || message.is_system() {
        return false;
    }
    if focused {
        return false;
    }

    // A per-channel setting overrides the instance-wide one.
    let mode = preferences
        .channels
        .iter()
        .find(|entry| entry.channel_id == message.channel_id)
        .map(|entry| entry.mode)
        .unwrap_or(preferences.mode);

    match mode {
        NotificationMode::None => false,
        NotificationMode::All => true,
        NotificationMode::Mentions => mentions_me(message, me_id, members),
    }
}

/// Whether a message mentions the member, by username or through @everyone.
///
/// Parsed rather than trusting the server's MENTION_ADD, because that event
/// and the message can arrive in either order and a notification that fires
/// on the wrong one is either late or duplicated.
pub fn mentions_me(message: &Message, me_id: &str, members: &[Member]) -> bool {
    let Some(me) = members.iter().find(|m| m.id == me_id) else {
        return false;
    };
    let context = crate::text::markdown::Context {
        members,
        emojis: &[],
        me_id,
    };
    let blocks = crate::text::markdown::parse(&message.content, &context);
    let _ = &me;

    fn scan(inlines: &[crate::text::markdown::Inline], me_id: &str) -> bool {
        inlines.iter().any(|inline| match inline {
            crate::text::markdown::Inline::Mention { run, id } => {
                id == me_id
                    // @everyone and @here have no ID but are addressed to
                    // everyone, this member included.
                    || (id.is_empty()
                        && run.style.kind == crate::text::markdown::Kind::MentionSelf)
            }
            _ => false,
        })
    }

    blocks.iter().any(|block| match block {
        crate::text::markdown::Block::Paragraph(inlines) => scan(inlines, me_id),
        crate::text::markdown::Block::Quote(lines) => lines.iter().any(|line| scan(line, me_id)),
        crate::text::markdown::Block::Code { .. } => false,
    })
}

/// Show a notification. Failure is never worth interrupting anyone over.
pub fn show(title: &str, body: &str) {
    let body = crate::format::elide(body, 180);
    let result = notify_rust::Notification::new()
        .summary(title)
        .body(&body)
        .appname("MiniChat")
        .show();
    if let Err(error) = result {
        eprintln!("minichat: could not show a notification: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::types::{ChannelNotification, MessageAuthor};

    fn member(id: &str, username: &str) -> Member {
        Member {
            id: id.into(),
            username: username.into(),
            display_name: username.into(),
            ..Default::default()
        }
    }

    fn message(author: &str, content: &str) -> Message {
        Message {
            id: "m1".into(),
            channel_id: "general".into(),
            author: Some(MessageAuthor {
                id: author.into(),
                username: author.into(),
                ..Default::default()
            }),
            content: content.into(),
            ..Default::default()
        }
    }

    fn people() -> Vec<Member> {
        vec![member("me", "ada"), member("them", "grace")]
    }

    fn prefs(mode: NotificationMode) -> NotificationPreferences {
        NotificationPreferences {
            mode,
            channels: Vec::new(),
        }
    }

    #[test]
    fn my_own_messages_never_notify() {
        assert!(!should_notify(
            &message("me", "@ada hello"),
            "me",
            &people(),
            &prefs(NotificationMode::All),
            false
        ));
    }

    #[test]
    fn nothing_notifies_while_you_are_looking_at_it() {
        assert!(!should_notify(
            &message("them", "hello"),
            "me",
            &people(),
            &prefs(NotificationMode::All),
            true
        ));
    }

    #[test]
    fn mentions_mode_only_notifies_on_a_mention() {
        let people = people();
        assert!(!should_notify(
            &message("them", "just chatting"),
            "me",
            &people,
            &prefs(NotificationMode::Mentions),
            false
        ));
        assert!(should_notify(
            &message("them", "hey @ada, look at this"),
            "me",
            &people,
            &prefs(NotificationMode::Mentions),
            false
        ));
    }

    #[test]
    fn everyone_counts_as_a_mention() {
        assert!(mentions_me(
            &message("them", "@everyone stand-up in five"),
            "me",
            &people()
        ));
        assert!(mentions_me(
            &message("them", "@here please"),
            "me",
            &people()
        ));
    }

    #[test]
    fn a_mention_of_someone_else_does_not_count() {
        assert!(!mentions_me(
            &message("them", "@grace can you look?"),
            "me",
            &people()
        ));
    }

    #[test]
    fn a_mention_inside_a_code_block_does_not_count() {
        // Otherwise pasting a log full of @names pings half the community.
        assert!(!mentions_me(
            &message("them", "```\n@ada was here\n```"),
            "me",
            &people()
        ));
    }

    #[test]
    fn a_per_channel_setting_overrides_the_instance_one() {
        let mut preferences = prefs(NotificationMode::All);
        preferences.channels.push(ChannelNotification {
            channel_id: "general".into(),
            mode: NotificationMode::None,
        });
        assert!(!should_notify(
            &message("them", "hello"),
            "me",
            &people(),
            &preferences,
            false
        ));
        // A channel with no override still follows the instance setting.
        let elsewhere = Message {
            channel_id: "design".into(),
            ..message("them", "hello")
        };
        assert!(should_notify(
            &elsewhere,
            "me",
            &people(),
            &preferences,
            false
        ));
    }

    #[test]
    fn muted_means_muted_even_for_a_mention() {
        let mut preferences = prefs(NotificationMode::None);
        preferences.channels.clear();
        assert!(!should_notify(
            &message("them", "@ada urgent"),
            "me",
            &people(),
            &preferences,
            false
        ));
    }

    #[test]
    fn a_system_notice_never_notifies() {
        let joined = Message {
            system_kind: Some("member_join".into()),
            ..message("them", "")
        };
        assert!(!should_notify(
            &joined,
            "me",
            &people(),
            &prefs(NotificationMode::All),
            false
        ));
    }
}
