//! Forwarding a message to another channel.
//!
//! A forward is a quote with attribution and a link back, not a copy under
//! your own name. The shape is the web client's (`ForwardDialog.tsx`) so a
//! forwarded message reads identically whichever client sent it — and, more
//! to the point, so the link at the bottom is one the web client can open.

use crate::api::types::Message;

/// Build the body of a forwarded message.
///
/// `public` is the instance's public URL, which the link back needs; an
/// empty one simply omits the link rather than producing a broken one.
pub fn compose(
    message: &Message,
    origin_channel: Option<&str>,
    note: &str,
    public: &str,
) -> String {
    let mut parts: Vec<String> = Vec::new();

    let author = message.author_name();
    let where_from = match origin_channel {
        Some(name) if !name.is_empty() => format!(" in #{name}"),
        _ => String::new(),
    };
    parts.push(format!("**{author}**{where_from}:"));

    // Every line quoted, so a multi-line message stays visually one block
    // instead of only its first line reading as a quote.
    if !message.content.is_empty() {
        let quoted: Vec<String> = message
            .content
            .split('\n')
            .map(|line| format!("> {line}"))
            .collect();
        parts.push(quoted.join("\n"));
    }

    // Attachments do not travel with the text, so at least link them.
    for file in &message.attachments {
        if !file.url.is_empty() {
            parts.push(absolute(&file.url, public));
        }
    }

    let note = note.trim();
    if !note.is_empty() {
        parts.push(note.to_string());
    }

    if !public.is_empty() && !message.channel_id.is_empty() {
        // A bare URL, not `[text](url)`: this renderer autolinks the former
        // and shows the latter as literal text.
        parts.push(format!(
            "{}/?channel={}&message={}",
            public.trim_end_matches('/'),
            message.channel_id,
            message.id
        ));
    }

    parts.join("\n")
}

fn absolute(url: &str, public: &str) -> String {
    if url.starts_with("http://") || url.starts_with("https://") || public.is_empty() {
        return url.to_string();
    }
    format!(
        "{}/{}",
        public.trim_end_matches('/'),
        url.trim_start_matches('/')
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::types::{Attachment, MessageAuthor};

    fn message(content: &str) -> Message {
        Message {
            id: "m1".into(),
            channel_id: "c1".into(),
            author: Some(MessageAuthor {
                id: "u1".into(),
                display_name: "Ada Lovelace".into(),
                ..Default::default()
            }),
            content: content.into(),
            ..Default::default()
        }
    }

    #[test]
    fn a_forward_carries_attribution_the_quote_and_a_link_back() {
        let body = compose(
            &message("the original"),
            Some("general"),
            "",
            "https://chat.example.com",
        );
        assert!(body.starts_with("**Ada Lovelace** in #general:"));
        assert!(body.contains("> the original"));
        assert!(body.contains("https://chat.example.com/?channel=c1&message=m1"));
    }

    #[test]
    fn every_line_of_a_multi_line_message_is_quoted() {
        // Quoting only the first line makes the rest read as the forwarder's
        // own words, which is the opposite of what a forward is for.
        let body = compose(&message("first\nsecond\nthird"), None, "", "");
        assert!(body.contains("> first"));
        assert!(body.contains("> second"));
        assert!(body.contains("> third"));
    }

    #[test]
    fn a_note_is_included_and_an_empty_one_is_not() {
        let with = compose(&message("x"), None, "  look at this  ", "");
        assert!(with.contains("look at this"));
        assert!(!with.contains("  look"), "the note should be trimmed");

        let without = compose(&message("x"), None, "   ", "");
        assert_eq!(without.lines().count(), 2, "an empty note adds no line");
    }

    #[test]
    fn attachments_are_linked_because_they_do_not_travel() {
        let mut m = message("with a file");
        m.attachments = vec![Attachment {
            url: "/uploads/a.png".into(),
            ..Default::default()
        }];
        let body = compose(&m, None, "", "https://chat.example.com");
        assert!(body.contains("https://chat.example.com/uploads/a.png"));
    }

    #[test]
    fn no_public_url_means_no_link_rather_than_a_broken_one() {
        let body = compose(&message("x"), Some("general"), "", "");
        assert!(!body.contains("?channel="));
        assert!(body.contains("> x"));
    }

    #[test]
    fn a_message_with_only_an_attachment_still_forwards() {
        let mut m = message("");
        m.attachments = vec![Attachment {
            url: "https://cdn.example.com/a.png".into(),
            ..Default::default()
        }];
        let body = compose(&m, None, "", "https://chat.example.com");
        assert!(body.contains("**Ada Lovelace**:"));
        assert!(body.contains("https://cdn.example.com/a.png"));
        // No empty quote line for a body that was not there.
        assert!(!body.contains("> \n") && !body.ends_with("> "));
    }
}
