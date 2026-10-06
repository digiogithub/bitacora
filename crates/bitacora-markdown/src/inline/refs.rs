//! Turns inline tokens into the reference sets Logseq computes for a block.
//!
//! This is the equivalent of the `with-page-refs` / `with-block-refs` walks
//! (`docs/analysis/logseq/03-parsing-indexing-search.md`): which tokens are page references, which
//! are block references, which are links that must *not* become references.

use crate::inline::scan::{self, InlineToken, LinkTarget, PageRef};

/// Which Logseq walk to mimic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefMode {
    /// Block title and body text: nested references count, `{{embed ...}}` is a reference, other
    /// macros are only recorded.
    Content,
    /// A property value: only top-level references count (a nested name is one page), block
    /// references are ignored and macro arguments are scanned for references.
    PropertyValue,
}

/// A macro occurrence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacroCall {
    /// The macro name as written (`embed`, `query`, ...).
    pub name: String,
    /// The arguments (trailing blanks removed).
    pub args: Vec<String>,
}

/// What a link that is not a reference points at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkRefKind {
    /// `[[assets/x.pdf]]` (a page reference to a local asset).
    Asset,
    /// `[[draws/x.excalidraw]]`.
    Draw,
    /// `[label](file:...)`.
    File,
    /// A URL with a scheme.
    Url,
    /// A relative path (`../assets/a.png`).
    Search,
}

/// A link that is not a page reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkRef {
    /// The kind of link.
    pub kind: LinkRefKind,
    /// The target (page name for assets and drawings, path or URL otherwise).
    pub target: String,
    /// The label, empty when there is none.
    pub label: String,
}

/// The references found in some text, in first-seen order without duplicates.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RefSet {
    /// Referenced page names (trimmed, never blank): page refs, tags, nested refs, embeds. Does
    /// **not** include namespace parents, see [`RefSet::pages_with_namespace_parents`].
    pub pages: Vec<String>,
    /// The subset of `pages` written as tags.
    pub tags: Vec<String>,
    /// Referenced block ids (valid UUIDs only).
    pub block_refs: Vec<String>,
    /// Every macro, embeds included.
    pub macros: Vec<MacroCall>,
    /// Asset/drawing/file/URL links.
    pub links: Vec<LinkRef>,
}

impl RefSet {
    /// `pages` followed by the namespace parents of each page (`a/b/c` adds `a` and `a/b`), like
    /// Logseq's `:block/refs`.
    #[must_use]
    pub fn pages_with_namespace_parents(&self) -> Vec<String> {
        let mut out = self.pages.clone();
        for p in &self.pages {
            for parent in namespace_parents(p) {
                if !out.contains(&parent) {
                    out.push(parent);
                }
            }
        }
        out
    }

    /// Adds everything from `other`.
    pub fn merge(&mut self, other: RefSet) {
        for p in other.pages {
            push_unique(&mut self.pages, p);
        }
        for p in other.tags {
            push_unique(&mut self.tags, p);
        }
        for p in other.block_refs {
            push_unique(&mut self.block_refs, p);
        }
        self.macros.extend(other.macros);
        self.links.extend(other.links);
    }
}

fn push_unique(v: &mut Vec<String>, s: String) {
    if !v.contains(&s) {
        v.push(s);
    }
}

/// True when Logseq treats a page reference as a link, not a page: a local asset
/// (`^[./]*assets`) or a drawing (`draws...`).
#[must_use]
pub fn is_asset_or_draw(name: &str) -> bool {
    name.starts_with("draws") || name.trim_start_matches(['.', '/']).starts_with("assets")
}

