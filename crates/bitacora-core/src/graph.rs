//! In-memory graph domain model: pages (file-backed and virtual), page properties, alias sets,
//! namespaces and duplicate-title resolution. Spec: `01-file-graph-layout.md` §3.1, §3.5, §3.6,
//! §9; BIT-SP-0002.R8, R12, R18, R20.
//!
//! Loading is strictly read-only. Virtual pages (alias, tag and namespace-parent pages) never
//! imply a file. Re-implemented from documented behaviour (ADR-015).

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use bitacora_config::EffectiveConfig;
use bitacora_markdown::ParserOptions;
use bitacora_markdown::inline::namespace_parents;
use bitacora_markdown::page_props::page_properties;
use bitacora_markdown::properties::{PropValue, PropertyConfig, interpret};

use crate::graph_path::GraphPath;
use crate::journal::{JournalPage, detect_journal};
use crate::naming::{derive_title, page_key};
use crate::scan::{ScanError, ScannedFile, decode_text, parse_order, scan_graph};

/// Journal information of a page (alias of [`JournalPage`]).
pub type JournalInfo = JournalPage;

/// Page identity: `page_name_sanity_lc(title)` (lower-case, one boundary `/` stripped, NFC).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PageKey(String);

impl PageKey {
    /// Key of a page title.
    pub fn from_title(title: &str) -> Self {
        Self(page_key(title))
    }

    /// The key text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Block identity (`id::` UUID, lower-case canonical form).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BlockId(String);

impl BlockId {
    /// Parse a canonical `8-4-4-4-12` hexadecimal UUID (case-insensitive; stored lower-case).
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        let groups: Vec<&str> = s.split('-').collect();
        let ok = groups.len() == 5
            && groups
                .iter()
                .zip([8usize, 4, 4, 4, 12])
                .all(|(g, n)| g.len() == n && g.bytes().all(|b| b.is_ascii_hexdigit()));
        ok.then(|| Self(s.to_ascii_lowercase()))
    }

    /// The canonical UUID text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A block of a page (identity and text only; the outline tree is built by the editor layer).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    /// Persisted `id::`, when the block has one.
    pub id: Option<BlockId>,
    /// Block content as written.
    pub content: String,
    /// Child blocks.
    pub children: Vec<Block>,
}

/// Page file format, decided per file by extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageFormat {
    /// `.md` / `.markdown`.
    Markdown,
    /// `.org` (read-only).
    Org,
}

impl PageFormat {
    /// Format for a file extension (case-insensitive); `None` for non-page extensions.
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_ascii_lowercase().as_str() {
            "md" | "markdown" => Some(Self::Markdown),
            "org" => Some(Self::Org),
            _ => None,
        }
    }
}

/// Why a page exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageOrigin {
    /// Backed by a file.
    File,
    /// Virtual: declared through `alias::`.
    Alias,
    /// Virtual: declared through `tags::`.
    Tag,
    /// Virtual: namespace parent of another page.
    Namespace,
}

/// A page, file-backed or virtual.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    /// Identity.
    pub key: PageKey,
    /// Display title (journals: the formatted journal title).
    pub original_name: String,
    /// Backing file; `None` for virtual pages.
    pub file: Option<GraphPath>,
    /// File format (Markdown for virtual pages).
    pub format: PageFormat,
    /// Journal info when the title is a journal date.
    pub journal: Option<JournalInfo>,
    /// Page properties from the pre-block / front matter: normalised key -> raw value.
    pub props: BTreeMap<String, String>,
    /// Alias names declared by this page (blank and self aliases dropped).
    pub aliases: Vec<String>,
    /// Tag page names declared by this page.
    pub tags: Vec<String>,
    /// Parent in the `a/b/c` namespace hierarchy.
    pub namespace_parent: Option<PageKey>,
    /// Never written by Bitacora (`.org` pages).
    pub read_only: bool,
    /// Why the page exists.
    pub origin: PageOrigin,
    /// For alias pages: the page that declared the alias.
    pub alias_of: Option<PageKey>,
}

impl Page {
    /// `public:: true|false`.
    pub fn public(&self) -> Option<bool> {
        match self.props.get("public")?.trim() {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        }
    }

    /// `icon::` value.
    pub fn icon(&self) -> Option<&str> {
        self.props.get("icon").map(String::as_str)
    }

