//! The merge page model: a page parsed into an arena of blocks with raw spans retained.
//!
//! Built from the lossless `bitacora-markdown` outline (ADR-003, ADR-016). Content, user
//! properties and metadata are split by the class table of `bitacora-markdown` (BIT-US-0094), never
//! re-derived here. Comparison uses normalised views only; the raw spans point into
//! [`MergePage::source`] so untouched blocks can be written back byte for byte.

use std::collections::HashSet;
use std::hash::{Hash, Hasher};

use bitacora_markdown::{
    PropClass, Span, build_tree, content_of, is_card_key, parse_block_text, pre_block_content,
    split, union_logbook,
};

/// Line ending style of a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Eol {
    /// `\n`.
    Lf,
    /// `\r\n`.
    CrLf,
}

/// Indentation unit of a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Indent {
    /// One tab per level.
    Tabs,
    /// `n` spaces per level.
    Spaces(usize),
}

/// File-level style: output uses ours' style, equality ignores it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileStyle {
    /// Line ending.
    pub eol: Eol,
    /// Whether the file starts with a UTF-8 BOM (stripped from [`MergePage::source`]).
    pub bom: bool,
    /// Indentation unit.
    pub indent: Indent,
}

/// A property line as written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropEntry {
    /// Key as written.
    pub key: String,
    /// Normalised key.
    pub norm: String,
    /// Trimmed value.
    pub value: String,
}

/// Metadata-class state of a block (ADR-009).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Meta {
    /// `collapsed::` value.
    pub collapsed: Option<String>,
    /// `id::` value (lower-case uuid text).
    pub id: Option<String>,
    /// Trimmed `:LOGBOOK:` lines, sorted by start time, without duplicates.
    pub logbook: Vec<String>,
    /// The `card-*` group as `(normalised key, value)` in written order.
    pub card: Vec<(String, String)>,
    /// Every other metadata-class key as `(normalised key, value)` in written order.
    pub lww: Vec<(String, String)>,
}

impl Meta {
    /// Equality ignoring the order of properties.
    #[must_use]
    pub fn same_as(&self, other: &Self) -> bool {
        fn sorted(v: &[(String, String)]) -> Vec<&(String, String)> {
            let mut v: Vec<_> = v.iter().collect();
            v.sort();
            v
        }
        self.collapsed == other.collapsed
            && self.id == other.id
            && self.logbook == other.logbook
            && sorted(&self.card) == sorted(&other.card)
            && sorted(&self.lww) == sorted(&other.lww)
    }
}

/// Identity of a block for matching.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum BlockKey {
    /// An explicit `id::` (lower-case).
    Id(String),
    /// No usable `id::`: the block's index in its page.
    Synthetic(usize),
}

/// A block of a [`MergePage`]. Blocks live in a preorder arena: `index` is the document order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeBlock {
    /// Position in [`MergePage::blocks`] (document order).
    pub index: usize,
    /// Parent block, `None` for top-level blocks.
    pub parent: Option<usize>,
    /// Children in order.
    pub children: Vec<usize>,
    /// 1-based tree depth.
    pub depth: usize,
    /// `id::` or synthetic key.
    pub key: BlockKey,
    /// First line plus continuation lines, normalised, without properties, `:LOGBOOK:` and
    /// `SCHEDULED:` / `DEADLINE:` lines. The first line still carries the task marker.
    pub content: String,
    /// `SCHEDULED` / `DEADLINE` lines as `(KEYWORD, rest of the line)`.
    pub planning: Vec<(String, String)>,
    /// Content-class properties in written order.
    pub props: Vec<PropEntry>,
    /// Metadata-class state.
    pub meta: Meta,
    /// Normalised keys of every property line in written order (all classes).
    pub prop_order: Vec<String>,
    /// The original bytes of the block itself (children are separate blocks with their own spans).
    pub raw: Span,
    /// Set when this block repeated an earlier block's `id::` (corruption): the id it carried.
    pub duplicate_of: Option<String>,
    /// Fresh uuid assigned to a duplicate-id block for the merge output.
    pub fresh_id: Option<String>,
}

