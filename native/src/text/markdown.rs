//! The markdown subset MiniChat messages use, mirroring
//! `web/src/lib/markdown.tsx`.
//!
//! The web client renders straight to React nodes and never inserts HTML, so
//! a message cannot inject markup no matter what it says. The same property
//! holds here for a different reason: the output is a tree of typed spans, and
//! nothing in it can become a control character or an element.
//!
//! Precedence matters and is not obvious: the rules are tried in order at
//! *every* position and the earliest match in the string wins, with ties going
//! to the earlier rule. That is what makes `` `**not bold**` `` stay literal
//! inside code, and it is reproduced here rather than approximated.

use crate::api::types::{Emoji, Member};

/// A contiguous run of text with one set of attributes.
#[derive(Clone, Debug, PartialEq)]
pub struct Run {
    pub text: String,
    pub style: Style,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Style {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub code: bool,
    pub spoiler: bool,
    pub kind: Kind,
}

/// What a run *is*, beyond how it is drawn. Anything other than `Plain` is
/// interactive, so the renderer needs to know which is which.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Kind {
    #[default]
    Plain,
    /// A hyperlink; the target is carried alongside.
    Link,
    /// A mention of a member the client knows about.
    Mention,
    /// A mention of the signed-in member, or of @everyone / @here.
    MentionSelf,
    /// A custom emoji, drawn as an image.
    Emoji,
}

/// An inline element: either styled text, or something drawn in its place.
#[derive(Clone, Debug, PartialEq)]
pub enum Inline {
    Text(Run),
    /// `href` is the full URL; the run carries the display text, which may be
    /// elided.
    Link { run: Run, href: String },
    /// `id` is the member's ID, so a click can open their profile.
    Mention { run: Run, id: String },
    /// A custom emoji, drawn at 1.4× the line's text size.
    Emoji { name: String, url: String },
}

/// A block-level element.
#[derive(Clone, Debug, PartialEq)]
pub enum Block {
    /// One line of inline content. Consecutive paragraphs are separate blocks
    /// because the source had a newline between them.
    Paragraph(Vec<Inline>),
    /// A fenced code block, kept verbatim.
    Code { language: String, text: String },
    /// One or more `>` lines.
    Quote(Vec<Vec<Inline>>),
}

/// What the parser needs to resolve mentions and emoji.
pub struct Context<'a> {
    pub members: &'a [Member],
    pub emojis: &'a [Emoji],
    pub me_id: &'a str,
}

impl Context<'_> {
    fn member_by_username(&self, name: &str) -> Option<&Member> {
        self.members
            .iter()
            .find(|m| m.username.eq_ignore_ascii_case(name))
    }

    fn emoji_by_name(&self, name: &str) -> Option<&Emoji> {
        let lowered = name.to_lowercase();
        self.emojis.iter().find(|e| e.name == lowered)
    }
}

/// Parse a message body into blocks.
pub fn parse(content: &str, context: &Context<'_>) -> Vec<Block> {
    if content.is_empty() {
        return Vec::new();
    }

    let mut blocks = Vec::new();
    for segment in split_fences(content) {
        match segment {
            Segment::Text(text) => blocks.extend(parse_blocks(text, context)),
            Segment::Code { language, text } => blocks.push(Block::Code {
                language: language.to_string(),
                // The closing fence's own newline is not part of the code.
                text: text.strip_suffix('\n').unwrap_or(text).to_string(),
            }),
        }
    }
    blocks
}

enum Segment<'a> {
    Text(&'a str),
    Code { language: &'a str, text: &'a str },
}

