-- v0.3: deeper instance customisation

-- ---- Theming -------------------------------------------------------------
-- Which theme members get before they choose one for themselves.
ALTER TABLE instance ADD COLUMN theme_mode TEXT NOT NULL DEFAULT 'dark'; -- dark | light | system
-- Tints every surface toward this hue, so an instance can read warm or cool
-- rather than only carrying a coloured accent.
ALTER TABLE instance ADD COLUMN surface_tint TEXT;
ALTER TABLE instance ADD COLUMN corner_radius INTEGER NOT NULL DEFAULT 14;
ALTER TABLE instance ADD COLUMN font_family TEXT NOT NULL DEFAULT '';
-- Escape hatch for anything the settings above don't cover. Operator-only.
ALTER TABLE instance ADD COLUMN custom_css TEXT NOT NULL DEFAULT '';

-- ---- The pages outsiders see ---------------------------------------------
ALTER TABLE instance ADD COLUMN login_headline TEXT NOT NULL DEFAULT '';
ALTER TABLE instance ADD COLUMN login_body TEXT NOT NULL DEFAULT '';
ALTER TABLE instance ADD COLUMN login_image_url TEXT;

-- ---- Channel polish ------------------------------------------------------
-- An emoji shown in place of the # / speaker glyph.
ALTER TABLE channels ADD COLUMN emoji TEXT NOT NULL DEFAULT '';
-- A short line under the channel name in the sidebar. Distinct from `topic`,
-- which is the longer text in the channel header.
ALTER TABLE channels ADD COLUMN description TEXT NOT NULL DEFAULT '';

-- ---- Role polish ---------------------------------------------------------
-- A small image shown beside the name of anyone holding the role.
ALTER TABLE roles ADD COLUMN icon_url TEXT;
-- A short text badge, e.g. "MOD". Falls back to nothing when empty.
ALTER TABLE roles ADD COLUMN badge TEXT NOT NULL DEFAULT '';
