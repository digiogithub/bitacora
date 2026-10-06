//! `merge_page`: the public entry point of the crate (`docs/design/git-sync-merge.md` §4.4).
//!
//! Short-circuits, merges the page properties, matches blocks, decides structure
//! ([`crate::structure`]), merges fields of matched blocks ([`merge_block`]) and writes the result
//! with `bitacora-markdown`'s [`serialize`]: blocks that did not change on our side keep their
//! original bytes, blocks taken from theirs are written in canonical form in our style, and blocks
//! that merged field by field are edited surgically (`set_property` / `remove_property`) or, when
//! that is not possible, rebuilt. The output never contains conflict markers; conflicts are data.

use std::borrow::Cow;

use bitacora_markdown::{
    Document, Eol, IndentUnit, Node, WriteOptions,
    edit::properties::{remove_property, set_property},
    parse_block_text, serialize, write_block,
};

use crate::block::{MergedBlock, merge_block};
use crate::conflict::{Conflict, ConflictKind, IdRewrite, Note, NoteKind, PageConflict};
use crate::fields::{diff3, merge_content, merge_user_props};
use crate::matcher::match_blocks;
use crate::meta::MergeEnv;
use crate::model::{BlockKey, MergeBlock, MergePage, PreBlock, PropEntry};
use crate::structure::plan;

/// The result of merging a page.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MergeResult {
    /// The merged page text. Never contains conflict markers; conflicting fields hold ours.
    pub output: String,
    /// Unresolved conflicts, for the UI.
    pub conflicts: Vec<PageConflict>,
    /// Informational notes about automatic decisions.
    pub notes: Vec<Note>,
}

impl MergeResult {
    fn clean(output: &str) -> Self {
        Self {
            output: output.to_owned(),
            ..Self::default()
        }
    }
}

/// Merges three versions of a page. See the module documentation.
#[must_use]
pub fn merge_page(base: &str, ours: &str, theirs: &str, env: &MergeEnv<'_>) -> MergeResult {
    if ours == theirs {
        return MergeResult::clean(ours);
    }
    if base == ours {
        return MergeResult::clean(theirs);
    }
    if base == theirs {
        return MergeResult::clean(ours);
    }
    // add/add (empty or trivial base): a trivial side counts as unchanged, the other wins.
    if is_trivial_page(base, env.template) {
        if is_trivial_page(ours, env.template) {
            return MergeResult::clean(theirs);
        }
        if is_trivial_page(theirs, env.template) {
            return MergeResult::clean(ours);
        }
    }
    structural(base, ours, theirs, env).unwrap_or_else(|| merge_lines(base, ours, theirs))
}

/// True when a page holds no user content: empty, whitespace, a lone `-` or `*` bullet, or only
/// the default journal template (BIT-SP-0006.R18, Logseq precedent `watcher_handler.cljs:95-101`).
#[must_use]
pub fn is_trivial_page(text: &str, template: Option<&str>) -> bool {
    let t = text.strip_prefix('\u{feff}').unwrap_or(text).trim();
    if t.is_empty() || t == "-" || t == "*" {
        return true;
    }
    template.is_some_and(|tpl| {
        let tpl = tpl.trim();
        !tpl.is_empty() && tpl == t
    })
}

/// Line-level three-way merge of plain text (the fallback when the block merge cannot be trusted).
/// Conflicting regions hold ours; one page-level conflict is reported when there were any.
#[must_use]
pub fn merge_lines(base: &str, ours: &str, theirs: &str) -> MergeResult {
    fn split(s: &str) -> Vec<String> {
        s.split_inclusive('\n').map(str::to_owned).collect()
    }
    fn refs(v: &[String]) -> Vec<&str> {
        v.iter().map(String::as_str).collect()
    }
    let (b, o, t) = (split(base), split(ours), split(theirs));
    let m = diff3(&refs(&b), &refs(&o), &refs(&t));
    let mut r = MergeResult {
        output: m.lines.concat(),
        ..MergeResult::default()
    };
    if m.conflicts > 0 {
        r.conflicts.push(PageConflict {
            block_key: None,
            breadcrumb: vec!["(whole page)".to_owned()],
            conflict: Conflict {
                kind: ConflictKind::Content,
                field: "page".to_owned(),
                base: Some(base.to_owned()),
                ours: Some(ours.to_owned()),
                theirs: Some(theirs.to_owned()),
            },
        });
    }
    r
}