/// Split on ``` fences. Extracted before anything else so nothing inside a
/// code block is parsed as markdown.
fn split_fences(content: &str) -> Vec<Segment<'_>> {
    let mut segments = Vec::new();
    let mut rest = content;

    while let Some(open) = rest.find("```") {
        let (before, after_open) = rest.split_at(open);
        if !before.is_empty() {
            segments.push(Segment::Text(before));
        }
        let after_open = &after_open[3..];

        let Some(close) = after_open.find("```") else {
            // An unclosed fence is literal text, not a code block that
            // swallows the rest of the message.
            segments.push(Segment::Text(&rest[open..]));
            return segments;
        };

        let body = &after_open[..close];
        // An info string is the first line, when that line is a bare word.
        let (language, text) = match body.split_once('\n') {
            Some((first, remainder))
                if !first.is_empty()
                    && first
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || "+#.-".contains(c)) =>
            {
                (first, remainder)
            }
            _ => ("", body),
        };
        segments.push(Segment::Code { language, text });
        rest = &after_open[close + 3..];
    }

    if !rest.is_empty() {
        segments.push(Segment::Text(rest));
    }
    segments
}

fn parse_blocks(text: &str, context: &Context<'_>) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut quote: Vec<Vec<Inline>> = Vec::new();

    for line in text.split('\n') {
        if let Some(quoted) = strip_quote(line) {
            quote.push(parse_inline(quoted, context));
            continue;
        }
        if !quote.is_empty() {
            blocks.push(Block::Quote(std::mem::take(&mut quote)));
        }
        blocks.push(Block::Paragraph(parse_inline(line, context)));
    }

    if !quote.is_empty() {
        blocks.push(Block::Quote(quote));
    }
    blocks
}

/// `> text` or `>text`, with at most one space eaten.
fn strip_quote(line: &str) -> Option<&str> {
    let rest = line.strip_prefix('>')?;
    Some(rest.strip_prefix(' ').unwrap_or(rest))
}

/// The inline rules, in precedence order. The first match at the earliest
/// position wins, which is why this is an ordered list and not a set.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Rule {
    Code,
    Bold,
    Underline,
    Strike,
    Italic,
    Spoiler,
    Link,
    Mention,
    Emoji,
}

const RULES: &[Rule] = &[
    Rule::Code,
    Rule::Bold,
    Rule::Underline,
    Rule::Strike,
    Rule::Italic,
    Rule::Spoiler,
    Rule::Link,
    Rule::Mention,
    Rule::Emoji,
];

/// Where a rule matched: the byte range of the whole match, and of its inner
/// content.
struct Match {
    rule: Rule,
    start: usize,
    end: usize,
    inner: (usize, usize),
}

pub fn parse_inline(text: &str, context: &Context<'_>) -> Vec<Inline> {
    let mut out = Vec::new();
    render_inline(text, Style::default(), context, &mut out);
    out
}

fn render_inline(text: &str, style: Style, context: &Context<'_>, out: &mut Vec<Inline>) {
    let mut cursor = 0usize;

    while cursor < text.len() {
        let remaining = &text[cursor..];

        let Some(found) = first_match(remaining) else {
            push_text(out, remaining, style);
            return;
        };

        if found.start > 0 {
            push_text(out, &remaining[..found.start], style);
        }

        let whole = &remaining[found.start..found.end];
        let inner = &remaining[found.inner.0..found.inner.1];

        match found.rule {
            Rule::Code => push_text(out, inner, Style { code: true, ..style }),
            Rule::Bold => render_inline(inner, Style { bold: true, ..style }, context, out),
            Rule::Underline => {
                render_inline(inner, Style { underline: true, ..style }, context, out)
            }
            Rule::Strike => render_inline(inner, Style { strike: true, ..style }, context, out),
            Rule::Italic => render_inline(inner, Style { italic: true, ..style }, context, out),
            Rule::Spoiler => render_inline(inner, Style { spoiler: true, ..style }, context, out),
            Rule::Link => out.push(Inline::Link {
                run: Run {
                    text: crate::format::elide(whole, 64),
                    style: Style { kind: Kind::Link, ..style },
                },
                href: whole.to_string(),
            }),
            Rule::Mention => push_mention(out, whole, inner, style, context),
            Rule::Emoji => match context.emoji_by_name(inner) {
                Some(emoji) => out.push(Inline::Emoji {
                    name: emoji.name.clone(),
                    url: emoji.url.clone(),
                }),
                // Not a real emoji — leave the `:name:` as written.
                None => push_text(out, whole, style),
            },
        }

        cursor += found.end;
    }
}

