//! EDN-aware three-way merge of `logseq/config.edn` (design `git-sync-merge` 5.1,
//! BIT-SP-0006.R17).
//!
//! The three texts are read with `bitacora-config`'s EDN reader. Every top-level key (and every
//! key of nested maps) is merged three-way; the resulting changes are applied as surgical text
//! edits on **ours** through [`ConfigEditor`], so comments, commas and formatting of ours survive.
//! Sets union, vectors merge as lists, and a key changed differently on both sides stays as ours
//! and is reported as a conflict (whole-key choice).

use bitacora_config::{ConfigEditor, Edn, read_str};
use bitacora_merge::merge_lines;

/// A key changed incompatibly on both sides.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyConflict {
    /// Key path (keyword names without `:`), outermost first.
    pub path: Vec<String>,
    /// Base value as EDN text (`None`: absent).
    pub base: Option<String>,
    /// Our value.
    pub ours: Option<String>,
    /// Their value.
    pub theirs: Option<String>,
}

/// Outcome of [`merge_config`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EdnMerge {
    /// Merged text (ours plus the clean changes of theirs) and the keys that conflict.
    Merged {
        /// The merged text; conflicting keys hold ours.
        text: String,
        /// Unresolved keys.
        conflicts: Vec<KeyConflict>,
    },
    /// One of the three texts is not a readable EDN map: nothing could be merged.
    Unparsable,
}

type Pairs = Vec<(Edn, Edn)>;

fn read_root(src: &str) -> Option<Pairs> {
    match read_str(src).ok()? {
        None => Some(Vec::new()),
        Some(Edn::Map(m)) => Some(m),
        Some(_) => None,
    }
}

fn keyword_name(e: &Edn) -> Option<&str> {
    match e {
        Edn::Keyword(k) => Some(k),
        _ => None,
    }
}

fn all_keyword(m: &Pairs) -> bool {
    m.iter().all(|(k, _)| keyword_name(k).is_some())
}

fn lookup<'a>(m: &'a Pairs, key: &str) -> Option<&'a Edn> {
    m.iter()
        .find(|(k, _)| keyword_name(k) == Some(key))
        .map(|(_, v)| v)
}

struct Merger {
    editor: ConfigEditor,
    conflicts: Vec<KeyConflict>,
    failed: bool,
}

impl Merger {
    fn assoc(&mut self, path: &[String], value: &Edn) {
        let p: Vec<&str> = path.iter().map(String::as_str).collect();
        if self.editor.assoc(&p, value).is_err() {
            self.failed = true;
        }
    }

    fn dissoc(&mut self, path: &[String]) {
        let p: Vec<&str> = path.iter().map(String::as_str).collect();
        if self.editor.dissoc(&p).is_err() {
            self.failed = true;
        }
    }

    fn conflict(&mut self, path: &[String], b: Option<&Edn>, o: Option<&Edn>, t: Option<&Edn>) {
        self.conflicts.push(KeyConflict {
            path: path.to_vec(),
            base: b.map(Edn::pr_str),
            ours: o.map(Edn::pr_str),
            theirs: t.map(Edn::pr_str),
        });
    }

    fn apply_theirs(&mut self, path: &[String], t: Option<&Edn>) {
        match t {
            Some(v) => self.assoc(path, v),
            None => self.dissoc(path),
        }
    }

    fn merge_map(&mut self, path: &[String], b: &Pairs, o: &Pairs, t: &Pairs) {
        // Maps with non-keyword keys are compared as one value.
        if !(all_keyword(b) && all_keyword(o) && all_keyword(t)) {
            let (bv, ov, tv) = (
                Edn::Map(b.clone()),
                Edn::Map(o.clone()),
                Edn::Map(t.clone()),
            );
            self.decide_atomic(path, Some(&bv), Some(&ov), Some(&tv));
            return;
        }
        let mut keys: Vec<&str> = Vec::new();
        for m in [t, b] {
            for (k, _) in m {
                if let Some(n) = keyword_name(k)
                    && !keys.contains(&n)
                {
                    keys.push(n);
                }
            }
        }
        for key in keys {
            let mut child = path.to_vec();
            child.push(key.to_owned());
            self.merge_value(&child, lookup(b, key), lookup(o, key), lookup(t, key));
        }
    }