/// How the text of one output block is obtained.
enum Choice {
    Ours(usize),
    Theirs(usize),
    Text(String),
}

enum PreChoice {
    Ours,
    Theirs,
    Text(String),
}

fn sorted_props(p: &[PropEntry]) -> Vec<(&str, &str)> {
    let mut v: Vec<_> = p
        .iter()
        .map(|p| (p.norm.as_str(), p.value.as_str()))
        .collect();
    v.sort_unstable();
    v
}

fn fields_equal(m: &MergedBlock, b: &MergeBlock) -> bool {
    m.content == b.content
        && m.planning == b.planning
        && sorted_props(&m.props) == sorted_props(&b.props)
        && m.meta.same_as(&b.meta)
}

fn wrap(page: &MergePage, i: usize, conflict: Conflict) -> PageConflict {
    let b = &page.blocks[i];
    let mut breadcrumb = page.breadcrumb(i);
    breadcrumb.push(b.first_line().to_owned());
    PageConflict {
        block_key: match &b.key {
            BlockKey::Id(id) => Some(id.clone()),
            BlockKey::Synthetic(_) => None,
        },
        breadcrumb,
        conflict,
    }
}

/// True when `text` (a de-indented block) parses back to exactly the fields of `m`.
fn parses_to(text: &str, m: &MergedBlock) -> bool {
    let src = write_block(text, 1, IndentUnit::TwoSpaces, Eol::Lf);
    let page = MergePage::parse(&format!("{src}\n"));
    page.blocks.len() == 1 && fields_equal(m, &page.blocks[0])
}

/// Writes a merged block from scratch: title, planning, properties (merged order), logbook, then
/// the remaining content lines.
fn rebuild(m: &MergedBlock) -> String {
    let mut content_lines = m.content.split('\n');
    let mut lines: Vec<String> = vec![content_lines.next().unwrap_or("").to_owned()];
    for (kw, rest) in &m.planning {
        lines.push(format!("{kw}: {rest}"));
    }
    for key in &m.prop_order {
        let value = if let Some(p) = m.props.iter().find(|p| &p.norm == key) {
            Some((p.key.as_str(), p.value.as_str()))
        } else if key == "collapsed" {
            m.meta.collapsed.as_deref().map(|v| ("collapsed", v))
        } else if key == "id" {
            m.meta.id.as_deref().map(|v| ("id", v))
        } else {
            m.meta
                .card
                .iter()
                .chain(&m.meta.lww)
                .find(|(k, _)| k == key)
                .map(|(k, v)| (k.as_str(), v.as_str()))
        };
        if let Some((k, v)) = value {
            lines.push(if v.is_empty() {
                format!("{k}::")
            } else {
                format!("{k}:: {v}")
            });
        }
    }
    if !m.meta.logbook.is_empty() {
        lines.push(":LOGBOOK:".to_owned());
        lines.extend(m.meta.logbook.iter().cloned());
        lines.push(":END:".to_owned());
    }
    lines.extend(content_lines.map(str::to_owned));
    lines.join("\n")
}

