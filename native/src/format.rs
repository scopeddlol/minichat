//! Presentation helpers, mirroring `web/src/lib/format.ts`.
//!
//! The whole helper set is ported together so a screen that needs one is
//! not also a change to this file.
#![allow(dead_code)]

use time::{format_description::FormatItem, macros::format_description, OffsetDateTime, UtcOffset};

const TIME: &[FormatItem<'_>] =
    format_description!("[hour repr:12 padding:none]:[minute] [period]");
const SHORT_DATE: &[FormatItem<'_>] = format_description!("[month repr:short] [day padding:none]");
const LONG_DATE: &[FormatItem<'_>] =
    format_description!("[month repr:long] [day padding:none], [year]");

/// The local offset, resolved once.
///
/// `UtcOffset::current_local_offset` fails on a multi-threaded process on
/// some platforms, and the answer never changes while the app runs, so it is
/// read at startup from the main thread and cached. UTC is the fallback:
/// timestamps an hour out are better than a panic.
static LOCAL_OFFSET: std::sync::OnceLock<UtcOffset> = std::sync::OnceLock::new();

pub fn init_local_offset() {
    let _ = LOCAL_OFFSET.set(UtcOffset::current_local_offset().unwrap_or(UtcOffset::UTC));
}

fn local_offset() -> UtcOffset {
    *LOCAL_OFFSET.get_or_init(|| UtcOffset::current_local_offset().unwrap_or(UtcOffset::UTC))
}

/// SQLite hands back `YYYY-MM-DD HH:MM:SS` in UTC; the API sometimes sends
/// RFC 3339 instead. Both parse here, and anything else becomes the epoch
/// rather than an error the UI would have to thread through every row.
pub fn parse(value: &str) -> OffsetDateTime {
    if value.is_empty() {
        return OffsetDateTime::UNIX_EPOCH;
    }
    if value.contains('T') {
        if let Ok(parsed) =
            OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339)
        {
            return parsed;
        }
    }
    let normalised = format!("{}Z", value.replace(' ', "T"));
    OffsetDateTime::parse(&normalised, &time::format_description::well_known::Rfc3339)
        .unwrap_or(OffsetDateTime::UNIX_EPOCH)
}

/// Unix seconds, for the arithmetic the grouping rules do.
pub fn parse_timestamp(value: &str) -> i64 {
    parse(value).unix_timestamp()
}

fn local(value: &str) -> OffsetDateTime {
    parse(value).to_offset(local_offset())
}

pub fn time_of_day(value: &str) -> String {
    local(value).format(TIME).unwrap_or_default().to_lowercase()
}

pub fn full_date(value: &str) -> String {
    local(value).format(LONG_DATE).unwrap_or_default()
}

pub fn short_date(value: &str) -> String {
    local(value).format(SHORT_DATE).unwrap_or_default()
}

fn now_local() -> OffsetDateTime {
    OffsetDateTime::now_utc().to_offset(local_offset())
}

/// "Today at 4:05 pm", "Yesterday at …", or the full date.
pub fn timestamp(value: &str) -> String {
    let moment = local(value);
    let now = now_local();
    let clock = moment.format(TIME).unwrap_or_default().to_lowercase();

    if moment.date() == now.date() {
        return format!("Today at {clock}");
    }
    if moment.date() == now.date().previous_day().unwrap_or(now.date()) {
        return format!("Yesterday at {clock}");
    }
    format!(
        "{} at {clock}",
        moment.format(LONG_DATE).unwrap_or_default()
    )
}

/// The label on the divider between two days of messages.
pub fn day_divider(value: &str) -> String {
    let moment = local(value);
    let now = now_local();
    if moment.date() == now.date() {
        return "Today".into();
    }
    if moment.date() == now.date().previous_day().unwrap_or(now.date()) {
        return "Yesterday".into();
    }
    moment.format(LONG_DATE).unwrap_or_default()
}

/// Whether two messages fall on different days, which is what puts a divider
/// between them.
pub fn same_day(a: &str, b: &str) -> bool {
    local(a).date() == local(b).date()
}

pub fn relative(value: &str) -> String {
    let seconds = (OffsetDateTime::now_utc() - parse(value)).whole_seconds();
    if seconds < 60 {
        return "just now".into();
    }
    let minutes = seconds / 60;
    if minutes < 60 {
        return format!("{minutes}m ago");
    }
    let hours = minutes / 60;
    if hours < 24 {
        return format!("{hours}h ago");
    }
    let days = hours / 24;
    if days < 30 {
        return format!("{days}d ago");
    }
    short_date(value)
}

pub fn bytes(count: i64) -> String {
    if count < 1024 {
        return format!("{count} B");
    }
    const UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];
    let mut value = count as f64 / 1024.0;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if value < 10.0 {
        format!("{value:.1} {}", UNITS[unit])
    } else {
        format!("{} {}", value.round() as i64, UNITS[unit])
    }
}

pub fn duration(seconds: i64) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let secs = seconds % 60;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{secs:02}")
    } else {
        format!("{minutes}:{secs:02}")
    }
}

