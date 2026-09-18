-- v0.4: channel and category permissions, friends and favourites

-- ---- Categories that can be private --------------------------------------
-- A private category hides itself and everything synced to it.
ALTER TABLE categories ADD COLUMN is_private INTEGER NOT NULL DEFAULT 0;

-- Permissions held by the category. Channels that sync take a copy of these;
-- see sync_category below and `resync_category` in routes/channels.rs.
CREATE TABLE category_overwrites (
    category_id TEXT NOT NULL REFERENCES categories(id) ON DELETE CASCADE,
    role_id     TEXT NOT NULL REFERENCES roles(id) ON DELETE CASCADE,
    allow       INTEGER NOT NULL DEFAULT 0,
    deny        INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (category_id, role_id)
);

-- Whether this channel inherits its category's permissions.
--
-- Copied on write rather than resolved on read: four separate places resolve
-- channel overwrites (channel_permissions, channel_permission_map,
-- visible_channel_ids, channel_viewers) and keeping inheritance out of all
-- four is worth a little fan-out when a category changes.
--
-- Defaults to 0 so an upgrade never silently rewrites the permissions of a
-- channel someone already configured by hand. New channels created inside a
-- category start synced.
ALTER TABLE channels ADD COLUMN sync_category INTEGER NOT NULL DEFAULT 0;

-- ---- Friends and favourites ----------------------------------------------
-- One row per direction, so a request and an acceptance are separate facts and
-- a block doesn't need the other side's agreement.
--
--   kind = 'friend'    accepted, mutual (both directions exist)
--   kind = 'pending'   user_id sent a request to other_id
--   kind = 'blocked'   user_id blocked other_id
CREATE TABLE relationships (
    user_id    TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    other_id   TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    kind       TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (user_id, other_id),
    CHECK (user_id <> other_id)
);

CREATE INDEX idx_relationships_other ON relationships(other_id, kind);

-- Favouriting is one-sided and needs no consent, so it is a flag rather than a
-- relationship kind: you can favourite someone who is not a friend.
CREATE TABLE favourites (
    user_id    TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    other_id   TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (user_id, other_id),
    CHECK (user_id <> other_id)
);

-- ---- Avatar and banner framing -------------------------------------------
-- How the member positioned their images, as percentages, so a wide banner can
-- be pulled to the interesting part instead of always centre-cropped.
-- Stored rather than baked into the file so re-framing costs no upload.
ALTER TABLE users ADD COLUMN avatar_x REAL NOT NULL DEFAULT 50;
ALTER TABLE users ADD COLUMN avatar_y REAL NOT NULL DEFAULT 50;
ALTER TABLE users ADD COLUMN avatar_zoom REAL NOT NULL DEFAULT 1;
ALTER TABLE users ADD COLUMN banner_x REAL NOT NULL DEFAULT 50;
ALTER TABLE users ADD COLUMN banner_y REAL NOT NULL DEFAULT 50;
ALTER TABLE users ADD COLUMN banner_zoom REAL NOT NULL DEFAULT 1;