/// The namespace prefixes of a page name: `a/b/c` gives `a`, `a/b`, `a/b/c`. A name that is a
/// relative path, a URL or has no `/` gives none. Names with a nested reference use the first
/// nested name, as Logseq does.
#[must_use]
pub fn namespace_parents(name: &str) -> Vec<String> {
    let mut base = name;
    if let Some(start) = name.find("[[")
        && let Some(end) = name[start..].find("]]")
    {
        base = &name[start + 2..start + end];
    }
    if !base.contains('/')
        || base.starts_with("../")
        || base.starts_with("./")
        || base.contains("://")
    {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut acc = String::new();
    for (i, part) in base.split('/').enumerate() {
        if i > 0 {
            acc.push('/');
        }
        acc.push_str(part);
        let t = acc.trim();
        if !t.is_empty() && !out.iter().any(|o: &String| o == t) {
            out.push(t.to_owned());
        }
    }
    out
}

/// Collects the references of `tokens` (the tokens of `text`).
#[must_use]
pub fn collect(text: &str, tokens: &[InlineToken], mode: RefMode) -> RefSet {
    let mut c = Collector {
        text,
        mode,
        set: RefSet::default(),
    };
    c.tokens(tokens);
    c.set
}

struct Collector<'a> {
    text: &'a str,
    mode: RefMode,
    set: RefSet,
}

