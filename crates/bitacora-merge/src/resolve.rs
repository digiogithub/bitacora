//! Applying a user's resolution of one conflict to the page text, and writing `id::` for blocks
//! that a merge result newly references (BIT-SP-0006.R14, R16).
//!
//! Both operations edit the page through the lossless document, so every block that is not
//! touched keeps its original bytes. Neither ever writes conflict markers.

use bitacora_markdown::{
    Document, Node, WriteOptions,
    edit::properties::{get_property, set_property},
    serialize,
};

use crate::block::MergedBlock;
use crate::conflict::{ConflictKind, PageConflict};
use crate::model::{BlockKey, MergeBlock, MergePage, PropEntry};
use crate::page::render;

/// What the user chose for one conflict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Choice {
    /// Keep our value (the work tree already holds it).
    Ours,
    /// Take their value.
    Theirs,
    /// Keep ours and add theirs as the next sibling (content conflicts), regenerating `id::`
    /// only when the original block had one. For other kinds this equals [`Choice::Ours`].
    Both,
    /// Replace the conflicting value by this text (no markers).
    Edit(String),
}

fn merged_of(b: &MergeBlock) -> MergedBlock {
    MergedBlock {
        content: b.content.clone(),
        planning: b.planning.clone(),
        props: b.props.clone(),
        meta: b.meta.clone(),
        prop_order: b.prop_order.clone(),
        conflicts: Vec::new(),
        id_rewrites: Vec::new(),
    }
}

fn locate(page: &MergePage, c: &PageConflict) -> Option<usize> {
    if let Some(id) = &c.block_key {
        return page
            .blocks
            .iter()
            .position(|b| matches!(&b.key, BlockKey::Id(k) if k == id));
    }
    let first = c.breadcrumb.last().map(String::as_str)?;
    let wanted = [
        c.conflict.ours.as_deref(),
        c.conflict.theirs.as_deref(),
        c.conflict.base.as_deref(),
    ];
    let candidates: Vec<usize> = page
        .blocks
        .iter()
        .filter(|b| b.first_line() == first)
        .map(|b| b.index)
        .collect();
    candidates
        .iter()
        .copied()
        .find(|&i| wanted.contains(&Some(page.blocks[i].content.as_str())))
        .or_else(|| (candidates.len() == 1).then(|| candidates[0]))
}

fn strip_bom(s: &str) -> (bool, &str) {
    match s.strip_prefix('\u{feff}') {
        Some(rest) => (true, rest),
        None => (false, s),
    }
}

fn finish(doc: &Document, bom: bool) -> Option<String> {
    let mut out = String::from_utf8(serialize(doc, &WriteOptions::default())).ok()?;
    if bom {
        out.insert(0, '\u{feff}');
    }
    Some(out)
}

fn subtree_end(page: &MergePage, i: usize) -> usize {
    let depth = page.blocks[i].depth;
    let mut end = i + 1;
    while end < page.blocks.len() && page.blocks[end].depth > depth {
        end += 1;
    }
    end
}

