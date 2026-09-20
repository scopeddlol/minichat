//! The palette, computed the way the stylesheet computes it.
//!
//! `web/src/index.css` builds every surface with
//! `color-mix(in oklab, <base> calc(100% - var(--tint-strength)), var(--tint))`
//! and derives the accent's soft/glow variants the same way. Slint has no
//! `color-mix`, so the mixing happens here and the results are pushed into the
//! `Theme` global. The conversions are the ones CSS Color 4 specifies, so a
//! given accent and tint produce the same bytes in both clients.

/// A colour as the app handles it: 8-bit sRGB, no alpha.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// Parse `#rrggbb`. Returns `None` for anything else, which is what keeps
    /// a malformed instance accent from blanking the UI.
    pub fn parse(hex: &str) -> Option<Self> {
        let hex = hex.strip_prefix('#')?;
        if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let value = u32::from_str_radix(hex, 16).ok()?;
        Some(Self::new(
            (value >> 16) as u8,
            ((value >> 8) & 0xff) as u8,
            (value & 0xff) as u8,
        ))
    }

    pub fn to_slint(self) -> slint::Color {
        slint::Color::from_rgb_u8(self.r, self.g, self.b)
    }

    pub fn with_alpha(self, alpha: f32) -> slint::Color {
        slint::Color::from_argb_u8(
            (alpha.clamp(0.0, 1.0) * 255.0).round() as u8,
            self.r,
            self.g,
            self.b,
        )
    }

    /// Perceived luminance, used to pick a readable foreground over the
    /// accent. Same coefficients as `applyAccent` in `web/src/lib/theme.ts`.
    fn luminance(self) -> f32 {
        (0.299 * self.r as f32 + 0.587 * self.g as f32 + 0.114 * self.b as f32) / 255.0
    }

    /// The foreground the web client picks for text on the accent.
    pub fn readable_ink(self) -> Rgb {
        if self.luminance() > 0.62 {
            Rgb::new(0x10, 0x13, 0x1c)
        } else {
            Rgb::new(0xff, 0xff, 0xff)
        }
    }
}

// --- sRGB <-> Oklab ------------------------------------------------------
//
// CSS mixes in Oklab, which means going through linear-light sRGB rather than
// lerping the encoded bytes. Mixing the bytes directly gives visibly different
// midpoints — muddier, and darker for the same inputs — so the round trip is
// not optional if the two clients are to match.

