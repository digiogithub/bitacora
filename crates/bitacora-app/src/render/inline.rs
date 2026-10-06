//! Inline renderer: block text to display text plus styled and clickable ranges.
//!
//! The reference constructs come from the `bitacora-markdown` inline scanner; emphasis
//! (`**bold**`, `*italic*`, `~~strike~~`, `^^highlight^^`) is handled here for the text
//! between tokens, because the scanner deliberately leaves it to the renderer.

use std::collections::HashMap;
use std::ops::Range;

use bitacora_markdown::Span;
use bitacora_markdown::image_meta::image_meta_at;
use bitacora_markdown::inline::{InlineToken, LinkTarget, scan_line};

/// Where a click on a rendered range leads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NavTarget {
    /// Open the page with this name (also used for tags).
    Page(String),
    /// Open the block with this UUID.
    Block(String),
    /// Open an external URL.
    Url(String),
}

/// Visual role of a styled range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// Ordinary text (only emphasis flags apply).
    Plain,
    /// `[[page]]`.
    PageRef,
    /// `#tag`.
    Tag,
    /// Resolved `((block ref))`.
    BlockRef,
    /// Unresolvable block ref, shown raw.
    BlockRefDangling,
    /// Labelled link or bare URL.
    Link,
    /// Inline code.
    Code,
    /// Inline math, shown as written.
    Math,
    /// Embeds, queries and other macros (placeholders until BIT-EP-0013).
    Placeholder,
    /// Raw inline HTML or other de-emphasised text.
    Dim,
}

/// Emphasis flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Emphasis {
    /// `**x**`.
    pub bold: bool,
    /// `*x*`.
    pub italic: bool,
    /// `~~x~~`.
    pub strike: bool,
    /// `^^x^^`.
    pub highlight: bool,
}

/// A styled range of the display text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyledRange {
    /// Byte range in [`TextLayout::text`].
    pub range: Range<usize>,
    /// Role.
    pub role: Role,
    /// Emphasis.
    pub emphasis: Emphasis,
}

/// An image found in the text, rendered below it.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageRef {
    /// Path or URL as written.
    pub src: String,
    /// Alt text.
    pub alt: String,
    /// `{:width N}` metadata.
    pub width: Option<f32>,
    /// `{:height N}` metadata.
    pub height: Option<f32>,
}

/// Display text with its styled and clickable ranges.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TextLayout {
    /// Text to show (markers such as `**` removed).
    pub text: String,
    /// Non-overlapping styled ranges in order (plain gaps are omitted).
    pub styled: Vec<StyledRange>,
    /// Clickable ranges with their targets.
    pub links: Vec<(Range<usize>, NavTarget)>,
    /// Images, in order of appearance.
    pub images: Vec<ImageRef>,
}

impl TextLayout {
    /// True when there is neither text nor an image.
    pub fn is_empty(&self) -> bool {
        self.text.is_empty() && self.images.is_empty()
    }

    fn push(&mut self, text: &str, role: Role, emphasis: Emphasis, target: Option<NavTarget>) {
        if text.is_empty() {
            return;
        }
        let start = self.text.len();
        self.text.push_str(text);
        let range = start..self.text.len();
        if role != Role::Plain || emphasis != Emphasis::default() {
            self.styled.push(StyledRange {
                range: range.clone(),
                role,
                emphasis,
            });
        }
        if let Some(target) = target {
            self.links.push((range, target));
        }
    }

    /// Appends a line break.
    pub fn push_newline(&mut self) {
        self.text.push('\n');
    }

    /// The navigation target at byte offset `at`, if any.
    pub fn target_at(&self, at: usize) -> Option<&NavTarget> {
        self.links
            .iter()
            .find(|(r, _)| r.contains(&at))
            .map(|(_, t)| t)
    }
}

/// Resolves a block UUID to the text shown for `((uuid))`.
pub trait BlockResolver {
    /// Title of the block, or `None` when it does not exist.
    fn resolve(&self, uuid: &str) -> Option<String>;
}

impl BlockResolver for HashMap<String, String> {
    fn resolve(&self, uuid: &str) -> Option<String> {
        self.get(&uuid.to_ascii_lowercase()).cloned()
    }
}

/// A resolver that knows no block (every ref is dangling).
#[derive(Debug, Clone, Copy, Default)]
pub struct NoBlocks;

impl BlockResolver for NoBlocks {
    fn resolve(&self, _: &str) -> Option<String> {
        None
    }
}

/// Renders several source lines (joined by line breaks) into one layout.
pub fn layout_lines<'a>(
    lines: impl IntoIterator<Item = &'a str>,
    resolver: &dyn BlockResolver,
) -> TextLayout {
    let mut out = TextLayout::default();
    for (i, line) in lines.into_iter().enumerate() {
        if i > 0 {
            out.push_newline();
        }
        append_line(&mut out, line, resolver);
    }
    out
}

/// Renders one source line.
pub fn layout_line(line: &str, resolver: &dyn BlockResolver) -> TextLayout {
    layout_lines([line], resolver)
}

