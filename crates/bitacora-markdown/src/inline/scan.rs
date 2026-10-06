//! The inline tokenizer. See the [module docs](super) for scope and provenance.

use crate::span::Span;

/// `[[name]]`, possibly with page references nested inside the name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageRef {
    /// The whole `[[...]]` including the brackets.
    pub span: Span,
    /// The text between the outer brackets (the page name as written).
    pub name: Span,
    /// References nested inside `name` (`[[a [[b]] c]]` has `[[b]]`), outermost first.
    pub nested: Vec<PageRef>,
}

/// `#tag` or `#[[multi word]]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tag {
    /// The whole token including `#` (and the brackets for the bracketed form).
    pub span: Span,
    /// The tag name (without `#` and without brackets).
    pub name: Span,
    /// True for `#[[...]]`.
    pub bracketed: bool,
    /// References nested inside a bracketed name.
    pub nested: Vec<PageRef>,
}

/// `((id))`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockRef {
    /// The whole `((...))`.
    pub span: Span,
    /// The text between the parentheses.
    pub id: Span,
    /// True when the id looks like a UUID (`8-4-4-4-12` hex digits). Only valid ids are refs.
    pub valid: bool,
}

/// Where a `[label](target)` link points.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkTarget {
    /// `[label]([[page]])`.
    Page(PageRef),
    /// `[label](((uuid)))`.
    Block(BlockRef),
    /// `[label](file:../pages/x.md)`: the *label* names the page.
    File(Span),
    /// A URL with a scheme (`https://...`, `mailto:...`).
    Url(Span),
    /// Anything else, typically a relative path (`../assets/a.png`).
    Search(Span),
}

/// `[label](target)` or `![label](target)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    /// The whole link, starting at `!` for images.
    pub span: Span,
    /// The label between `[` and `]`.
    pub label: Span,
    /// True for `![...](...)`.
    pub image: bool,
    /// The target.
    pub target: LinkTarget,
}

/// `{{name arg, arg}}` or `{{{name arg}}}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Macro {
    /// The whole macro including the braces.
    pub span: Span,
    /// The macro name (up to a space, `(` or `}`).
    pub name: Span,
    /// The arguments, split on top-level commas. Leading blanks are removed, the rest is verbatim.
    pub args: Vec<Span>,
    /// True for `{{{...}}}`.
    pub triple: bool,
}

/// One recognised inline construct.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InlineToken {
    /// `` `code` `` (the span includes the backticks).
    Code(Span),
    /// `$x$`, `$$x$$`, `\(x\)` or `\[x\]`.
    Math(Span),
    /// Inline HTML (an element, or just a lone opening tag).
    Html(Span),
    /// A bare or `<...>` URL.
    Url(Span),
    /// A page reference.
    PageRef(PageRef),
    /// A tag.
    Tag(Tag),
    /// A block reference.
    BlockRef(BlockRef),
    /// A labelled link.
    Link(Link),
    /// A macro.
    Macro(Macro),
}

impl InlineToken {
    /// The span of the whole token.
    #[must_use]
    pub const fn span(&self) -> Span {
        match self {
            Self::Code(s) | Self::Math(s) | Self::Html(s) | Self::Url(s) => *s,
            Self::PageRef(p) => p.span,
            Self::Tag(t) => t.span,
            Self::BlockRef(b) => b.span,
            Self::Link(l) => l.span,
            Self::Macro(m) => m.span,
        }
    }
}

/// Tokenizes `text` (one or more lines). Spans are byte offsets into `text`.
#[must_use]
pub fn scan(text: &str) -> Vec<InlineToken> {
    let mut out = Vec::new();
    let mut start = 0;
    for line in text.split_inclusive('\n') {
        let mut end = start + line.len();
        let next = end;
        if text[start..end].ends_with('\n') {
            end -= 1;
        }
        if text[start..end].ends_with('\r') {
            end -= 1;
        }
        scan_range(text, start, end, false, &mut out);
        start = next;
    }
    out
}

/// Tokenizes `text[lo..hi]`, which must not contain a line break. Spans stay relative to `text`.
#[must_use]
pub fn scan_line(text: &str, lo: usize, hi: usize) -> Vec<InlineToken> {
    let mut out = Vec::new();
    scan_range(text, lo, hi.min(text.len()), false, &mut out);
    out
}

