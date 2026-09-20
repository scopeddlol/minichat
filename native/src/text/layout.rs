//! Inline flow layout for message bodies.
//!
//! Slint has no rich text: a `Text` element carries one font, one weight and
//! one colour. A chat message is the opposite of that — bold inside a
//! sentence, a mention pill mid-line, inline code, a custom emoji between two
//! words — and all of it has to wrap as one paragraph.
//!
//! So the flow is computed here and handed to Slint as positioned pieces.
//! Widths are measured with rustybuzz against the *same font files* the
//! renderer draws with (see `fonts::measurement_face`), which is what keeps a
//! measured advance and a painted advance the same number. Measuring against
//! anything else — a metrics table, an estimate, a different copy of Inter —
//! shows up immediately as runs that overlap or stop short of the margin.

use std::collections::HashMap;

use rustybuzz::{Face, UnicodeBuffer};

use super::markdown::{Block, Inline, Kind, Style};

/// A piece of text placed at a point, ready to draw.
#[derive(Clone, Debug)]
pub struct Placed {
    pub text: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub style: Style,
    /// The size the run was measured at, and must be painted at. Carried
    /// rather than recomputed in the UI: `.md code` is 0.86em and
    /// `.md pre code` is 0.84em, and a renderer that picks the wrong one
    /// paints text wider than the box that was measured for it.
    pub size: f32,
    /// The link target, mentioned member's ID, or emoji name, by kind.
    pub target: String,
    /// Set for a custom emoji, which is drawn instead of the text.
    pub emoji_url: String,
}

/// A background drawn under some runs: a code block's panel, a quote's bar,
/// a mention's pill, an inline code chip.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Decoration {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub kind: DecorationKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecorationKind {
    /// `.md pre` — a panel behind a fenced code block.
    CodeBlock,
    /// `.md code` — a chip behind inline code.
    CodeChip,
    /// `.md blockquote` — the 3px bar down the left.
    QuoteBar,
    /// `.md .mention` — an accent pill.
    Mention,
    /// `.md .mention-self` — the warning-tinted pill.
    MentionSelf,
    /// `.md .spoiler` — a solid block hiding the text until clicked.
    Spoiler,
}

#[derive(Clone, Debug, Default)]
pub struct Laid {
    pub runs: Vec<Placed>,
    pub decorations: Vec<Decoration>,
    pub height: f32,
}

/// Type metrics for message bodies, from `.prose-chat` and `.direct-message`
/// in the stylesheet: 14px at a 1.65 line height.
pub const BODY_SIZE: f32 = 14.0;
pub const LINE_HEIGHT: f32 = 1.65;
/// `.md pre` padding: `0.7rem 0.85rem`.
const CODE_PAD_X: f32 = 13.6;
const CODE_PAD_Y: f32 = 11.2;
/// `.md blockquote { padding-left: .75rem }`, after a 3px bar.
const QUOTE_INDENT: f32 = 15.0;
/// `.md .mention { padding: .05rem .3rem }`.
const MENTION_PAD_X: f32 = 4.8;
/// `.md code { padding: .1rem .34rem }`.
const CHIP_PAD_X: f32 = 5.4;
/// Custom emoji render at 1.4em.
const EMOJI_SCALE: f32 = 1.4;
/// `.md code { font-size: .86em }` and `.md pre code { font-size: .84em }`.
/// They genuinely differ, so both are named rather than one being reused.
const INLINE_CODE_SIZE: f32 = BODY_SIZE * 0.86;
const BLOCK_CODE_SIZE: f32 = BODY_SIZE * 0.84;
/// The gap the stylesheet leaves around a code block (`margin: .4rem 0`).
const BLOCK_GAP: f32 = 6.4;

/// Which bundled face a run is drawn with.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum FaceKey {
    Regular,
    Medium,
    SemiBold,
    Bold,
    Italic,
    Mono,
}

fn face_for(style: &Style) -> FaceKey {
    if style.code {
        return FaceKey::Mono;
    }
    if style.bold {
        return FaceKey::Bold;
    }
    if style.italic {
        return FaceKey::Italic;
    }
    match style.kind {
        // Mentions render at weight 560, links at the body weight.
        Kind::Mention | Kind::MentionSelf => FaceKey::Medium,
        _ => FaceKey::Regular,
    }
}

