//! Content vs metadata vs identity classification used by the merge engines (ADR-008, ADR-009).
//!
//! The class table is the one in `docs/analysis/logseq/02-markdown-block-syntax.md` §5.4, kept in a
//! single place ([`class_of`]):
//!
//! * **Identity** (`id`, with `custom-id` / `custom_id` normalised to `id`): never lost, never
//!   duplicated.
//! * **Metadata** (UI / app state: `collapsed`, `card-*`, `query-*`, `hl-*`, legacy marker
//!   timestamps, `:LOGBOOK:` drawers, ...): safe to resolve automatically.
//! * **Content** (everything else, including user keys listed in `:block-hidden-properties`):
//!   merged like text.
//!
//! Everything here is read-only and works on the semantic text of a block (see
//! [`crate::outline::content_of`]) or on a raw byte range; nothing is ever rewritten.

use std::collections::{BTreeMap, BTreeSet};

use crate::lines::{Line, Lines, ParserOptions, is_ws};
use crate::outline::{RawBlock, content_of};
use crate::properties::{GroupOrigin, normalize_key, scan_properties};
use crate::span::Span;

/// The merge class of a property, drawer or line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PropClass {
    /// User intent: merged like text.
    Content,
    /// UI / app state: last-writer-wins or union is acceptable.
    Metadata,
    /// Block identity (`id::`): must never be lost or duplicated.
    Identity,
}

/// Class of a single line (same three classes as properties).
pub type LineClass = PropClass;

/// Which side wins when both sides changed the same metadata key (the newer writer).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Side {
    /// The local side.
    Ours,
    /// The incoming side.
    Theirs,
}

/// Metadata keys of the §5.4 table, normalised (`_` is `-`, lower case). `last-modified-at` and
/// `updated-at` both appear because drawers rename one to the other.
const METADATA_KEYS: &[&str] = &[
    "collapsed",
    "card-last-interval",
    "card-repeats",
    "card-last-reviewed",
    "card-next-schedule",
    "card-ease-factor",
    "card-last-score",
    "query-table",
    "query-properties",
    "query-sort-by",
    "query-sort-desc",
    "filters",
    "ls-type",
    "hl-type",
    "hl-page",
    "hl-stamp",
    "hl-color",
    "created-at",
    "updated-at",
    "last-modified-at",
    "todo",
    "doing",
    "now",
    "later",
    "done",
    "logseq.query/nlp-date",
    "exclude-from-graph-view",
];

/// Metadata key prefixes (`logseq.tldraw.page`, `logseq.tldraw.shape`, ...).
const METADATA_PREFIXES: &[&str] = &["logseq.tldraw."];

/// The SRS review-state keys. They are merged as one group (never mixing fields across sides).
pub const CARD_KEYS: &[&str] = &[
    "card-last-interval",
    "card-repeats",
    "card-last-reviewed",
    "card-next-schedule",
    "card-ease-factor",
    "card-last-score",
];

/// Class of a property key. The key is normalised first, so `Collapsed`, `created_at` and
/// `custom_id` classify like their canonical forms.
#[must_use]
pub fn class_of(key: &str) -> PropClass {
    let norm = normalize_key(key);
    if norm == "id" {
        PropClass::Identity
    } else if METADATA_KEYS.contains(&norm.as_str())
        || METADATA_PREFIXES.iter().any(|p| norm.starts_with(p))
    {
        PropClass::Metadata
    } else {
        PropClass::Content
    }
}

/// True when `key` belongs to the SRS `card-*` group.
#[must_use]
pub fn is_card_key(key: &str) -> bool {
    CARD_KEYS.contains(&normalize_key(key).as_str())
}

/// What a line of a block is, before it is mapped to a class.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Role {
    /// Title, body, `SCHEDULED:` / `DEADLINE:` and every unrecognised line.
    Text,
    /// A valid property line of the effective group.
    Prop { key: String, value: String },
    /// `:PROPERTIES:` / `:END:` of a properties drawer.
    PropsWrapper,
    /// `:LOGBOOK:` / `:END:` of a logbook drawer.
    LogbookWrapper,
    /// A line inside a logbook drawer.
    Logbook,
}