/// True for `8-4-4-4-12` hexadecimal digits.
#[must_use]
pub fn is_uuid(s: &str) -> bool {
    let groups: Vec<&str> = s.split('-').collect();
    const LENS: [usize; 5] = [8, 4, 4, 4, 12];
    groups.len() == 5
        && groups
            .iter()
            .zip(LENS)
            .all(|(g, n)| g.len() == n && g.bytes().all(|b| b.is_ascii_hexdigit()))
}

fn char_len_at(text: &str, i: usize, hi: usize) -> usize {
    if i >= hi {
        return 0;
    }
    text[i..].chars().next().map_or(1, char::len_utf8)
}

fn is_ws(b: u8) -> bool {
    b == b' ' || b == b'\t'
}

/// Scans `text[lo..hi]`. Inside emphasis (`restricted`) mldoc only recognises code spans, page
/// references and labelled links: tags, block references, macros, math, HTML and URLs are plain.
fn scan_range(text: &str, lo: usize, hi: usize, restricted: bool, out: &mut Vec<InlineToken>) {
    let b = text.as_bytes();
    let mut i = lo;
    while i < hi {
        let step = char_len_at(text, i, hi).max(1);
        match b[i] {
            b'\\' => {
                if !restricted && let Some(end) = latex_at(text, i, hi) {
                    out.push(InlineToken::Math(Span::new(i, end)));
                    i = end;
                } else {
                    // An escaped character is plain text: `\[[x]]` is not a reference.
                    i += 1 + char_len_at(text, i + 1, hi);
                }
            }
            b'`' => {
                let run = b[i..hi].iter().take_while(|&&c| c == b'`').count();
                if let Some(off) = find_backtick_run(&text[i + run..hi], run) {
                    let end = i + run + off + run;
                    out.push(InlineToken::Code(Span::new(i, end)));
                    i = end;
                } else {
                    i += run;
                }
            }
            b'$' if !restricted => {
                if let Some(end) = math_at(text, i, hi) {
                    out.push(InlineToken::Math(Span::new(i, end)));
                    i = end;
                } else {
                    i += 1;
                }
            }
            b'<' if !restricted => {
                if let Some(tok) = angle_at(text, i, hi) {
                    i = tok.span().end;
                    out.push(tok);
                } else {
                    i += 1;
                }
            }
            b'{' if !restricted => {
                if let Some(m) = macro_at(text, i, hi) {
                    i = m.span.end;
                    out.push(InlineToken::Macro(m));
                } else {
                    i += 1;
                }
            }
            b'(' if !restricted => {
                if let Some(r) = block_ref_at(text, i, hi) {
                    i = r.span.end;
                    out.push(InlineToken::BlockRef(r));
                } else {
                    i += 1;
                }
            }
            b'[' => {
                if b.get(i + 1) == Some(&b'[') {
                    if let Some(p) = page_ref_at(text, i, hi) {
                        i = p.span.end;
                        out.push(InlineToken::PageRef(p));
                    } else {
                        i += 1;
                    }
                } else if let Some(l) = link_at(text, i, hi, false) {
                    i = l.span.end;
                    out.push(InlineToken::Link(l));
                } else {
                    i += 1;
                }
            }
            b'!' => {
                if b.get(i + 1) == Some(&b'[')
                    && b.get(i + 2) != Some(&b'[')
                    && let Some(l) = link_at(text, i + 1, hi, true)
                {
                    let l = Link {
                        span: Span::new(i, l.span.end),
                        ..l
                    };
                    i = l.span.end;
                    out.push(InlineToken::Link(l));
                    continue;
                }
                i += 1;
            }
            b'#' if !restricted => {
                if let Some(t) = tag_at(text, i, hi) {
                    i = t.span.end;
                    out.push(InlineToken::Tag(t));
                } else {
                    i += 1;
                }
            }
            b'*' | b'_' | b'~' | b'=' | b'^' => {
                if let Some(em) = emphasis_at(text, i, hi) {
                    if !em.opaque {
                        scan_range(text, em.content.start, em.content.end, true, out);
                    }
                    i = em.end;
                } else {
                    i += marker_run(b, i, hi).min(2);
                }
            }
            c => {
                if !restricted
                    && c.is_ascii_alphabetic()
                    && (i == lo || !b[i - 1].is_ascii_alphanumeric())
                    && let Some(end) = bare_url_at(text, i, hi)
                {
                    out.push(InlineToken::Url(Span::new(i, end)));
                    i = end;
                } else {
                    i += step;
                }
            }
        }
    }
}