fn push_text(out: &mut Vec<Inline>, text: &str, style: Style) {
    if text.is_empty() {
        return;
    }
    // Merge with the previous run when the attributes match, so a paragraph
    // of plain text is one run and not one per parse step.
    if let Some(Inline::Text(previous)) = out.last_mut() {
        if previous.style == style {
            previous.text.push_str(text);
            return;
        }
    }
    out.push(Inline::Text(Run {
        text: text.to_string(),
        style,
    }));
}

fn push_mention(
    out: &mut Vec<Inline>,
    whole: &str,
    inner: &str,
    style: Style,
    context: &Context<'_>,
) {
    // Trailing punctuation isn't part of a username: "@ada." → "ada".
    let name = inner.trim_end_matches(['.', '-']);
    let trailing = &inner[name.len()..];

    if name.eq_ignore_ascii_case("everyone") || name.eq_ignore_ascii_case("here") {
        out.push(Inline::Mention {
            run: Run {
                text: format!("@{name}"),
                style: Style { kind: Kind::MentionSelf, ..style },
            },
            id: String::new(),
        });
        push_text(out, trailing, style);
        return;
    }

    match context.member_by_username(name) {
        Some(member) => {
            let kind = if member.id == context.me_id {
                Kind::MentionSelf
            } else {
                Kind::Mention
            };
            out.push(Inline::Mention {
                run: Run {
                    text: format!("@{}", member.name()),
                    style: Style { kind, ..style },
                },
                id: member.id.clone(),
            });
            push_text(out, trailing, style);
        }
        // Not a real member — ordinary text, exactly as typed.
        None => push_text(out, whole, style),
    }
}

/// The earliest match in `text`, breaking ties by rule order.
fn first_match(text: &str) -> Option<Match> {
    let mut best: Option<Match> = None;
    for &rule in RULES {
        if let Some(found) = match_rule(rule, text) {
            let better = match &best {
                None => true,
                Some(current) => found.start < current.start,
            };
            if better {
                best = Some(found);
            }
        }
    }
    best
}

fn match_rule(rule: Rule, text: &str) -> Option<Match> {
    match rule {
        Rule::Code => delimited(text, "`", "`", false),
        Rule::Bold => delimited(text, "**", "**", true),
        Rule::Underline => delimited(text, "__", "__", true),
        Rule::Strike => delimited(text, "~~", "~~", true),
        Rule::Italic => delimited(text, "*", "*", false).or_else(|| delimited(text, "_", "_", false)),
        Rule::Spoiler => delimited(text, "||", "||", true),
        Rule::Link => match_link(text),
        Rule::Mention => match_mention(text),
        Rule::Emoji => match_emoji(text),
    }
    .map(|found| Match { rule, ..found })
}

/// A run between two delimiters.
///
/// `multiline` mirrors the web client's `[\s\S]` patterns: bold, underline,
/// strike and spoiler may span newlines; code and italic may not.
fn delimited(text: &str, open: &str, close: &str, multiline: bool) -> Option<Match> {
    let start = text.find(open)?;
    let after_open = start + open.len();
    let rest = &text[after_open..];

    // The content must be non-empty, so `****` is literal.
    let mut search = 0usize;
    loop {
        let offset = rest[search..].find(close)? + search;
        if offset == 0 {
            search = close.len();
            if search >= rest.len() {
                return None;
            }
            continue;
        }
        let inner = &rest[..offset];
        if !multiline && inner.contains('\n') {
            return None;
        }
        return Some(Match {
            rule: Rule::Code, // replaced by the caller
            start,
            end: after_open + offset + close.len(),
            inner: (after_open, after_open + offset),
        });
    }
}