impl MergeBlock {
    /// Stable hash of everything that defines a block's own text for matching: content,
    /// content properties (as a set) and planning lines. Metadata is excluded, so an `id::`
    /// addition or a collapse does not change identity.
    #[must_use]
    pub fn normalized_hash(&self) -> u64 {
        let mut props: Vec<(&str, &str)> = self
            .props
            .iter()
            .map(|p| (p.norm.as_str(), p.value.as_str()))
            .collect();
        props.sort_unstable();
        let mut planning: Vec<&(String, String)> = self.planning.iter().collect();
        planning.sort();
        let mut h = std::hash::DefaultHasher::new();
        self.content.hash(&mut h);
        props.hash(&mut h);
        planning.hash(&mut h);
        h.finish()
    }

    /// First line of the content.
    #[must_use]
    pub fn first_line(&self) -> &str {
        self.content.lines().next().unwrap_or("")
    }

    /// True when the two blocks are equal after normalisation (content, properties as a set,
    /// planning, metadata). Position is not compared.
    #[must_use]
    pub fn same_as(&self, other: &Self) -> bool {
        self.normalized_hash() == other.normalized_hash()
            && self.content == other.content
            && self.meta.same_as(&other.meta)
    }
}

/// The page-level property block (pre-block).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreBlock {
    /// The original bytes.
    pub raw: Span,
    /// Valid property lines (all classes) in written order.
    pub props: Vec<PropEntry>,
    /// Remaining non-property text.
    pub text: String,
}

/// A parsed page: pre-block plus an arena of blocks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergePage {
    /// The page bytes (BOM stripped); raw spans index into it.
    pub source: String,
    /// The pre-block, if any.
    pub pre_block: Option<PreBlock>,
    /// All blocks in document (preorder) order.
    pub blocks: Vec<MergeBlock>,
    /// Indices of the top-level blocks.
    pub roots: Vec<usize>,
    /// File style.
    pub style: FileStyle,
}

const BOM: &str = "\u{feff}";

fn detect_indent(source: &str) -> Indent {
    for line in source.lines() {
        if line.starts_with('\t') {
            return Indent::Tabs;
        }
        let n = line.bytes().take_while(|&b| b == b' ').count();
        if n > 0 && line[n..].starts_with("- ") {
            return Indent::Spaces(n);
        }
    }
    Indent::Spaces(2)
}