/// The measuring apparatus: the parsed faces, plus a cache.
///
/// Shaping every word of every message on every repaint would be the single
/// most expensive thing the client does. Widths depend only on the text, the
/// face and the size, so they cache cleanly and the cache survives scrolling,
/// re-theming and window resizes.
pub struct Measurer {
    faces: HashMap<FaceKey, Face<'static>>,
    cache: std::cell::RefCell<HashMap<(FaceKey, u32, String), f32>>,
}

impl Measurer {
    pub fn new() -> Self {
        let mut faces = HashMap::new();
        let sources: [(FaceKey, &'static [u8]); 6] = [
            (FaceKey::Regular, crate::fonts::INTER_REGULAR),
            (FaceKey::Medium, crate::fonts::INTER_MEDIUM),
            (FaceKey::SemiBold, crate::fonts::INTER_SEMIBOLD),
            (FaceKey::Bold, crate::fonts::INTER_BOLD),
            (FaceKey::Italic, crate::fonts::INTER_ITALIC),
            (FaceKey::Mono, crate::fonts::MONO_REGULAR),
        ];
        for (key, bytes) in sources {
            if let Some(face) = Face::from_slice(bytes, 0) {
                faces.insert(key, face);
            }
        }
        Self {
            faces,
            cache: std::cell::RefCell::new(HashMap::new()),
        }
    }

    /// The advance width of `text`, in pixels, at `size`.
    fn width(&self, text: &str, key: FaceKey, size: f32) -> f32 {
        if text.is_empty() {
            return 0.0;
        }
        let cache_key = (key, size.to_bits(), text.to_string());
        if let Some(&cached) = self.cache.borrow().get(&cache_key) {
            return cached;
        }

        let Some(face) = self
            .faces
            .get(&key)
            .or_else(|| self.faces.get(&FaceKey::Regular))
        else {
            // No bundled font parsed, which the font test would have caught.
            // Fall back to a monospace-ish estimate rather than zero, so text
            // still flows instead of piling up at x = 0.
            return text.chars().count() as f32 * size * 0.5;
        };

        let mut buffer = UnicodeBuffer::new();
        buffer.push_str(text);
        let shaped = rustybuzz::shape(face, &[], buffer);
        let units: i32 = shaped.glyph_positions().iter().map(|p| p.x_advance).sum();
        let width = units as f32 * size / face.units_per_em() as f32;

        self.cache.borrow_mut().insert(cache_key, width);
        width
    }

    /// Drop cached widths.
    ///
    /// Called when the bundled faces would change, which today means never —
    /// it exists so a future custom-font setting has a way to invalidate
    /// without restarting.
    #[allow(dead_code)]
    pub fn clear(&self) {
        self.cache.borrow_mut().clear();
    }

    /// How many measurements are held. Used by the tests to prove the cache
    /// is actually being hit rather than silently reshaping every time.
    #[cfg(test)]
    pub fn cached_entries(&self) -> usize {
        self.cache.borrow().len()
    }
}

impl Default for Measurer {
    fn default() -> Self {
        Self::new()
    }
}

/// A single atom of the flow: a word, a space, an emoji, or a whole pill.
struct Atom {
    text: String,
    /// The advance of the text itself, without any padding.
    width: f32,
    /// The size `width` was measured at.
    size: f32,
    /// Space reserved either side of the text for a background that is wider
    /// than its glyphs — a mention pill, an inline-code chip. Without this
    /// the pill is painted over whatever follows it.
    pad: f32,
    style: Style,
    target: String,
    emoji_url: String,
    /// Whitespace can be dropped at a line break; a word cannot.
    breakable: bool,
}

impl Atom {
    /// What the atom advances the cursor by.
    fn advance(&self) -> f32 {
        self.width + self.pad * 2.0
    }
}

/// Lay a parsed message body out into `max_width`.
pub fn layout(blocks: &[Block], max_width: f32, measurer: &Measurer) -> Laid {
    let mut out = Laid::default();
    let mut y = 0.0f32;
    let line_height = (BODY_SIZE * LINE_HEIGHT).round();

    for (index, block) in blocks.iter().enumerate() {
        match block {
            Block::Paragraph(inlines) => {
                // An empty line in the source is a blank line in the output,
                // not a collapsed nothing.
                if inlines.is_empty() {
                    y += line_height;
                    continue;
                }
                y = flow(inlines, 0.0, max_width, y, measurer, &mut out);
            }

            Block::Quote(lines) => {
                let start = y;
                for line in lines {
                    y = flow(
                        line,
                        QUOTE_INDENT,
                        max_width - QUOTE_INDENT,
                        y,
                        measurer,
                        &mut out,
                    );
                }
                out.decorations.push(Decoration {
                    x: 0.0,
                    y: start,
                    width: 3.0,
                    height: (y - start).max(line_height),
                    kind: DecorationKind::QuoteBar,
                });
            }

            Block::Code { text, .. } => {
                if index > 0 {
                    y += BLOCK_GAP;
                }
                let start = y;
                let mono_line = (BLOCK_CODE_SIZE * 1.55).round();
                let size = BLOCK_CODE_SIZE;
                let mut inner = y + CODE_PAD_Y;
                for line in text.split('\n') {
                    let width = measurer.width(line, FaceKey::Mono, size);
                    out.runs.push(Placed {
                        text: line.to_string(),
                        x: CODE_PAD_X,
                        y: inner,
                        width,
                        height: mono_line,
                        style: Style {
                            code: true,
                            ..Default::default()
                        },
                        size,
                        target: String::new(),
                        emoji_url: String::new(),
                    });
                    inner += mono_line;
                }
                y = inner + CODE_PAD_Y;
                out.decorations.push(Decoration {
                    x: 0.0,
                    y: start,
                    width: max_width,
                    height: y - start,
                    kind: DecorationKind::CodeBlock,
                });
                y += BLOCK_GAP;
            }
        }
    }

    out.height = y;
    out
}

/// Flow one line's worth of inline content, wrapping as needed. Returns the
/// y the next line starts at.
fn flow(
    inlines: &[Inline],
    indent: f32,
    max_width: f32,
    start_y: f32,
    measurer: &Measurer,
    out: &mut Laid,
) -> f32 {
    let line_height = (BODY_SIZE * LINE_HEIGHT).round();
    let atoms = atomise(inlines, measurer);

    let mut x = indent;
    let mut y = start_y;
    // Runs on the current line that still need a decoration drawn under them,
    // paired with where the decoration starts.
    let mut pending: Vec<(usize, DecorationKind, f32)> = Vec::new();

    for atom in atoms {
        // Wrap before placing, unless this is the first thing on the line —
        // a single word longer than the column has to go somewhere.
        if x > indent && x + atom.advance() > max_width {
            flush_decorations(out, &mut pending, line_height);
            x = indent;
            y += line_height;
            if atom.breakable {
                // A space that fell at a break is not drawn.
                continue;
            }
        }

        if atom.breakable && x == indent && atom.text.trim().is_empty() {
            // Leading whitespace after a wrap: skip it rather than indent.
            continue;
        }

        let is_emoji = !atom.emoji_url.is_empty();
        let height = if is_emoji {
            BODY_SIZE * EMOJI_SCALE
        } else {
            line_height
        };

        let decoration = match atom.style.kind {
            Kind::Mention => Some(DecorationKind::Mention),
            Kind::MentionSelf => Some(DecorationKind::MentionSelf),
            _ if atom.style.spoiler => Some(DecorationKind::Spoiler),
            _ if atom.style.code => Some(DecorationKind::CodeChip),
            _ => None,
        };

        let pad = atom.pad;
        let advance = atom.advance();
        out.runs.push(Placed {
            text: atom.text,
            // The text sits inside its padding; the decoration below covers
            // the padding too.
            x: x + pad,
            y: if is_emoji {
                y + (line_height - height) / 2.0
            } else {
                y
            },
            width: atom.width,
            height,
            style: atom.style,
            size: atom.size,
            target: atom.target,
            emoji_url: atom.emoji_url,
        });

        if let Some(kind) = decoration {
            pending.push((out.runs.len() - 1, kind, pad));
        }

        x += advance;
    }

    flush_decorations(out, &mut pending, line_height);
    y + line_height
}

/// Turn the runs that were marked during placement into decoration rects.
///
/// Done per line rather than per run so a mention that wrapped gets one pill
/// per line, which is what a browser does with an inline background.
fn flush_decorations(out: &mut Laid, pending: &mut Vec<(usize, DecorationKind, f32)>, line: f32) {
    for (index, kind, pad) in pending.drain(..) {
        let run = &out.runs[index];
        out.decorations.push(Decoration {
            x: run.x - pad,
            y: run.y + (line - BODY_SIZE * 1.35) / 2.0,
            width: run.width + pad * 2.0,
            height: BODY_SIZE * 1.35,
            kind,
        });
    }
}

/// Split inline content into wrappable atoms, measuring each.
fn atomise(inlines: &[Inline], measurer: &Measurer) -> Vec<Atom> {
    let mut atoms = Vec::new();

    for inline in inlines {
        match inline {
            Inline::Emoji { name, url } => atoms.push(Atom {
                text: format!(":{name}:"),
                width: BODY_SIZE * EMOJI_SCALE,
                size: BODY_SIZE,
                pad: 0.0,
                style: Style {
                    kind: Kind::Emoji,
                    ..Default::default()
                },
                target: name.clone(),
                emoji_url: url.clone(),
                breakable: false,
            }),

            // A mention or a link is one atom: neither should be split across
            // a line, because the pill and the underline would be split too.
            Inline::Mention { run, id } => {
                let key = face_for(&run.style);
                atoms.push(Atom {
                    width: measurer.width(&run.text, key, BODY_SIZE),
                    size: BODY_SIZE,
                    pad: MENTION_PAD_X,
                    text: run.text.clone(),
                    style: run.style,
                    target: id.clone(),
                    emoji_url: String::new(),
                    breakable: false,
                });
            }
            Inline::Link { run, href } => {
                let key = face_for(&run.style);
                atoms.push(Atom {
                    width: measurer.width(&run.text, key, BODY_SIZE),
                    size: BODY_SIZE,
                    pad: 0.0,
                    text: run.text.clone(),
                    style: run.style,
                    target: href.clone(),
                    emoji_url: String::new(),
                    breakable: false,
                });
            }

            // Inline code is one atom, chip and all. Splitting it per word
            // would give `let x = 1` four chips with gaps between them.
            Inline::Text(run) if run.style.code => atoms.push(Atom {
                width: measurer.width(&run.text, FaceKey::Mono, INLINE_CODE_SIZE),
                size: INLINE_CODE_SIZE,
                pad: CHIP_PAD_X,
                text: run.text.clone(),
                style: run.style,
                target: String::new(),
                emoji_url: String::new(),
                breakable: false,
            }),

            Inline::Text(run) => {
                let key = face_for(&run.style);
                for piece in split_keeping_spaces(&run.text) {
                    atoms.push(Atom {
                        width: measurer.width(piece, key, BODY_SIZE),
                        size: BODY_SIZE,
                        pad: 0.0,
                        text: piece.to_string(),
                        style: run.style,
                        target: String::new(),
                        emoji_url: String::new(),
                        breakable: piece.trim().is_empty(),
                    });
                }
            }
        }
    }

    atoms
}

/// Split into words and the whitespace between them, keeping both.
///
/// The spaces are kept as their own atoms so a line that wraps can drop the
/// one at the break without the words on either side running together.
fn split_keeping_spaces(text: &str) -> Vec<&str> {
    let mut pieces = Vec::new();
    let mut start = 0usize;
    let mut in_space: Option<bool> = None;

    for (index, ch) in text.char_indices() {
        let space = ch.is_whitespace();
        match in_space {
            Some(previous) if previous != space => {
                pieces.push(&text[start..index]);
                start = index;
                in_space = Some(space);
            }
            None => in_space = Some(space),
            _ => {}
        }
    }
    if start < text.len() {
        pieces.push(&text[start..]);
    }
    pieces
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::markdown;

    fn context() -> markdown::Context<'static> {
        markdown::Context {
            members: &[],
            emojis: &[],
            me_id: "",
        }
    }

