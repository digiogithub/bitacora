//! Page properties: the properties Logseq attaches to a *page* (`title`, `alias`, `tags`, ...).
//!
//! Logseq reads them from three places (`docs/analysis/logseq/02-markdown-block-syntax.md` §2.5;
//! `extract.cljc:215-241`, `mldoc.cljc:117-131`, `block.cljs:515-553,640-683`):
//!
//! 1. **YAML front matter** at byte 0 (`---` ... `---`). mldoc turns every `key: value` line into
//!    a `Directive`; if any line is not of that shape (a blank line, a comment, a list item, ...)
//!    the whole front matter yields nothing but is still consumed.
//! 2. **`#+key: value` directives** anywhere in the file, even deep inside a block
//!    (`- #+title: x`). Directives that directly follow a `key:: value` line join that property
//!    group instead and are not hoisted.
//! 3. **The pre-block**: the first property group (`key:: value` lines, or a `:PROPERTIES:`
//!    drawer) among the text before the first block.
//!
//! If a file has any front-matter or standalone directive, `collect-page-properties` prepends a
//! `Properties` node built from *all* of them and that node wins over the pre-block properties
//! (observed with mldoc 1.5.7). Otherwise the pre-block group is the page's properties.
//!
//! Everything is read-only; spans point into the input and the bytes are never touched, so front
//! matter and the pre-block are preserved verbatim.

use crate::lines::{LineKind, Lines, ParserOptions, RegionKind, RegionPart, is_ws};
use crate::outline::split_with;
use crate::properties::{
    PropLineKind, PropValue, PropertyConfig, interpret, is_valid_key, normalize_key,
    scan_properties,
};
use crate::span::Span;
use crate::tasks::find_drawers;

/// Where one page property was written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageOrigin {
    /// A `key: value` line of the YAML front matter.
    FrontMatter,
    /// A standalone `#+key: value` line.
    Directive,
    /// A `key:: value` line of the pre-block (or a `#+key:` line joined to it).
    PropertyLine,
    /// A line of a `:PROPERTIES:` drawer in the pre-block.
    Drawer,
}

/// Which source supplied the page properties.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageSource {
    /// Front matter and/or `#+key:` directives (they override the pre-block).
    Directives,
    /// The first property group of the pre-block.
    PreBlock,
}

/// One page property.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageProperty {
    /// The key as written.
    pub key_raw: String,
    /// The key normalised for lookup (lower-case, `_` and spaces become `-`).
    pub key_norm: String,
    /// The trimmed value as written.
    pub value_raw: String,
    /// False when Logseq would reject the key.
    pub valid: bool,
    /// Where it was written.
    pub origin: PageOrigin,
    /// The whole line including its EOL.
    pub line: Span,
    /// The key bytes.
    pub key_span: Span,
    /// The trimmed value bytes.
    pub value_span: Span,
}

/// The page properties of a file.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PageProperties {
    /// Which source won, `None` when the page has no properties.
    pub source: Option<PageSource>,
    /// The properties in document order (duplicates kept; the last one wins in [`Self::get`]).
    pub properties: Vec<PageProperty>,
    /// The front matter region (from `---` through the closing `---` line), when present, even if
    /// it yielded no properties.
    pub front_matter: Option<Span>,
    /// The pre-block span (text before the first block), when present.
    pub pre_block: Option<Span>,
    /// False when the pre-block's properties contain `heading`: Logseq then does not treat it as a
    /// pre-block (`block.cljs:532`).
    pub is_pre_block: bool,
}

impl PageProperties {
    /// The last valid property with the normalised key `key`.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&PageProperty> {
        self.properties
            .iter()
            .rev()
            .find(|p| p.valid && p.key_norm == key)
    }

    /// The interpreted value of `key` (see [`interpret`]).
    #[must_use]
    pub fn value(&self, key: &str, cfg: &PropertyConfig) -> Option<PropValue> {
        self.get(key)
            .map(|p| interpret(&p.key_norm, &p.value_raw, cfg))
    }

    /// The `title` property, verbatim (Logseq never parses it).
    #[must_use]
    pub fn title(&self) -> Option<&str> {
        self.get("title").map(|p| p.value_raw.as_str())
    }
}

const BOM: &[u8] = b"\xef\xbb\xbf";

