//! Block UUID assignment (`docs/design/sqlite-index-schema.md` §2.2, §2.3, ADR-006).
//!
//! Precedence: explicit `id::` > carried over from the previous state of the file by a 2-way
//! diff > fresh UUIDv7. No UUID is used twice. The baseline is the previous `blocks` rows of the
//! file (there is no snapshot table, ADR-017).

use std::collections::HashSet;

use similar::{Algorithm, DiffOp, TextDiff, capture_diff_slices};

use crate::parsed::ParsedBlock;

/// Minimum normalised edit similarity to pair a replaced block with its predecessor.
const MIN_SIMILARITY: f32 = 0.5;
/// Characters compared when measuring similarity (keeps the diff cheap on huge blocks).
const SIMILARITY_WINDOW: usize = 2000;

/// `blocks.uuid_source`: a freshly generated UUIDv7.
pub const SOURCE_GENERATED: u8 = 0;
/// `blocks.uuid_source`: the block's `id::` property.
pub const SOURCE_EXPLICIT: u8 = 1;
/// `blocks.uuid_source`: carried over from the previous index state.
pub const SOURCE_CARRIED: u8 = 2;

/// A block of the previous index state of a file.
#[derive(Debug, Clone)]
pub struct OldBlock {
    /// Depth (1 = top level).
    pub depth: u32,
    /// `blake3(content)[..16]`.
    pub content_hash: [u8; 16],
    /// Raw content, used to measure similarity of replaced hunks.
    pub content: String,
    /// The UUID the block had.
    pub uuid: String,
}

/// The UUID decided for one new block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assigned {
    /// The UUID (lower-case, hyphenated).
    pub uuid: String,
    /// One of the `SOURCE_*` constants.
    pub source: u8,
    /// An explicit `id::` was refused because another file's block owns it.
    pub conflicts_with_other_file: bool,
}

/// Result of [`assign_uuids`].
#[derive(Debug, Clone)]
pub struct Assignment {
    /// One entry per new block.
    pub assigned: Vec<Assigned>,
    /// For each new block, the index in the old sequence of the block the diff found
    /// **identical** (same depth and content hash), if any. The writer keeps such rows in place.
    pub equal_old: Vec<Option<usize>>,
}

/// Assign a UUID to every new block.
///
/// `owned_elsewhere(uuid)` reports whether a block of **another** file already has the UUID
/// (called only for explicit ids).
pub fn assign_uuids(
    old: &[OldBlock],
    new: &[ParsedBlock],
    mut owned_elsewhere: impl FnMut(&str) -> bool,
) -> Assignment {
    let mut equal_old: Vec<Option<usize>> = vec![None; new.len()];
    let mut out: Vec<Option<Assigned>> = vec![None; new.len()];
    let mut used: HashSet<String> = HashSet::new();

    // 1. Explicit ids win (first occurrence in the file; cross-file clashes are refused).
    for (i, b) in new.iter().enumerate() {
        let Some(id) = &b.explicit_uuid else { continue };
        if used.contains(id) {
            continue;
        }
        if owned_elsewhere(id) {
            out[i] = Some(Assigned {
                uuid: String::new(),
                source: SOURCE_GENERATED,
                conflicts_with_other_file: true,
            });
            continue;
        }
        used.insert(id.clone());
        out[i] = Some(Assigned {
            uuid: id.clone(),
            source: SOURCE_EXPLICIT,
            conflicts_with_other_file: false,
        });
    }

    // 2. Carry over by diff over (depth, content hash).
    if !old.is_empty() && !new.is_empty() {
        let old_keys: Vec<(u32, [u8; 16])> =
            old.iter().map(|b| (b.depth, b.content_hash)).collect();
        let new_keys: Vec<(u32, [u8; 16])> =
            new.iter().map(|b| (b.depth, b.content_hash)).collect();
        let mut pairs: Vec<(usize, usize)> = Vec::new();
        for op in capture_diff_slices(Algorithm::Myers, &old_keys, &new_keys) {
            match op {
                DiffOp::Equal {
                    old_index,
                    new_index,
                    len,
                } => {
                    for k in 0..len {
                        equal_old[new_index + k] = Some(old_index + k);
                        pairs.push((old_index + k, new_index + k));
                    }
                }
                DiffOp::Replace {
                    old_index,
                    old_len,
                    new_index,
                    new_len,
                } => {
                    for k in 0..old_len.min(new_len) {
                        let (o, n) = (old_index + k, new_index + k);
                        if old[o].depth == new[n].depth
                            && similarity(&old[o].content, &new[n].content) >= MIN_SIMILARITY
                        {
                            pairs.push((o, n));
                        }
                    }
                }
                DiffOp::Delete { .. } | DiffOp::Insert { .. } => {}
            }
        }
        for (o, n) in pairs {
            if out[n].is_some() || used.contains(&old[o].uuid) {
                continue;
            }
            used.insert(old[o].uuid.clone());
            out[n] = Some(Assigned {
                uuid: old[o].uuid.clone(),
                source: SOURCE_CARRIED,
                conflicts_with_other_file: false,
            });
        }
    }

    // 3. Fresh UUIDv7 for the rest (including refused explicit ids).
    let assigned = out
        .into_iter()
        .map(|a| match a {
            Some(a) if !a.uuid.is_empty() => a,
            other => {
                let conflicts = other.is_some_and(|a| a.conflicts_with_other_file);
                Assigned {
                    uuid: fresh(&mut used),
                    source: SOURCE_GENERATED,
                    conflicts_with_other_file: conflicts,
                }
            }
        })
        .collect();
    Assignment {
        assigned,
        equal_old,
    }
}