fn srgb_to_linear(channel: f32) -> f32 {
    if channel <= 0.04045 {
        channel / 12.92
    } else {
        ((channel + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(channel: f32) -> f32 {
    if channel <= 0.0031308 {
        channel * 12.92
    } else {
        1.055 * channel.powf(1.0 / 2.4) - 0.055
    }
}

// The matrices are quoted at the precision Björn Ottosson published them
// at. Trimming them to what an f32 holds exactly would make this file
// disagree with every other implementation, including the browser's, which
// is the one thing it must not do.
#[allow(clippy::excessive_precision)]
fn to_oklab(colour: Rgb) -> [f32; 3] {
    let r = srgb_to_linear(colour.r as f32 / 255.0);
    let g = srgb_to_linear(colour.g as f32 / 255.0);
    let b = srgb_to_linear(colour.b as f32 / 255.0);

    let l = (0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b).cbrt();
    let m = (0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b).cbrt();
    let s = (0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b).cbrt();

    [
        0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s,
        1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s,
        0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s,
    ]
}

#[allow(clippy::excessive_precision)]
fn from_oklab(lab: [f32; 3]) -> Rgb {
    let [big_l, a, b] = lab;

    let l = (big_l + 0.3963377774 * a + 0.2158037573 * b).powi(3);
    let m = (big_l - 0.1055613458 * a - 0.0638541728 * b).powi(3);
    let s = (big_l - 0.0894841775 * a - 1.2914855480 * b).powi(3);

    let red = 4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s;
    let green = -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s;
    let blue = -0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s;

    let encode = |channel: f32| (linear_to_srgb(channel).clamp(0.0, 1.0) * 255.0).round() as u8;
    Rgb::new(encode(red), encode(green), encode(blue))
}

/// `color-mix(in oklab, base <ratio>%, other)`.
///
/// `ratio` is the weight of `base`, matching the CSS argument order.
pub fn mix(base: Rgb, other: Rgb, ratio: f32) -> Rgb {
    let ratio = ratio.clamp(0.0, 1.0);
    let base = to_oklab(base);
    let other = to_oklab(other);
    from_oklab([
        base[0] * ratio + other[0] * (1.0 - ratio),
        base[1] * ratio + other[1] * (1.0 - ratio),
        base[2] * ratio + other[2] * (1.0 - ratio),
    ])
}

/// The instance's visual identity, as far as the palette is concerned.
#[derive(Clone, Debug)]
pub struct Branding {
    pub accent: Rgb,
    /// The operator's surface tint. `None` leaves every surface at its base,
    /// which is what removing the CSS property does.
    pub tint: Option<Rgb>,
    pub corner_radius: f32,
    pub light: bool,
}

impl Default for Branding {
    fn default() -> Self {
        Self {
            accent: Rgb::new(0x5b, 0x6e, 0xe8),
            tint: None,
            corner_radius: 10.0,
            light: false,
        }
    }
}

/// Base surfaces before any tint, per theme. Straight from `:root` and
/// `html.light` in the stylesheet, in the same order.
struct Surfaces {
    bg: Rgb,
    surface_0: Rgb,
    surface_1: Rgb,
    surface_2: Rgb,
    surface_3: Rgb,
    border: Rgb,
    border_soft: Rgb,
    text: Rgb,
    text_muted: Rgb,
    text_faint: Rgb,
    /// `--tint-strength`: light surfaces take a lighter touch, because the
    /// same strength reads muddy against them.
    tint_strength: f32,
}

const DARK: Surfaces = Surfaces {
    bg: Rgb::new(0x10, 0x11, 0x12),
    surface_0: Rgb::new(0x15, 0x16, 0x17),
    surface_1: Rgb::new(0x1b, 0x1c, 0x1e),
    surface_2: Rgb::new(0x23, 0x25, 0x27),
    surface_3: Rgb::new(0x2c, 0x2e, 0x31),
    border: Rgb::new(0x34, 0x36, 0x39),
    border_soft: Rgb::new(0x29, 0x2b, 0x2e),
    text: Rgb::new(0xed, 0xed, 0xee),
    text_muted: Rgb::new(0xb2, 0xb4, 0xb8),
    text_faint: Rgb::new(0x85, 0x89, 0x8f),
    tint_strength: 0.12,
};

const LIGHT: Surfaces = Surfaces {
    bg: Rgb::new(0xf4, 0xf4, 0xf4),
    surface_0: Rgb::new(0xff, 0xff, 0xff),
    surface_1: Rgb::new(0xfa, 0xfa, 0xfa),
    surface_2: Rgb::new(0xee, 0xee, 0xef),
    surface_3: Rgb::new(0xe4, 0xe4, 0xe6),
    border: Rgb::new(0xd6, 0xd7, 0xd9),
    border_soft: Rgb::new(0xe7, 0xe7, 0xe8),
    text: Rgb::new(0x20, 0x21, 0x24),
    text_muted: Rgb::new(0x5d, 0x60, 0x66),
    text_faint: Rgb::new(0x72, 0x76, 0x7c),
    tint_strength: 0.07,
};

/// Every colour the UI needs, resolved.
#[derive(Clone, Debug)]
pub struct Palette {
    pub bg: Rgb,
    pub surface_0: Rgb,
    pub surface_1: Rgb,
    pub surface_2: Rgb,
    pub surface_3: Rgb,
    pub border: Rgb,
    pub border_soft: Rgb,
    pub text: Rgb,
    pub text_muted: Rgb,
    pub text_faint: Rgb,
    pub accent: Rgb,
    pub accent_hover: Rgb,
    pub accent_ink: Rgb,
    pub light: bool,
    pub corner_radius: f32,
}

pub const SUCCESS: Rgb = Rgb::new(0x34, 0xd3, 0x99);
pub const WARNING: Rgb = Rgb::new(0xfb, 0xbf, 0x24);
pub const DANGER: Rgb = Rgb::new(0xf8, 0x71, 0x71);

/// The alpha `color-mix(in oklab, <colour> N%, transparent)` produces.
///
/// Mixing with `transparent` in CSS is a plain alpha scale — the colour keeps
/// its channels and takes the weight as its opacity — so these stay as alpha
/// rather than being pre-blended against a surface. That matters for the
/// accent-soft pills, which sit on three different surfaces.
pub const ACCENT_SOFT_ALPHA: f32 = 0.18;
pub const ACCENT_GLOW_ALPHA: f32 = 0.38;

impl Palette {
    pub fn resolve(branding: &Branding) -> Self {
        let base = if branding.light { &LIGHT } else { &DARK };

        // Each surface mixes `(100% - tint-strength)` of itself with the
        // tint. With no tint the CSS falls back to the surface's own colour,
        // so the mix is the identity — reproduced here by skipping it.
        let tinted = |colour: Rgb| match branding.tint {
            Some(tint) => mix(colour, tint, 1.0 - base.tint_strength),
            None => colour,
        };

        Self {
            bg: tinted(base.bg),
            surface_0: tinted(base.surface_0),
            surface_1: tinted(base.surface_1),
            surface_2: tinted(base.surface_2),
            surface_3: tinted(base.surface_3),
            border: tinted(base.border),
            border_soft: tinted(base.border_soft),
            text: base.text,
            text_muted: base.text_muted,
            text_faint: base.text_faint,
            accent: branding.accent,
            // `.btn-primary:hover` — the accent lifted 12% toward white.
            accent_hover: mix(branding.accent, Rgb::new(0xff, 0xff, 0xff), 0.88),
            accent_ink: branding.accent.readable_ink(),
            light: branding.light,
            corner_radius: branding.corner_radius.clamp(0.0, 28.0),
        }
    }
}

/// Push a palette into the `Theme` global so the whole interface retints.
pub fn apply(app: &crate::ui::App, palette: &Palette) {
    use crate::ui::Theme as Tokens;
    use slint::ComponentHandle;

    let theme = app.global::<Tokens>();

    theme.set_bg(palette.bg.to_slint());
    theme.set_surface_0(palette.surface_0.to_slint());
    theme.set_surface_1(palette.surface_1.to_slint());
    theme.set_surface_2(palette.surface_2.to_slint());
    theme.set_surface_3(palette.surface_3.to_slint());
    theme.set_border(palette.border.to_slint());
    theme.set_border_soft(palette.border_soft.to_slint());

    theme.set_text(palette.text.to_slint());
    theme.set_text_muted(palette.text_muted.to_slint());
    theme.set_text_faint(palette.text_faint.to_slint());

    theme.set_accent(palette.accent.to_slint());
    theme.set_accent_soft(palette.accent.with_alpha(ACCENT_SOFT_ALPHA));
    theme.set_accent_glow(palette.accent.with_alpha(ACCENT_GLOW_ALPHA));
    theme.set_accent_ink(palette.accent_ink.to_slint());
    theme.set_accent_hover(palette.accent_hover.to_slint());

    theme.set_success(SUCCESS.to_slint());
    theme.set_warning(WARNING.to_slint());
    theme.set_danger(DANGER.to_slint());
    // `.btn-danger` mixes the danger colour with transparency at 16/30/26%.
    theme.set_danger_bg(DANGER.with_alpha(0.16));
    theme.set_danger_border(DANGER.with_alpha(0.30));
    theme.set_danger_hover(DANGER.with_alpha(0.26));

    theme.set_radius_card(palette.corner_radius);
    theme.set_light(palette.light);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_only_six_digit_hex() {
        assert_eq!(Rgb::parse("#5b6ee8"), Some(Rgb::new(0x5b, 0x6e, 0xe8)));
        assert_eq!(Rgb::parse("5b6ee8"), None);
        assert_eq!(Rgb::parse("#5b6ee"), None);
        assert_eq!(Rgb::parse("#5b6ee8ff"), None);
        assert_eq!(Rgb::parse("#zzzzzz"), None);
        assert_eq!(Rgb::parse(""), None);
    }

    #[test]
    fn oklab_round_trips() {
        // Every channel combination at the extremes, plus the default accent,
        // has to survive the conversion unchanged or the palette drifts every
        // time the theme is reapplied.
        for colour in [
            Rgb::new(0, 0, 0),
            Rgb::new(255, 255, 255),
            Rgb::new(0x5b, 0x6e, 0xe8),
            Rgb::new(0x10, 0x11, 0x12),
            Rgb::new(0xf8, 0x71, 0x71),
        ] {
            let round_tripped = from_oklab(to_oklab(colour));
            assert_eq!(round_tripped, colour, "{colour:?} did not round-trip");
        }
    }

    #[test]
    fn mixing_at_the_extremes_is_the_identity() {
        let a = Rgb::new(0x10, 0x11, 0x12);
        let b = Rgb::new(0xf4, 0xf4, 0xf4);
        assert_eq!(mix(a, b, 1.0), a);
        assert_eq!(mix(a, b, 0.0), b);
    }

    #[test]
    fn an_absent_tint_leaves_the_base_palette_alone() {
        let palette = Palette::resolve(&Branding::default());
        assert_eq!(palette.bg, DARK.bg);
        assert_eq!(palette.surface_1, DARK.surface_1);
        assert_eq!(palette.border, DARK.border);
    }

    #[test]
    fn a_tint_moves_every_surface_but_not_the_text() {
        let branding = Branding {
            tint: Some(Rgb::new(0x8b, 0x5c, 0xf6)),
            ..Branding::default()
        };
        let palette = Palette::resolve(&branding);
        assert_ne!(palette.bg, DARK.bg);
        assert_ne!(palette.surface_2, DARK.surface_2);
        // Text colours are stated outright in the stylesheet, not mixed.
        assert_eq!(palette.text, DARK.text);
        assert_eq!(palette.text_faint, DARK.text_faint);
    }

    #[test]
    fn ink_follows_the_accent_brightness() {
        // A pale accent needs dark text on it; a saturated one needs white.
        assert_eq!(
            Rgb::new(0xfb, 0xbf, 0x24).readable_ink(),
            Rgb::new(0x10, 0x13, 0x1c)
        );
        assert_eq!(
            Rgb::new(0x5b, 0x6e, 0xe8).readable_ink(),
            Rgb::new(0xff, 0xff, 0xff)
        );
    }

    #[test]
    fn the_corner_radius_stays_in_the_range_the_operator_is_offered() {
        let clamped = |radius: f32| {
            Palette::resolve(&Branding {
                corner_radius: radius,
                ..Branding::default()
            })
            .corner_radius
        };
        assert_eq!(clamped(-5.0), 0.0);
        assert_eq!(clamped(100.0), 28.0);
        assert_eq!(clamped(14.0), 14.0);
    }
}