fn trim_span(input: &[u8], mut start: usize, mut end: usize) -> Span {
    while start < end && is_ws(input[start]) {
        start += 1;
    }
    while end > start && is_ws(input[end - 1]) {
        end -= 1;
    }
    Span::new(start, end)
}

fn property(input: &[u8], origin: PageOrigin, line: Span, key: Span, value: Span) -> PageProperty {
    let key_raw = String::from_utf8_lossy(key.slice(input)).into_owned();
    let key_norm = normalize_key(&key_raw);
    PageProperty {
        valid: is_valid_key(&key_norm),
        key_raw,
        key_norm,
        value_raw: String::from_utf8_lossy(value.slice(input)).into_owned(),
        origin,
        line,
        key_span: key,
        value_span: value,
    }
}

/// `#+name: value` at the start of `text` (an absolute offset `at` into `input`).
fn directive_at(input: &[u8], at: usize, end: usize, line: Span) -> Option<PageProperty> {
    let text = &input[at..end];
    let body = text.strip_prefix(b"#+")?;
    let name_len = body.iter().take_while(|&&b| b != b':' && !is_ws(b)).count();
    if name_len == 0 || body.get(name_len) != Some(&b':') {
        return None;
    }
    let after = at + 2 + name_len + 1;
    if input.get(after).is_some_and(|&b| after < end && !is_ws(b)) {
        return None;
    }
    Some(property(
        input,
        PageOrigin::Directive,
        line,
        Span::new(at + 2, at + 2 + name_len),
        trim_span(input, after.min(end), end),
    ))
}

/// Front matter lines: every line must be `key: value` (a colon after a non-empty key).
fn front_matter_properties(input: &[u8], lines: &[(Span, usize, usize)]) -> Vec<PageProperty> {
    let mut out = Vec::new();
    for &(line, start, end) in lines {
        let text = &input[start..end];
        let Some(colon) = text.iter().position(|&b| b == b':') else {
            return Vec::new();
        };
        if colon == 0 {
            return Vec::new();
        }
        out.push(property(
            input,
            PageOrigin::FrontMatter,
            line,
            Span::new(start, start + colon),
            trim_span(input, start + colon + 1, end),
        ));
    }
    out
}