struct LineInfo {
    span: Span,
    role: Role,
}

fn trimmed(content: &[u8]) -> &[u8] {
    let s = content.iter().take_while(|&&b| is_ws(b)).count();
    let e = content.len() - content[s..].iter().rev().take_while(|&&b| is_ws(b)).count();
    &content[s..e]
}

fn analyse(input: &[u8], range: Span) -> Vec<LineInfo> {
    let opts = ParserOptions::default();
    let slice = range.slice(input);
    let lines: Vec<Line<'_>> = Lines::with_options(slice, opts).collect();
    let scan = scan_properties(input, range, opts);

    // Property lines of the effective group, by absolute start offset.
    let mut props: Vec<(Span, String, String)> = Vec::new();
    let mut wrapper_lines: BTreeSet<usize> = BTreeSet::new();
    if let Some(group) = scan.effective() {
        for l in group.lines.iter().filter(|l| l.valid) {
            props.push((l.span, l.key_norm.clone(), l.value_raw.clone()));
        }
        if group.origin == GroupOrigin::Drawer {
            // The first and last line of the drawer are its wrappers.
            wrapper_lines.insert(group.span.start);
            if let Some(last) = lines
                .iter()
                .map(|l| range.start + l.start)
                .rfind(|&s| s < group.span.end)
            {
                wrapper_lines.insert(last);
            }
        }
    }

    let mut out: Vec<LineInfo> = Vec::with_capacity(lines.len());
    let mut i = 0;
    while i < lines.len() {
        let line = &lines[i];
        let abs = Span::new(range.start + line.start, range.start + line.end);
        if line.region.is_none() && trimmed(line.content).eq_ignore_ascii_case(b":LOGBOOK:") {
            let close = (i + 1..lines.len()).find(|&j| {
                lines[j].region.is_none()
                    && trimmed(lines[j].content).eq_ignore_ascii_case(b":END:")
            });
            if let Some(close) = close {
                for (j, l) in lines.iter().enumerate().take(close + 1).skip(i) {
                    out.push(LineInfo {
                        span: Span::new(range.start + l.start, range.start + l.end),
                        role: if j == i || j == close {
                            Role::LogbookWrapper
                        } else {
                            Role::Logbook
                        },
                    });
                }
                i = close + 1;
                continue;
            }
        }
        let role = if let Some((_, k, v)) = props
            .iter()
            .find(|(s, _, _)| s.start >= abs.start && s.start < abs.end)
        {
            Role::Prop {
                key: k.clone(),
                value: v.clone(),
            }
        } else if wrapper_lines.contains(&abs.start) {
            Role::PropsWrapper
        } else {
            Role::Text
        };
        out.push(LineInfo { span: abs, role });
        i += 1;
    }
    out
}

/// Classifies every line of the byte range `range` of `input` (a raw block span, or any text).
///
/// One entry per line, spans include the EOL, in order. Property lines of the effective group take
/// the class of their key, `:LOGBOOK:` drawer lines (wrappers included) are Metadata, and every
/// other line (title, body, `SCHEDULED:` / `DEADLINE:`, `:PROPERTIES:` wrappers, properties with
/// invalid keys or outside the effective group) is Content.
#[must_use]
pub fn classify_lines(input: &[u8], range: Span) -> Vec<(Span, LineClass)> {
    analyse(input, range)
        .into_iter()
        .map(|li| {
            let class = match &li.role {
                Role::Prop { key, .. } => class_of(key),
                Role::LogbookWrapper | Role::Logbook => PropClass::Metadata,
                Role::Text | Role::PropsWrapper => PropClass::Content,
            };
            (li.span, class)
        })
        .collect()
}

/// A block reduced to what a merge compares: whitespace-normalised content lines, content
/// properties as a set, metadata, identity and the logbook as a set of lines.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CanonicalView {
    /// Title and body lines (trailing whitespace removed, trailing blank lines dropped).
    pub content: Vec<String>,
    /// Content-class properties as `(normalised key, trimmed value)`, sorted (order is irrelevant).
    pub props: Vec<(String, String)>,
    /// Metadata-class properties by normalised key (last occurrence wins).
    pub metadata: BTreeMap<String, String>,
    /// Metadata keys in the order they were written (first occurrence).
    pub metadata_order: Vec<String>,
    /// The block `id::`, if any.
    pub identity: Option<String>,
    /// The trimmed lines of the `:LOGBOOK:` drawer(s), as a set.
    pub logbook: BTreeSet<String>,
}