impl MergePage {
    /// Parses `text` into a merge page. Never fails: the outline parser is tolerant.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let (bom, source) = match text.strip_prefix(BOM) {
            Some(rest) => (true, rest),
            None => (false, text),
        };
        let style = FileStyle {
            eol: if source.contains("\r\n") {
                Eol::CrLf
            } else {
                Eol::Lf
            },
            bom,
            indent: detect_indent(source),
        };
        let bytes = source.as_bytes();
        let outline = split(bytes);
        let links = build_tree(&outline.blocks);

        let pre_block = outline.pre_block.map(|span| {
            let content = pre_block_content(bytes, span);
            let parts = parse_block_text(&content);
            PreBlock {
                raw: span,
                props: parts
                    .props
                    .into_iter()
                    .map(|p| PropEntry {
                        key: p.key_raw,
                        norm: p.key,
                        value: p.value,
                    })
                    .collect(),
                text: parts.content.join("\n"),
            }
        });

        let mut blocks: Vec<MergeBlock> = Vec::with_capacity(outline.blocks.len());
        let mut roots = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        for (i, rb) in outline.blocks.iter().enumerate() {
            let parts = parse_block_text(&content_of(bytes, rb));
            let mut content_lines = Vec::new();
            let mut planning = Vec::new();
            for (n, line) in parts.content.iter().enumerate() {
                let t = line.trim_start();
                let kw = ["SCHEDULED:", "DEADLINE:"]
                    .into_iter()
                    .find(|k| n > 0 && t.starts_with(k));
                match kw {
                    Some(k) => planning.push((
                        k.trim_end_matches(':').to_owned(),
                        t[k.len()..].trim().to_owned(),
                    )),
                    None => content_lines.push(line.as_str()),
                }
            }
            let mut props = Vec::new();
            let mut meta = Meta::default();
            let mut prop_order = Vec::new();
            for p in &parts.props {
                prop_order.push(p.key.clone());
                match p.class {
                    PropClass::Content => props.push(PropEntry {
                        key: p.key_raw.clone(),
                        norm: p.key.clone(),
                        value: p.value.clone(),
                    }),
                    PropClass::Identity => {
                        if meta.id.is_none() {
                            meta.id = Some(p.value.to_lowercase());
                        }
                    }
                    PropClass::Metadata => {
                        let kv = (p.key.clone(), p.value.clone());
                        if p.key == "collapsed" {
                            meta.collapsed = Some(p.value.clone());
                        } else if is_card_key(&p.key) {
                            meta.card.push(kv);
                        } else {
                            meta.lww.push(kv);
                        }
                    }
                }
            }
            meta.logbook = union_logbook(&parts.logbook, &[]);

            let (key, duplicate_of, fresh_id) = match meta.id.clone() {
                Some(id) if !seen.insert(id.clone()) => {
                    meta.id = None;
                    (
                        BlockKey::Synthetic(i),
                        Some(id),
                        Some(uuid::Uuid::new_v4().to_string()),
                    )
                }
                Some(id) => (BlockKey::Id(id), None, None),
                None => (BlockKey::Synthetic(i), None, None),
            };

            let parent = links[i].parent;
            match parent {
                Some(p) => blocks[p].children.push(i),
                None => roots.push(i),
            }
            blocks.push(MergeBlock {
                index: i,
                parent,
                children: Vec::new(),
                depth: links[i].depth,
                key,
                content: content_lines.join("\n"),
                planning,
                props,
                meta,
                prop_order,
                raw: rb.span,
                duplicate_of,
                fresh_id,
            });
        }
        Self {
            source: source.to_owned(),
            pre_block,
            blocks,
            roots,
            style,
        }
    }

    /// The children of `parent` (`None` = top level).
    #[must_use]
    pub fn children_of(&self, parent: Option<usize>) -> &[usize] {
        match parent {
            Some(p) => &self.blocks[p].children,
            None => &self.roots,
        }
    }

    /// The original bytes of block `i`.
    #[must_use]
    pub fn raw_text(&self, i: usize) -> &str {
        let r = self.blocks[i].raw;
        &self.source[r.start..r.end]
    }

    /// Titles of the ancestors of block `i`, outermost first (first content line), for conflict
    /// breadcrumbs.
    #[must_use]
    pub fn breadcrumb(&self, i: usize) -> Vec<String> {
        let mut out = Vec::new();
        let mut cur = self.blocks[i].parent;
        while let Some(p) = cur {
            out.push(self.blocks[p].first_line().to_owned());
            cur = self.blocks[p].parent;
        }
        out.reverse();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_tree_with_raw_spans_that_tile_the_input() {
        let src = "title:: x\n\n- a\n  collapsed:: true\n  - b\n  - c\n- d\n";
        let p = MergePage::parse(src);
        assert_eq!(p.roots, [0, 3]);
        assert_eq!(p.blocks[0].children, [1, 2]);
        assert_eq!(p.blocks[1].parent, Some(0));
        assert_eq!(p.blocks[0].meta.collapsed.as_deref(), Some("true"));
        assert_eq!(p.blocks[0].content, "a");
        let mut joined = String::new();
        if let Some(pre) = &p.pre_block {
            joined.push_str(&src[pre.raw.start..pre.raw.end]);
            assert_eq!(pre.props[0].norm, "title");
        }
        for i in 0..p.blocks.len() {
            joined.push_str(p.raw_text(i));
        }
        assert_eq!(joined, src);
        assert_eq!(p.breadcrumb(2), ["a"]);
    }

    #[test]
    fn splits_content_props_meta_planning() {
        let src = "- TODO write\n  SCHEDULED: <2026-10-06 Tue .+1d>\n  owner:: [[Ana]]\n  card-repeats:: 2\n  query-table:: true\n  id:: 66AA0000-0000-4000-8000-000000000001\n  :LOGBOOK:\n  CLOCK: [2026-10-05 Mon 09:00:00]\n  :END:\n  more text\n";
        let p = MergePage::parse(src);
        let b = &p.blocks[0];
        assert_eq!(b.content, "TODO write\nmore text");
        assert_eq!(
            b.planning,
            [("SCHEDULED".to_owned(), "<2026-10-06 Tue .+1d>".to_owned())]
        );
        assert_eq!(b.props.len(), 1);
        assert_eq!(b.props[0].value, "[[Ana]]");
        assert_eq!(b.meta.card, [("card-repeats".to_owned(), "2".to_owned())]);
        assert_eq!(b.meta.lww, [("query-table".to_owned(), "true".to_owned())]);
        assert_eq!(
            b.key,
            BlockKey::Id("66aa0000-0000-4000-8000-000000000001".to_owned())
        );
        assert_eq!(b.meta.logbook.len(), 1);
        assert_eq!(b.prop_order, ["owner", "card-repeats", "query-table", "id"]);
    }

    #[test]
    fn normalization_ignores_whitespace_crlf_indent_and_property_order() {
        let a = MergePage::parse("- a  \n  x:: 1\n  y:: 2\n  - child\n");
        let b = MergePage::parse("- a\r\n\ty:: 2\r\n\tx:: 1\r\n\t- child\r\n");
        assert_eq!(a.blocks[0].normalized_hash(), b.blocks[0].normalized_hash());
        assert!(a.blocks[0].same_as(&b.blocks[0]));
        assert_eq!(b.style.eol, Eol::CrLf);
        assert_eq!(b.style.indent, Indent::Tabs);
        assert_eq!(a.style.indent, Indent::Spaces(2));
        assert_eq!(a.blocks[1].content, "child");
        assert_eq!(b.blocks[1].content, "child");
    }

    #[test]
    fn bom_is_recorded_and_stripped() {
        let p = MergePage::parse("\u{feff}- a\n");
        assert!(p.style.bom);
        assert_eq!(p.blocks[0].content, "a");
    }

    #[test]
    fn duplicate_id_gets_fresh_uuid_on_second_occurrence() {
        let id = "66aa0000-0000-4000-8000-000000000001";
        let p = MergePage::parse(&format!("- a\n  id:: {id}\n- b\n  id:: {id}\n"));
        assert_eq!(p.blocks[0].key, BlockKey::Id(id.to_owned()));
        assert!(p.blocks[0].fresh_id.is_none());
        assert_eq!(p.blocks[1].key, BlockKey::Synthetic(1));
        assert_eq!(p.blocks[1].duplicate_of.as_deref(), Some(id));
        let fresh = p.blocks[1].fresh_id.as_deref().expect("fresh id");
        assert_ne!(fresh, id);
        assert!(uuid::Uuid::parse_str(fresh).is_ok());
    }

    #[test]
    fn id_addition_does_not_change_identity_hash() {
        let a = MergePage::parse("- Idea A\n");
        let b = MergePage::parse("- Idea A\n  id:: 66bb0000-0000-4000-8000-000000000001\n");
        assert_eq!(a.blocks[0].normalized_hash(), b.blocks[0].normalized_hash());
        assert!(!a.blocks[0].same_as(&b.blocks[0]));
    }
}