/// Surgical attempt: ours' text with only property lines changed.
fn patch_props(m: &MergedBlock, ours: &MergeBlock, ours_text: &str) -> Option<String> {
    if m.content != ours.content
        || m.planning != ours.planning
        || m.meta.logbook != ours.meta.logbook
    {
        return None;
    }
    let mut text = ours_text.to_owned();
    // Desired (key as written, normalised key, value) for every property of the merge result.
    let mut want: Vec<(String, String, String)> = m
        .props
        .iter()
        .map(|p| (p.key.clone(), p.norm.clone(), p.value.clone()))
        .collect();
    let mut meta_kv: Vec<(String, String)> = Vec::new();
    meta_kv.extend(
        m.meta
            .collapsed
            .clone()
            .map(|v| ("collapsed".to_owned(), v)),
    );
    meta_kv.extend(m.meta.id.clone().map(|v| ("id".to_owned(), v)));
    meta_kv.extend(m.meta.card.iter().cloned());
    meta_kv.extend(m.meta.lww.iter().cloned());
    want.extend(meta_kv.into_iter().map(|(k, v)| (k.clone(), k, v)));
    // Existing keys of ours.
    let mut had: Vec<(String, String)> = ours
        .props
        .iter()
        .map(|p| (p.norm.clone(), p.value.clone()))
        .collect();
    had.extend(
        ours.meta
            .collapsed
            .clone()
            .map(|v| ("collapsed".to_owned(), v)),
    );
    had.extend(ours.meta.id.clone().map(|v| ("id".to_owned(), v)));
    had.extend(ours.meta.card.iter().cloned());
    had.extend(ours.meta.lww.iter().cloned());
    for (norm, _) in &had {
        if !want.iter().any(|(_, n, _)| n == norm) {
            text = remove_property(&text, norm);
        }
    }
    for (key, norm, value) in &want {
        if had.iter().any(|(n, v)| n == norm && v == value) {
            continue;
        }
        text = set_property(&text, key, value);
    }
    parses_to(&text, m).then_some(text)
}

pub(crate) fn render(m: &MergedBlock, ours: Option<(&MergeBlock, &str)>) -> String {
    ours.and_then(|(blk, text)| patch_props(m, blk, text))
        .unwrap_or_else(|| rebuild(m))
}

fn pre_props_text(props: &[PropEntry], text: &str) -> String {
    let mut lines: Vec<String> = props
        .iter()
        .map(|p| {
            if p.value.is_empty() {
                format!("{}::", p.key)
            } else {
                format!("{}:: {}", p.key, p.value)
            }
        })
        .collect();
    if !text.is_empty() {
        lines.push(text.to_owned());
    }
    lines.join("\n")
}

fn merge_pre(
    b: Option<&PreBlock>,
    o: Option<&PreBlock>,
    t: Option<&PreBlock>,
    ours_text: Option<Cow<'_, str>>,
    conflicts: &mut Vec<PageConflict>,
) -> Option<PreChoice> {
    let none: &[PropEntry] = &[];
    fn props<'a>(p: Option<&'a PreBlock>, none: &'a [PropEntry]) -> &'a [PropEntry] {
        p.map_or(none, |p| p.props.as_slice())
    }
    let text = |p: Option<&PreBlock>| p.map_or("", |p| p.text.as_str()).to_owned();
    let (mp, pc) = merge_user_props(props(b, none), props(o, none), props(t, none));
    let mt = merge_content(Some(&text(b)), &text(o), &text(t));
    for c in pc.into_iter().chain(mt.conflict) {
        conflicts.push(PageConflict {
            block_key: None,
            breadcrumb: vec!["(page properties)".to_owned()],
            conflict: c,
        });
    }
    let same = |p: Option<&PreBlock>| {
        sorted_props(&mp) == sorted_props(props(p, none)) && mt.value == text(p)
    };
    if same(o) {
        return Some(PreChoice::Ours);
    }
    if same(t) {
        return Some(PreChoice::Theirs);
    }
    // Surgical when only properties changed, else rebuilt.
    if let (Some(ot), true) = (&ours_text, mt.value == text(o)) {
        let mut s = ot.to_string();
        for p in props(o, none) {
            if !mp.iter().any(|m| m.norm == p.norm) {
                s = remove_property(&s, &p.norm);
            }
        }
        for p in &mp {
            if !props(o, none)
                .iter()
                .any(|x| x.norm == p.norm && x.value == p.value)
            {
                s = set_property(&s, &p.key, &p.value);
            }
        }
        let parsed = parse_block_text(&s);
        let got: Vec<(String, String)> = parsed
            .props
            .iter()
            .map(|p| (p.key.clone(), p.value.clone()))
            .collect();
        let mut want: Vec<(String, String)> = mp
            .iter()
            .map(|p| (p.norm.clone(), p.value.clone()))
            .collect();
        let mut got_sorted = got;
        got_sorted.sort();
        want.sort();
        if got_sorted == want {
            return Some(PreChoice::Text(s));
        }
    }
    Some(PreChoice::Text(pre_props_text(&mp, &mt.value)))
}