impl CanonicalView {
    /// True when the content lines and content properties are equal.
    #[must_use]
    pub fn content_eq(&self, other: &Self) -> bool {
        self.content == other.content && self.props == other.props
    }

    /// True when metadata and logbook are equal (order and whitespace ignored).
    #[must_use]
    pub fn metadata_eq(&self, other: &Self) -> bool {
        self.metadata == other.metadata && self.logbook == other.logbook
    }
}

/// Builds the [`CanonicalView`] of a block's semantic text (the output of
/// [`crate::outline::content_of`], with `\n` line endings).
#[must_use]
pub fn canonical_view(text: &str) -> CanonicalView {
    let bytes = text.as_bytes();
    let mut view = CanonicalView::default();
    for li in analyse(bytes, Span::new(0, bytes.len())) {
        let raw = String::from_utf8_lossy(li.span.slice(bytes));
        let line = raw.trim_end_matches(['\n', '\r']).trim_end();
        match li.role {
            Role::Text => view.content.push(line.to_owned()),
            Role::PropsWrapper | Role::LogbookWrapper => {}
            Role::Logbook => {
                let t = line.trim();
                if !t.is_empty() {
                    view.logbook.insert(t.to_owned());
                }
            }
            Role::Prop { key, value } => {
                let value = value.trim().to_owned();
                match class_of(&key) {
                    PropClass::Content => view.props.push((key, value)),
                    PropClass::Metadata => {
                        if !view.metadata.contains_key(&key) {
                            view.metadata_order.push(key.clone());
                        }
                        view.metadata.insert(key, value);
                    }
                    PropClass::Identity => {
                        if view.identity.is_none() {
                            view.identity = Some(value);
                        }
                    }
                }
            }
        }
    }
    while view.content.last().is_some_and(String::is_empty) {
        view.content.pop();
    }
    view.props.sort();
    view
}

/// [`canonical_view`] of a raw block of `input`.
#[must_use]
pub fn canonical_view_of(input: &[u8], block: &RawBlock) -> CanonicalView {
    canonical_view(&content_of(input, block))
}

/// How three versions of a block differ, by class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiffClass {
    /// Equal after normalisation (whitespace, property order, logbook order).
    Identical,
    /// Content is equal on all three sides; only metadata / `id::` additions differ. Resolved
    /// automatically by [`merge_metadata`].
    MetadataOnly,
    /// Title, body or a content property differs between some pair of sides.
    Content,
    /// Content and metadata agree enough, but both sides carry different `id::` values that
    /// neither inherited from the base.
    IdentityConflict,
}

/// Classifies the difference between `base`, `ours` and `theirs`.
///
/// Whitespace-only edits and property reorders never count (they vanish in the
/// [`CanonicalView`]); `Content` wins over `IdentityConflict`, which wins over `MetadataOnly`.
#[must_use]
pub fn classify_diff(
    base: &CanonicalView,
    ours: &CanonicalView,
    theirs: &CanonicalView,
) -> DiffClass {
    if !(base.content_eq(ours) && base.content_eq(theirs)) {
        return DiffClass::Content;
    }
    if identity_conflict(base, ours, theirs).is_some() {
        return DiffClass::IdentityConflict;
    }
    if base == ours && base == theirs
        || (base.metadata_eq(ours)
            && base.metadata_eq(theirs)
            && base.identity == ours.identity
            && base.identity == theirs.identity)
    {
        DiffClass::Identical
    } else {
        DiffClass::MetadataOnly
    }
}

/// `Some((ours, theirs))` when both sides have a different `id::` and neither is the base's.
fn identity_conflict(
    base: &CanonicalView,
    ours: &CanonicalView,
    theirs: &CanonicalView,
) -> Option<(String, String)> {
    match (&ours.identity, &theirs.identity) {
        (Some(o), Some(t))
            if o != t && base.identity.as_ref() != Some(o) && base.identity.as_ref() != Some(t) =>
        {
            Some((o.clone(), t.clone()))
        }
        _ => None,
    }
}