struct Emphasis {
    content: Span,
    end: usize,
    /// `***x***`: mldoc keeps the content as plain text.
    opaque: bool,
}

fn marker_run(b: &[u8], i: usize, hi: usize) -> usize {
    b[i..hi].iter().take_while(|&&c| c == b[i]).count()
}

/// `**bold**`, `*italic*`, `__bold__`, `_italic_`, `~~strike~~`, `==highlight==`, `^^highlight^^`
/// at `i`. The opener must be followed by a non-blank character; the closer is the first
/// occurrence of the marker (observed on mldoc 1.5.7). `_` markers do not open inside a word and
/// do not close in front of one.
fn emphasis_at(text: &str, i: usize, hi: usize) -> Option<Emphasis> {
    let b = text.as_bytes();
    let c = b[i];
    let run = marker_run(b, i, hi);
    let len = match c {
        b'*' if run >= 3 => 3,
        b'*' | b'_' => run.min(2),
        _ if run >= 2 => 2,
        _ => return None,
    };
    if c == b'_'
        && text[..i]
            .chars()
            .next_back()
            .is_some_and(char::is_alphanumeric)
    {
        return None;
    }
    let start = i + len;
    let first = text[start..hi].chars().next()?;
    if first.is_whitespace() {
        return None;
    }
    let marker = &text[i..start];
    let mut j = start + first.len_utf8();
    while j < hi {
        if text[j..hi].starts_with(marker) {
            let after = text[j + len..hi].chars().next();
            if c != b'_' || !after.is_some_and(char::is_alphanumeric) {
                return Some(Emphasis {
                    content: Span::new(start, j),
                    end: j + len,
                    opaque: len == 3,
                });
            }
        }
        j += char_len_at(text, j, hi).max(1);
    }
    None
}

/// Position of the closing run of exactly `run` backticks in `s`.
fn find_backtick_run(s: &str, run: usize) -> Option<usize> {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'`' {
            let n = b[i..].iter().take_while(|&&c| c == b'`').count();
            if n == run {
                return Some(i);
            }
            i += n;
        } else {
            i += 1;
        }
    }
    None
}

/// `\(...\)` and `\[...\]`; returns the end offset.
fn latex_at(text: &str, i: usize, hi: usize) -> Option<usize> {
    let b = text.as_bytes();
    let close = match b.get(i + 1)? {
        b'(' => "\\)",
        b'[' => "\\]",
        _ => return None,
    };
    let from = i + 2;
    if from >= hi {
        return None;
    }
    let off = text[from..hi].find(close)?;
    (off > 0).then_some(from + off + 2)
}

/// `$$...$$` and `$...$`; returns the end offset.
fn math_at(text: &str, i: usize, hi: usize) -> Option<usize> {
    let b = text.as_bytes();
    if b.get(i + 1) == Some(&b'$') {
        let from = i + 2;
        if from < hi
            && let Some(off) = text[from..hi].find("$$")
            && off > 0
        {
            return Some(from + off + 2);
        }
        return None;
    }
    let first = *b.get(i + 1)?;
    if i + 1 >= hi || first.is_ascii_whitespace() {
        return None;
    }
    let mut j = i + 2;
    while j < hi {
        if b[j] == b'$' && !b[j - 1].is_ascii_whitespace() {
            return Some(j + 1);
        }
        j += 1;
    }
    None
}

const VOID_ELEMENTS: &[&str] = &["br", "hr", "img", "input", "meta", "link", "wbr", "col"];

/// `<https://...>` autolinks and inline HTML.
fn angle_at(text: &str, i: usize, hi: usize) -> Option<InlineToken> {
    let rest = &text[i + 1..hi];
    let close = rest.find('>')?;
    let inner = &rest[..close];
    if has_scheme_slashes(inner) && !inner.contains(char::is_whitespace) {
        return Some(InlineToken::Url(Span::new(i, i + close + 2)));
    }
    let name_len = inner
        .bytes()
        .take_while(|b| b.is_ascii_alphanumeric() || *b == b'-')
        .count();
    if name_len == 0 || !inner.as_bytes()[0].is_ascii_alphabetic() {
        return None;
    }
    let name = &inner[..name_len];
    let open_end = i + 1 + close + 1;
    if inner.ends_with('/') || VOID_ELEMENTS.iter().any(|v| v.eq_ignore_ascii_case(name)) {
        return Some(InlineToken::Html(Span::new(i, open_end)));
    }
    let closer = format!("</{name}>");
    let lower = text[open_end..hi].to_ascii_lowercase();
    if let Some(off) = lower.find(&closer.to_ascii_lowercase()) {
        return Some(InlineToken::Html(Span::new(
            i,
            open_end + off + closer.len(),
        )));
    }
    Some(InlineToken::Html(Span::new(i, open_end)))
}