    /// `filters::` raw EDN map text.
    pub fn filters(&self) -> Option<&str> {
        self.props.get("filters").map(String::as_str)
    }

    /// Whether the page has no file.
    pub fn is_virtual(&self) -> bool {
        self.file.is_none()
    }
}

/// Problems found while loading a graph. None of them aborts the load.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Diagnostic {
    /// Two files map to the same page key; the first in `filter-files` order is kept.
    DuplicateTitle {
        /// Shared page key.
        key: PageKey,
        /// File that became the page.
        kept: GraphPath,
        /// File that was skipped.
        skipped: GraphPath,
    },
    /// A file could not be read.
    Unreadable {
        /// The file.
        path: GraphPath,
        /// Error text.
        error: String,
    },
}

/// The in-memory graph.
#[derive(Debug, Clone, Default)]
pub struct Graph {
    pages: BTreeMap<PageKey, Page>,
    alias_edges: BTreeMap<PageKey, BTreeSet<PageKey>>,
    diagnostics: Vec<Diagnostic>,
}

fn property_config(cfg: &EffectiveConfig) -> PropertyConfig {
    PropertyConfig {
        ignored_page_references_keywords: cfg
            .ignored_page_references_keywords()
            .into_iter()
            .collect(),
        separated_by_commas: cfg.property_separated_by_commas().into_iter().collect(),
    }
}

/// Page names listed by a property (`alias`/`aliases`/`tags`), blank names dropped.
fn listed_pages(
    props: &BTreeMap<String, String>,
    keys: &[&str],
    pc: &PropertyConfig,
) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for k in keys {
        if let Some(v) = props.get(*k)
            && let PropValue::Pages(set) = interpret(k, v, pc)
        {
            for n in set {
                let n = n.trim();
                if !n.is_empty()
                    && !out
                        .iter()
                        .any(|o| PageKey::from_title(o) == PageKey::from_title(n))
                {
                    out.push(n.to_owned());
                }
            }
        }
    }
    out
}

impl Graph {
    /// Scan `root`, read every page file (read-only) and build the graph.
    pub fn load(root: &Path, cfg: &EffectiveConfig) -> Result<Self, ScanError> {
        let files = parse_order(&scan_graph(root, cfg)?);
        let mut inputs: Vec<(ScannedFile, Result<String, String>)> = Vec::new();
        for f in files {
            let text = std::fs::read(&f.abs)
                .map(|b| decode_text(&b).text)
                .map_err(|e| e.to_string());
            inputs.push((f, text));
        }
        let mut g = Self::default();
        g.build(
            inputs
                .iter()
                .map(|(f, t)| (&f.path, t.as_deref().map_err(String::clone))),
            cfg,
        );
        Ok(g)
    }

    /// Build from already-read files, given in `filter-files` order (first wins on duplicates).
    pub fn from_texts<'a, I>(files: I, cfg: &EffectiveConfig) -> Self
    where
        I: IntoIterator<Item = (&'a GraphPath, &'a str)>,
    {
        let mut g = Self::default();
        g.build(files.into_iter().map(|(p, t)| (p, Ok(t))), cfg);
        g
    }

    fn build<'a, I>(&mut self, files: I, cfg: &EffectiveConfig)
    where
        I: Iterator<Item = (&'a GraphPath, Result<&'a str, String>)>,
    {
        let pc = property_config(cfg);
        for (path, text) in files {
            let Some(format) = path.extension().and_then(PageFormat::from_extension) else {
                continue;
            };
            let text = match text {
                Ok(t) => t,
                Err(error) => {
                    self.diagnostics.push(Diagnostic::Unreadable {
                        path: path.clone(),
                        error,
                    });
                    continue;
                }
            };
            let props = page_properties(text.as_bytes(), ParserOptions::default());
            let title = props.title().map(str::trim).filter(|t| !t.is_empty());
            let title = derive_title(path.as_str(), title, cfg);
            let journal = detect_journal(&title, cfg);
            let (original_name, key) = match &journal {
                Some(j) => (j.title.clone(), PageKey(j.key.clone())),
                None => (title.clone(), PageKey::from_title(&title)),
            };
            if let Some(kept) = self.pages.get(&key) {
                if let Some(kept_path) = &kept.file {
                    self.diagnostics.push(Diagnostic::DuplicateTitle {
                        key,
                        kept: kept_path.clone(),
                        skipped: path.clone(),
                    });
                }
                continue;
            }
            let map: BTreeMap<String, String> = props
                .properties
                .iter()
                .filter(|p| p.valid)
                .map(|p| (p.key_norm.clone(), p.value_raw.clone()))
                .collect();
            let aliases: Vec<String> = listed_pages(&map, &["alias", "aliases"], &pc)
                .into_iter()
                .filter(|a| PageKey::from_title(a) != key)
                .collect();
            let tags = listed_pages(&map, &["tags"], &pc);
            self.pages.insert(
                key.clone(),
                Page {
                    key,
                    original_name,
                    file: Some(path.clone()),
                    format,
                    journal,
                    props: map,
                    aliases,
                    tags,
                    namespace_parent: None,
                    read_only: format == PageFormat::Org,
                    origin: PageOrigin::File,
                    alias_of: None,
                },
            );
        }
        self.add_virtual_pages();
    }