/// Two different `id::` values on the same block. Ours is kept; the caller de-duplicates by uuid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityConflict {
    /// The id kept in the merge output (ours).
    pub kept: String,
    /// The other side's id.
    pub other: String,
}

/// The result of [`merge_metadata`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MetadataMerge {
    /// Merged metadata properties in output order: ours' order, then keys only theirs has.
    pub metadata: Vec<(String, String)>,
    /// The merged `id::`: never dropped when either side has one.
    pub identity: Option<String>,
    /// Set when both sides added different ids.
    pub identity_conflict: Option<IdentityConflict>,
    /// Merged logbook lines: union of both sides, sorted by start time, open clocks reduced.
    pub logbook: Vec<String>,
}

/// Deterministic metadata merge of one block (ADR-009, BIT-SP-0001.R16).
///
/// * Metadata keys: per-key three-way (one side changed: take it; deletions are changes); when both
///   sides changed a key differently, `prefer` (the newer writer) wins.
/// * `id::`: kept from whichever side has it; two different new ids keep ours and report an
///   [`IdentityConflict`].
/// * Logbook: union of the lines of both sides ([`union_logbook`]).
///
/// The function is deterministic and idempotent: `merge(x, x, x)` reproduces `x`.
#[must_use]
pub fn merge_metadata(
    base: &CanonicalView,
    ours: &CanonicalView,
    theirs: &CanonicalView,
    prefer: Side,
) -> MetadataMerge {
    let mut keys: Vec<&String> = Vec::new();
    for k in ours
        .metadata_order
        .iter()
        .chain(theirs.metadata_order.iter())
    {
        if !keys.contains(&k) {
            keys.push(k);
        }
    }
    let mut metadata = Vec::new();
    for k in keys {
        let (b, o, t) = (
            base.metadata.get(k),
            ours.metadata.get(k),
            theirs.metadata.get(k),
        );
        let v = three_way(b, o, t, prefer);
        if let Some(v) = v {
            metadata.push((k.clone(), v.clone()));
        }
    }

    let identity_conflict =
        identity_conflict(base, ours, theirs).map(|(o, t)| IdentityConflict { kept: o, other: t });
    let identity = match (&ours.identity, &theirs.identity) {
        (Some(o), Some(t)) if o == t => Some(o.clone()),
        (Some(o), Some(t)) => {
            // Different ids: one of them is the base's (take the changed one) or both are new.
            if base.identity.as_ref() == Some(o) {
                Some(t.clone())
            } else {
                Some(o.clone())
            }
        }
        (Some(o), None) => Some(o.clone()),
        (None, Some(t)) => Some(t.clone()),
        (None, None) => None,
    };

    let logbook = union_logbook(
        &ours.logbook.iter().cloned().collect::<Vec<_>>(),
        &theirs.logbook.iter().cloned().collect::<Vec<_>>(),
    );
    MetadataMerge {
        metadata,
        identity,
        identity_conflict,
        logbook,
    }
}

/// Per-key three-way over optional values; both changed differently: `prefer`.
fn three_way<'a, T: PartialEq>(
    base: Option<&'a T>,
    ours: Option<&'a T>,
    theirs: Option<&'a T>,
    prefer: Side,
) -> Option<&'a T> {
    if ours == theirs || theirs == base {
        ours
    } else if ours == base {
        theirs
    } else if prefer == Side::Ours {
        ours
    } else {
        theirs
    }
}

/// The text of the first `[...]` of a logbook line without its weekday: `2024-01-01 10:00:00`, so
/// lines sort chronologically as plain strings.
fn start_key(line: &str) -> String {
    let Some(open) = line.find('[') else {
        return String::new();
    };
    let inner = line[open + 1..].split(']').next().unwrap_or("");
    let mut parts = inner.split_whitespace();
    let date = parts.next().unwrap_or("");
    let time = parts.find(|p| p.contains(':')).unwrap_or("");
    format!("{date} {time}")
}

fn is_open_clock(line: &str) -> bool {
    line.starts_with("CLOCK:") && !line.contains("--")
}

