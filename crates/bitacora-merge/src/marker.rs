//! Task marker combination and `SCHEDULED:` / `DEADLINE:` line merge (BIT-SP-0006.R13).
//!
//! The first line of a block is split into `heading prefix`, `marker`, `priority` and `text`
//! before the content merge. A side that changed only the marker and a side that changed only the
//! text combine; two different new markers with identical text take the more advanced state.

use crate::conflict::{Conflict, ConflictKind};

/// Task markers recognised by Logseq (`02-markdown-block-syntax.md` §5.3).
const MARKERS: &[&str] = &[
    "TODO",
    "DOING",
    "DONE",
    "LATER",
    "NOW",
    "WAITING",
    "WAIT",
    "CANCELED",
    "CANCELLED",
    "STARTED",
    "IN-PROGRESS",
];

/// A first line split into its parts.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TitleParts {
    /// Heading hashes with their trailing space (`"## "`), or empty.
    pub heading: String,
    /// Task marker.
    pub marker: Option<String>,
    /// Priority letter of `[#A]`.
    pub priority: Option<char>,
    /// The rest of the line.
    pub text: String,
}

/// Splits a first line. A marker must be followed by a space.
#[must_use]
pub fn split_title(line: &str) -> TitleParts {
    let mut rest = line;
    let hashes = rest.bytes().take_while(|&b| b == b'#').count();
    let mut heading = String::new();
    if hashes > 0 && rest[hashes..].starts_with(' ') {
        heading = rest[..=hashes].to_owned();
        rest = &rest[hashes + 1..];
    }
    let mut marker = None;
    if let Some(m) = MARKERS.iter().find(|m| {
        rest.strip_prefix(**m)
            .is_some_and(|after| after.starts_with(' '))
    }) {
        marker = Some((*m).to_owned());
        rest = rest[m.len() + 1..].trim_start();
    }
    let mut priority = None;
    if let Some(after) = rest.strip_prefix("[#") {
        let mut chars = after.chars();
        if let (Some(c), Some(']')) = (chars.next(), chars.next()) {
            priority = Some(c);
            rest = chars.as_str().trim_start();
        }
    }
    TitleParts {
        heading,
        marker,
        priority,
        text: rest.to_owned(),
    }
}

impl TitleParts {
    /// Re-assembles the line with single spaces between parts.
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = self.heading.clone();
        let mut push = |s: &str| {
            if !out.is_empty() && !out.ends_with(' ') {
                out.push(' ');
            }
            out.push_str(s);
        };
        if let Some(m) = &self.marker {
            push(m);
        }
        if let Some(p) = self.priority {
            push(&format!("[#{p}]"));
        }
        if !self.text.is_empty() {
            push(&self.text);
        }
        out
    }
}

/// How advanced a marker is: `DONE` > `CANCELED` > `DOING`/`NOW` > `TODO`/`LATER`.
#[must_use]
pub fn marker_rank(marker: Option<&str>) -> u8 {
    match marker {
        Some("DONE") => 5,
        Some("CANCELED" | "CANCELLED") => 4,
        Some("DOING" | "NOW" | "STARTED" | "IN-PROGRESS") => 3,
        Some("TODO" | "LATER" | "WAITING" | "WAIT") => 2,
        Some(_) => 1,
        None => 0,
    }
}

fn three_way<T: PartialEq + Clone>(b: &T, o: &T, t: &T) -> Option<T> {
    if o == t || t == b {
        Some(o.clone())
    } else if o == b {
        Some(t.clone())
    } else {
        None
    }
}

/// Three-way merge of a block's first line. `None` is a content conflict (both sides changed the
/// text or priority, or both changed the marker while the text also differs).
#[must_use]
pub fn merge_title(base: &str, ours: &str, theirs: &str) -> Option<String> {
    if ours == theirs || theirs == base {
        return Some(ours.to_owned());
    }
    if ours == base {
        return Some(theirs.to_owned());
    }
    let (b, o, t) = (split_title(base), split_title(ours), split_title(theirs));
    let heading = three_way(&b.heading, &o.heading, &t.heading)?;
    let priority = three_way(&b.priority, &o.priority, &t.priority)?;
    let text = three_way(&b.text, &o.text, &t.text)?;
    let marker = if let Some(m) = three_way(&b.marker, &o.marker, &t.marker) {
        m
    } else if o.text == t.text {
        // Both changed the marker, text otherwise identical: the more advanced state wins.
        if marker_rank(t.marker.as_deref()) > marker_rank(o.marker.as_deref()) {
            t.marker.clone()
        } else {
            o.marker.clone()
        }
    } else {
        return None;
    };
    let merged = TitleParts {
        heading,
        marker,
        priority,
        text,
    };
    // Keep a side's bytes when the merge equals it, so whitespace is not normalised needlessly.
    if merged == o {
        Some(ours.to_owned())
    } else if merged == t {
        Some(theirs.to_owned())
    } else {
        Some(merged.render())
    }
}