    fn ensure_virtual(
        &mut self,
        name: &str,
        origin: PageOrigin,
        alias_of: Option<&PageKey>,
    ) -> PageKey {
        let key = PageKey::from_title(name);
        self.pages.entry(key.clone()).or_insert_with(|| Page {
            key: key.clone(),
            original_name: name.to_owned(),
            file: None,
            format: PageFormat::Markdown,
            journal: None,
            props: BTreeMap::new(),
            aliases: Vec::new(),
            tags: Vec::new(),
            namespace_parent: None,
            read_only: false,
            origin,
            alias_of: alias_of.cloned(),
        });
        key
    }

    fn add_virtual_pages(&mut self) {
        let declared: Vec<(PageKey, Vec<String>, Vec<String>)> = self
            .pages
            .values()
            .map(|p| (p.key.clone(), p.aliases.clone(), p.tags.clone()))
            .collect();
        for (owner, aliases, tags) in &declared {
            let keys: Vec<PageKey> = aliases
                .iter()
                .map(|a| self.ensure_virtual(a, PageOrigin::Alias, Some(owner)))
                .collect();
            // Aliases link both ways with the owner and with each other.
            for (i, a) in keys.iter().enumerate() {
                self.link_alias(owner, a);
                for b in &keys[i + 1..] {
                    self.link_alias(a, b);
                }
            }
            for t in tags {
                self.ensure_virtual(t, PageOrigin::Tag, None);
            }
        }
        // Namespace parents, for every page including the virtual ones created above.
        let names: Vec<(PageKey, String)> = self
            .pages
            .values()
            .map(|p| (p.key.clone(), p.original_name.clone()))
            .collect();
        for (key, name) in names {
            let chain = namespace_parents(&name);
            if chain.len() < 2 {
                continue;
            }
            let mut parent: Option<PageKey> = None;
            for (i, seg) in chain.iter().enumerate() {
                let last = i + 1 == chain.len();
                let k = if last {
                    key.clone()
                } else {
                    self.ensure_virtual(seg, PageOrigin::Namespace, None)
                };
                if let Some(p) = self.pages.get_mut(&k)
                    && p.namespace_parent.is_none()
                    && parent.as_ref() != Some(&k)
                {
                    p.namespace_parent = parent.clone();
                }
                parent = Some(k);
            }
        }
    }

    fn link_alias(&mut self, a: &PageKey, b: &PageKey) {
        if a == b {
            return;
        }
        self.alias_edges
            .entry(a.clone())
            .or_default()
            .insert(b.clone());
        self.alias_edges
            .entry(b.clone())
            .or_default()
            .insert(a.clone());
    }

    /// Page by key.
    pub fn page(&self, key: &PageKey) -> Option<&Page> {
        self.pages.get(key)
    }

    /// Resolve a page name as written in `[[...]]`: case-insensitive and NFC-normalised; an alias
    /// that has no file of its own resolves to the page that declared it.
    pub fn resolve(&self, name: &str) -> Option<&Page> {
        let p = self.pages.get(&PageKey::from_title(name))?;
        match (&p.origin, &p.alias_of) {
            (PageOrigin::Alias, Some(owner)) => self.pages.get(owner).or(Some(p)),
            _ => Some(p),
        }
    }

    /// All pages ordered by key.
    pub fn pages(&self) -> impl Iterator<Item = &Page> {
        self.pages.values()
    }

    /// Number of pages (file-backed and virtual).
    pub fn len(&self) -> usize {
        self.pages.len()
    }

