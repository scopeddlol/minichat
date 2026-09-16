//! Mention parsing and storage.
//!
//! Mentions are written as plain `@username` in the message body rather than
//! an opaque `<@id>` token. Usernames are unique and restricted to a safe
//! alphabet, so they parse unambiguously, and the raw text stays readable in
//! the composer, in search results and in a push notification.

use std::collections::HashSet;

use crate::error::AppResult;
use crate::state::AppState;

#[derive(Debug, Default)]
pub struct Resolved {
    /// User IDs of members named directly.
    pub user_ids: Vec<String>,
    /// Whether the message carries a valid @everyone / @here.
    pub everyone: bool,
}

/// Strip fenced and inline code so `@example` in a code sample doesn't ping.
fn without_code(content: &str) -> String {
    let mut out = String::with_capacity(content.len());
    let mut chars = content.chars().peekable();
    let mut in_fence = false;
    let mut in_inline = false;
    let mut backticks = 0;

    while let Some(c) = chars.next() {
        if c == '`' {
            backticks += 1;
            // Three in a row toggles a fenced block.
            if backticks == 3 {
                in_fence = !in_fence;
                backticks = 0;
                in_inline = false;
            } else if chars.peek() != Some(&'`') {
                if !in_fence {
                    in_inline = !in_inline;
                }
                backticks = 0;
            }
            continue;
        }
        backticks = 0;
        if !in_fence && !in_inline {
            out.push(c);
        } else {
            // Preserve length-ish structure without leaking mentionable text.
            out.push(' ');
        }
    }
    out
}

/// Pull candidate `@name` tokens out of a message body.
fn scan(content: &str) -> (HashSet<String>, bool) {
    let text = without_code(content);
    let chars: Vec<char> = text.chars().collect();
    let mut names = HashSet::new();
    let mut everyone = false;

    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '@' {
            i += 1;
            continue;
        }
        // A mention must start at a boundary, so emails don't become pings.
        if i > 0 {
            let previous = chars[i - 1];
            if previous.is_alphanumeric() || previous == '_' || previous == '.' || previous == '-' {
                i += 1;
                continue;
            }
        }

        let mut j = i + 1;
        let mut name = String::new();
        while j < chars.len() && name.len() < 32 {
            let c = chars[j];
            if c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-' {
                name.push(c);
                j += 1;
            } else {
                break;
            }
        }

        // Trailing punctuation shouldn't be part of the name: "@ada." → "ada".
        while name.ends_with('.') || name.ends_with('-') {
            name.pop();
        }

        if name.eq_ignore_ascii_case("everyone") || name.eq_ignore_ascii_case("here") {
            everyone = true;
        } else if name.len() >= 2 {
            names.insert(name.to_lowercase());
        }
        i = j.max(i + 1);
    }

    (names, everyone)
}

/// Resolve the names in a message to real members.
///
/// `can_mention_everyone` gates @everyone: without the permission the text
/// still renders, it just doesn't notify anyone.
pub async fn resolve(
    state: &AppState,
    content: &str,
    can_mention_everyone: bool,
) -> AppResult<Resolved> {
    let (names, everyone) = scan(content);
    if names.is_empty() {
        return Ok(Resolved {
            user_ids: Vec::new(),
            everyone: everyone && can_mention_everyone,
        });
    }

    let names: Vec<String> = names.into_iter().collect();
    let placeholders = vec!["?"; names.len()].join(",");
    let sql = format!("SELECT id FROM users WHERE lower(username) IN ({placeholders})");
    let mut query = sqlx::query_scalar::<_, String>(&sql);
    for name in &names {
        query = query.bind(name);
    }

    Ok(Resolved {
        user_ids: query.fetch_all(&state.db).await?,
        everyone: everyone && can_mention_everyone,
    })
}

/// Replace the stored mentions for a message. Called on create and on edit, so
/// editing a mention out of a message also clears the ping.
pub async fn store(
    state: &AppState,
    message_id: &str,
    channel_id: &str,
    author_id: Option<&str>,
    resolved: &Resolved,
) -> AppResult<()> {
    sqlx::query("DELETE FROM mentions WHERE message_id = ?")
        .bind(message_id)
        .execute(&state.db)
        .await?;

    if resolved.everyone {
        // Materialise @everyone into one row per viewer so the unread-mention
        // query stays a single indexed lookup.
        let viewers = crate::access::channel_viewers(state, channel_id).await?;
        for user_id in viewers {
            if Some(user_id.as_str()) == author_id {
                continue;
            }
            sqlx::query(
                "INSERT OR IGNORE INTO mentions (message_id, user_id, channel_id, kind)
                 VALUES (?, ?, ?, 'everyone')",
            )
            .bind(message_id)
            .bind(&user_id)
            .bind(channel_id)
            .execute(&state.db)
            .await?;
        }
    }

    for user_id in &resolved.user_ids {
        if Some(user_id.as_str()) == author_id {
            continue;
        }
        sqlx::query(
            "INSERT INTO mentions (message_id, user_id, channel_id, kind)
             VALUES (?, ?, ?, 'direct')
             ON CONFLICT(message_id, user_id) DO UPDATE SET kind = 'direct'",
        )
        .bind(message_id)
        .bind(user_id)
        .bind(channel_id)
        .execute(&state.db)
        .await?;
    }
    Ok(())
}

/// Unread mention count per channel for one member.
pub async fn unread_counts(state: &AppState, user_id: &str) -> AppResult<Vec<(String, i64)>> {
    Ok(sqlx::query_as(
        "SELECT m.channel_id, COUNT(*)
         FROM mentions m
         LEFT JOIN read_state rs ON rs.user_id = m.user_id AND rs.channel_id = m.channel_id
         WHERE m.user_id = ? AND m.message_id > COALESCE(rs.last_read_id, '')
         GROUP BY m.channel_id",
    )
    .bind(user_id)
    .fetch_all(&state.db)
    .await?)
}