impl Collector<'_> {
    fn slice(&self, s: crate::span::Span) -> &str {
        &self.text[s.range()]
    }

    fn tokens(&mut self, tokens: &[InlineToken]) {
        for t in tokens {
            self.token(t);
        }
    }

    fn add_page(&mut self, name: &str) {
        let name = name.trim();
        if !name.is_empty() {
            push_unique(&mut self.set.pages, name.to_owned());
        }
    }

    fn add_block(&mut self, id: &str) {
        if scan::is_uuid(id) {
            push_unique(&mut self.set.block_refs, id.to_owned());
        }
    }

    fn page_ref(&mut self, p: &PageRef) {
        let name = self.slice(p.name).to_owned();
        if is_asset_or_draw(&name) {
            let kind = if name.starts_with("draws") {
                LinkRefKind::Draw
            } else {
                LinkRefKind::Asset
            };
            self.set.links.push(LinkRef {
                kind,
                target: name,
                label: String::new(),
            });
            return;
        }
        self.add_page(&name);
        if self.mode == RefMode::Content {
            for n in &p.nested {
                self.page_ref(n);
            }
        }
    }

    fn token(&mut self, t: &InlineToken) {
        match t {
            InlineToken::Code(_)
            | InlineToken::Math(_)
            | InlineToken::Html(_)
            | InlineToken::Url(_) => {}
            InlineToken::PageRef(p) => self.page_ref(p),
            InlineToken::Tag(tag) => {
                let name = self.slice(tag.name).trim().to_owned();
                if !name.is_empty() {
                    self.add_page(&name);
                    push_unique(&mut self.set.tags, name);
                }
                if self.mode == RefMode::Content {
                    for n in &tag.nested {
                        self.page_ref(n);
                    }
                }
            }
            InlineToken::BlockRef(b) => {
                if self.mode == RefMode::Content {
                    let id = self.slice(b.id).to_owned();
                    self.add_block(&id);
                }
            }
            InlineToken::Link(l) => self.link(l),
            InlineToken::Macro(m) => self.macro_call(m),
        }
    }

    fn link(&mut self, l: &scan::Link) {
        let label = self.slice(l.label).to_owned();
        match &l.target {
            LinkTarget::Page(p) => self.page_ref(p),
            LinkTarget::Block(b) => {
                if self.mode == RefMode::Content {
                    let id = self.slice(b.id).to_owned();
                    self.add_block(&id);
                }
            }
            LinkTarget::File(s) => {
                let target = self.slice(*s).to_owned();
                if self.mode == RefMode::Content {
                    self.add_page(&label);
                }
                self.set.links.push(LinkRef {
                    kind: LinkRefKind::File,
                    target,
                    label,
                });
            }
            LinkTarget::Url(s) | LinkTarget::Search(s) => {
                let kind = if matches!(l.target, LinkTarget::Url(_)) {
                    LinkRefKind::Url
                } else {
                    LinkRefKind::Search
                };
                let target = self.slice(*s).to_owned();
                self.set.links.push(LinkRef {
                    kind,
                    target,
                    label,
                });
            }
        }
    }

    fn macro_call(&mut self, m: &scan::Macro) {
        let name = self.slice(m.name).to_owned();
        let args: Vec<String> = m
            .args
            .iter()
            .map(|a| self.slice(*a).trim_end().to_owned())
            .collect();
        match self.mode {
            RefMode::Content => {
                if name == "embed" {
                    let joined = args.join(", ");
                    let joined = joined.trim();
                    if let Some(inner) =
                        joined.strip_prefix("[[").and_then(|s| s.strip_suffix("]]"))
                    {
                        self.add_page(inner);
                    } else if let Some(id) =
                        joined.strip_prefix("((").and_then(|s| s.strip_suffix("))"))
                    {
                        // An embedded block is a block reference (not a page named after the id).
                        self.add_block(id.trim());
                    } else {
                        self.add_page(joined);
                    }
                }
            }
            RefMode::PropertyValue => {
                for a in &m.args {
                    let toks = scan::scan_line(self.text, a.start, a.end);
                    self.tokens(&toks);
                }
            }
        }
        self.set.macros.push(MacroCall { name, args });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refs(text: &str, mode: RefMode) -> RefSet {
        collect(text, &scan::scan(text), mode)
    }

    fn pages(text: &str) -> Vec<String> {
        refs(text, RefMode::Content).pages
    }

    #[test]
    fn content_refs_and_nesting() {
        assert_eq!(pages("[[a [[b]] c]]"), ["a [[b]] c", "b"]);
        assert_eq!(pages("#[[nested [[tag]]]]"), ["nested [[tag]]", "tag"]);
        assert_eq!(pages("[[a]] #b #[[c d]] [[a]]"), ["a", "b", "c d"]);
        assert_eq!(refs("#b [[a]]", RefMode::Content).tags, ["b"]);
        assert_eq!(pages("[[ ]] [[ a ]]"), ["a"]);
    }

    #[test]
    fn assets_and_drawings_are_links() {
        let r = refs(
            "[[assets/x.pdf]] [[../assets/y.png]] [[draws/x.excalidraw]]",
            RefMode::Content,
        );
        assert!(r.pages.is_empty());
        assert_eq!(r.links.len(), 3);
        assert_eq!(r.links[2].kind, LinkRefKind::Draw);
        assert_eq!(r.links[0].kind, LinkRefKind::Asset);
    }

    #[test]
    fn embeds_and_queries() {
        let r = refs(
            "{{embed [[page]]}} {{embed ((6500c1a4-0000-4000-8000-000000000001))}} {{query (and [[a]] (task TODO))}}",
            RefMode::Content,
        );
        assert_eq!(r.pages, ["page"]);
        assert_eq!(r.block_refs, ["6500c1a4-0000-4000-8000-000000000001"]);
        assert_eq!(r.macros.len(), 3);
        assert_eq!(r.macros[2].name, "query");
        assert_eq!(r.macros[2].args, ["(and [[a]] (task TODO))"]);
    }

    #[test]
    fn block_refs_need_a_valid_uuid() {
        let r = refs(
            "((6500c1a4-0000-4000-8000-000000000001)) ((nope))",
            RefMode::Content,
        );
        assert_eq!(r.block_refs, ["6500c1a4-0000-4000-8000-000000000001"]);
        let r = refs(
            "[l](((6500c1a4-0000-4000-8000-000000000002)))",
            RefMode::Content,
        );
        assert_eq!(r.block_refs, ["6500c1a4-0000-4000-8000-000000000002"]);
    }

    #[test]
    fn file_links_reference_their_label() {
        assert_eq!(
            pages("[hello world](file:../pages/hello_world.md)"),
            ["hello world"]
        );
    }

    #[test]
    fn namespace_parents_are_added() {
        let r = refs("[[a/b/c]] [[x]] [[../r/s]]", RefMode::Content);
        assert_eq!(
            r.pages_with_namespace_parents(),
            ["a/b/c", "x", "../r/s", "a", "a/b"]
        );
        assert_eq!(namespace_parents("a [[b/c]] d"), ["b", "b/c"]);
        assert!(namespace_parents("https://x/y").is_empty());
    }

    #[test]
    fn property_values_use_top_level_refs_only() {
        let r = refs(
            "[[a [[b]] c]] ((6500c1a4-0000-4000-8000-000000000001))",
            RefMode::PropertyValue,
        );
        assert_eq!(r.pages, ["a [[b]] c"]);
        assert!(r.block_refs.is_empty());
        let r = refs(
            "{{embed [[x]]}} {{query [[q]]}} x(#t)",
            RefMode::PropertyValue,
        );
        assert_eq!(r.pages, ["x", "q", "t)"]);
    }
}