    /// Whether the graph has no pages.
    pub fn is_empty(&self) -> bool {
        self.pages.is_empty()
    }

    /// Everything reachable through alias links from `key`, including `key` (empty if unknown).
    pub fn alias_group(&self, key: &PageKey) -> BTreeSet<PageKey> {
        let mut seen = BTreeSet::new();
        if !self.pages.contains_key(key) {
            return seen;
        }
        let mut stack = vec![key.clone()];
        while let Some(k) = stack.pop() {
            if seen.insert(k.clone())
                && let Some(next) = self.alias_edges.get(&k)
            {
                stack.extend(next.iter().cloned());
            }
        }
        seen
    }

    /// Direct namespace children of `key`.
    pub fn namespace_children(&self, key: &PageKey) -> Vec<&Page> {
        self.pages
            .values()
            .filter(|p| p.namespace_parent.as_ref() == Some(key))
            .collect()
    }

    /// Load diagnostics (duplicate titles, unreadable files).
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(src: &str) -> EffectiveConfig {
        EffectiveConfig::from_texts(None, Some(src))
    }

    fn graph(files: &[(&str, &str)], c: &EffectiveConfig) -> Graph {
        let paths: Vec<GraphPath> = files
            .iter()
            .map(|(p, _)| GraphPath::new(p).expect("path"))
            .collect();
        Graph::from_texts(paths.iter().zip(files).map(|(p, (_, t))| (p, *t)), c)
    }

    fn key(s: &str) -> PageKey {
        PageKey::from_title(s)
    }

    #[test]
    fn page_key_and_block_id() {
        assert_eq!(key("/Foo/").as_str(), "foo");
        assert_eq!(key("Cafe\u{301}"), key("CAF\u{e9}"));
        assert!(BlockId::parse("65F3A1B2-0000-4000-8000-000000000001").is_some());
        assert_eq!(BlockId::parse("nope"), None);
    }

    #[test]
    fn aliases_resolve_without_files() {
        let c = cfg("{}");
        let g = graph(
            &[(
                "pages/My Page.md",
                "alias:: Mine, [[My page alias]], my page, \n\n- x\n",
            )],
            &c,
        );
        assert!(g.resolve("nothing").is_none());
        assert_eq!(g.resolve("mine").expect("alias").original_name, "My Page");
        assert_eq!(
            g.resolve("MY PAGE ALIAS").expect("alias").original_name,
            "My Page"
        );
        let p = g.page(&key("my page")).expect("page");
        assert_eq!(
            p.aliases.len(),
            2,
            "self alias and blank dropped: {:?}",
            p.aliases
        );
        assert!(g.page(&key("mine")).expect("virtual").is_virtual());
        assert_eq!(g.pages().filter(|p| p.file.is_some()).count(), 1);
        // Alias group is symmetric and includes siblings.
        let grp = g.alias_group(&key("mine"));
        assert_eq!(grp.len(), 3);
        assert!(grp.contains(&key("my page")) && grp.contains(&key("my page alias")));
    }

    #[test]
    fn aliases_key_and_front_matter() {
        let c = cfg("{}");
        let g = graph(
            &[(
                "pages/A.md",
                "---\ntitle: Real Title\naliases: x1\n---\n\n- b\n",
            )],
            &c,
        );
        assert!(g.page(&key("real title")).is_some());
        assert!(g.page(&key("a")).is_none());
        assert_eq!(g.resolve("x1").expect("alias").original_name, "Real Title");
    }

    #[test]
    fn title_property_wins_over_file_name_and_keys_lowercase() {
        let c = cfg("{}");
        let g = graph(
            &[(
                "pages/file.md",
                "Title:: Shown\nPublic:: true\nicon:: x\ntags:: a, [[B]]\n\n- y\n",
            )],
            &c,
        );
        let p = g.page(&key("shown")).expect("page");
        assert_eq!(p.public(), Some(true));
        assert_eq!(p.icon(), Some("x"));
        let mut tags = p.tags.clone();
        tags.sort();
        assert_eq!(tags, vec!["B".to_owned(), "a".to_owned()]);
        assert_eq!(g.page(&key("b")).expect("tag").origin, PageOrigin::Tag);
        assert!(g.page(&key("a")).expect("tag").is_virtual());
    }

