//! Block identity: `id::` insertion, duplicate detection and self-reference stripping
//! (`docs/analysis/logseq/02-markdown-block-syntax.md` §4, ADR-006: an `id::` is written only when
//! the block is referenced).

use std::collections::HashMap;

use uuid::Uuid;

use crate::edit::properties::set_property;
use crate::lines::ParserOptions;
use crate::properties::scan_properties;
use crate::span::Span;

/// Why an id could not be added.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum IdError {
    /// Pre-blocks (page properties) never get an `id::`.
    #[error("a pre-block cannot get a block id")]
    PreBlock,
}

/// Parses a block id: hyphenated 8-4-4-4-12 hex (what Logseq's `parse-uuid` accepts), any case.
#[must_use]
pub fn parse_block_id(value: &str) -> Option<Uuid> {
    let v = value.trim();
    if v.len() != 36 {
        return None;
    }
    Uuid::parse_str(v).ok()
}

/// The valid id of a block: the first `id::` / `custom-id::` / `custom_id::` line of the effective
/// property group whose value is a UUID. An invalid value yields `None`.
#[must_use]
pub fn block_id(content: &str) -> Option<Uuid> {
    let scan = scan_properties(
        content.as_bytes(),
        Span::new(0, content.len()),
        ParserOptions::default(),
    );
    scan.valid_lines()
        .find(|l| l.key_norm == "id")
        .and_then(|l| parse_block_id(&l.value_raw))
}

/// Returns the block's id, adding `id:: <new_id>` (after the title, end of the property group)
/// when it has no valid one. Existing ids leave the content unchanged. Pre-blocks are refused.
///
/// # Errors
/// [`IdError::PreBlock`] when `is_pre_block`.
pub fn ensure_block_id(
    content: &str,
    new_id: Uuid,
    is_pre_block: bool,
) -> Result<(String, Uuid), IdError> {
    if is_pre_block {
        return Err(IdError::PreBlock);
    }
    if let Some(id) = block_id(content) {
        return Ok((content.to_owned(), id));
    }
    Ok((
        set_property(content, "id", &new_id.hyphenated().to_string()),
        new_id,
    ))
}

/// A block id seen more than once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Duplicate<K> {
    /// The repeated id.
    pub id: Uuid,
    /// Page and block index of the first occurrence in load order (the one that keeps the id).
    pub first: (K, usize),
    /// Page and block index of the later occurrence.
    pub duplicate: (K, usize),
}

/// Finds ids that appear on more than one block. Input is `(page, [(block index, id)])` in load
/// order; the first occurrence wins. Bitacora only reports: the duplicate line is removed only when
/// that block is edited.
pub fn find_duplicate_ids<K, I>(pages: I) -> Vec<Duplicate<K>>
where
    K: Clone,
    I: IntoIterator<Item = (K, Vec<(usize, Uuid)>)>,
{
    let mut seen: HashMap<Uuid, (K, usize)> = HashMap::new();
    let mut out = Vec::new();
    for (page, blocks) in pages {
        for (idx, id) in blocks {
            match seen.get(&id) {
                Some(first) => out.push(Duplicate {
                    id,
                    first: first.clone(),
                    duplicate: (page.clone(), idx),
                }),
                None => {
                    seen.insert(id, (page.clone(), idx));
                }
            }
        }
    }
    out
}

/// Removes `((own-uuid))` from the content (a block cannot reference itself).
#[must_use]
pub fn strip_self_ref(content: &str, own: Uuid) -> String {
    let lower = own.hyphenated().to_string();
    let needle = format!("(({lower}))");
    let upper = format!("(({}))", lower.to_uppercase());
    content.replace(&needle, "").replace(&upper, "")
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "6500c1a4-0000-4000-8000-000000000001";

    fn id() -> Uuid {
        Uuid::parse_str(ID).expect("uuid")
    }

    #[test]
    fn appends_after_property_group() {
        let (c, got) = ensure_block_id("Parent block\ntags:: demo", id(), false).expect("ok");
        assert_eq!(c, format!("Parent block\ntags:: demo\nid:: {ID}"));
        assert_eq!(got, id());
    }

    #[test]
    fn existing_id_is_a_no_op_and_aliases_count() {
        let src = format!("a\ncustom_id:: {ID}");
        let (c, got) = ensure_block_id(&src, Uuid::nil(), false).expect("ok");
        assert_eq!(c, src);
        assert_eq!(got, id());
    }

    #[test]
    fn invalid_id_is_replaced_in_place() {
        let (c, _) = ensure_block_id("a\nid:: nope\nz:: 1", id(), false).expect("ok");
        assert_eq!(c, format!("a\nid:: {ID}\nz:: 1"));
    }

    #[test]
    fn pre_block_is_refused() {
        assert_eq!(
            ensure_block_id("title:: x", id(), true),
            Err(IdError::PreBlock)
        );
    }

    #[test]
    fn duplicates_first_wins() {
        let a = id();
        let b = Uuid::from_u128(2);
        let d = find_duplicate_ids([("p1", vec![(0, a), (1, b)]), ("p2", vec![(0, b), (3, a)])]);
        assert_eq!(d.len(), 2);
        assert_eq!(d[0].id, b);
        assert_eq!(d[0].first, ("p1", 1));
        assert_eq!(d[0].duplicate, ("p2", 0));
        assert_eq!(d[1].duplicate, ("p2", 3));
    }

    #[test]
    fn strips_self_ref_only() {
        let c = format!("see (({ID})) and (({}))", Uuid::from_u128(9).hyphenated());
        assert_eq!(
            strip_self_ref(&c, id()),
            format!("see  and (({}))", Uuid::from_u128(9).hyphenated())
        );
    }
}