/// Per-keyword three-way merge of `SCHEDULED` / `DEADLINE` lines. Both sides changing the same
/// line differently is a `Property` conflict (ours kept). Output order: ours, then keywords only
/// theirs has.
#[must_use]
pub fn merge_planning(
    base: &[(String, String)],
    ours: &[(String, String)],
    theirs: &[(String, String)],
) -> (Vec<(String, String)>, Vec<Conflict>) {
    let get = |v: &'_ [(String, String)], k: &str| {
        v.iter().find(|(kw, _)| kw == k).map(|(_, val)| val.clone())
    };
    let mut keys: Vec<&String> = ours.iter().map(|(k, _)| k).collect();
    for (k, _) in theirs {
        if !keys.contains(&k) {
            keys.push(k);
        }
    }
    let mut out = Vec::new();
    let mut conflicts = Vec::new();
    for k in keys {
        let (b, o, t) = (get(base, k), get(ours, k), get(theirs, k));
        let value = if o == t || t == b {
            o.clone()
        } else if o == b {
            t.clone()
        } else {
            conflicts.push(Conflict {
                kind: ConflictKind::Property,
                field: k.clone(),
                base: b,
                ours: o.clone(),
                theirs: t,
            });
            o.clone()
        };
        if let Some(v) = value {
            out.push((k.clone(), v));
        }
    }
    (out, conflicts)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pl(kw: &str, v: &str) -> Vec<(String, String)> {
        vec![(kw.to_owned(), v.to_owned())]
    }

    #[test]
    fn splits_parts() {
        let p = split_title("## TODO [#A] write report");
        assert_eq!(p.heading, "## ");
        assert_eq!(p.marker.as_deref(), Some("TODO"));
        assert_eq!(p.priority, Some('A'));
        assert_eq!(p.text, "write report");
        assert_eq!(p.render(), "## TODO [#A] write report");
        assert_eq!(split_title("TODOx foo").marker, None);
        assert_eq!(split_title("LATER").marker, None);
        assert_eq!(split_title("plain").text, "plain");
    }

    #[test]
    fn r13_marker_vs_text_combines() {
        assert_eq!(
            merge_title(
                "TODO write report",
                "DONE write report",
                "TODO write final report"
            )
            .as_deref(),
            Some("DONE write final report")
        );
    }

    #[test]
    fn r13_both_changed_marker_takes_more_advanced() {
        assert_eq!(
            merge_title("TODO call", "DOING call", "DONE call").as_deref(),
            Some("DONE call")
        );
        assert_eq!(
            merge_title("TODO call", "DONE call", "DOING call").as_deref(),
            Some("DONE call")
        );
        assert_eq!(
            merge_title("TODO call", "CANCELED call", "NOW call").as_deref(),
            Some("CANCELED call")
        );
        assert_eq!(
            merge_title("LATER call", "NOW call", "TODO call").as_deref(),
            Some("NOW call")
        );
    }

    #[test]
    fn both_changed_marker_and_text_differs_is_conflict() {
        assert_eq!(
            merge_title("TODO call", "DOING call now", "DONE call"),
            None
        );
    }

    #[test]
    fn same_text_edit_on_both_sides_conflicts() {
        assert_eq!(merge_title("Meet at 10", "Meet at 11", "Meet at 12"), None);
        assert_eq!(
            merge_title("Meet at 10", "Meet at 11", "Meet at 11").as_deref(),
            Some("Meet at 11")
        );
    }

    #[test]
    fn marker_added_and_priority_changed_combine() {
        assert_eq!(
            merge_title("write", "TODO write", "[#B] write").as_deref(),
            Some("TODO [#B] write")
        );
    }

    #[test]
    fn marker_with_leading_hashes_keeps_heading() {
        assert_eq!(
            merge_title("# TODO a b", "# DONE a b", "# TODO a b c").as_deref(),
            Some("# DONE a b c")
        );
    }

    #[test]
    fn r13_both_rescheduled_conflicts_and_keeps_ours() {
        let (out, c) = merge_planning(
            &pl("SCHEDULED", "<2026-10-06 Tue>"),
            &pl("SCHEDULED", "<2026-10-07 Wed>"),
            &pl("SCHEDULED", "<2026-10-08 Thu>"),
        );
        assert_eq!(out, pl("SCHEDULED", "<2026-10-07 Wed>"));
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].kind, ConflictKind::Property);
        assert_eq!(c[0].field, "SCHEDULED");
    }

    #[test]
    fn planning_one_side_changed_and_repeater_preserved() {
        let b = pl("SCHEDULED", "<2026-10-06 Tue .+1d>");
        let o = pl("SCHEDULED", "<2026-10-06 Tue .+1d>");
        let t = pl("SCHEDULED", "<2026-10-07 Wed .+1d>");
        let (out, c) = merge_planning(&b, &o, &t);
        assert!(c.is_empty());
        assert_eq!(out, t);
    }

    #[test]
    fn planning_added_on_one_side_and_different_keywords() {
        let (out, c) = merge_planning(
            &[],
            &pl("SCHEDULED", "<2026-10-06 Tue>"),
            &pl("DEADLINE", "<2026-10-09 Fri>"),
        );
        assert!(c.is_empty());
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].0, "SCHEDULED");
        assert_eq!(out[1].0, "DEADLINE");
        // Removed on one side, unchanged on the other: removed.
        let b = pl("DEADLINE", "<2026-10-09 Fri>");
        let (out, c) = merge_planning(&b, &[], &b);
        assert!(out.is_empty() && c.is_empty());
    }
}