    #[test]
    fn namespaces() {
        let c = cfg("{:file/name-format :triple-lowbar}");
        let g = graph(
            &[
                ("pages/a___b___c.md", "- x\n"),
                ("pages/a.md", "- real a\n"),
                ("pages/r.md", "title:: ./rel/page\n"),
                ("pages/u.md", "title:: https://x.org/y\n"),
            ],
            &c,
        );
        let abc = g.page(&key("a/b/c")).expect("c");
        assert_eq!(abc.namespace_parent, Some(key("a/b")));
        let ab = g.page(&key("a/b")).expect("virtual a/b");
        assert!(ab.is_virtual());
        assert_eq!(ab.origin, PageOrigin::Namespace);
        assert_eq!(ab.namespace_parent, Some(key("a")));
        let a = g.page(&key("a")).expect("a");
        assert!(!a.is_virtual(), "file-backed page is not replaced");
        assert_eq!(a.namespace_parent, None);
        assert_eq!(g.namespace_children(&key("a")).len(), 1);
        for t in ["./rel/page", "https://x.org/y"] {
            assert_eq!(g.page(&key(t)).expect("page").namespace_parent, None, "{t}");
        }
        assert!(g.page(&key("rel")).is_none());
        assert!(g.page(&key("https:")).is_none());
    }

    #[test]
    fn journals_are_detected_by_title() {
        let c = cfg("{}");
        let g = graph(
            &[
                ("journals/2025_11_14.md", "- a\n"),
                ("pages/2024_01_01.md", "- b\n"),
            ],
            &c,
        );
        let j = g.resolve("Nov 14th, 2025").expect("journal");
        assert_eq!(j.journal.as_ref().map(|j| j.journal_day), Some(20251114));
        assert_eq!(j.original_name, "Nov 14th, 2025");
        assert!(g.resolve("jan 1st, 2024").expect("p").journal.is_some());
    }

    #[test]
    fn duplicate_titles_keep_first_and_report() {
        let c = cfg("{}");
        let g = graph(
            &[("pages/Foo.md", "- 1\n"), ("pages/sub/foo.md", "- 2\n")],
            &c,
        );
        let p = g.resolve("FOO").expect("page");
        assert_eq!(p.file.as_ref().map(GraphPath::as_str), Some("pages/Foo.md"));
        assert_eq!(
            g.diagnostics(),
            [Diagnostic::DuplicateTitle {
                key: key("foo"),
                kept: GraphPath::new("pages/Foo.md").expect("p"),
                skipped: GraphPath::new("pages/sub/foo.md").expect("p"),
            }]
        );
        assert_eq!(g.len(), 1);
    }

    #[test]
    fn duplicate_via_title_property_and_nfd() {
        let c = cfg("{}");
        let g = graph(
            &[
                ("pages/a.md", "title:: Cafe\u{301}\n"),
                ("pages/Caf\u{e9}.md", "- x\n"),
            ],
            &c,
        );
        assert_eq!(g.diagnostics().len(), 1);
    }

    #[test]
    fn org_pages_are_read_only_and_adoc_ignored() {
        let c = cfg("{}");
        let g = graph(
            &[
                ("pages/Meeting.org", "#+TITLE: Weekly Meeting\n* x\n"),
                ("pages/spec.adoc", "= Spec\n"),
                ("pages/plain.org", "* y\n"),
            ],
            &c,
        );
        let p = g.resolve("weekly meeting").expect("org page");
        assert!(p.read_only);
        assert_eq!(p.format, PageFormat::Org);
        assert_eq!(
            p.file.as_ref().map(GraphPath::as_str),
            Some("pages/Meeting.org")
        );
        assert!(g.resolve("spec").is_none());
        assert!(g.resolve("plain").expect("p").read_only);
        assert!(g.page(&key("meeting")).is_none());
    }

    #[test]
    fn md_and_org_with_same_title_conflict() {
        let c = cfg("{}");
        let g = graph(&[("pages/Foo.md", "- 1\n"), ("pages/Foo.org", "* 2\n")], &c);
        assert_eq!(g.diagnostics().len(), 1);
        assert!(!g.resolve("foo").expect("p").read_only);
    }

    #[test]
    fn unreadable_files_do_not_abort() {
        let c = cfg("{}");
        let mut g = Graph::default();
        let p = GraphPath::new("pages/x.md").expect("p");
        g.build([(&p, Err("boom".to_owned()))].into_iter(), &c);
        assert!(matches!(g.diagnostics(), [Diagnostic::Unreadable { .. }]));
    }
}