fn rewrite_refs(text: &str, rewrites: &[IdRewrite]) -> Option<String> {
    let mut out = text.to_owned();
    for r in rewrites {
        out = out.replace(&format!("(({}))", r.from), &format!("(({}))", r.to));
    }
    (out != text).then_some(out)
}

fn structural(base: &str, ours: &str, theirs: &str, env: &MergeEnv<'_>) -> Option<MergeResult> {
    let (bp, op, tp) = (
        MergePage::parse(base),
        MergePage::parse(ours),
        MergePage::parse(theirs),
    );
    // Documents are built on the BOM-less text (as the merge pages are); ours' BOM is re-added.
    let strip = |s: &'_ str| s.strip_prefix('\u{feff}').unwrap_or(s).to_owned();
    let (od, td) = (Document::parse(strip(ours)), Document::parse(strip(theirs)));
    if od.blocks.len() != op.blocks.len() || td.blocks.len() != tp.blocks.len() {
        return None;
    }
    let matching = match_blocks(&bp, &op, &tp);
    let pl = plan(&bp, &op, &tp, &matching);
    let mut result = MergeResult {
        notes: pl.notes.clone(),
        conflicts: pl.conflicts.clone(),
        ..MergeResult::default()
    };

    // Page properties.
    let pre = merge_pre(
        bp.pre_block.as_ref(),
        op.pre_block.as_ref(),
        tp.pre_block.as_ref(),
        od.pre_block_text(),
        &mut result.conflicts,
    );

    // Blocks.
    let mut chosen: Vec<(usize, Choice, String)> = Vec::with_capacity(pl.order.len());
    let mut rewrites: Vec<IdRewrite> = Vec::new();
    for pb in &pl.order {
        let t = matching.triples[pb.triple];
        let base_blk = t.base.map(|i| &bp.blocks[i]);
        let (choice, content) = match (t.ours, t.theirs) {
            (Some(o), Some(x)) => {
                let (ob, tb) = (&op.blocks[o], &tp.blocks[x]);
                let m = merge_block(base_blk, ob, tb, env);
                result
                    .conflicts
                    .extend(m.conflicts.iter().cloned().map(|c| wrap(&op, o, c)));
                rewrites.extend(m.id_rewrites.iter().cloned());
                if fields_equal(&m, ob) {
                    (Choice::Ours(o), ob.content.clone())
                } else if fields_equal(&m, tb) {
                    (Choice::Theirs(x), tb.content.clone())
                } else {
                    let ours_text = od.block_content(o)?;
                    let text = render(&m, Some((ob, &ours_text)));
                    (Choice::Text(text), m.content.clone())
                }
            }
            (Some(o), None) => (Choice::Ours(o), op.blocks[o].content.clone()),
            (None, Some(x)) => (Choice::Theirs(x), tp.blocks[x].content.clone()),
            (None, None) => return None,
        };
        chosen.push((pb.depth, choice, content));
    }
    rewrites.sort_by(|a, b| a.from.cmp(&b.from));
    rewrites.dedup();

    // Build the output document from ours.
    let mut doc = od.clone();
    let mut nodes: Vec<Node> = Vec::with_capacity(chosen.len());
    let mut expected: Vec<(usize, String)> = Vec::with_capacity(chosen.len());
    for (depth, choice, content) in chosen {
        let text: Cow<'_, str> = match &choice {
            Choice::Ours(i) => od.block_content(*i)?,
            Choice::Theirs(i) => td.block_content(*i)?,
            Choice::Text(s) => Cow::Borrowed(s.as_str()),
        };
        let rewritten = rewrite_refs(&text, &rewrites);
        let content = rewrite_refs(&content, &rewrites).unwrap_or(content);
        let keep_ours = match choice {
            Choice::Ours(i) if od.blocks[i].depth() == depth => Some(i),
            _ => None,
        };
        let node = match (rewritten, keep_ours) {
            (Some(t), _) => Node::Edited { depth, content: t },
            (None, Some(i)) => od.blocks[i].clone(),
            (None, None) => Node::Edited {
                depth,
                content: text.into_owned(),
            },
        };
        nodes.push(node);
        expected.push((depth, content));
    }
    if !rewrites.is_empty() {
        for r in &rewrites {
            result.notes.push(Note {
                kind: NoteKind::IdRewritten,
                breadcrumb: Vec::new(),
                message: format!("references (({})) were rewritten to (({}))", r.from, r.to),
            });
        }
    }
    doc.blocks = nodes;
    doc.pre_block = match pre? {
        PreChoice::Ours => od.pre_block.clone(),
        PreChoice::Theirs => td
            .pre_block_text()
            .map(|c| Node::Edited {
                depth: 0,
                content: c.into_owned(),
            })
            .filter(|n| matches!(n, Node::Edited { content, .. } if !content.trim().is_empty())),
        PreChoice::Text(s) if s.trim().is_empty() => None,
        PreChoice::Text(s) => Some(Node::Edited {
            depth: 0,
            content: s,
        }),
    };

    let bytes = serialize(&doc, &WriteOptions::default());
    result.output = String::from_utf8(bytes).ok()?;
    if ours.trim().is_empty() && !result.output.is_empty() && !result.output.ends_with('\n') {
        result.output.push('\n');
    }
    if op.style.bom {
        result.output.insert(0, '\u{feff}');
    }

    // The output must re-parse to the planned tree.
    let check = MergePage::parse(&result.output);
    if check.blocks.len() != expected.len()
        || check
            .blocks
            .iter()
            .zip(&expected)
            .any(|(b, (d, c))| b.depth != *d || &b.content != c)
    {
        return None;
    }
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(b: &str, o: &str, t: &str) -> MergeResult {
        merge_page(b, o, t, &MergeEnv::new())
    }

    #[test]
    fn short_circuits() {
        assert_eq!(m("- a\n", "- b\n", "- b\n").output, "- b\n");
        assert_eq!(m("- a\n", "- a\n", "- c\n").output, "- c\n");
        assert_eq!(m("- a\n", "- b\n", "- a\n").output, "- b\n");
    }

    #[test]
    fn trivial_pages() {
        assert!(is_trivial_page("", None) && is_trivial_page("-\n", None));
        assert!(is_trivial_page(" \n*\n", None) && is_trivial_page("\u{feff}-", None));
        assert!(!is_trivial_page("- x\n", None));
        assert!(is_trivial_page("- [ ] plan\n", Some("- [ ] plan")));
        assert!(!is_trivial_page("- [ ] plan\n", None));
    }

    #[test]
    fn add_add_template_side_is_unchanged() {
        let env = MergeEnv::new();
        let r = merge_page("", "-\n", "- buy milk\n", &env);
        assert_eq!(r.output, "- buy milk\n");
        let r = merge_page("", "- call Ana\n", "- \n", &env);
        assert_eq!(r.output, "- call Ana\n");
        let env = MergeEnv {
            template: Some("- [ ] plan\n- [ ] review"),
            ..MergeEnv::new()
        };
        let r = merge_page("", "- [ ] plan\n- [ ] review\n", "- note\n", &env);
        assert_eq!(r.output, "- note\n");
    }

    #[test]
    fn add_add_unions_with_ours_first_and_dedupes() {
        let env = MergeEnv::new();
        let r = merge_page("", "- call Ana\n", "- buy milk\n", &env);
        assert_eq!(r.output, "- call Ana\n- buy milk\n");
        assert!(r.conflicts.is_empty());
        let r = merge_page("", "- call Ana\n- same\n", "- same\n- buy milk\n", &env);
        assert_eq!(r.output.matches("- same").count(), 1, "{}", r.output);
    }

    #[test]
    fn non_overlapping_edits_keep_untouched_bytes() {
        let b = "- one  \n- two\n- three\n";
        let o = "- one  \n- two!\n- three\n";
        let t = "- one  \n- two\n- three?\n";
        let r = m(b, o, t);
        assert_eq!(r.output, "- one  \n- two!\n- three?\n");
        assert!(r.conflicts.is_empty());
    }
}
