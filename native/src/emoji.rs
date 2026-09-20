//! The unicode emoji the picker offers.
//!
//! A deliberately small, curated set rather than the whole Unicode table: the
//! picker is for reacting to a message, and a list long enough to need its own
//! index is slower to use than typing. The instance's custom emoji are listed
//! above these and come from the server.

/// `(glyph, searchable name)`, grouped roughly the way pickers usually group.
pub const COMMON: &[(&str, &str)] = &[
    ("👍", "thumbs up like yes"),
    ("👎", "thumbs down dislike no"),
    ("❤️", "heart love red"),
    ("🎉", "party tada celebrate"),
    ("🚀", "rocket ship launch ship-it"),
    ("🔥", "fire hot lit"),
    ("✅", "check tick done yes"),
    ("❌", "cross no wrong fail"),
    ("👀", "eyes looking watching"),
    ("🙏", "pray thanks please"),
    ("😀", "grin smile happy"),
    ("😂", "joy laugh crying tears"),
    ("🙂", "slight smile"),
    ("😅", "sweat smile nervous"),
    ("😍", "heart eyes love"),
    ("🤔", "thinking hmm think"),
    ("😐", "neutral face meh"),
    ("😴", "sleep tired zzz"),
    ("😭", "sob crying sad"),
    ("😡", "angry rage mad"),
    ("🤝", "handshake deal agree"),
    ("👋", "wave hello hi bye"),
    ("💪", "muscle strong flex"),
    ("🧠", "brain smart clever"),
    ("💡", "idea bulb light"),
    ("⚡", "zap lightning fast"),
    ("🐛", "bug insect defect"),
    ("🛠️", "tools build fix"),
    ("📦", "package box ship release"),
    ("📝", "memo note write docs"),
    ("⏰", "alarm clock time"),
    ("☕", "coffee tea break"),
    ("🍕", "pizza food lunch"),
    ("🎯", "target bullseye goal"),
    ("🏆", "trophy win award"),
    ("💯", "hundred perfect full"),
    ("🤖", "robot bot automation"),
    ("👻", "ghost boo spooky"),
    ("🌱", "seedling grow plant new"),
    ("🌈", "rainbow colour pride"),
];

/// The entries whose name or glyph matches `filter`.
///
/// An empty filter returns everything, which is the picker's opening state.
pub fn search(filter: &str) -> Vec<(&'static str, &'static str)> {
    let needle = filter.trim().to_lowercase();
    if needle.is_empty() {
        return COMMON.to_vec();
    }
    COMMON
        .iter()
        .filter(|(glyph, name)| name.contains(&needle) || *glyph == needle)
        .copied()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_filter_offers_everything() {
        assert_eq!(search("").len(), COMMON.len());
        assert_eq!(search("   ").len(), COMMON.len());
    }

    #[test]
    fn a_filter_matches_any_of_an_entrys_names() {
        // Each entry carries several words so the obvious search finds it.
        assert!(search("tada").iter().any(|(glyph, _)| *glyph == "🎉"));
        assert!(search("celebrate").iter().any(|(glyph, _)| *glyph == "🎉"));
        assert!(search("ship").iter().any(|(glyph, _)| *glyph == "🚀"));
    }

    #[test]
    fn a_filter_that_matches_nothing_returns_nothing_rather_than_everything() {
        assert!(search("zzzzznotanemoji").is_empty());
    }

    #[test]
    fn searching_ignores_case_and_surrounding_space() {
        assert_eq!(search(" FIRE ").len(), search("fire").len());
        assert!(!search("fire").is_empty());
    }

    #[test]
    fn every_entry_has_a_glyph_and_a_name() {
        for (glyph, name) in COMMON {
            assert!(!glyph.is_empty());
            assert!(!name.is_empty(), "{glyph} has no searchable name");
            assert_eq!(
                name.to_lowercase(),
                *name,
                "{glyph}'s name must be lowercase"
            );
        }
    }
}