fn match_link(text: &str) -> Option<Match> {
    let start = ["https://", "http://"]
        .iter()
        .filter_map(|scheme| text.find(scheme))
        .min()?;

    let rest = &text[start..];
    let mut end = rest.len();
    for (offset, ch) in rest.char_indices() {
        if ch.is_whitespace() || "<>()".contains(ch) {
            end = offset;
            break;
        }
    }
    // Trailing punctuation belongs to the sentence, not the URL.
    let trimmed = rest[..end].trim_end_matches(['.', ',', '!', '?', ';', ':', '\'', '"']);
    if trimmed.len() <= "https://".len() {
        return None;
    }
    Some(Match {
        rule: Rule::Link,
        start,
        end: start + trimmed.len(),
        inner: (start, start + trimmed.len()),
    })
}

/// `@username`, which must start at a word boundary so an email address does
/// not become a ping. Mirrors the server's parser in `server/src/mentions.rs`.
fn match_mention(text: &str) -> Option<Match> {
    let bytes = text.as_bytes();
    let mut index = 0usize;

    while let Some(offset) = text[index..].find('@') {
        let at = index + offset;

        let preceded_by_word = at > 0
            && matches!(bytes[at - 1], b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'_' | b'.' | b'-');
        if preceded_by_word {
            index = at + 1;
            continue;
        }

        let name_start = at + 1;
        let first = bytes.get(name_start)?;
        if !matches!(first, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'_') {
            index = at + 1;
            continue;
        }

        let mut end = name_start + 1;
        while end < bytes.len()
            && matches!(bytes[end], b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'_' | b'.' | b'-')
            // The web client's pattern caps the name at 32 characters.
            && end - name_start < 32
        {
            end += 1;
        }
        if end - name_start < 2 {
            index = at + 1;
            continue;
        }
        return Some(Match {
            rule: Rule::Mention,
            start: at,
            end,
            inner: (name_start, end),
        });
    }
    None
}

/// `:name:`, 2–32 characters of `[a-z0-9_]`, case-insensitive.
fn match_emoji(text: &str) -> Option<Match> {
    let bytes = text.as_bytes();
    let mut index = 0usize;

    while let Some(offset) = text[index..].find(':') {
        let open = index + offset;
        let name_start = open + 1;
        let mut end = name_start;
        while end < bytes.len()
            && matches!(bytes[end], b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'_')
            && end - name_start < 32
        {
            end += 1;
        }
        let length = end - name_start;
        if (2..=32).contains(&length) && bytes.get(end) == Some(&b':') {
            return Some(Match {
                rule: Rule::Emoji,
                start: open,
                end: end + 1,
                inner: (name_start, end),
            });
        }
        index = open + 1;
    }
    None
}

/// Whether a message is nothing but emoji, in which case it renders larger —
/// the way chat apps usually do.
pub fn is_jumbo(content: &str) -> bool {
    let trimmed = content.trim();
    if trimmed.is_empty() || trimmed.chars().count() > 24 {
        return false;
    }
    trimmed.chars().all(|c| {
        c.is_whitespace()
            || c == '\u{200d}'
            || c == '\u{fe0f}'
            || !c.is_ascii() && !c.is_alphanumeric()
    })
}

/// The plain text of a message, for search matching and notifications.
pub fn to_plain(blocks: &[Block]) -> String {
    let mut out = String::new();
    for block in blocks {
        match block {
            Block::Paragraph(inlines) => {
                push_inlines(&mut out, inlines);
                out.push('\n');
            }
            Block::Code { text, .. } => {
                out.push_str(text);
                out.push('\n');
            }
            Block::Quote(lines) => {
                for line in lines {
                    push_inlines(&mut out, line);
                    out.push('\n');
                }
            }
        }
    }
    out.trim_end().to_string()
}

fn push_inlines(out: &mut String, inlines: &[Inline]) {
    for inline in inlines {
        match inline {
            Inline::Text(run) => out.push_str(&run.text),
            Inline::Link { run, .. } | Inline::Mention { run, .. } => out.push_str(&run.text),
            Inline::Emoji { name, .. } => {
                out.push(':');
                out.push_str(name);
                out.push(':');
            }
        }
    }
}