/// Computes the page properties of `input` (a whole page file).
#[must_use]
pub fn page_properties(input: &[u8], opts: ParserOptions) -> PageProperties {
    let outline = split_with(input, opts);

    // Spans of every property group (their `#+` lines are not standalone directives).
    let mut group_spans: Vec<Span> = Vec::new();
    let mut pre_group = None;
    let ranges = outline
        .pre_block
        .into_iter()
        .chain(outline.blocks.iter().map(|b| b.span));
    for (i, range) in ranges.enumerate() {
        let scan = scan_properties(input, range, opts);
        group_spans.extend(scan.groups.iter().map(|g| g.span));
        if i == 0 && outline.pre_block.is_some() {
            pre_group = scan.groups.into_iter().next();
        }
    }

    // `:LOGBOOK:` ... `:END:` drawers hide their content.
    if let Ok(text) = std::str::from_utf8(input) {
        group_spans.extend(find_drawers(text, opts).iter().map(|d| d.span));
    }

    // One pass over the lines: front matter and standalone directives.
    let mut front_lines: Vec<(Span, usize, usize)> = Vec::new();
    let mut front_span: Option<Span> = None;
    let mut directives: Vec<PageProperty> = Vec::new();
    for line in Lines::with_options(input, opts) {
        match line.region {
            Some((RegionKind::FrontMatter, part)) => {
                match part {
                    RegionPart::Open => front_span = Some(line.span()),
                    RegionPart::Inside => {
                        front_lines.push((line.span(), line.start, line.content_end()));
                    }
                    RegionPart::Close => {
                        if let Some(s) = &mut front_span {
                            s.end = line.end;
                        }
                    }
                }
                continue;
            }
            Some(_) => continue,
            None => {}
        }
        if group_spans
            .iter()
            .any(|s| line.start < s.end && s.start < line.end)
        {
            continue;
        }
        let mut at = match line.kind {
            LineKind::BulletStart { after_dash, .. } => line.start + after_dash,
            _ => line.start + line.indent.len(),
        };
        if line.start == 0 && line.content.starts_with(BOM) {
            at = BOM.len()
                + line.content[BOM.len()..]
                    .iter()
                    .take_while(|&&b| is_ws(b))
                    .count();
        }
        if at < line.content_end()
            && let Some(p) = directive_at(input, at, line.content_end(), line.span())
        {
            directives.push(p);
        }
    }

    let mut result = PageProperties {
        front_matter: front_span,
        pre_block: outline.pre_block,
        is_pre_block: outline.pre_block.is_some(),
        ..PageProperties::default()
    };

    let mut props = if front_span.is_some() {
        front_matter_properties(input, &front_lines)
    } else {
        Vec::new()
    };
    props.extend(directives);
    if !props.is_empty() {
        result.source = Some(PageSource::Directives);
        result.properties = props;
    } else if let Some(group) = pre_group {
        result.source = Some(PageSource::PreBlock);
        result.properties = group
            .lines
            .iter()
            .map(|l| {
                let origin = match l.kind {
                    PropLineKind::Drawer => PageOrigin::Drawer,
                    _ => PageOrigin::PropertyLine,
                };
                let mut p = property(input, origin, l.span, l.key_span, l.value_span);
                p.valid = l.valid;
                p.key_norm.clone_from(&l.key_norm);
                p
            })
            .collect();
    }
    if result.source == Some(PageSource::PreBlock)
        && result.properties.iter().any(|p| p.key_norm == "heading")
    {
        result.is_pre_block = false;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn props(s: &str) -> PageProperties {
        page_properties(s.as_bytes(), ParserOptions::default())
    }

    fn kv(p: &PageProperties) -> Vec<(String, String)> {
        p.properties
            .iter()
            .map(|p| (p.key_norm.clone(), p.value_raw.clone()))
            .collect()
    }

    fn pairs(items: &[(&str, &str)]) -> Vec<(String, String)> {
        items
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    #[test]
    fn pre_block_properties() {
        let p = props(
            "title:: My Page\nalias:: Mine, [[My page alias]]\ntags:: project, [[multi word]]\n\n- first block",
        );
        assert_eq!(p.source, Some(PageSource::PreBlock));
        assert_eq!(
            kv(&p),
            pairs(&[
                ("title", "My Page"),
                ("alias", "Mine, [[My page alias]]"),
                ("tags", "project, [[multi word]]")
            ])
        );
        assert_eq!(p.title(), Some("My Page"));
        assert!(p.is_pre_block);
        let cfg = PropertyConfig::default();
        let Some(PropValue::Pages(pages)) = p.value("tags", &cfg) else {
            panic!("tags should be pages")
        };
        assert_eq!(
            pages.into_iter().collect::<Vec<_>>(),
            ["multi word", "project"]
        );
    }

    #[test]
    fn yaml_front_matter_is_preserved_and_yields_properties() {
        let src = "---\ntitle: Front\ntags: a\n---\n- block";
        let p = props(src);
        assert_eq!(p.source, Some(PageSource::Directives));
        assert_eq!(p.title(), Some("Front"));
        assert_eq!(p.get("tags").map(|t| t.value_raw.as_str()), Some("a"));
        assert_eq!(
            p.get("title").map(|t| t.origin),
            Some(PageOrigin::FrontMatter)
        );
        let fm = p.front_matter.expect("front matter span");
        assert_eq!(&src[fm.range()], "---\ntitle: Front\ntags: a\n---\n");
        // The first block starts right after it; the bytes are untouched.
        assert_eq!(p.pre_block, Some(fm));
    }

    #[test]
    fn front_matter_needs_every_line_to_be_key_value() {
        for src in [
            "---\ntitle: x\n\nb: c\n---\n- a",
            "---\n# comment\ntitle: x\n---\n- a",
            "---\ntitle: x\n  - item\n---\n- a",
            "---\n---\n- a",
        ] {
            let p = props(src);
            assert!(p.properties.is_empty(), "{src:?}");
            assert!(p.front_matter.is_some(), "{src:?}");
        }
        // Not at byte 0, unclosed, or TOML: no front matter at all.
        for src in [
            " ---\ntitle: x\n---\n- a",
            "---\ntitle: x\n- a",
            "+++\ntitle = \"x\"\n+++\n- a",
            "\n---\ntitle: x\n---\n- a",
        ] {
            let p = props(src);
            assert!(p.front_matter.is_none(), "{src:?}");
            assert!(p.properties.is_empty(), "{src:?}");
        }
    }

    #[test]
    fn front_matter_key_rules() {
        let p = props(
            "---\nTitle: Mixed Case\nkey with space: v\na:b\nempty:\nurl: http://x/y\n---\n- b",
        );
        assert_eq!(
            kv(&p),
            pairs(&[
                ("title", "Mixed Case"),
                ("key-with-space", "v"),
                ("a", "b"),
                ("empty", ""),
                ("url", "http://x/y"),
            ])
        );
        assert_eq!(props("---\r\ntitle: x\r\n---\r\n- b").title(), Some("x"));
    }

    #[test]
    fn directives_anywhere_are_hoisted() {
        let p = props("- a\n\t- #+title: x\n- b");
        assert_eq!(p.source, Some(PageSource::Directives));
        assert_eq!(p.title(), Some("x"));
        assert_eq!(
            p.get("title").map(|t| t.origin),
            Some(PageOrigin::Directive)
        );

        let p = props("#+title: Directive\n#+alias: a, b\n#+TAGS: x\n- block");
        assert_eq!(
            kv(&p),
            pairs(&[("title", "Directive"), ("alias", "a, b"), ("tags", "x")])
        );
        assert_eq!(props("text\n#+title: mid\n- a").title(), Some("mid"));
        assert_eq!(props("#+title:\n- a").title(), Some(""));
    }

    #[test]
    fn directives_override_the_pre_block() {
        let p = props("title:: P\n\n- a\n- #+title: D");
        assert_eq!(p.source, Some(PageSource::Directives));
        assert_eq!(p.title(), Some("D"));
        assert_eq!(kv(&p).len(), 1);
        // Front matter first, then directives.
        let p = props("---\ntitle: F\n---\n- a\n- #+alias: x");
        assert_eq!(kv(&p), pairs(&[("title", "F"), ("alias", "x")]));
    }

    #[test]
    fn directive_joined_to_a_property_group_is_not_hoisted() {
        let p = props("title:: P\n#+title: D\n- block");
        assert_eq!(p.source, Some(PageSource::PreBlock));
        assert_eq!(kv(&p), pairs(&[("title", "P"), ("title", "D")]));
        assert_eq!(p.title(), Some("D"));
    }

    #[test]
    fn heading_property_is_not_a_pre_block() {
        let p = props("heading:: true\n- a");
        assert_eq!(p.source, Some(PageSource::PreBlock));
        assert!(!p.is_pre_block);
        assert!(props("title:: x\n- a").is_pre_block);
        assert!(!props("- a").is_pre_block);
    }

    #[test]
    fn bom_does_not_leak_into_the_key() {
        let p = props("\u{feff}title:: x\n- a");
        assert_eq!(p.title(), Some("x"));
        assert_eq!(p.properties[0].key_raw, "title");
        let p = props("\u{feff}#+title: y\n- a");
        assert_eq!(p.title(), Some("y"));
    }

    #[test]
    fn code_drawers_and_quotes_hide_directives() {
        assert!(
            props("- a\n  ```\n  #+title: x\n  ```\n")
                .properties
                .is_empty()
        );
        assert!(
            props("- a\n  :LOGBOOK:\n  #+title: x\n  :END:\n")
                .properties
                .is_empty()
        );
        assert!(props("- a\n  > #+title: x\n").properties.is_empty());
        assert!(
            props("- a\n  #+BEGIN_SRC\n  #+title: x\n  #+END_SRC\n")
                .properties
                .is_empty()
        );
    }

    #[test]
    fn properties_drawer_in_the_pre_block() {
        let p = props(":PROPERTIES:\n:title: T\n:END:\n- a");
        assert_eq!(p.source, Some(PageSource::PreBlock));
        assert_eq!(p.title(), Some("T"));
        assert_eq!(p.get("title").map(|t| t.origin), Some(PageOrigin::Drawer));
    }

    #[test]
    fn no_properties() {
        let p = props("- just a block\n- another");
        assert_eq!(p.source, None);
        assert!(p.properties.is_empty());
        assert_eq!(props("").source, None);
    }
}
