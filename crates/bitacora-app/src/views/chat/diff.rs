//! Diff preview of an approval card (BIT-T-0458): the lines an approved `propose_edit` would
//! remove and add, computed from the [`Preview`] the backend validated against the live page.

use bitacora_runtime::ai::{Preview, PreviewOp};
use similar::{ChangeTag, TextDiff};

/// What a preview line does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    /// Unchanged context.
    Same,
    /// Removed by the edit.
    Removed,
    /// Added by the edit.
    Added,
}

/// One line of a diff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffLine {
    /// Change.
    pub kind: LineKind,
    /// Text without the trailing newline.
    pub text: String,
}

/// Line diff between two texts.
#[must_use]
pub fn diff_lines(before: &str, after: &str) -> Vec<DiffLine> {
    TextDiff::from_lines(before, after)
        .iter_all_changes()
        .map(|c| DiffLine {
            kind: match c.tag() {
                ChangeTag::Equal => LineKind::Same,
                ChangeTag::Delete => LineKind::Removed,
                ChangeTag::Insert => LineKind::Added,
            },
            text: c.value().trim_end_matches('\n').to_owned(),
        })
        .collect()
}

/// The diff of one op of a proposal. `id::` and `collapsed::` lines are never part of what an
/// agent changes, so they stay as context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpDiff {
    /// `insert`, `update`, `move`, `delete` or `property`.
    pub kind: &'static str,
    /// Lines to show.
    pub lines: Vec<DiffLine>,
}

fn lines_of(kind: LineKind, text: &str) -> Vec<DiffLine> {
    text.lines()
        .map(|l| DiffLine {
            kind,
            text: l.to_owned(),
        })
        .collect()
}

/// Diffs of every op of `preview`, in order.
#[must_use]
pub fn op_diffs(preview: &Preview) -> Vec<OpDiff> {
    preview.ops.iter().map(op_diff).collect()
}

fn op_diff(op: &PreviewOp) -> OpDiff {
    let before = op.before.as_deref().unwrap_or("");
    let after = op.after.as_deref().unwrap_or("");
    let lines = match op.kind {
        "insert" => lines_of(LineKind::Added, after),
        "delete" => lines_of(LineKind::Removed, before),
        // A move changes no text: show the moved block as context.
        "move" => lines_of(LineKind::Same, before),
        _ => diff_lines(before, after),
    };
    OpDiff {
        kind: op.kind,
        lines,
    }
}

/// Counts of added and removed lines of a whole preview (the card's summary).
#[must_use]
pub fn totals(diffs: &[OpDiff]) -> (usize, usize) {
    let count = |k| {
        diffs
            .iter()
            .flat_map(|d| &d.lines)
            .filter(|l| l.kind == k)
            .count()
    };
    (count(LineKind::Added), count(LineKind::Removed))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn op(kind: &'static str, before: Option<&str>, after: Option<&str>) -> PreviewOp {
        PreviewOp {
            kind,
            uuid: None,
            before: before.map(str::to_owned),
            after: after.map(str::to_owned),
        }
    }

    #[test]
    fn update_keeps_untouched_lines_as_context() {
        let d = diff_lines("old\nid:: 1", "new\nid:: 1");
        assert_eq!(
            d.iter().map(|l| l.kind).collect::<Vec<_>>(),
            [LineKind::Removed, LineKind::Added, LineKind::Same]
        );
        assert_eq!(d[2].text, "id:: 1");
    }

    #[test]
    fn ops_map_to_lines_and_totals() {
        let preview = Preview {
            title: "t".into(),
            page: "P".into(),
            ops: vec![
                op("insert", None, Some("a\nb")),
                op("delete", Some("gone"), None),
                op("move", Some("moved"), None),
                op("update", Some("x"), Some("y")),
            ],
        };
        let diffs = op_diffs(&preview);
        assert_eq!(diffs.len(), 4);
        assert_eq!(diffs[2].lines[0].kind, LineKind::Same);
        assert_eq!(totals(&diffs), (3, 2));
    }
}
