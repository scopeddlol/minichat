-- v0.2: push notifications, mentions, custom emoji, notification preferences

-- Web Push subscriptions. One row per browser/device a member has enabled
-- notifications on; the endpoint is unique per subscription.
CREATE TABLE push_subscriptions (
    id          TEXT PRIMARY KEY,
    user_id     TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    endpoint    TEXT NOT NULL UNIQUE,
    p256dh      TEXT NOT NULL,
    auth        TEXT NOT NULL,
    user_agent  TEXT NOT NULL DEFAULT '',
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    last_used_at TEXT
);
CREATE INDEX idx_push_user ON push_subscriptions(user_id);

-- Resolved mentions, written when a message is created or edited. Keeping them
-- in their own table makes "how many unread mentions in this channel" a single
-- indexed query instead of a scan over message bodies.
CREATE TABLE mentions (
    message_id TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    user_id    TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    channel_id TEXT NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    -- 'direct' for @username, 'everyone' for @everyone/@here.
    kind       TEXT NOT NULL DEFAULT 'direct',
    PRIMARY KEY (message_id, user_id)
);
CREATE INDEX idx_mentions_user_channel ON mentions(user_id, channel_id, message_id DESC);

-- Per-channel notification overrides. The absence of a row means "inherit the
-- member's global setting".
CREATE TABLE channel_notifications (
    user_id    TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    channel_id TEXT NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    -- all | mentions | none
    mode       TEXT NOT NULL DEFAULT 'mentions',
    PRIMARY KEY (user_id, channel_id)
);

-- Custom emoji, usable as :name: in messages and as reactions.
CREATE TABLE emojis (
    id         TEXT PRIMARY KEY,
    name       TEXT NOT NULL UNIQUE,
    url        TEXT NOT NULL,
    created_by TEXT REFERENCES users(id) ON DELETE SET NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Member-wide notification default, applied where no channel override exists.
ALTER TABLE users ADD COLUMN notification_mode TEXT NOT NULL DEFAULT 'mentions';
