//! Permission bitflags. Stored as an i64 column so SQLite can hold them directly.

pub const VIEW_CHANNELS: i64 = 1 << 0;
pub const SEND_MESSAGES: i64 = 1 << 1;
pub const MANAGE_MESSAGES: i64 = 1 << 2;
pub const ATTACH_FILES: i64 = 1 << 3;
pub const ADD_REACTIONS: i64 = 1 << 4;
pub const MENTION_EVERYONE: i64 = 1 << 5;
pub const CONNECT: i64 = 1 << 6;
pub const SPEAK: i64 = 1 << 7;
pub const VIDEO: i64 = 1 << 8;
pub const SCREEN_SHARE: i64 = 1 << 9;
pub const MUTE_MEMBERS: i64 = 1 << 10;
pub const MOVE_MEMBERS: i64 = 1 << 11;
pub const MANAGE_CHANNELS: i64 = 1 << 12;
pub const MANAGE_ROLES: i64 = 1 << 13;
pub const MANAGE_INSTANCE: i64 = 1 << 14;
pub const CREATE_INVITES: i64 = 1 << 15;
pub const KICK_MEMBERS: i64 = 1 << 16;
pub const BAN_MEMBERS: i64 = 1 << 17;
pub const VIEW_AUDIT_LOG: i64 = 1 << 18;
pub const MANAGE_WEBHOOKS: i64 = 1 << 19;
pub const MANAGE_NICKNAMES: i64 = 1 << 20;
pub const PIN_MESSAGES: i64 = 1 << 21;
pub const ADMINISTRATOR: i64 = 1 << 30;

/// Sensible defaults handed to the `@everyone`-style default role at setup time.
pub const DEFAULT_MEMBER: i64 = VIEW_CHANNELS
    | SEND_MESSAGES
    | ATTACH_FILES
    | ADD_REACTIONS
    | CONNECT
    | SPEAK
    | VIDEO
    | SCREEN_SHARE;

pub const ALL: i64 = VIEW_CHANNELS
    | SEND_MESSAGES
    | MANAGE_MESSAGES
    | ATTACH_FILES
    | ADD_REACTIONS
    | MENTION_EVERYONE
    | CONNECT
    | SPEAK
    | VIDEO
    | SCREEN_SHARE
    | MUTE_MEMBERS
    | MOVE_MEMBERS
    | MANAGE_CHANNELS
    | MANAGE_ROLES
    | MANAGE_INSTANCE
    | CREATE_INVITES
    | KICK_MEMBERS
    | BAN_MEMBERS
    | VIEW_AUDIT_LOG
    | MANAGE_WEBHOOKS
    | MANAGE_NICKNAMES
    | PIN_MESSAGES
    | ADMINISTRATOR;

/// A catalogue the admin UI renders so permission names never drift between
/// client and server.
pub const CATALOG: &[(&str, i64, &str, &str)] = &[
    ("VIEW_CHANNELS", VIEW_CHANNELS, "View channels", "general"),
    ("SEND_MESSAGES", SEND_MESSAGES, "Send messages", "text"),
    (
        "MANAGE_MESSAGES",
        MANAGE_MESSAGES,
        "Delete others' messages",
        "text",
    ),
    ("PIN_MESSAGES", PIN_MESSAGES, "Pin messages", "text"),
    ("ATTACH_FILES", ATTACH_FILES, "Attach files", "text"),
    ("ADD_REACTIONS", ADD_REACTIONS, "Add reactions", "text"),
    (
        "MENTION_EVERYONE",
        MENTION_EVERYONE,
        "Mention @everyone",
        "text",
    ),
    ("CONNECT", CONNECT, "Join voice channels", "voice"),
    ("SPEAK", SPEAK, "Speak", "voice"),
    ("VIDEO", VIDEO, "Share camera", "voice"),
    ("SCREEN_SHARE", SCREEN_SHARE, "Share screen", "voice"),
    ("MUTE_MEMBERS", MUTE_MEMBERS, "Mute members", "voice"),
    ("MOVE_MEMBERS", MOVE_MEMBERS, "Disconnect members", "voice"),
    (
        "MANAGE_CHANNELS",
        MANAGE_CHANNELS,
        "Manage channels",
        "admin",
    ),
    ("MANAGE_ROLES", MANAGE_ROLES, "Manage roles", "admin"),
    (
        "MANAGE_INSTANCE",
        MANAGE_INSTANCE,
        "Manage instance",
        "admin",
    ),
    ("CREATE_INVITES", CREATE_INVITES, "Create invites", "admin"),
    ("KICK_MEMBERS", KICK_MEMBERS, "Kick members", "admin"),
    ("BAN_MEMBERS", BAN_MEMBERS, "Ban members", "admin"),
    ("VIEW_AUDIT_LOG", VIEW_AUDIT_LOG, "View audit log", "admin"),
    (
        "MANAGE_WEBHOOKS",
        MANAGE_WEBHOOKS,
        "Manage integrations",
        "admin",
    ),
    (
        "MANAGE_NICKNAMES",
        MANAGE_NICKNAMES,
        "Manage nicknames",
        "admin",
    ),
    (
        "ADMINISTRATOR",
        ADMINISTRATOR,
        "Administrator (all permissions)",
        "admin",
    ),
];

/// `ADMINISTRATOR` implies everything else, which keeps every call site from
/// having to special-case it.
pub fn has(bits: i64, perm: i64) -> bool {
    bits & ADMINISTRATOR != 0 || bits & perm == perm
}