/// `scheme://...` with an alphabetic scheme at the start of `s`.
fn has_scheme_slashes(s: &str) -> bool {
    let n = s
        .bytes()
        .take_while(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'-' | b'.'))
        .count();
    n > 0 && s.as_bytes()[0].is_ascii_alphabetic() && s[n..].starts_with("://")
}

/// A bare URL (`https://x.com/...`) at `i`; it runs to the next whitespace.
fn bare_url_at(text: &str, i: usize, hi: usize) -> Option<usize> {
    let s = &text[i..hi];
    if !has_scheme_slashes(s) {
        return None;
    }
    let end = s.find(char::is_whitespace).unwrap_or(s.len());
    Some(i + end)
}

/// A balanced `[[...]]` at `i`, searched within `text[..hi]`.
fn page_ref_at(text: &str, i: usize, hi: usize) -> Option<PageRef> {
    let b = text.as_bytes();
    if b.get(i) != Some(&b'[') || b.get(i + 1) != Some(&b'[') {
        return None;
    }
    let mut depth = 0usize;
    let mut j = i;
    while j + 1 < hi {
        if b[j] == b'[' && b[j + 1] == b'[' {
            depth += 1;
            j += 2;
        } else if b[j] == b']' && b[j + 1] == b']' {
            depth -= 1;
            j += 2;
            if depth == 0 {
                let name = Span::new(i + 2, j - 2);
                if name.is_empty() {
                    return None;
                }
                return Some(PageRef {
                    span: Span::new(i, j),
                    name,
                    nested: nested_in(text, name),
                });
            }
        } else {
            j += 1;
        }
    }
    // Unbalanced (`[[a [[b]]`): the reference ends at the first `]]`.
    let off = text[i + 2..hi].find("]]")?;
    let name = Span::new(i + 2, i + 2 + off);
    (!name.is_empty()).then(|| PageRef {
        span: Span::new(i, name.end + 2),
        name,
        nested: Vec::new(),
    })
}

fn nested_in(text: &str, name: Span) -> Vec<PageRef> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut j = name.start;
    while j + 1 < name.end {
        if b[j] == b'['
            && b[j + 1] == b'['
            && let Some(p) = page_ref_at(text, j, name.end)
        {
            j = p.span.end;
            out.push(p);
            continue;
        }
        j += 1;
    }
    out
}

fn block_ref_at(text: &str, i: usize, hi: usize) -> Option<BlockRef> {
    if !text[i..hi].starts_with("((") {
        return None;
    }
    let from = i + 2;
    let off = text[from..hi].find("))")?;
    if off == 0 {
        return None;
    }
    let id = Span::new(from, from + off);
    Some(BlockRef {
        span: Span::new(i, from + off + 2),
        id,
        valid: is_uuid(&text[id.range()]),
    })
}

/// `[label](target)` where `text[i]` is the opening `[`.
fn link_at(text: &str, i: usize, hi: usize, image: bool) -> Option<Link> {
    let b = text.as_bytes();
    let close = i + 1 + text[i + 1..hi].find(']')?;
    if b.get(close + 1) != Some(&b'(') {
        return None;
    }
    let label = Span::new(i + 1, close);
    let t = close + 2;
    let rest = &text[t..hi];
    let (target, end) = if rest.starts_with("[[") {
        let p = page_ref_at(text, t, hi)?;
        if b.get(p.span.end) != Some(&b')') {
            return None;
        }
        let end = p.span.end + 1;
        (LinkTarget::Page(p), end)
    } else if rest.starts_with("((") {
        let skip = if rest.starts_with("(((") { 3 } else { 2 };
        let from = t + skip;
        let off = text[from..hi].find("))")?;
        if off == 0 || b.get(from + off + 2) != Some(&b')') {
            return None;
        }
        let id = Span::new(from, from + off);
        let r = BlockRef {
            span: Span::new(t, from + off + 2),
            id,
            valid: is_uuid(&text[id.range()]),
        };
        (LinkTarget::Block(r), from + off + 3)
    } else {
        let mut depth = 0usize;
        let mut k = t;
        while k < hi {
            match b[k] {
                b'(' => depth += 1,
                b')' if depth == 0 => break,
                b')' => depth -= 1,
                _ => {}
            }
            k += 1;
        }
        if k >= hi || k == t {
            return None;
        }
        let target = Span::new(t, k);
        let s = &text[t..k];
        let kind = if s.starts_with("file:") {
            LinkTarget::File(target)
        } else if has_scheme_slashes(s) || s.starts_with("mailto:") {
            LinkTarget::Url(target)
        } else {
            LinkTarget::Search(target)
        };
        (kind, k + 1)
    };
    Some(Link {
        span: Span::new(i, end),
        label,
        image,
        target,
    })
}

