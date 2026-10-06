//! Metadata resolvers (ADR-009, BIT-SP-0006.R11): never raise conflicts.
//!
//! * `collapsed::`: the preferred side (ours by default) if it changed, else the other side.
//! * `id::`: union; two different new ids keep the one referenced elsewhere, else the
//!   lexicographically smaller, and report an [`IdRewrite`] for `((loser))` references.
//! * `:LOGBOOK:`: union of lines sorted by start time, latest open clock kept.
//! * `card-*`: the whole group from the side with the later `card-last-reviewed` (ties: ours).
//! * every other metadata key: per-key three-way, `prefer` (the newer writer) when both changed.
//! * property order: ours first, then keys only theirs has (handled by the callers).

use bitacora_markdown::{Side, union_logbook};

use crate::conflict::IdRewrite;
use crate::model::Meta;

fn never_referenced(_: &str) -> bool {
    false
}

/// Inputs of the metadata rules that depend on the repository, the user settings or the commits.
pub struct MergeEnv<'a> {
    /// The newer writer: wins last-writer-wins keys when both sides changed them.
    pub prefer: Side,
    /// `sync.collapsed_policy`: whose `collapsed::` wins.
    pub collapsed: Side,
    /// Index lookup: is this block uuid referenced (`((uuid))`, embed) anywhere in the graph?
    pub is_referenced: &'a dyn Fn(&str) -> bool,
    /// Default journal template text (`:default-templates {:journals ...}` rendered), if any. A
    /// side holding only this text counts as "unchanged" in add/add merges (BIT-SP-0006.R18).
    pub template: Option<&'a str>,
}

impl std::fmt::Debug for MergeEnv<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MergeEnv")
            .field("prefer", &self.prefer)
            .field("collapsed", &self.collapsed)
            .finish_non_exhaustive()
    }
}

impl Default for MergeEnv<'static> {
    fn default() -> Self {
        Self {
            prefer: Side::Ours,
            collapsed: Side::Ours,
            is_referenced: &never_referenced,
            template: None,
        }
    }
}

impl MergeEnv<'static> {
    /// Defaults: ours everywhere, nothing referenced.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

/// Merges the metadata of one block. Output order of keys: ours, then keys only theirs has.
#[must_use]
pub fn merge_meta(
    base: Option<&Meta>,
    ours: &Meta,
    theirs: &Meta,
    env: &MergeEnv<'_>,
) -> (Meta, Vec<IdRewrite>) {
    let empty = Meta::default();
    let base = base.unwrap_or(&empty);
    let (id, rewrites) = merge_id(&base.id, &ours.id, &theirs.id, env);
    let meta = Meta {
        collapsed: merge_collapsed(&base.collapsed, &ours.collapsed, &theirs.collapsed, env),
        id,
        logbook: union_logbook(&ours.logbook, &theirs.logbook),
        card: merge_card(&ours.card, &theirs.card),
        lww: merge_lww(&base.lww, &ours.lww, &theirs.lww, env.prefer),
    };
    (meta, rewrites)
}

fn merge_collapsed(
    base: &Option<String>,
    ours: &Option<String>,
    theirs: &Option<String>,
    env: &MergeEnv<'_>,
) -> Option<String> {
    let (first, second) = match env.collapsed {
        Side::Ours => (ours, theirs),
        Side::Theirs => (theirs, ours),
    };
    if first != base { first } else { second }.clone()
}

fn merge_id(
    base: &Option<String>,
    ours: &Option<String>,
    theirs: &Option<String>,
    env: &MergeEnv<'_>,
) -> (Option<String>, Vec<IdRewrite>) {
    match (ours, theirs) {
        (Some(o), Some(t)) if o != t => {
            if base.as_ref() == Some(o) {
                return (Some(t.clone()), Vec::new());
            }
            if base.as_ref() == Some(t) {
                return (Some(o.clone()), Vec::new());
            }
            let (ro, rt) = ((env.is_referenced)(o), (env.is_referenced)(t));
            let winner_is_ours = match (ro, rt) {
                (true, false) => true,
                (false, true) => false,
                _ => o <= t,
            };
            let (win, lose) = if winner_is_ours { (o, t) } else { (t, o) };
            (
                Some(win.clone()),
                vec![IdRewrite {
                    from: lose.clone(),
                    to: win.clone(),
                }],
            )
        }
        (Some(v), _) | (None, Some(v)) => (Some(v.clone()), Vec::new()),
        (None, None) => (None, Vec::new()),
    }
}