fn append_line(out: &mut TextLayout, line: &str, resolver: &dyn BlockResolver) {
    let tokens = scan_line(line, 0, line.len());
    let mut cursor = 0;
    let plain = Emphasis::default();
    for token in &tokens {
        let span = token.span();
        if span.start < cursor || span.end > line.len() {
            continue;
        }
        if span.start > cursor {
            emphasis_text(out, &line[cursor..span.start], plain);
        }
        cursor = span.end;
        let slice = |s: Span| &line[s.start..s.end];
        match token {
            InlineToken::Code(s) => {
                let inner = slice(*s).trim_matches('`');
                out.push(inner, Role::Code, plain, None);
            }
            InlineToken::Math(s) => out.push(slice(*s), Role::Math, plain, None),
            InlineToken::Html(s) => out.push(slice(*s), Role::Dim, plain, None),
            InlineToken::Url(s) => {
                let raw = slice(*s);
                let url = raw.trim_start_matches('<').trim_end_matches('>');
                out.push(raw, Role::Link, plain, Some(NavTarget::Url(url.to_owned())));
            }
            InlineToken::PageRef(p) => {
                let name = slice(p.name).to_owned();
                out.push(
                    slice(p.span),
                    Role::PageRef,
                    plain,
                    Some(NavTarget::Page(name)),
                );
            }
            InlineToken::Tag(t) => {
                let name = slice(t.name).to_owned();
                let shown = format!("#{name}");
                out.push(&shown, Role::Tag, plain, Some(NavTarget::Page(name)));
            }
            InlineToken::BlockRef(b) => {
                let id = slice(b.id).to_ascii_lowercase();
                match b.valid.then(|| resolver.resolve(&id)).flatten() {
                    Some(title) => {
                        out.push(&title, Role::BlockRef, plain, Some(NavTarget::Block(id)));
                    }
                    None => out.push(slice(b.span), Role::BlockRefDangling, plain, None),
                }
            }
            InlineToken::Link(l) => {
                if l.image {
                    let src = match &l.target {
                        LinkTarget::Url(s) | LinkTarget::Search(s) | LinkTarget::File(s) => {
                            slice(*s).to_owned()
                        }
                        _ => String::new(),
                    };
                    let mut image = ImageRef {
                        src,
                        alt: slice(l.label).to_owned(),
                        width: None,
                        height: None,
                    };
                    if let Some((meta_span, meta)) = image_meta_at(line, l.span.start) {
                        image.width = meta.get("width").and_then(|v| v.trim().parse().ok());
                        image.height = meta.get("height").and_then(|v| v.trim().parse().ok());
                        cursor = cursor.max(meta_span.end);
                    }
                    out.images.push(image);
                } else {
                    let label = slice(l.label);
                    let target = match &l.target {
                        LinkTarget::Page(p) => NavTarget::Page(slice(p.name).to_owned()),
                        LinkTarget::Block(b) => NavTarget::Block(slice(b.id).to_ascii_lowercase()),
                        LinkTarget::File(_) => NavTarget::Page(label.to_owned()),
                        LinkTarget::Url(s) | LinkTarget::Search(s) => {
                            NavTarget::Url(slice(*s).to_owned())
                        }
                    };
                    out.push(label, Role::Link, plain, Some(target));
                }
            }
            InlineToken::Macro(m) => out.push(slice(m.span), Role::Placeholder, plain, None),
        }
    }
    if cursor < line.len() {
        emphasis_text(out, &line[cursor..], plain);
    }
}

type Apply = fn(&mut Emphasis);

const DELIMS: [(&str, Apply); 6] = [
    ("**", |e| e.bold = true),
    ("__", |e| e.bold = true),
    ("~~", |e| e.strike = true),
    ("^^", |e| e.highlight = true),
    ("*", |e| e.italic = true),
    ("_", |e| e.italic = true),
];

/// Renders plain text between tokens, applying emphasis delimiters.
fn emphasis_text(out: &mut TextLayout, text: &str, base: Emphasis) {
    let mut plain = String::new();
    let mut i = 0;
    'outer: while i < text.len() {
        let rest = &text[i..];
        let ch = rest.chars().next().unwrap_or(' ');
        if ch == '\\'
            && let Some(next) = rest[1..].chars().next()
            && next.is_ascii_punctuation()
        {
            plain.push(next);
            i += 1 + next.len_utf8();
            continue;
        }
        for (delim, apply) in DELIMS {
            if !rest.starts_with(delim) {
                continue;
            }
            let before = text[..i].chars().next_back();
            let intraword = delim.starts_with('_') && before.is_some_and(char::is_alphanumeric);
            let body_start = i + delim.len();
            let first = text[body_start..].chars().next();
            if intraword || first.is_none_or(char::is_whitespace) || first == delim.chars().next() {
                continue;
            }
            if let Some(close) = find_close(text, body_start, delim) {
                out.push(&plain, Role::Plain, base, None);
                plain.clear();
                let mut inner = base;
                apply(&mut inner);
                emphasis_text(out, &text[body_start..close], inner);
                i = close + delim.len();
                continue 'outer;
            }
        }
        plain.push(ch);
        i += ch.len_utf8();
    }
    out.push(&plain, Role::Plain, base, None);
}