/// A `{{macro}}` / `{{{macro}}}` at `i`.
fn macro_at(text: &str, i: usize, hi: usize) -> Option<Macro> {
    let b = text.as_bytes();
    let s = &text[i..hi];
    if !s.starts_with("{{") {
        return None;
    }
    let triple = s.starts_with("{{{");
    let open = if triple { 3 } else { 2 };
    let close = if triple { "}}}" } else { "}}" };
    let name_start = i + open;
    let mut j = name_start;
    while j < hi && !matches!(b[j], b' ' | b'\t' | b'(' | b'}') {
        j += 1;
    }
    if j == name_start || j >= hi {
        return None;
    }
    let name = Span::new(name_start, j);
    let mut args: Vec<Span> = Vec::new();
    if text[j..hi].starts_with(close) {
        return Some(Macro {
            span: Span::new(i, j + close.len()),
            name,
            args,
            triple,
        });
    }
    if !matches!(b[j], b' ' | b'\t' | b'(') {
        return None;
    }
    loop {
        while j < hi && is_ws(b[j]) {
            j += 1;
        }
        if j >= hi {
            return None;
        }
        if text[j..hi].starts_with(close) {
            return args.is_empty().then(|| Macro {
                span: Span::new(i, j + close.len()),
                name,
                args: Vec::new(),
                triple,
            });
        }
        // A strict argument (page ref, block ref, quoted string) must be followed by `,` or the
        // closing braces; otherwise the whole macro is rejected (observed on mldoc 1.5.7).
        let strict_end = if let Some(p) = page_ref_at(text, j, hi) {
            Some((p.span.end, true))
        } else if text[j..hi].starts_with("((") {
            text[j + 2..hi]
                .find("))")
                .map(|off| (j + 2 + off + 2, true))
        } else if b[j] == b'"' {
            text[j + 1..hi]
                .find('"')
                .map(|off| (j + 1 + off + 1, false))
        } else {
            None
        };
        let mut done = false;
        let mut matched = false;
        if let Some((end, hard)) = strict_end {
            let mut k = end;
            while k < hi && is_ws(b[k]) {
                k += 1;
            }
            if k < hi && b[k] == b',' {
                args.push(Span::new(j, end));
                j = k + 1;
                matched = true;
            } else if text[k.min(hi)..hi].starts_with(close) {
                args.push(Span::new(j, end));
                j = k;
                matched = true;
                done = true;
            } else if hard {
                return None;
            }
        }
        if !matched {
            let mut k = j;
            while k < hi && b[k] != b',' && !b[k..hi].starts_with(close.as_bytes()) {
                k += 1;
            }
            if k >= hi || k == j {
                return None;
            }
            args.push(Span::new(j, k));
            if b[k] == b',' {
                j = k + 1;
            } else {
                j = k;
                done = true;
            }
        }
        if done {
            return Some(Macro {
                span: Span::new(i, j + close.len()),
                name,
                args,
                triple,
            });
        }
    }
}

/// Characters that always end a tag.
const TAG_STOP: &[char] = &[',', '!', '?', '\'', '"', ':', '#'];

