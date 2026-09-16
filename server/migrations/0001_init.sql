-- MiniChat core schema (SQLite)

CREATE TABLE instance (
    id               INTEGER PRIMARY KEY CHECK (id = 1),
    name             TEXT    NOT NULL DEFAULT 'MiniChat',
    tagline          TEXT    NOT NULL DEFAULT '',
    description      TEXT    NOT NULL DEFAULT '',
    icon_url         TEXT,
    banner_url       TEXT,
    accent_color     TEXT    NOT NULL DEFAULT '#5b6ee8',
    rules            TEXT    NOT NULL DEFAULT '',
    welcome_message  TEXT    NOT NULL DEFAULT '',
    setup_complete   INTEGER NOT NULL DEFAULT 0,
    registration_mode TEXT   NOT NULL DEFAULT 'invite', -- invite | open | closed
    require_rules_accept INTEGER NOT NULL DEFAULT 1,
    default_role_id  TEXT,
    system_channel_id TEXT,
    max_upload_mb    INTEGER NOT NULL DEFAULT 25,
    created_at       TEXT    NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE users (
    id             TEXT PRIMARY KEY,
    username       TEXT NOT NULL UNIQUE,
    display_name   TEXT NOT NULL,
    email          TEXT UNIQUE,
    password_hash  TEXT NOT NULL,
    avatar_url     TEXT,
    banner_url     TEXT,
    bio            TEXT NOT NULL DEFAULT '',
    pronouns       TEXT NOT NULL DEFAULT '',
    favorite_game  TEXT NOT NULL DEFAULT '',
    accent_color   TEXT NOT NULL DEFAULT '#5b6ee8',
    custom_status  TEXT NOT NULL DEFAULT '',
    presence       TEXT NOT NULL DEFAULT 'offline', -- online|idle|dnd|offline
    is_operator    INTEGER NOT NULL DEFAULT 0,
    is_suspended   INTEGER NOT NULL DEFAULT 0,
    token_version  INTEGER NOT NULL DEFAULT 1,
    accepted_rules INTEGER NOT NULL DEFAULT 0,
    invited_by     TEXT REFERENCES users(id) ON DELETE SET NULL,
    created_at     TEXT NOT NULL DEFAULT (datetime('now')),
    last_seen_at   TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX idx_users_username ON users(lower(username));

CREATE TABLE roles (
    id           TEXT PRIMARY KEY,
    name         TEXT NOT NULL,
    color        TEXT,
    permissions  INTEGER NOT NULL DEFAULT 0,
    position     INTEGER NOT NULL DEFAULT 0,
    is_default   INTEGER NOT NULL DEFAULT 0,
    hoist        INTEGER NOT NULL DEFAULT 0,
    mentionable  INTEGER NOT NULL DEFAULT 1,
    created_at   TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE user_roles (
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role_id TEXT NOT NULL REFERENCES roles(id) ON DELETE CASCADE,
    PRIMARY KEY (user_id, role_id)
);

CREATE TABLE categories (
    id       TEXT PRIMARY KEY,
    name     TEXT NOT NULL,
    position INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE channels (
    id          TEXT PRIMARY KEY,
    category_id TEXT REFERENCES categories(id) ON DELETE SET NULL,
    kind        TEXT NOT NULL DEFAULT 'text', -- text | voice | announcement
    name        TEXT NOT NULL,
    topic       TEXT NOT NULL DEFAULT '',
    position    INTEGER NOT NULL DEFAULT 0,
    slowmode    INTEGER NOT NULL DEFAULT 0,
    is_private  INTEGER NOT NULL DEFAULT 0,
    user_limit  INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Per-channel role permission overwrites
CREATE TABLE channel_overwrites (
    channel_id TEXT NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    role_id    TEXT NOT NULL REFERENCES roles(id) ON DELETE CASCADE,
    allow      INTEGER NOT NULL DEFAULT 0,
    deny       INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (channel_id, role_id)
);

CREATE TABLE messages (
    id          TEXT PRIMARY KEY,
    channel_id  TEXT NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    author_id   TEXT REFERENCES users(id) ON DELETE SET NULL,
    content     TEXT NOT NULL DEFAULT '',
    reply_to_id TEXT REFERENCES messages(id) ON DELETE SET NULL,
    system_kind TEXT,
    webhook_name TEXT,
    pinned      INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    edited_at   TEXT
);
CREATE INDEX idx_messages_channel ON messages(channel_id, id DESC);

CREATE TABLE attachments (
    id           TEXT PRIMARY KEY,
    message_id   TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    filename     TEXT NOT NULL,
    content_type TEXT NOT NULL,
    size         INTEGER NOT NULL,
    width        INTEGER,
    height       INTEGER,
    url          TEXT NOT NULL
);
CREATE INDEX idx_attachments_message ON attachments(message_id);

CREATE TABLE reactions (
    message_id TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    user_id    TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    emoji      TEXT NOT NULL,
    PRIMARY KEY (message_id, user_id, emoji)
);

CREATE TABLE invites (
    code       TEXT PRIMARY KEY,
    created_by TEXT REFERENCES users(id) ON DELETE SET NULL,
    role_id    TEXT REFERENCES roles(id) ON DELETE SET NULL,
    note       TEXT NOT NULL DEFAULT '',
    max_uses   INTEGER NOT NULL DEFAULT 0, -- 0 = unlimited
    uses       INTEGER NOT NULL DEFAULT 0,
    expires_at TEXT,
    revoked    INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE bans (
    user_id    TEXT PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    reason     TEXT NOT NULL DEFAULT '',
    banned_by  TEXT REFERENCES users(id) ON DELETE SET NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE audit_log (
    id          TEXT PRIMARY KEY,
    actor_id    TEXT REFERENCES users(id) ON DELETE SET NULL,
    action      TEXT NOT NULL,
    target_type TEXT NOT NULL DEFAULT '',
    target_id   TEXT NOT NULL DEFAULT '',
    detail      TEXT NOT NULL DEFAULT '',
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX idx_audit_created ON audit_log(id DESC);

CREATE TABLE webhooks (
    id         TEXT PRIMARY KEY,
    channel_id TEXT NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    name       TEXT NOT NULL,
    token      TEXT NOT NULL UNIQUE,
    avatar_url TEXT,
    created_by TEXT REFERENCES users(id) ON DELETE SET NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE read_state (
    user_id    TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    channel_id TEXT NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    last_read_id TEXT NOT NULL DEFAULT '',
    PRIMARY KEY (user_id, channel_id)
);

INSERT INTO instance (id) VALUES (1);