/// Union of two logbook line sets, sorted by start time (ties by text). Open clocks (`CLOCK:
/// [start]` without an end) are dropped when a closed clock with the same start exists, and only
/// the latest remaining open clock is kept.
#[must_use]
pub fn union_logbook(a: &[String], b: &[String]) -> Vec<String> {
    let all: BTreeSet<&String> = a.iter().chain(b.iter()).collect();
    let closed_starts: BTreeSet<String> = all
        .iter()
        .filter(|l| l.starts_with("CLOCK:") && l.contains("--"))
        .map(|l| start_key(l))
        .collect();
    let latest_open = all
        .iter()
        .filter(|l| is_open_clock(l) && !closed_starts.contains(&start_key(l)))
        .map(|l| (start_key(l), (*l).clone()))
        .max();
    let mut out: Vec<(String, String)> = all
        .into_iter()
        .filter(|l| !is_open_clock(l))
        .map(|l| (start_key(l), l.clone()))
        .collect();
    if let Some(open) = latest_open {
        out.push(open);
    }
    out.sort();
    out.into_iter().map(|(_, l)| l).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(text: &str) -> CanonicalView {
        canonical_view(text)
    }

    // One test per row of §5.4.
    #[test]
    fn class_table_rows() {
        use PropClass::{Content, Identity, Metadata};
        let rows: &[(&str, PropClass)] = &[
            ("id", Identity),
            ("custom-id", Identity),
            ("custom_id", Identity),
            ("title", Content),
            ("alias", Content),
            ("aliases", Content),
            ("tags", Content),
            ("template", Content),
            ("template-including-parent", Content),
            ("public", Content),
            ("icon", Content),
            ("filters", Metadata),
            ("exclude-from-graph-view", Metadata),
            ("heading", Content),
            ("collapsed", Metadata),
            ("background-color", Content),
            ("background_color", Content),
            ("logseq.order-list-type", Content),
            ("logseq.color", Content),
            ("logseq.table.version", Content),
            ("logseq.table.stripes", Content),
            ("logseq.query/nlp-date", Metadata),
            ("logseq.tldraw.page", Metadata),
            ("logseq.tldraw.shape", Metadata),
            ("created-at", Metadata),
            ("updated-at", Metadata),
            ("created_at", Metadata),
            ("last-modified-at", Metadata),
            ("last_modified_at", Metadata),
            ("query-table", Metadata),
            ("query-properties", Metadata),
            ("query-sort-by", Metadata),
            ("query-sort-desc", Metadata),
            ("card-last-interval", Metadata),
            ("card-repeats", Metadata),
            ("card-last-reviewed", Metadata),
            ("card-next-schedule", Metadata),
            ("card-ease-factor", Metadata),
            ("card-last-score", Metadata),
            ("ls-type", Metadata),
            ("hl-type", Metadata),
            ("hl-page", Metadata),
            ("hl-stamp", Metadata),
            ("hl-color", Metadata),
            ("todo", Metadata),
            ("doing", Metadata),
            ("now", Metadata),
            ("later", Metadata),
            ("done", Metadata),
            ("foo", Content),
            ("owner", Content),
        ];
        for (k, c) in rows {
            assert_eq!(class_of(k), *c, "key {k}");
        }
        // Case and separators are normalised.
        assert_eq!(class_of("Collapsed"), Metadata);
        assert_eq!(class_of("Card_Repeats"), Metadata);
    }

    #[test]
    fn block_hidden_properties_stay_content() {
        // User-hidden keys are config, not metadata: the table never lists them.
        assert_eq!(class_of("secret"), PropClass::Content);
    }

    #[test]
    fn acceptance_lines() {
        let text = "TODO title\ncollapsed:: true\ncard-repeats:: 2\nid:: 6650e1f2-0000-4000-8000-000000000001\nowner:: [[Ana]]\n:LOGBOOK:\nCLOCK: [2024-01-01 Mon 10:00:00]\n:END:\nbody\n";
        let b = text.as_bytes();
        let got = classify_lines(b, Span::new(0, b.len()));
        let classes: Vec<PropClass> = got.iter().map(|(_, c)| *c).collect();
        use PropClass::{Content, Identity, Metadata};
        assert_eq!(
            classes,
            [
                Content, Metadata, Metadata, Identity, Content, Metadata, Metadata, Metadata,
                Content
            ]
        );
        // Spans tile the input.
        assert_eq!(got.iter().map(|(s, _)| s.len()).sum::<usize>(), b.len());
    }

    #[test]
    fn classify_raw_block_with_bullet_properties() {
        let input = b"- a\n  collapsed:: true\n  x:: 1\n- b\n";
        let out = crate::outline::split(input);
        let got = classify_lines(input, out.blocks[0].span);
        let classes: Vec<_> = got.iter().map(|(_, c)| *c).collect();
        assert_eq!(
            classes,
            [PropClass::Content, PropClass::Metadata, PropClass::Content]
        );
    }

    #[test]
    fn properties_drawer_is_classified_by_key() {
        let t = "a\n:PROPERTIES:\n:collapsed: true\n:foo: bar\n:END:\n";
        let view = v(t);
        assert_eq!(view.content, ["a"]);
        assert_eq!(view.props, [("foo".to_owned(), "bar".to_owned())]);
        assert_eq!(
            view.metadata.get("collapsed").map(String::as_str),
            Some("true")
        );
    }

    #[test]
    fn metadata_only_collapsed_and_extra_clock() {
        let base = v(
            "a\n:LOGBOOK:\nCLOCK: [2024-01-01 Mon 10:00:00]--[2024-01-01 Mon 11:00:00] =>  01:00:00\n:END:\n",
        );
        let ours = v(
            "a\ncollapsed:: true\n:LOGBOOK:\nCLOCK: [2024-01-01 Mon 10:00:00]--[2024-01-01 Mon 11:00:00] =>  01:00:00\n:END:\n",
        );
        let theirs = v(
            "a\n:LOGBOOK:\nCLOCK: [2024-01-01 Mon 10:00:00]--[2024-01-01 Mon 11:00:00] =>  01:00:00\nCLOCK: [2024-01-02 Tue 09:00:00]--[2024-01-02 Tue 09:30:00] =>  00:30:00\n:END:\n",
        );
        assert_eq!(
            classify_diff(&base, &ours, &theirs),
            DiffClass::MetadataOnly
        );
        let m = merge_metadata(&base, &ours, &theirs, Side::Ours);
        assert_eq!(m.metadata, [("collapsed".to_owned(), "true".to_owned())]);
        assert_eq!(m.logbook.len(), 2);
        assert!(m.logbook[0].contains("2024-01-01"));
        assert!(m.logbook[1].contains("2024-01-02"));
    }

    #[test]
    fn property_reorder_and_whitespace_are_metadata_only_or_identical() {
        let a = v("a  \nx:: 1\ny:: 2\ncollapsed:: true\nz:: 3\n");
        let b = v("a\ncollapsed:: true\nz:: 3\ny:: 2\nx:: 1\n");
        assert_eq!(classify_diff(&a, &a, &b), DiffClass::Identical);
        assert!(a.content_eq(&b));
    }

    #[test]
    fn title_change_is_content() {
        let base = v("a\n");
        let ours = v("b\n");
        assert_eq!(classify_diff(&base, &ours, &base), DiffClass::Content);
        let p = v("a\nowner:: x\n");
        assert_eq!(classify_diff(&base, &p, &base), DiffClass::Content);
    }

    #[test]
    fn identity_rules() {
        let base = v("a\n");
        let o = v("a\nid:: 11111111-1111-4111-8111-111111111111\n");
        let t = v("a\nid:: 22222222-2222-4222-8222-222222222222\n");
        assert_eq!(classify_diff(&base, &o, &t), DiffClass::IdentityConflict);
        let m = merge_metadata(&base, &o, &t, Side::Theirs);
        assert_eq!(
            m.identity.as_deref(),
            Some("11111111-1111-4111-8111-111111111111")
        );
        let c = m.identity_conflict.expect("conflict");
        assert_eq!(c.other, "22222222-2222-4222-8222-222222222222");
        // One side adds an id: metadata-only, id kept.
        assert_eq!(classify_diff(&base, &o, &base), DiffClass::MetadataOnly);
        let m = merge_metadata(&base, &base, &o, Side::Ours);
        assert_eq!(
            m.identity.as_deref(),
            Some("11111111-1111-4111-8111-111111111111")
        );
        assert!(m.identity_conflict.is_none());
    }

    #[test]
    fn lww_per_key_and_order() {
        let base = v("a\ncollapsed:: false\nquery-table:: true\n");
        let ours = v("a\ncollapsed:: true\nquery-table:: true\n");
        let theirs = v("a\nquery-table:: false\ncollapsed:: false\nhl-page:: 3\n");
        let m = merge_metadata(&base, &ours, &theirs, Side::Theirs);
        assert_eq!(
            m.metadata,
            [
                ("collapsed".to_owned(), "true".to_owned()),
                ("query-table".to_owned(), "false".to_owned()),
                ("hl-page".to_owned(), "3".to_owned()),
            ]
        );
        // Both changed differently: prefer decides.
        let ours = v("a\ncollapsed:: true\n");
        let theirs = v("a\ncollapsed:: maybe\n");
        let base = v("a\ncollapsed:: false\n");
        assert_eq!(
            merge_metadata(&base, &ours, &theirs, Side::Theirs).metadata[0].1,
            "maybe"
        );
        assert_eq!(
            merge_metadata(&base, &ours, &theirs, Side::Ours).metadata[0].1,
            "true"
        );
    }

    #[test]
    fn open_clock_is_closed_by_union() {
        let a = vec!["CLOCK: [2024-01-01 Mon 10:00:00]".to_owned()];
        let b = vec![
            "CLOCK: [2024-01-01 Mon 10:00:00]--[2024-01-01 Mon 11:00:00] =>  01:00:00".to_owned(),
        ];
        assert_eq!(union_logbook(&a, &b), b);
        let two = vec![
            "CLOCK: [2024-01-01 Mon 10:00:00]".to_owned(),
            "CLOCK: [2024-01-03 Wed 10:00:00]".to_owned(),
        ];
        assert_eq!(
            union_logbook(&two, &[]),
            ["CLOCK: [2024-01-03 Wed 10:00:00]"]
        );
    }

    #[test]
    fn unclosed_logbook_is_content() {
        let view = v("a\n:LOGBOOK:\nCLOCK: [2024-01-01 Mon 10:00:00]\n");
        assert!(view.logbook.is_empty());
        assert_eq!(view.content.len(), 3);
    }

    mod props {
        use super::*;
        use proptest::prelude::*;

        const LINES: &[&str] = &[
            "TODO title",
            "text",
            "collapsed:: true",
            "card-repeats:: 2",
            "owner:: [[Ana]]",
            "id:: 6650e1f2-0000-4000-8000-000000000001",
            ":LOGBOOK:",
            "CLOCK: [2024-01-01 Mon 10:00:00]",
            "CLOCK: [2024-01-01 Mon 10:00:00]--[2024-01-01 Mon 11:00:00] =>  01:00:00",
            ":END:",
        ];

        fn block() -> impl Strategy<Value = String> {
            proptest::collection::vec(proptest::sample::select(LINES), 1..8)
                .prop_map(|ls| ls.join("\n") + "\n")
        }

        proptest! {
            #[test]
            fn merge_is_idempotent_and_deterministic(x in block(), y in block(), z in block()) {
                let (cx, cy, cz) = (canonical_view(&x), canonical_view(&y), canonical_view(&z));
                let once = merge_metadata(&cx, &cx, &cx, Side::Ours);
                prop_assert_eq!(&once, &merge_metadata(&cx, &cx, &cx, Side::Theirs));
                let expected: Vec<(String, String)> = cx
                    .metadata_order
                    .iter()
                    .map(|k| (k.clone(), cx.metadata[k].clone()))
                    .collect();
                prop_assert_eq!(&once.metadata, &expected);
                prop_assert_eq!(&once.identity, &cx.identity);
                prop_assert!(once.identity_conflict.is_none());
                // Deterministic for arbitrary triples.
                prop_assert_eq!(
                    merge_metadata(&cx, &cy, &cz, Side::Ours),
                    merge_metadata(&cx, &cy, &cz, Side::Ours)
                );
                let _ = (&cy, &cz);
            }
        }
    }
}
