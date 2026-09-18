//! Input validation. Messages here are shown directly to the user, so they
//! explain what to do rather than just what went wrong.

use crate::error::{AppError, AppResult};

pub fn username(value: &str) -> AppResult<String> {
    let v = value.trim();
    if v.len() < 2 || v.len() > 32 {
        return Err(AppError::bad("Username must be 2-32 characters."));
    }
    if !v
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-')
    {
        return Err(AppError::bad(
            "Username can only use letters, numbers, dots, dashes and underscores.",
        ));
    }
    if v.starts_with('.') || v.ends_with('.') {
        return Err(AppError::bad("Username can't start or end with a dot."));
    }
    Ok(v.to_string())
}

pub fn password(value: &str) -> AppResult<()> {
    if value.chars().count() < 8 {
        return Err(AppError::bad("Password must be at least 8 characters."));
    }
    if value.len() > 512 {
        return Err(AppError::bad("Password is too long."));
    }
    Ok(())
}

pub fn email(value: &str) -> AppResult<String> {
    let v = value.trim().to_lowercase();
    let valid = v.len() <= 254
        && v.matches('@').count() == 1
        && v.split('@').all(|part| !part.is_empty())
        && v.split('@')
            .nth(1)
            .is_some_and(|d| d.contains('.') && !d.starts_with('.') && !d.ends_with('.'));
    if !valid {
        return Err(AppError::bad(
            "That doesn't look like a valid email address.",
        ));
    }
    Ok(v)
}

pub fn text(value: &str, field: &str, min: usize, max: usize) -> AppResult<String> {
    let v = value.trim();
    let len = v.chars().count();
    if len < min {
        return Err(AppError::bad(format!("{field} can't be empty.")));
    }
    if len > max {
        return Err(AppError::bad(format!(
            "{field} must be {max} characters or fewer."
        )));
    }
    Ok(v.to_string())
}

pub fn optional_text(value: &str, field: &str, max: usize) -> AppResult<String> {
    let v = value.trim();
    if v.chars().count() > max {
        return Err(AppError::bad(format!(
            "{field} must be {max} characters or fewer."
        )));
    }
    Ok(v.to_string())
}

/// Channel names are whatever the admin typed.
///
/// Text channels used to be forced through `slugify_channel`, which lowercased
/// them and turned every space into a hyphen. That convention comes from
/// platforms where the name is also an identifier; here channels are addressed
/// by id everywhere — the sidebar, `?channel=`, mentions — so the name is
/// presentation only and "Game Night" can just be "Game Night".
///
/// Control characters are stripped because they would let a name break the
/// sidebar or a log line; everything else, spaces and capitals included, is
/// kept as written.
pub fn channel_name(value: &str) -> AppResult<String> {
    // Split before stripping, not after: tab and newline are themselves
    // control characters, so filtering first would delete the separator and
    // turn "a\tb" into "ab" rather than "a b".
    let v = value
        .split_whitespace()
        .map(|part| part.chars().filter(|c| !c.is_control()).collect::<String>())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    if v.is_empty() || v.chars().count() > 48 {
        return Err(AppError::bad("Channel name must be 1-48 characters."));
    }
    Ok(v)
}

pub fn color(value: &str) -> AppResult<String> {
    let v = value.trim();
    let ok = v.len() == 7 && v.starts_with('#') && v[1..].chars().all(|c| c.is_ascii_hexdigit());
    if !ok {
        return Err(AppError::bad("Colour must be a hex value like #5b6ee8."));
    }
    Ok(v.to_lowercase())
}

/// Only allow URLs we serve ourselves or plain https links, so a profile field
/// can't be used to point at javascript: or data: payloads.
pub fn safe_url(value: &str, field: &str) -> AppResult<Option<String>> {
    let v = value.trim();
    if v.is_empty() {
        return Ok(None);
    }
    if v.len() > 1024 {
        return Err(AppError::bad(format!("{field} URL is too long.")));
    }
    if v.starts_with("/uploads/") || v.starts_with("https://") {
        Ok(Some(v.to_string()))
    } else {
        Err(AppError::bad(format!(
            "{field} must be an uploaded file or an https:// URL."
        )))
    }
}

/// A reaction is either a short unicode emoji or a `:custom_name:` reference.
pub fn emoji(value: &str) -> AppResult<String> {
    let v = value.trim();
    let len = v.chars().count();
    if v.is_empty() {
        return Err(AppError::bad("That's not a valid reaction."));
    }
    if v.starts_with(':') && v.ends_with(':') && (4..=34).contains(&len) {
        let name = &v[1..v.len() - 1];
        if name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Ok(v.to_lowercase());
        }
        return Err(AppError::bad("That's not a valid reaction."));
    }
    if len > 8 {
        return Err(AppError::bad("That's not a valid reaction."));
    }
    Ok(v.to_string())
}

#[cfg(test)]
mod tests {
    use super::channel_name;

    #[test]
    fn keeps_spaces_and_capitals() {
        // The whole point of dropping slugification: these used to come back
        // as "game-night" and "aq".
        assert_eq!(channel_name("Game Night").unwrap(), "Game Night");
        assert_eq!(channel_name("AQ&A").unwrap(), "AQ&A");
        assert_eq!(
            channel_name("📣 Announcements").unwrap(),
            "📣 Announcements"
        );
        assert_eq!(channel_name("general").unwrap(), "general");
    }

    #[test]
    fn trims_and_collapses_whitespace() {
        assert_eq!(channel_name("  spaced  out  ").unwrap(), "spaced out");
        // Runs of spaces would otherwise let a name fake indentation in the
        // channel list, or mimic another channel.
        assert_eq!(channel_name("a\t\tb").unwrap(), "a b");
        assert_eq!(channel_name("a\nb").unwrap(), "a b");
    }

    #[test]
    fn rejects_empty_and_overlong() {
        assert!(channel_name("").is_err());
        assert!(channel_name("   ").is_err());
        // Control characters alone leave nothing behind.
        assert!(channel_name("\u{0}\u{7}").is_err());
        assert!(channel_name(&"x".repeat(48)).is_ok());
        assert!(channel_name(&"x".repeat(49)).is_err());
    }

    #[test]
    fn strips_control_characters() {
        // A bare CR could otherwise overwrite a log line or the sidebar row.
        assert_eq!(channel_name("na\u{0}me").unwrap(), "name");
        assert_eq!(channel_name("bell\u{7}").unwrap(), "bell");
    }
}