    #[test]
    fn splitting_keeps_words_and_spaces_separate() {
        assert_eq!(split_keeping_spaces("a b"), vec!["a", " ", "b"]);
        assert_eq!(split_keeping_spaces("  lead"), vec!["  ", "lead"]);
        assert_eq!(split_keeping_spaces("trail  "), vec!["trail", "  "]);
        assert_eq!(split_keeping_spaces(""), Vec::<&str>::new());
    }

    #[test]
    fn measuring_is_stable_and_proportional_to_size() {
        let measurer = Measurer::new();
        let small = measurer.width("hello world", FaceKey::Regular, 14.0);
        let large = measurer.width("hello world", FaceKey::Regular, 28.0);
        assert!(small > 0.0);
        // Twice the size is twice the advance, within rounding.
        assert!((large / small - 2.0).abs() < 0.01, "{small} then {large}");
    }

    #[test]
    fn widths_are_cached_rather_than_reshaped() {
        let measurer = Measurer::new();
        assert_eq!(measurer.cached_entries(), 0);
        measurer.width("repeat me", FaceKey::Regular, 14.0);
        measurer.width("repeat me", FaceKey::Regular, 14.0);
        assert_eq!(measurer.cached_entries(), 1);
        // A different size is a different measurement.
        measurer.width("repeat me", FaceKey::Regular, 15.0);
        assert_eq!(measurer.cached_entries(), 2);
    }

