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

/// Channel names follow the familiar lowercase-hyphenated convention for text
/// channels; voice channels keep their spacing and capitalisation.
pub fn channel_name(value: &str) -> AppResult<String> {
    let v = value.trim();
    if v.is_empty() || v.chars().count() > 48 {
        return Err(AppError::bad("Channel name must be 1-48 characters."));
    }
    Ok(v.to_string())
}

pub fn slugify_channel(value: &str) -> String {
    let mut out = String::new();
    let mut last_dash = true;
    for c in value.trim().to_lowercase().chars() {
        if c.is_alphanumeric() {
            out.push(c);
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    out.trim_matches('-').to_string()
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