    fn decide_atomic(
        &mut self,
        path: &[String],
        b: Option<&Edn>,
        o: Option<&Edn>,
        t: Option<&Edn>,
    ) {
        if path.is_empty() {
            self.failed = true;
        } else if o == b {
            self.apply_theirs(path, t);
        } else {
            self.conflict(path, b, o, t);
        }
    }

    fn merge_value(&mut self, path: &[String], b: Option<&Edn>, o: Option<&Edn>, t: Option<&Edn>) {
        if t == b || o == t {
            return;
        }
        match (o, t) {
            (Some(Edn::Map(om)), Some(Edn::Map(tm))) => {
                let empty = Vec::new();
                let bm = match b {
                    Some(Edn::Map(m)) => m,
                    _ => &empty,
                };
                self.merge_map(path, bm, om, tm);
            }
            _ if o == b => self.apply_theirs(path, t),
            (Some(Edn::Set(os)), Some(Edn::Set(ts))) => {
                let bs: &[Edn] = match b {
                    Some(Edn::Set(s)) => s,
                    _ => &[],
                };
                let mut out: Vec<Edn> = os
                    .iter()
                    .filter(|e| !(bs.contains(e) && !ts.contains(e)))
                    .cloned()
                    .collect();
                for e in ts {
                    if !bs.contains(e) && !out.contains(e) {
                        out.push(e.clone());
                    }
                }
                if out != *os {
                    self.assoc(path, &Edn::Set(out));
                }
            }
            (Some(Edn::Vector(ov)), Some(Edn::Vector(tv))) => {
                let bv: &[Edn] = match b {
                    Some(Edn::Vector(v)) => v,
                    _ => &[],
                };
                match merge_vectors(bv, ov, tv) {
                    Some(v) if v != *ov => self.assoc(path, &Edn::Vector(v)),
                    Some(_) => {}
                    None => self.conflict(path, b, o, t),
                }
            }
            _ => self.conflict(path, b, o, t),
        }
    }
}

/// Three-way merge of vectors as lists of printed elements (line diff3 over one element per line).
fn merge_vectors(b: &[Edn], o: &[Edn], t: &[Edn]) -> Option<Vec<Edn>> {
    let join = |v: &[Edn]| {
        let mut s = String::new();
        for e in v {
            s.push_str(&e.pr_str());
            s.push('\n');
        }
        s
    };
    let r = merge_lines(&join(b), &join(o), &join(t));
    if !r.conflicts.is_empty() {
        return None;
    }
    r.output
        .lines()
        .map(|l| match read_str(l) {
            Ok(Some(e)) => Some(e),
            _ => None,
        })
        .collect()
}

/// Merges `config.edn` texts. See the module documentation.
pub fn merge_config(base: &str, ours: &str, theirs: &str) -> EdnMerge {
    let (Some(b), Some(o), Some(t)) = (read_root(base), read_root(ours), read_root(theirs)) else {
        return EdnMerge::Unparsable;
    };
    let Ok(editor) = ConfigEditor::parse(ours) else {
        return EdnMerge::Unparsable;
    };
    let mut m = Merger {
        editor,
        conflicts: Vec::new(),
        failed: false,
    };
    m.merge_map(&[], &b, &o, &t);
    if m.failed {
        return EdnMerge::Unparsable;
    }
    let text = m.editor.into_text();
    if read_root(&text).is_none() {
        return EdnMerge::Unparsable;
    }
    EdnMerge::Merged {
        text,
        conflicts: m.conflicts,
    }
}