fn fresh(used: &mut HashSet<String>) -> String {
    loop {
        let id = uuid::Uuid::now_v7().hyphenated().to_string();
        if used.insert(id.clone()) {
            return id;
        }
    }
}

fn similarity(a: &str, b: &str) -> f32 {
    if a == b {
        return 1.0;
    }
    let cut = |s: &str| -> String { s.chars().take(SIMILARITY_WINDOW).collect() };
    TextDiff::from_chars(cut(a).as_str(), cut(b).as_str()).ratio()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn old(depth: u32, content: &str, uuid: &str) -> OldBlock {
        OldBlock {
            depth,
            content_hash: hash(content),
            content: content.to_owned(),
            uuid: uuid.to_owned(),
        }
    }

    fn hash(c: &str) -> [u8; 16] {
        let mut h = [0u8; 16];
        h.copy_from_slice(&blake3::hash(c.as_bytes()).as_bytes()[..16]);
        h
    }

    fn new_block(depth: u32, content: &str, explicit: Option<&str>) -> ParsedBlock {
        ParsedBlock {
            ord: 0,
            subtree_end: 0,
            depth,
            sibling_idx: 0,
            parent_ord: None,
            is_pre_block: false,
            content: content.to_owned(),
            title: String::new(),
            search_text: String::new(),
            marker: None,
            priority: None,
            scheduled: None,
            scheduled_raw: None,
            deadline: None,
            deadline_raw: None,
            repeated: false,
            collapsed: false,
            heading: None,
            explicit_uuid: explicit.map(str::to_owned),
            created_at: None,
            updated_at: None,
            byte_start: 0,
            byte_end: 0,
            line_start: 1,
            content_hash: hash(content),
            properties: vec![],
            page_refs: vec![],
            block_refs: vec![],
        }
    }

    #[test]
    fn equal_blocks_keep_uuid_and_edited_block_is_paired() {
        let o = [
            old(1, "alpha", "u-a"),
            old(1, "the quick brown fox", "u-b"),
            old(2, "gamma", "u-c"),
        ];
        let n = [
            new_block(1, "alpha", None),
            new_block(1, "the quick brown fox jumps", None),
            new_block(2, "gamma", None),
        ];
        let a = assign_uuids(&o, &n, |_| false).assigned;
        assert_eq!(a[0].uuid, "u-a");
        assert_eq!(a[1].uuid, "u-b");
        assert_eq!(a[2].uuid, "u-c");
        assert!(a.iter().all(|x| x.source == SOURCE_CARRIED));
    }

    #[test]
    fn dissimilar_replacement_gets_fresh_uuid() {
        let o = [old(1, "aaaaaaaaaa", "u-a")];
        let n = [new_block(1, "zzzzzzzzzzzzzzzz", None)];
        let a = assign_uuids(&o, &n, |_| false).assigned;
        assert_ne!(a[0].uuid, "u-a");
        assert_eq!(a[0].source, SOURCE_GENERATED);
    }

    #[test]
    fn explicit_wins_and_is_never_reused() {
        let o = [old(1, "same", "u-a")];
        let n = [
            new_block(1, "same", None),
            new_block(1, "other", Some("u-a")),
        ];
        let a = assign_uuids(&o, &n, |_| false).assigned;
        assert_eq!(a[1].uuid, "u-a");
        assert_eq!(a[1].source, SOURCE_EXPLICIT);
        assert_ne!(a[0].uuid, "u-a");
        let uniq: HashSet<_> = a.iter().map(|x| x.uuid.clone()).collect();
        assert_eq!(uniq.len(), 2);
    }

    #[test]
    fn explicit_owned_by_another_file_gets_fresh_uuid_and_flag() {
        let n = [new_block(1, "x", Some("taken"))];
        let a = assign_uuids(&[], &n, |u| u == "taken").assigned;
        assert_ne!(a[0].uuid, "taken");
        assert!(a[0].conflicts_with_other_file);
    }

    #[test]
    fn inserted_block_in_the_middle_keeps_neighbours() {
        let o = [old(1, "a", "u1"), old(1, "c", "u3")];
        let n = [
            new_block(1, "a", None),
            new_block(1, "b", None),
            new_block(1, "c", None),
        ];
        let a = assign_uuids(&o, &n, |_| false).assigned;
        assert_eq!(a[0].uuid, "u1");
        assert_eq!(a[2].uuid, "u3");
        assert_eq!(a[1].source, SOURCE_GENERATED);
    }
}
