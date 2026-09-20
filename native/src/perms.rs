//! Permission bits, mirroring `server/src/perms.rs` and `web/src/lib/perms.ts`.
//!
//! The web client carries these as `BigInt` because the flags run past the
//! safe integer range once ADMINISTRATOR is set. Rust has no such problem —
//! `u64` holds them all — but the server still sends them as a decimal string,
//! so they are parsed rather than deserialised as a number.
//!
//! Every flag the server defines is named here, not only the ones a screen
//! checks today: the set is the permission model, and a partial copy is how
//! a client ends up silently ignoring a permission it should honour.
#![allow(dead_code)]

macro_rules! permissions {
    ($($name:ident = $bit:expr, $label:expr, $group:expr;)*) => {
        $(pub const $name: u64 = 1 << $bit;)*

        /// Every permission, in the order the admin panel lists them.
        pub const CATALOG: &[(&str, u64, &str, Group)] = &[
            $((stringify!($name), $name, $label, $group),)*
        ];
    };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    General,
    Text,
    Voice,
    Admin,
}

permissions! {
    VIEW_CHANNELS     =  0, "View channels",        Group::General;
    SEND_MESSAGES     =  1, "Send messages",        Group::Text;
    MANAGE_MESSAGES   =  2, "Manage messages",      Group::Text;
    ATTACH_FILES      =  3, "Attach files",         Group::Text;
    ADD_REACTIONS     =  4, "Add reactions",        Group::Text;
    MENTION_EVERYONE  =  5, "Mention @everyone",    Group::Text;
    CONNECT           =  6, "Connect to voice",     Group::Voice;
    SPEAK             =  7, "Speak",                Group::Voice;
    VIDEO             =  8, "Share camera",         Group::Voice;
    SCREEN_SHARE      =  9, "Share screen",         Group::Voice;
    MUTE_MEMBERS      = 10, "Mute members",         Group::Voice;
    MOVE_MEMBERS      = 11, "Move members",         Group::Voice;
    MANAGE_CHANNELS   = 12, "Manage channels",      Group::Admin;
    MANAGE_ROLES      = 13, "Manage roles",         Group::Admin;
    MANAGE_INSTANCE   = 14, "Manage instance",      Group::Admin;
    CREATE_INVITES    = 15, "Create invites",       Group::General;
    KICK_MEMBERS      = 16, "Kick members",         Group::Admin;
    BAN_MEMBERS       = 17, "Ban members",          Group::Admin;
    VIEW_AUDIT_LOG    = 18, "View audit log",       Group::Admin;
    MANAGE_WEBHOOKS   = 19, "Manage webhooks",      Group::Admin;
    MANAGE_NICKNAMES  = 20, "Manage nicknames",     Group::Admin;
    PIN_MESSAGES      = 21, "Pin messages",         Group::Text;
    MANAGE_EMOJI      = 22, "Manage emoji",         Group::Admin;
    ADMINISTRATOR     = 30, "Administrator",        Group::Admin;
}

/// Parse the decimal string the server sends. Anything unparseable is no
/// permissions at all — the server is authoritative regardless, so the worst
/// a wrong answer here does is hide a button the server would have allowed.
pub fn parse(value: &str) -> u64 {
    value.trim().parse().unwrap_or(0)
}

/// ADMINISTRATOR implies every other permission.
pub fn can(bits: u64, permission: u64) -> bool {
    bits & ADMINISTRATOR != 0 || bits & permission == permission
}

/// The permissions that open an admin surface.
pub const ADMIN_PERMS: &[u64] = &[
    MANAGE_INSTANCE,
    MANAGE_CHANNELS,
    MANAGE_ROLES,
    KICK_MEMBERS,
    BAN_MEMBERS,
    VIEW_AUDIT_LOG,
    MANAGE_WEBHOOKS,
    MANAGE_EMOJI,
];

pub fn can_see_admin_panel(bits: u64) -> bool {
    ADMIN_PERMS.iter().any(|&p| can(bits, p))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn administrator_implies_everything() {
        assert!(can(ADMINISTRATOR, BAN_MEMBERS));
        assert!(can(ADMINISTRATOR, VIEW_CHANNELS));
        assert!(can_see_admin_panel(ADMINISTRATOR));
    }

    #[test]
    fn a_plain_member_gets_only_what_is_set() {
        let bits = VIEW_CHANNELS | SEND_MESSAGES;
        assert!(can(bits, VIEW_CHANNELS));
        assert!(can(bits, SEND_MESSAGES));
        assert!(!can(bits, MANAGE_MESSAGES));
        assert!(!can_see_admin_panel(bits));
    }

    #[test]
    fn the_bits_survive_the_decimal_string_the_server_sends() {
        // ADMINISTRATOR alone is 2^30, past what a float would hold exactly
        // once combined with the rest.
        assert_eq!(parse("1073741824"), ADMINISTRATOR);
        assert_eq!(
            parse("1073741827"),
            ADMINISTRATOR | SEND_MESSAGES | VIEW_CHANNELS
        );
        assert_eq!(parse(""), 0);
        assert_eq!(parse("nonsense"), 0);
    }

    #[test]
    fn every_permission_has_a_distinct_bit() {
        let mut seen = 0u64;
        for (name, bit, _, _) in CATALOG {
            assert_eq!(seen & bit, 0, "{name} collides with an earlier permission");
            seen |= bit;
        }
        assert_eq!(CATALOG.len(), 24);
    }

    #[test]
    fn a_partial_admin_still_sees_the_panel() {
        assert!(can_see_admin_panel(VIEW_AUDIT_LOG));
        assert!(can_see_admin_panel(MANAGE_EMOJI));
    }
}