/// Sets (or, with `None`, removes) the value at `path` in `text`, as a surgical edit. Used to apply
/// a `config` conflict resolution. Returns `None` when `text` or `value` is not valid EDN.
pub fn set_config_value(text: &str, path: &[String], value: Option<&str>) -> Option<String> {
    let mut editor = ConfigEditor::parse(text).ok()?;
    let p: Vec<&str> = path.iter().map(String::as_str).collect();
    match value {
        Some(v) => {
            let edn = read_str(v).ok()??;
            editor.assoc(&p, &edn).ok()?;
        }
        None => {
            editor.dissoc(&p).ok()?;
        }
    }
    Some(editor.into_text())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn merged(b: &str, o: &str, t: &str) -> (String, Vec<KeyConflict>) {
        match merge_config(b, o, t) {
            EdnMerge::Merged { text, conflicts } => (text, conflicts),
            EdnMerge::Unparsable => panic!("unparsable"),
        }
    }

    #[test]
    fn disjoint_keys_merge_and_comments_survive() {
        let base = "{:journal/page-title-format \"MMM do, yyyy\"\n :preferred-format :markdown}\n";
        let ours = ";; my settings\n{:journal/page-title-format \"yyyy-MM-dd\" ; ISO\n :preferred-format :markdown}\n";
        let theirs = "{:journal/page-title-format \"MMM do, yyyy\"\n :preferred-format :markdown\n :default-templates {:journals \"daily\"}}\n";
        let (text, c) = merged(base, ours, theirs);
        assert!(c.is_empty(), "{c:?}");
        assert!(text.starts_with(";; my settings\n"), "{text}");
        assert!(text.contains("\"yyyy-MM-dd\" ; ISO"), "{text}");
        assert!(
            text.contains(":default-templates {:journals \"daily\"}"),
            "{text}"
        );
        assert!(read_root(&text).is_some());
    }

    #[test]
    fn same_key_divergence_is_a_conflict_keeping_ours() {
        let (text, c) = merged("{:a 1 :b 2}", "{:a 2 :b 2}", "{:a 3 :b 5}");
        assert!(text.contains(":a 2") && text.contains(":b 5"), "{text}");
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].path, vec!["a".to_owned()]);
        assert_eq!(
            (
                c[0].base.as_deref(),
                c[0].ours.as_deref(),
                c[0].theirs.as_deref()
            ),
            (Some("1"), Some("2"), Some("3"))
        );
    }

    #[test]
    fn nested_maps_recurse_and_deletions_apply() {
        let base = "{:ui {:a 1 :b 2} :x 1 :y 2}";
        let ours = "{:ui {:a 10 :b 2} :x 1 :y 2}";
        let theirs = "{:ui {:a 1 :b 20} :x 1}";
        let (text, c) = merged(base, ours, theirs);
        assert!(c.is_empty(), "{c:?}");
        assert!(text.contains(":a 10") && text.contains(":b 20"), "{text}");
        assert!(!text.contains(":y"), "{text}");
        // Delete on one side vs modify on the other is a conflict at that key.
        let (_, c) = merged("{:k 1}", "{:k 2}", "{}");
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].theirs, None);
    }

    #[test]
    fn sets_union_and_vectors_merge_as_lists() {
        let (text, c) = merged(
            "{:s #{1 2} :v [\"a\" \"b\" \"c\"]}",
            "{:s #{1 2 3} :v [\"a\" \"b\" \"c\" \"d\"]}",
            "{:s #{2 4} :v [\"x\" \"a\" \"b\" \"c\"]}",
        );
        assert!(c.is_empty(), "{c:?} {text}");
        let Some(Edn::Map(m)) = read_str(&text).ok().flatten() else {
            panic!("not a map: {text}");
        };
        let s = lookup(&m, "s")
            .and_then(Edn::as_items)
            .unwrap_or(&[])
            .to_vec();
        assert!(s.contains(&Edn::Int(3)) && s.contains(&Edn::Int(4)) && !s.contains(&Edn::Int(1)));
        let v = lookup(&m, "v")
            .and_then(Edn::as_items)
            .unwrap_or(&[])
            .to_vec();
        assert_eq!(
            v,
            ["x", "a", "b", "c", "d"]
                .iter()
                .map(|s| Edn::str(s))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn unparsable_input_is_reported() {
        assert_eq!(
            merge_config("{:a 1}", "{:a 2", "{:a 3}"),
            EdnMerge::Unparsable
        );
        assert_eq!(
            merge_config("{:a 1}", "{:a 2}", "[1 2]"),
            EdnMerge::Unparsable
        );
    }

    #[test]
    fn empty_base_adds_keys_from_both_sides() {
        let (text, c) = merged("", "{:a 1}", "{:b 2}");
        assert!(c.is_empty());
        assert!(text.contains(":a 1") && text.contains(":b 2"), "{text}");
    }

    #[test]
    fn set_config_value_edits_surgically() {
        let text = ";; keep\n{:a 1 ; one\n :b {:c 2}}\n";
        let out = set_config_value(text, &["b".into(), "c".into()], Some("9")).unwrap_or_default();
        assert_eq!(out, ";; keep\n{:a 1 ; one\n :b {:c 9}}\n");
        let out = set_config_value(text, &["a".into()], None).unwrap_or_default();
        assert!(!out.contains(":a 1") && out.starts_with(";; keep"), "{out}");
        assert!(set_config_value(text, &["a".into()], Some("{")).is_none());
    }
}