fn card_reviewed(group: &[(String, String)]) -> Option<&str> {
    group
        .iter()
        .find(|(k, _)| k == "card-last-reviewed")
        .map(|(_, v)| v.as_str())
        .filter(|v| !v.is_empty() && *v != "nil")
}

/// SRS review times are ISO-8601 UTC strings (`2026-10-04T08:00:00.000Z`), which sort
/// chronologically as plain strings.
fn merge_card(ours: &[(String, String)], theirs: &[(String, String)]) -> Vec<(String, String)> {
    let sorted = |g: &[(String, String)]| {
        let mut v = g.to_vec();
        v.sort();
        v
    };
    if sorted(ours) == sorted(theirs) {
        return ours.to_vec();
    }
    if card_reviewed(theirs) > card_reviewed(ours) {
        theirs.to_vec()
    } else {
        ours.to_vec()
    }
}

fn merge_lww(
    base: &[(String, String)],
    ours: &[(String, String)],
    theirs: &[(String, String)],
    prefer: Side,
) -> Vec<(String, String)> {
    let get = |v: &'_ [(String, String)], k: &str| {
        v.iter().find(|(kk, _)| kk == k).map(|(_, x)| x.clone())
    };
    let mut keys: Vec<&String> = ours.iter().map(|(k, _)| k).collect();
    for (k, _) in theirs {
        if !keys.contains(&k) {
            keys.push(k);
        }
    }
    let mut out = Vec::new();
    for k in keys {
        let (b, o, t) = (get(base, k), get(ours, k), get(theirs, k));
        let v = if o == t || t == b {
            o
        } else if o == b {
            t
        } else if prefer == Side::Ours {
            o
        } else {
            t
        };
        if let Some(v) = v {
            out.push((k.clone(), v));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "aaaa0000-0000-4000-8000-000000000001";
    const B: &str = "bbbb0000-0000-4000-8000-000000000002";

    fn m(f: impl FnOnce(&mut Meta)) -> Meta {
        let mut x = Meta::default();
        f(&mut x);
        x
    }

    fn kv(k: &str, v: &str) -> (String, String) {
        (k.to_owned(), v.to_owned())
    }

    #[test]
    fn collapsed_table() {
        let env = MergeEnv::new();
        type Case = (
            Option<&'static str>,
            Option<&'static str>,
            Option<&'static str>,
            Option<&'static str>,
        );
        let cases: &[Case] = &[
            // base, ours, theirs, expected
            (None, Some("true"), None, Some("true")), // ours changed
            (None, None, Some("true"), Some("true")), // ours unchanged: theirs
            (Some("true"), None, Some("true"), None), // ours expanded: ours wins
            (Some("true"), Some("true"), None, None), // ours unchanged, theirs expanded
            (None, Some("true"), Some("false"), Some("true")), // both changed: ours
            (None, None, None, None),
        ];
        for (b, o, t, want) in cases {
            let mk = |v: &Option<&str>| m(|x| x.collapsed = v.map(str::to_owned));
            let (got, _) = merge_meta(Some(&mk(b)), &mk(o), &mk(t), &env);
            assert_eq!(got.collapsed.as_deref(), *want, "{b:?} {o:?} {t:?}");
        }
        // Policy theirs.
        let env = MergeEnv {
            collapsed: Side::Theirs,
            ..MergeEnv::new()
        };
        let (got, _) = merge_meta(
            None,
            &m(|x| x.collapsed = Some("true".into())),
            &m(|x| x.collapsed = Some("false".into())),
            &env,
        );
        assert_eq!(got.collapsed.as_deref(), Some("false"));
    }

    #[test]
    fn id_union_and_referenced_wins() {
        let o = m(|x| x.id = Some(A.into()));
        let t = m(|x| x.id = Some(B.into()));
        let refd = |id: &str| id == B;
        let env = MergeEnv {
            is_referenced: &refd,
            ..MergeEnv::new()
        };
        let (got, rw) = merge_meta(None, &o, &t, &env);
        assert_eq!(got.id.as_deref(), Some(B));
        assert_eq!(
            rw,
            [IdRewrite {
                from: A.into(),
                to: B.into()
            }]
        );
        // Nothing referenced: lexicographically smaller wins.
        let (got, rw) = merge_meta(None, &t, &o, &MergeEnv::new());
        assert_eq!(got.id.as_deref(), Some(A));
        assert_eq!(rw[0].from, B);
        // Added on one side only: kept, no rewrite.
        let (got, rw) = merge_meta(None, &o, &Meta::default(), &MergeEnv::new());
        assert_eq!(got.id.as_deref(), Some(A));
        assert!(rw.is_empty());
        let (got, _) = merge_meta(None, &Meta::default(), &o, &MergeEnv::new());
        assert_eq!(got.id.as_deref(), Some(A));
    }

    #[test]
    fn logbook_union_sorted() {
        let c1 = "CLOCK: [2026-10-05 Mon 09:00:00]--[2026-10-05 Mon 10:00:00] =>  01:00:00";
        let c2 = "CLOCK: [2026-10-04 Sun 09:00:00]--[2026-10-04 Sun 09:30:00] =>  00:30:00";
        let o = m(|x| x.logbook = vec![c1.into()]);
        let t = m(|x| x.logbook = vec![c2.into()]);
        let (got, _) = merge_meta(None, &o, &t, &MergeEnv::new());
        assert_eq!(got.logbook, [c2, c1]);
    }

    #[test]
    fn card_group_taken_whole_from_later_review() {
        let ours = m(|x| {
            x.card = vec![
                kv("card-last-reviewed", "2026-10-01T08:00:00.000Z"),
                kv("card-repeats", "3"),
                kv("card-ease-factor", "2.5"),
            ];
        });
        let theirs = m(|x| {
            x.card = vec![
                kv("card-last-reviewed", "2026-10-04T08:00:00.000Z"),
                kv("card-repeats", "4"),
            ];
        });
        let (got, _) = merge_meta(None, &ours, &theirs, &MergeEnv::new());
        assert_eq!(
            got.card, theirs.card,
            "no field mixing (ease-factor dropped)"
        );
        // Ours later: ours whole.
        let (got, _) = merge_meta(None, &theirs, &ours, &MergeEnv::new());
        assert_eq!(got.card, theirs.card);
        // Tie: ours.
        let tie_o = m(|x| {
            x.card = vec![
                kv("card-last-reviewed", "2026-10-04T08:00:00.000Z"),
                kv("card-repeats", "5"),
            ];
        });
        let (got, _) = merge_meta(None, &tie_o, &theirs, &MergeEnv::new());
        assert_eq!(got.card, tie_o.card);
        // Missing last-reviewed is the oldest.
        let none = m(|x| x.card = vec![kv("card-repeats", "9")]);
        let (got, _) = merge_meta(None, &none, &theirs, &MergeEnv::new());
        assert_eq!(got.card, theirs.card);
    }

    #[test]
    fn lww_keys_and_order() {
        let base = m(|x| x.lww = vec![kv("query-table", "true"), kv("hl-page", "1")]);
        let ours = m(|x| x.lww = vec![kv("hl-page", "2"), kv("query-table", "true")]);
        let theirs = m(|x| {
            x.lww = vec![
                kv("query-table", "false"),
                kv("hl-page", "3"),
                kv("filters", "{}"),
            ];
        });
        let env = MergeEnv {
            prefer: Side::Theirs,
            ..MergeEnv::new()
        };
        let (got, _) = merge_meta(Some(&base), &ours, &theirs, &env);
        // ours order, then theirs-only keys appended; both changed hl-page: theirs preferred.
        assert_eq!(
            got.lww,
            [
                kv("hl-page", "3"),
                kv("query-table", "false"),
                kv("filters", "{}")
            ]
        );
        let (got, _) = merge_meta(Some(&base), &ours, &theirs, &MergeEnv::new());
        assert_eq!(got.lww[0], kv("hl-page", "2"));
        // Removed by ours and untouched by theirs: removed.
        let ours2 = m(|x| x.lww = vec![kv("query-table", "true")]);
        let theirs2 = base.clone();
        let (got, _) = merge_meta(Some(&base), &ours2, &theirs2, &MergeEnv::new());
        assert_eq!(got.lww, [kv("query-table", "true")]);
    }

    #[test]
    fn merging_identical_sides_is_identity() {
        let x = m(|x| {
            x.collapsed = Some("true".into());
            x.id = Some(A.into());
            x.logbook = vec![
                "CLOCK: [2026-10-05 Mon 09:00:00]--[2026-10-05 Mon 10:00:00] =>  01:00:00".into(),
            ];
            x.card = vec![kv("card-repeats", "1")];
            x.lww = vec![kv("hl-page", "1")];
        });
        let (got, rw) = merge_meta(Some(&x), &x, &x, &MergeEnv::new());
        assert_eq!(got, x);
        assert!(rw.is_empty());
    }
}