    #[test]
    fn a_long_paragraph_wraps_within_the_column() {
        let measurer = Measurer::new();
        let blocks = markdown::parse(
            "the quick brown fox jumps over the lazy dog and keeps on running \
             well past the end of any one line",
            &context(),
        );
        let laid = layout(&blocks, 240.0, &measurer);

        assert!(laid.runs.len() > 1);
        // Nothing may be placed past the column, and the block must be taller
        // than one line — which together is what "it wrapped" means.
        for run in &laid.runs {
            assert!(
                run.x + run.width <= 240.5,
                "run {:?} ends at {}",
                run.text,
                run.x + run.width
            );
        }
        assert!(laid.height > BODY_SIZE * LINE_HEIGHT * 1.5);
    }

    #[test]
    fn a_word_longer_than_the_column_is_still_placed() {
        let measurer = Measurer::new();
        let blocks = markdown::parse(&"x".repeat(200), &context());
        let laid = layout(&blocks, 100.0, &measurer);
        // It overflows rather than vanishing: dropping it would lose content.
        assert_eq!(laid.runs.len(), 1);
        assert_eq!(laid.runs[0].x, 0.0);
    }

    #[test]
    fn a_code_block_gets_a_panel_behind_every_line() {
        let measurer = Measurer::new();
        let blocks = markdown::parse("```rust\nfn main() {}\nlet x = 1;\n```", &context());
        let laid = layout(&blocks, 400.0, &measurer);

        assert_eq!(laid.runs.len(), 2);
        assert!(laid.runs.iter().all(|r| r.style.code));
        let panels: Vec<_> = laid
            .decorations
            .iter()
            .filter(|d| d.kind == DecorationKind::CodeBlock)
            .collect();
        assert_eq!(panels.len(), 1);
        // The panel has to cover both lines plus its padding.
        assert!(panels[0].height > laid.runs[1].y - laid.runs[0].y);
    }

    #[test]
    fn a_quote_gets_a_bar_spanning_its_lines() {
        let measurer = Measurer::new();
        let blocks = markdown::parse("> first\n> second", &context());
        let laid = layout(&blocks, 400.0, &measurer);

        let bars: Vec<_> = laid
            .decorations
            .iter()
            .filter(|d| d.kind == DecorationKind::QuoteBar)
            .collect();
        assert_eq!(bars.len(), 1);
        assert_eq!(bars[0].width, 3.0);
        assert!(bars[0].height >= BODY_SIZE * LINE_HEIGHT * 2.0 - 1.0);
        // Quoted text is indented past the bar.
        assert!(laid.runs.iter().all(|r| r.x >= QUOTE_INDENT));
    }

    #[test]
    fn an_empty_body_lays_out_to_nothing() {
        let measurer = Measurer::new();
        let laid = layout(&markdown::parse("", &context()), 400.0, &measurer);
        assert!(laid.runs.is_empty());
        assert_eq!(laid.height, 0.0);
    }
}