fn find_close(text: &str, from: usize, delim: &str) -> Option<usize> {
    let mut at = from;
    while let Some(rel) = text[at..].find(delim) {
        let mut pos = at + rel;
        // `***` closes `**` with its last two characters (the first closes an inner `*`).
        if delim.len() == 2 {
            while text[pos + 1..].starts_with(delim) {
                pos += 1;
            }
        }
        let prev = text[..pos].chars().next_back();
        let next = text[pos + delim.len()..].chars().next();
        let ok = pos > from
            && prev.is_some_and(|c| !c.is_whitespace() && c != '\\')
            && !(delim.starts_with('_') && next.is_some_and(char::is_alphanumeric));
        if ok {
            return Some(pos);
        }
        at = pos + delim.len();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(s: &str) -> TextLayout {
        layout_line(s, &NoBlocks)
    }

    #[test]
    fn page_refs_and_tags_are_clickable() {
        let l = render("see [[Alpha]] and #beta and #[[multi word]]");
        assert_eq!(l.text, "see [[Alpha]] and #beta and #multi word");
        let targets: Vec<_> = l.links.iter().map(|(_, t)| t.clone()).collect();
        assert_eq!(
            targets,
            [
                NavTarget::Page("Alpha".into()),
                NavTarget::Page("beta".into()),
                NavTarget::Page("multi word".into())
            ]
        );
        let at = l.text.find("Alpha").expect("alpha");
        assert_eq!(l.target_at(at), Some(&NavTarget::Page("Alpha".into())));
        assert_eq!(l.target_at(0), None);
    }

    #[test]
    fn emphasis_nests_and_hides_markers() {
        let l = render("a **bold *both*** ~~gone~~ ^^hi^^ z");
        assert_eq!(l.text, "a bold both gone hi z");
        let both = l.text.find("both").expect("both");
        let r = l
            .styled
            .iter()
            .find(|s| s.range.contains(&both))
            .expect("styled");
        assert!(r.emphasis.bold && r.emphasis.italic);
        assert!(l.styled.iter().any(|s| s.emphasis.strike));
        assert!(l.styled.iter().any(|s| s.emphasis.highlight));
    }

    #[test]
    fn unmatched_delimiters_stay_literal() {
        assert_eq!(render("2 * 3 * 4").text, "2 * 3 * 4");
        assert_eq!(render("snake_case_name").text, "snake_case_name");
        assert_eq!(render("lone ** star").text, "lone ** star");
    }

    #[test]
    fn code_and_refs_inside_code_are_literal() {
        let l = render("run `[[not a ref]]` now");
        assert_eq!(l.text, "run [[not a ref]] now");
        assert!(l.links.is_empty());
        assert_eq!(l.styled[0].role, Role::Code);
    }

    #[test]
    fn block_refs_resolve_or_stay_raw() {
        let id = "6500c1a4-0000-4000-8000-000000000001";
        let mut known = HashMap::new();
        known.insert(id.to_owned(), "Target block".to_owned());
        let l = layout_line(&format!("see (({id}))"), &known);
        assert_eq!(l.text, "see Target block");
        assert_eq!(l.links[0].1, NavTarget::Block(id.into()));
        let l = render(&format!("see (({id}))"));
        assert_eq!(l.text, format!("see (({id}))"));
        assert!(l.links.is_empty());
        assert_eq!(l.styled[0].role, Role::BlockRefDangling);
    }

    #[test]
    fn links_images_and_macros() {
        let l = render("[site](https://example.com) and [[x]]");
        assert_eq!(l.links[0].1, NavTarget::Url("https://example.com".into()));
        let l = render("![pic](../assets/a.png){:width 120, :height 80} after");
        assert_eq!(l.text, " after");
        assert_eq!(l.images.len(), 1);
        assert_eq!(l.images[0].src, "../assets/a.png");
        assert_eq!(l.images[0].width, Some(120.0));
        assert_eq!(l.images[0].alt, "pic");
        let l = render("{{embed [[Page]]}} {{query (todo now)}}");
        assert_eq!(
            l.styled
                .iter()
                .filter(|s| s.role == Role::Placeholder)
                .count(),
            2
        );
    }

    #[test]
    fn multiline_layout_joins_lines() {
        let l = layout_lines(["one **b**", "two"], &NoBlocks);
        assert_eq!(l.text, "one b\ntwo");
    }

    #[test]
    fn never_panics_on_odd_input() {
        for s in [
            "",
            "[[",
            "]]",
            "((",
            "**",
            "* *",
            "\\",
            "\\*x*",
            "#",
            "#[[",
            "![",
            "![a](",
            "`",
            "é**é**é",
            "_a_b_",
            "~~",
            "^^x",
            "{{",
            "[a]()",
        ] {
            let _ = render(s);
        }
    }
}