/// `#tag` / `#[[multi word]]` at `i`.
fn tag_at(text: &str, i: usize, hi: usize) -> Option<Tag> {
    let rest = &text[i + 1..hi];
    let first = rest.chars().next()?;
    if first.is_whitespace() || first == '#' {
        return None;
    }
    if rest.starts_with("[[")
        && let Some(p) = page_ref_at(text, i + 1, hi)
    {
        return Some(Tag {
            span: Span::new(i, p.span.end),
            name: p.name,
            bracketed: true,
            nested: p.nested,
        });
    }
    let mut end = rest.len();
    let mut soft = true;
    for (off, ch) in rest.char_indices() {
        if ch.is_whitespace() {
            end = off;
            break;
        }
        if TAG_STOP.contains(&ch) || (off > 0 && rest[off..].starts_with("[[")) {
            end = off;
            soft = false;
            break;
        }
    }
    let mut name = &rest[..end];
    if soft {
        name = name.trim_end_matches(['.', ';']);
    }
    if name.is_empty() {
        return None;
    }
    let name_span = Span::new(i + 1, i + 1 + name.len());
    Some(Tag {
        span: Span::new(i, name_span.end),
        name: name_span,
        bracketed: false,
        nested: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_of(text: &str, s: Span) -> &str {
        &text[s.range()]
    }

    fn kinds(text: &str) -> Vec<String> {
        scan(text)
            .iter()
            .map(|t| match t {
                InlineToken::Code(s) => format!("code:{}", text_of(text, *s)),
                InlineToken::Math(s) => format!("math:{}", text_of(text, *s)),
                InlineToken::Html(s) => format!("html:{}", text_of(text, *s)),
                InlineToken::Url(s) => format!("url:{}", text_of(text, *s)),
                InlineToken::PageRef(p) => format!("page:{}", text_of(text, p.name)),
                InlineToken::Tag(t) => format!("tag:{}", text_of(text, t.name)),
                InlineToken::BlockRef(b) => {
                    format!("block:{}:{}", text_of(text, b.id), b.valid)
                }
                InlineToken::Link(l) => format!("link:{}", text_of(text, l.label)),
                InlineToken::Macro(m) => format!(
                    "macro:{}[{}]",
                    text_of(text, m.name),
                    m.args
                        .iter()
                        .map(|a| text_of(text, *a))
                        .collect::<Vec<_>>()
                        .join("|")
                ),
            })
            .collect()
    }

    #[test]
    fn all_basic_forms() {
        assert_eq!(
            kinds("[[a]] #t #[[m w]] ((6500c1a4-0000-4000-8000-000000000001)) x"),
            [
                "page:a",
                "tag:t",
                "tag:m w",
                "block:6500c1a4-0000-4000-8000-000000000001:true"
            ]
        );
    }

    #[test]
    fn nested_page_refs() {
        let text = "[[a [[b]] c]] #[[n [[t]]]]";
        let toks = scan(text);
        let InlineToken::PageRef(p) = &toks[0] else {
            panic!("page ref expected")
        };
        assert_eq!(text_of(text, p.name), "a [[b]] c");
        assert_eq!(p.nested.len(), 1);
        assert_eq!(text_of(text, p.nested[0].name), "b");
        let InlineToken::Tag(t) = &toks[1] else {
            panic!("tag expected")
        };
        assert!(t.bracketed);
        assert_eq!(text_of(text, t.name), "n [[t]]");
        assert_eq!(t.nested.len(), 1);
    }

    #[test]
    fn tag_end_rules() {
        assert_eq!(kinds("#tag."), ["tag:tag"]);
        assert_eq!(kinds("#foo:"), ["tag:foo"]);
        assert_eq!(kinds("#a.b"), ["tag:a.b"]);
        assert_eq!(kinds("#a,#b"), ["tag:a", "tag:b"]);
        assert_eq!(kinds("#a;x"), ["tag:a;x"]);
        assert_eq!(kinds("#a;; b"), ["tag:a"]);
        assert_eq!(kinds("#a!x"), ["tag:a"]);
        assert_eq!(kinds("#a.,x"), ["tag:a."]);
        assert_eq!(kinds("#a)"), ["tag:a)"]);
        assert_eq!(kinds("#a#b"), ["tag:a", "tag:b"]);
        assert_eq!(kinds("# x ## y #"), Vec::<String>::new());
        assert_eq!(kinds("foo#bar"), ["tag:bar"]);
        assert_eq!(kinds("#a[[b]]"), ["tag:a", "page:b"]);
    }

    #[test]
    fn suppressed_regions() {
        assert!(kinds("`[[x]]`").iter().all(|k| k.starts_with("code")));
        assert_eq!(kinds("\\[[x]]"), Vec::<String>::new());
        assert_eq!(kinds("\\\\[[c]]"), ["page:c"]);
        assert_eq!(kinds("``[[c]] ` [[d]]``"), ["code:``[[c]] ` [[d]]``"]);
        assert_eq!(kinds("`[[e]]"), ["page:e"]);
        assert_eq!(
            kinds("$$[[m]]$$ \\(x [[m]]\\)"),
            ["math:$$[[m]]$$", "math:\\(x [[m]]\\)"]
        );
        assert_eq!(kinds("cost $5 and [[a]] then $6"), ["page:a"]);
        assert_eq!(
            kinds("<span>[[c]]</span> [[d]]"),
            ["html:<span>[[c]]</span>", "page:d"]
        );
        assert_eq!(
            kinds("https://x.com/[[b]] [[c]]"),
            ["url:https://x.com/[[b]]", "page:c"]
        );
        assert_eq!(
            kinds("<https://x.com/[[a]]>"),
            ["url:<https://x.com/[[a]]>"]
        );
    }

    #[test]
    fn links() {
        assert_eq!(kinds("[l]([[p]])"), ["link:l"]);
        let text = "[l](((6500c1a4-0000-4000-8000-000000000001))) [x](file:../p.md) ![i](a.png) [t](http://x.com)";
        let toks = scan(text);
        assert_eq!(toks.len(), 4);
        let InlineToken::Link(l0) = &toks[0] else {
            panic!()
        };
        assert!(matches!(&l0.target, LinkTarget::Block(b) if b.valid));
        let InlineToken::Link(l1) = &toks[1] else {
            panic!()
        };
        assert!(matches!(l1.target, LinkTarget::File(_)));
        let InlineToken::Link(l2) = &toks[2] else {
            panic!()
        };
        assert!(l2.image && matches!(l2.target, LinkTarget::Search(_)));
        assert_eq!(&text[l2.span.range()], "![i](a.png)");
        let InlineToken::Link(l3) = &toks[3] else {
            panic!()
        };
        assert!(matches!(l3.target, LinkTarget::Url(_)));
        assert_eq!(kinds("![[x]]"), ["page:x"]);
        assert_eq!(kinds("[ [[a]] ]"), ["page:a"]);
    }

    #[test]
    fn macros() {
        assert_eq!(kinds("{{embed [[p]]}}"), ["macro:embed[[[p]]]"]);
        assert_eq!(
            kinds("{{query (and [[a]] (task TODO))}}"),
            ["macro:query[(and [[a]] (task TODO))]"]
        );
        assert_eq!(kinds("{{{x a,b}}}"), ["macro:x[a|b]"]);
        assert_eq!(kinds("{{a b [[c]], d}}"), ["macro:a[b [[c]]|d]"]);
        assert_eq!(kinds("{{a [[c]] b, d}}"), ["page:c"]);
        assert_eq!(kinds("{{a \"b, c\", d}}"), ["macro:a[\"b, c\"|d]"]);
        assert_eq!(kinds("{{embed}}"), ["macro:embed[]"]);
        assert_eq!(kinds("{{ embed [[e]]}}"), ["page:e"]);
        assert_eq!(kinds("{{a ,}}"), Vec::<String>::new());
        assert_eq!(kinds("{{a}}}"), ["macro:a[]"]);
        assert_eq!(kinds("{{embed [[f]]"), ["page:f"]);
    }

    #[test]
    fn block_refs() {
        assert_eq!(kinds("((not-a-uuid))"), ["block:not-a-uuid:false"]);
        assert_eq!(kinds("(())"), Vec::<String>::new());
        assert!(is_uuid("6500c1a4-0000-4000-8000-000000000001"));
        assert!(!is_uuid("6500c1a4-0000-4000-8000-00000000000"));
    }

    #[test]
    fn inline_constructs_do_not_span_lines() {
        assert_eq!(kinds("[[a\nb]]"), Vec::<String>::new());
        assert_eq!(kinds("x `a\nb [[c]]` [[d]]"), ["page:c", "page:d"]);
        assert_eq!(kinds("a\r\n[[b]]\r\n"), ["page:b"]);
    }

    #[test]
    fn unicode_is_safe() {
        assert_eq!(
            kinds("日本語 [[ページ]] #タグ ñ"),
            ["page:ページ", "tag:タグ"]
        );
    }
}