pub fn slowmode(seconds: i64) -> String {
    if seconds < 60 {
        format!("{seconds}s")
    } else if seconds < 3600 {
        format!("{}m", (seconds as f64 / 60.0).round() as i64)
    } else {
        format!("{}h", (seconds as f64 / 3600.0).round() as i64)
    }
}

/// Up to two letters from a name, for an avatar with no image.
pub fn initials(name: &str) -> String {
    let parts: Vec<&str> = name.split_whitespace().filter(|p| !p.is_empty()).collect();
    match parts.as_slice() {
        [] => "?".into(),
        [single] => single.chars().take(2).collect::<String>().to_uppercase(),
        [first, .., last] => {
            let mut out = String::new();
            out.extend(first.chars().take(1));
            out.extend(last.chars().take(1));
            out.to_uppercase()
        }
    }
}

/// The deterministic fallback avatar colour, derived from a user ID.
///
/// `avatarGradient` in the web client mixes 12% of the accent into a grey
/// whose lightness comes from a hash of the ID. Same hash, same mix, so a
/// member's fallback avatar is the same colour in both clients.
pub fn avatar_colour(id: &str, accent: crate::theme::Rgb) -> crate::theme::Rgb {
    let mut hash: u32 = 0;
    for byte in id.encode_utf16() {
        hash = hash.wrapping_mul(31).wrapping_add(byte as u32);
    }
    let lightness = 30.0 + (hash % 9) as f32;
    let grey = hsl_to_rgb(220.0, 0.04, lightness / 100.0);
    crate::theme::mix(accent, grey, 0.12)
}

fn hsl_to_rgb(hue: f32, saturation: f32, lightness: f32) -> crate::theme::Rgb {
    let chroma = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
    let sector = hue / 60.0;
    let second = chroma * (1.0 - (sector % 2.0 - 1.0).abs());
    let (r, g, b) = match sector as i32 {
        0 => (chroma, second, 0.0),
        1 => (second, chroma, 0.0),
        2 => (0.0, chroma, second),
        3 => (0.0, second, chroma),
        4 => (second, 0.0, chroma),
        _ => (chroma, 0.0, second),
    };
    let offset = lightness - chroma / 2.0;
    let encode = |channel: f32| (((channel + offset).clamp(0.0, 1.0)) * 255.0).round() as u8;
    crate::theme::Rgb::new(encode(r), encode(g), encode(b))
}

/// Trim a URL for display, the way the message renderer does.
pub fn elide(value: &str, limit: usize) -> String {
    if value.chars().count() <= limit {
        return value.to_string();
    }
    let kept: String = value.chars().take(limit.saturating_sub(1)).collect();
    format!("{kept}…")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_both_shapes_the_server_sends() {
        assert_eq!(
            parse("2026-03-04 15:30:00").unix_timestamp(),
            parse("2026-03-04T15:30:00Z").unix_timestamp()
        );
    }

    #[test]
    fn an_unparseable_timestamp_becomes_the_epoch_rather_than_panicking() {
        assert_eq!(parse("not a date"), OffsetDateTime::UNIX_EPOCH);
        assert_eq!(parse(""), OffsetDateTime::UNIX_EPOCH);
    }

    #[test]
    fn formats_sizes_the_way_the_web_client_does() {
        assert_eq!(bytes(512), "512 B");
        assert_eq!(bytes(2048), "2.0 KB");
        assert_eq!(bytes(15 * 1024), "15 KB");
        assert_eq!(bytes(5 * 1024 * 1024), "5.0 MB");
    }

    #[test]
    fn formats_call_durations() {
        assert_eq!(duration(45), "0:45");
        assert_eq!(duration(605), "10:05");
        assert_eq!(duration(3661), "1:01:01");
    }

    #[test]
    fn initials_cover_one_and_many_word_names() {
        assert_eq!(initials("Ada Lovelace"), "AL");
        assert_eq!(initials("ada"), "AD");
        assert_eq!(initials("  "), "?");
        assert_eq!(initials("Ada B Lovelace"), "AL");
    }

    #[test]
    fn the_fallback_avatar_colour_is_stable_for_an_id() {
        let accent = crate::theme::Rgb::new(0x5b, 0x6e, 0xe8);
        assert_eq!(
            avatar_colour("user-1", accent),
            avatar_colour("user-1", accent)
        );
        assert_ne!(
            avatar_colour("user-1", accent),
            avatar_colour("user-9", accent)
        );
    }

    #[test]
    fn elides_only_when_it_has_to() {
        assert_eq!(elide("short", 64), "short");
        assert_eq!(elide(&"x".repeat(80), 10), format!("{}…", "x".repeat(9)));
    }

    #[test]
    fn slowmode_reads_in_the_largest_sensible_unit() {
        assert_eq!(slowmode(5), "5s");
        assert_eq!(slowmode(120), "2m");
        assert_eq!(slowmode(7200), "2h");
    }
}