/// Applies `choice` to the conflict `c` in `text` (the page as it is in the work tree). Returns
/// the new text, or `None` when the block cannot be found any more.
#[must_use]
pub fn resolve_conflict(text: &str, c: &PageConflict, choice: &Choice) -> Option<String> {
    let (bom, body) = strip_bom(text);
    if c.conflict.field == "page" {
        return Some(match choice {
            Choice::Ours => text.to_owned(),
            Choice::Theirs => c.conflict.theirs.clone().unwrap_or_default(),
            Choice::Edit(s) => s.clone(),
            Choice::Both => union_lines(text, c.conflict.theirs.as_deref().unwrap_or("")),
        });
    }
    let page = MergePage::parse(text);
    let mut doc = Document::parse(body.to_owned());
    if doc.blocks.len() != page.blocks.len() {
        return None;
    }
    let i = locate(&page, c)?;
    let blk = &page.blocks[i];
    let depth = blk.depth;
    let field = c.conflict.field.as_str();

    if c.conflict.kind == ConflictKind::DeleteVsModify {
        if let Choice::Edit(s) = choice {
            let mut m = merged_of(blk);
            m.content.clone_from(s);
            let ours_text = doc.block_content(i)?.into_owned();
            doc.blocks[i] = Node::Edited {
                depth,
                content: render(&m, Some((blk, &ours_text))),
            };
            return finish(&doc, bom);
        }
        let remove = (matches!(choice, Choice::Ours) && c.conflict.ours.is_none())
            || (matches!(choice, Choice::Theirs) && c.conflict.theirs.is_none());
        if !remove {
            return Some(text.to_owned());
        }
        // Remove the block only; its children move up one level.
        let end = subtree_end(&page, i);
        let mut nodes: Vec<Node> = Vec::new();
        for j in i + 1..end {
            nodes.push(Node::Edited {
                depth: page.blocks[j].depth - 1,
                content: doc.block_content(j)?.into_owned(),
            });
        }
        doc.blocks.splice(i..end, nodes);
        return finish(&doc, bom);
    }

    let value: Option<String> = match choice {
        Choice::Ours => return Some(text.to_owned()),
        // Keeping both only makes sense for text content.
        Choice::Both if field != "content" => return Some(text.to_owned()),
        Choice::Both | Choice::Theirs => c.conflict.theirs.clone(),
        Choice::Edit(s) => Some(s.clone()),
    };
    if matches!(choice, Choice::Both) {
        // Ours stays, theirs becomes the next sibling (after the subtree).
        let mut copy = merged_of(blk);
        copy.content = value.unwrap_or_default();
        copy.meta.id = blk
            .meta
            .id
            .as_ref()
            .map(|_| uuid::Uuid::new_v4().to_string());
        copy.meta.logbook = Vec::new();
        let end = subtree_end(&page, i);
        doc.blocks.insert(
            end,
            Node::Edited {
                depth,
                content: render(&copy, None),
            },
        );
        return finish(&doc, bom);
    }
    let mut m = merged_of(blk);
    apply_field(&mut m, field, value);
    let ours_text = doc.block_content(i)?.into_owned();
    doc.blocks[i] = Node::Edited {
        depth,
        content: render(&m, Some((blk, &ours_text))),
    };
    finish(&doc, bom)
}

fn apply_field(m: &mut MergedBlock, field: &str, value: Option<String>) {
    match field {
        "content" => m.content = value.unwrap_or_default(),
        "SCHEDULED" | "DEADLINE" => {
            m.planning.retain(|(k, _)| k != field);
            if let Some(v) = value {
                m.planning.push((field.to_owned(), v));
            }
        }
        key => {
            let pos = m.props.iter().position(|p| p.norm == key);
            match (pos, value) {
                (Some(p), Some(v)) => m.props[p].value = v,
                (Some(p), None) => {
                    m.props.remove(p);
                    m.prop_order.retain(|k| k != key);
                }
                (None, Some(v)) => {
                    m.props.push(PropEntry {
                        key: key.to_owned(),
                        norm: key.to_owned(),
                        value: v,
                    });
                    m.prop_order.push(key.to_owned());
                }
                (None, None) => {}
            }
        }
    }
}

/// Ours followed by the lines of `theirs` that ours does not have.
fn union_lines(ours: &str, theirs: &str) -> String {
    let have: std::collections::HashSet<&str> = ours.lines().collect();
    let mut out = ours.to_owned();
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    for l in theirs.lines().filter(|l| !have.contains(l)) {
        out.push_str(l);
        out.push('\n');
    }
    out
}

/// Adds `[[alias]]` to the page's `alias::` property (creating the page properties when there are
/// none). Returns `None` when the page already has the alias or cannot be edited.
#[must_use]
pub fn add_page_alias(text: &str, alias: &str) -> Option<String> {
    let (bom, body) = strip_bom(text);
    let mut doc = Document::parse(body.to_owned());
    let current = doc
        .pre_block_text()
        .map(std::borrow::Cow::into_owned)
        .unwrap_or_default();
    let existing = get_property(&current, "alias").unwrap_or_default();
    let have = |v: &str| {
        v.split(',').any(|i| {
            i.trim()
                .trim_start_matches("[[")
                .trim_end_matches("]]")
                .eq_ignore_ascii_case(alias)
        })
    };
    if have(&existing) {
        return None;
    }
    let value = if existing.trim().is_empty() {
        format!("[[{alias}]]")
    } else {
        format!("{existing}, [[{alias}]]")
    };
    let new = if current.trim().is_empty() {
        format!("alias:: {value}")
    } else {
        set_property(&current, "alias", &value)
    };
    doc.set_pre_block_content(&new);
    finish(&doc, bom)
}

/// Writes `id:: <uuid>` on blocks that a merge result references but whose text lacks the id.
///
/// `wants` pairs a uuid with the first line of the block that should carry it (the index knows
/// which block a uuid belongs to). A uuid already present on some block is skipped, as is an
/// ambiguous first line: it never guesses. Returns the new text and the uuids that were written,
/// or `None` when nothing changed.
#[must_use]
pub fn ensure_block_ids(text: &str, wants: &[(String, String)]) -> Option<(String, Vec<String>)> {
    let (bom, body) = strip_bom(text);
    let page = MergePage::parse(text);
    let mut doc = Document::parse(body.to_owned());
    if doc.blocks.len() != page.blocks.len() {
        return None;
    }
    let mut written = Vec::new();
    for (uuid, first_line) in wants {
        let uuid = uuid.to_lowercase();
        if page
            .blocks
            .iter()
            .any(|b| b.meta.id.as_deref() == Some(uuid.as_str()))
        {
            continue;
        }
        let mut hits = page
            .blocks
            .iter()
            .filter(|b| b.meta.id.is_none() && b.first_line() == first_line);
        let (Some(b), None) = (hits.next(), hits.next()) else {
            continue;
        };
        let current = doc.block_content(b.index)?.into_owned();
        doc.blocks[b.index] = Node::Edited {
            depth: b.depth,
            content: set_property(&current, "id", &uuid),
        };
        written.push(uuid);
    }
    if written.is_empty() {
        return None;
    }
    Some((finish(&doc, bom)?, written))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conflict::Conflict;

    const ID: &str = "66500000-0000-4000-8000-0000000000aa";

    fn pc(
        key: Option<&str>,
        crumb: &[&str],
        kind: ConflictKind,
        field: &str,
        sides: [Option<&str>; 3],
    ) -> PageConflict {
        let own = |s: Option<&str>| s.map(str::to_owned);
        PageConflict {
            block_key: own(key),
            breadcrumb: crumb.iter().map(|s| (*s).to_owned()).collect(),
            conflict: Conflict {
                kind,
                field: field.to_owned(),
                base: own(sides[0]),
                ours: own(sides[1]),
                theirs: own(sides[2]),
            },
        }
    }

    fn content(o: &str, t: &str) -> PageConflict {
        pc(
            None,
            &["Beta ours"],
            ConflictKind::Content,
            "content",
            [Some("Beta"), Some(o), Some(t)],
        )
    }

    #[test]
    fn content_theirs_edit_and_ours() {
        let text = "- Alpha\n- Beta ours\n- Gamma\n";
        let c = content("Beta ours", "Beta theirs");
        assert_eq!(
            resolve_conflict(text, &c, &Choice::Ours).as_deref(),
            Some(text)
        );
        assert_eq!(
            resolve_conflict(text, &c, &Choice::Theirs).as_deref(),
            Some("- Alpha\n- Beta theirs\n- Gamma\n")
        );
        assert_eq!(
            resolve_conflict(text, &c, &Choice::Edit("Beta both".into())).as_deref(),
            Some("- Alpha\n- Beta both\n- Gamma\n")
        );
    }

    #[test]
    fn keep_both_inserts_sibling_and_regenerates_id_only_when_present() {
        let c = content("Beta ours", "Beta theirs");
        let out = resolve_conflict(
            "- Alpha\n- Beta ours\n  - kid\n- Gamma\n",
            &c,
            &Choice::Both,
        )
        .unwrap_or_default();
        assert_eq!(
            out,
            "- Alpha\n- Beta ours\n  - kid\n- Beta theirs\n- Gamma\n"
        );
        let with_id = format!("- Alpha\n- Beta ours\n  id:: {ID}\n- Gamma\n");
        let c = pc(
            Some(ID),
            &["Beta ours"],
            ConflictKind::Content,
            "content",
            [Some("Beta"), Some("Beta ours"), Some("Beta theirs")],
        );
        let out = resolve_conflict(&with_id, &c, &Choice::Both).unwrap_or_default();
        assert!(out.contains(&format!("id:: {ID}")), "{out}");
        assert_eq!(out.matches("id::").count(), 2, "{out}");
        assert!(out.contains("- Beta theirs\n  id:: "), "{out}");
        assert!(!out.contains("<<<<<<<"));
    }

    #[test]
    fn property_resolution() {
        let text = "- Task\n  due:: 2026-10-07\n- Other\n";
        let c = pc(
            None,
            &["Task"],
            ConflictKind::Property,
            "due",
            [Some("2026-10-06"), Some("2026-10-07"), Some("2026-10-08")],
        );
        let out = resolve_conflict(text, &c, &Choice::Theirs).unwrap_or_default();
        assert_eq!(out, "- Task\n  due:: 2026-10-08\n- Other\n");
        let c = pc(
            None,
            &["Task"],
            ConflictKind::Property,
            "due",
            [Some("x"), Some("2026-10-07"), None],
        );
        let out = resolve_conflict(text, &c, &Choice::Theirs).unwrap_or_default();
        assert_eq!(out, "- Task\n- Other\n");
    }

    #[test]
    fn block_delete_vs_modify() {
        // We deleted, they modified: the modified block is in the work tree.
        let text = "- A\n- Draft v2\n  - child\n- C\n";
        let c = pc(
            None,
            &["Draft v2"],
            ConflictKind::DeleteVsModify,
            "block",
            [Some("Draft"), None, Some("Draft v2")],
        );
        assert_eq!(
            resolve_conflict(text, &c, &Choice::Theirs).as_deref(),
            Some(text)
        );
        assert_eq!(
            resolve_conflict(text, &c, &Choice::Ours).as_deref(),
            Some("- A\n- child\n- C\n")
        );
        // They deleted, we modified.
        let c = pc(
            None,
            &["Draft v2"],
            ConflictKind::DeleteVsModify,
            "block",
            [Some("Draft"), Some("Draft v2"), None],
        );
        assert_eq!(
            resolve_conflict(text, &c, &Choice::Ours).as_deref(),
            Some(text)
        );
        assert_eq!(
            resolve_conflict(text, &c, &Choice::Theirs).as_deref(),
            Some("- A\n- child\n- C\n")
        );
    }

    #[test]
    fn whole_page_conflict() {
        let c = pc(
            None,
            &["(whole page)"],
            ConflictKind::Content,
            "page",
            [Some("b"), Some("o"), Some("t")],
        );
        assert_eq!(
            resolve_conflict("o", &c, &Choice::Theirs).as_deref(),
            Some("t")
        );
        assert_eq!(
            resolve_conflict("o\n", &c, &Choice::Both).as_deref(),
            Some("o\nt\n")
        );
    }

    #[test]
    fn missing_block_is_none_and_bom_is_kept() {
        let c = content("Nope", "x");
        assert!(resolve_conflict("- Alpha\n", &c, &Choice::Theirs).is_none());
        let c = content("Beta ours", "Beta theirs");
        let out =
            resolve_conflict("\u{feff}- Beta ours\n", &c, &Choice::Theirs).unwrap_or_default();
        assert_eq!(out, "\u{feff}- Beta theirs\n");
    }

    #[test]
    fn page_alias_is_added_once() {
        let out = add_page_alias("- body\n", "Other").unwrap_or_default();
        assert_eq!(out, "alias:: [[Other]]\n\n- body\n");
        let out2 = add_page_alias("alias:: [[A]]\n\n- body\n", "B").unwrap_or_default();
        assert!(out2.starts_with("alias:: [[A]], [[B]]\n"), "{out2}");
        assert!(add_page_alias(&out, "other").is_none());
    }

    #[test]
    fn ensure_ids_writes_only_missing_unambiguous_ids() {
        let text = "- Alpha\n- Beta\n  note:: x\n- Beta\n";
        let (out, done) =
            ensure_block_ids(text, &[(ID.to_owned(), "Alpha".to_owned())]).unwrap_or_default();
        assert_eq!(
            out,
            format!("- Alpha\n  id:: {ID}\n- Beta\n  note:: x\n- Beta\n")
        );
        assert_eq!(done, vec![ID.to_owned()]);
        // Ambiguous first line, and an id already present: nothing to do.
        assert!(ensure_block_ids(text, &[(ID.to_owned(), "Beta".to_owned())]).is_none());
        assert!(ensure_block_ids(&out, &[(ID.to_owned(), "Alpha".to_owned())]).is_none());
    }
}
