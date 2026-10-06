//! Tree-level merge: applies the per-path policies of design `git-sync-merge` 4.4 and 5 to the
//! difference between `base`, `ours` (HEAD) and `theirs` (the remote tip).
//!
//! The planner is read-only: it reads blobs through the [`GitBackend`] and returns a
//! [`MergePlan`] (tree edits for the merge commit, work-tree changes for the writer, conflicts as
//! data). It never writes files and never produces conflict markers; conflicting regions hold
//! "ours" (BIT-SP-0006.R8).

use std::collections::HashMap;

use bitacora_merge::{ConflictKind, MergeEnv, NoteKind, PageConflict, merge_lines, merge_page};

use crate::autocommit::is_ignored_path;
use crate::backend::{GitBackend, GitError, Oid, TreeChange, TreeEdit};
use crate::writer::{FileChange, is_safe_relative};

/// Kind of a persisted conflict (design 4.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConflictType {
    /// Overlapping edits of one block's text.
    Content,
    /// Same user property changed on both sides.
    Property,
    /// Block deleted on one side and modified on the other.
    DeleteVsModify,
    /// File deleted on one side and modified on the other.
    FileDeleteVsModify,
    /// File renamed differently on both sides.
    RenameRename,
    /// `logseq/config.edn` changed incompatibly on both sides.
    Config,
    /// Overlapping line edits in a plain-text file (css, js, ...).
    Text,
}

/// One unresolved conflict, ready to be persisted and shown by the resolver (BIT-US-0053/0054).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConflictRecord {
    /// Stable id within one merge (`c-0001`, ...).
    pub id: String,
    /// Graph-relative path of the (surviving) file.
    pub path: String,
    /// Kind.
    pub kind: ConflictType,
    /// `id::` of the block, when the conflict is block-level.
    pub block_key: Option<String>,
    /// Ancestor titles followed by the block's first line.
    pub breadcrumb: Vec<String>,
    /// Base value (text), when available.
    pub base: Option<String>,
    /// Our value.
    pub ours: Option<String>,
    /// Their value.
    pub theirs: Option<String>,
}

/// Result of planning a merge.
#[derive(Debug, Clone, Default)]
pub struct MergePlan {
    /// Edits to apply on top of the `ours` tree to obtain the merged tree.
    pub edits: Vec<TreeEdit>,
    /// The same changes as work-tree mutations (for the graph writer).
    pub changes: Vec<FileChange>,
    /// Unresolved conflicts; the plan keeps "ours" for each of them.
    pub conflicts: Vec<ConflictRecord>,
    /// Informational notes about automatic decisions.
    pub notes: Vec<String>,
    /// Paths whose content changes in the merged tree.
    pub touched: Vec<String>,
}

/// Inputs of [`plan_merge`].
pub struct MergeInput<'a> {
    /// Backend used to read blobs and diff trees.
    pub backend: &'a dyn GitBackend,
    /// Our tip (HEAD).
    pub ours: &'a Oid,
    /// Their tip (the remote-tracking commit).
    pub theirs: &'a Oid,
    /// Merge base; `None` for unrelated histories (an empty base).
    pub base: Option<&'a Oid>,
    /// Index lookup used by the `id::` union rule.
    pub is_referenced: &'a dyn Fn(&str) -> bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Policy {
    Ignore,
    Markdown,
    Config,
    WholeFile,
    Binary,
    Text,
}

fn policy_for(path: &str, contents: [Option<&[u8]>; 3]) -> Policy {
    if is_ignored_path(path) {
        return Policy::Ignore;
    }
    let all_utf8 = contents
        .iter()
        .flatten()
        .all(|b| std::str::from_utf8(b).is_ok());
    if path.ends_with(".md") {
        return if all_utf8 {
            Policy::Markdown
        } else {
            Policy::Binary
        };
    }
    if path == "logseq/config.edn" {
        return if all_utf8 {
            Policy::Config
        } else {
            Policy::Binary
        };
    }
    if (path.starts_with("whiteboards/") && path.ends_with(".edn"))
        || path.starts_with("draws/")
        || path.ends_with(".tldr")
        || path.ends_with(".excalidraw")
    {
        return Policy::WholeFile;
    }
    if path.starts_with("assets/") || !all_utf8 {
        return Policy::Binary;
    }
    Policy::Text
}

fn has_marker_line(s: &str) -> bool {
    s.lines()
        .any(|l| l.starts_with("<<<<<<< ") || l.starts_with(">>>>>>> ") || l == "=======")
}

fn text(b: Option<&[u8]>) -> Option<String> {
    b.and_then(|b| std::str::from_utf8(b).ok())
        .map(str::to_owned)
}

/// A page whose only content is the default journal template, `-` or nothing counts as
/// "unchanged" in add/add merges (design 5.3).
fn is_blank_page(s: &str) -> bool {
    let t = s.trim();
    t.is_empty() || t == "-"
}

fn conflict_copy_path(path: &str, tag: &str) -> String {
    let (dir, name) = match path.rsplit_once('/') {
        Some((d, n)) => (Some(d), n),
        None => (None, path),
    };
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() => (s, Some(e)),
        _ => (name, None),
    };
    let mut out = String::new();
    if let Some(d) = dir {
        out.push_str(d);
        out.push('/');
    }
    out.push_str(&format!("{stem} (conflict-{tag})"));
    if let Some(e) = ext {
        out.push('.');
        out.push_str(e);
    }
    out
}

/// (kind, block-level detail, base, ours, theirs)
type RawConflict = (
    ConflictType,
    Option<PageConflict>,
    Option<String>,
    Option<String>,
    Option<String>,
);

struct FileOutcome {
    /// Final content at the result path (`None`: deleted).
    content: Option<Vec<u8>>,
    /// Extra files to create (conflict copies).
    extra: Vec<(String, Vec<u8>)>,
    conflicts: Vec<RawConflict>,
    notes: Vec<String>,
}

impl FileOutcome {
    fn take(content: Option<&[u8]>) -> Self {
        Self {
            content: content.map(<[u8]>::to_vec),
            extra: Vec::new(),
            conflicts: Vec::new(),
            notes: Vec::new(),
        }
    }
}

struct FileCtx<'a> {
    path: &'a str,
    theirs_tag: &'a str,
    env: &'a MergeEnv<'a>,
}

fn merge_file(
    ctx: &FileCtx<'_>,
    base: Option<&[u8]>,
    ours: Option<&[u8]>,
    theirs: Option<&[u8]>,
) -> FileOutcome {
    if ours == theirs || base == theirs {
        return FileOutcome::take(ours);
    }
    if base == ours {
        return FileOutcome::take(theirs);
    }
    // Both sides changed differently.
    let policy = policy_for(ctx.path, [base, ours, theirs]);
    if policy == Policy::Ignore {
        return FileOutcome::take(ours);
    }
    let (Some(o), Some(t)) = (ours, theirs) else {
        // Deleted on one side, modified on the other: keep ours, record the choice.
        let mut out = FileOutcome::take(ours);
        out.conflicts.push((
            ConflictType::FileDeleteVsModify,
            None,
            text(base),
            text(ours),
            text(theirs),
        ));
        return out;
    };
    match policy {
        Policy::Markdown => merge_markdown(ctx, base, o, t),
        Policy::Config | Policy::Text => {
            let (b, os, ts) = (
                text(base).unwrap_or_default(),
                text(Some(o)).unwrap_or_default(),
                text(Some(t)).unwrap_or_default(),
            );
            let r = merge_lines(&b, &os, &ts);
            if r.conflicts.is_empty() && !(has_marker_line(&r.output) && !has_marker_line(&os)) {
                let mut out = FileOutcome::take(Some(r.output.as_bytes()));
                out.notes
                    .push(format!("`{}` merged line by line", ctx.path));
                out
            } else {
                let kind = if policy == Policy::Config {
                    ConflictType::Config
                } else {
                    ConflictType::Text
                };
                let mut out = FileOutcome::take(Some(o));
                out.conflicts
                    .push((kind, None, text(base), Some(os), Some(ts)));
                out
            }
        }
        Policy::WholeFile | Policy::Binary => {
            // Keep ours at the path; theirs survives as a conflict copy (design 5.1).
            let mut out = FileOutcome::take(Some(o));
            let copy = conflict_copy_path(ctx.path, ctx.theirs_tag);
            out.notes.push(format!(
                "`{}` changed on both sides; their version was saved as `{copy}`",
                ctx.path
            ));
            out.extra.push((copy, t.to_vec()));
            out
        }
        Policy::Ignore => FileOutcome::take(ours),
    }
}

fn merge_markdown(ctx: &FileCtx<'_>, base: Option<&[u8]>, o: &[u8], t: &[u8]) -> FileOutcome {
    let (b, os, ts) = (
        text(base).unwrap_or_default(),
        text(Some(o)).unwrap_or_default(),
        text(Some(t)).unwrap_or_default(),
    );
    // add/add of an empty or template-only page: the other side wins (design 5.3).
    if is_blank_page(&b) {
        if is_blank_page(&os) {
            return FileOutcome::take(Some(t));
        }
        if is_blank_page(&ts) {
            return FileOutcome::take(Some(o));
        }
    }
    let r = merge_page(&b, &os, &ts, ctx.env);
    if has_marker_line(&r.output) && !has_marker_line(&os) {
        // Defensive: never let marker lines reach a graph file.
        let mut out = FileOutcome::take(Some(o));
        out.conflicts
            .push((ConflictType::Content, None, text(base), Some(os), Some(ts)));
        return out;
    }
    let mut out = FileOutcome::take(Some(r.output.as_bytes()));
    for c in r.conflicts {
        let kind = match c.conflict.kind {
            ConflictKind::Content => ConflictType::Content,
            ConflictKind::Property => ConflictType::Property,
            ConflictKind::DeleteVsModify => ConflictType::DeleteVsModify,
        };
        let (b, o, t) = (
            c.conflict.base.clone(),
            c.conflict.ours.clone(),
            c.conflict.theirs.clone(),
        );
        out.conflicts.push((kind, Some(c), b, o, t));
    }
    for n in r.notes {
        let label = match n.kind {
            NoteKind::IdRewritten => "ids",
            _ => "structure",
        };
        out.notes
            .push(format!("`{}` ({label}): {}", ctx.path, n.message));
    }
    out
}

/// Plans the merge of `theirs` into `ours`. Pure with respect to the repository and work tree.
pub fn plan_merge(input: &MergeInput<'_>) -> Result<MergePlan, GitError> {
    let backend = input.backend;
    let empty = Oid::empty_tree();
    let base_tree = input.base.unwrap_or(&empty);
    let ours_changes = backend.diff_trees(base_tree, input.ours)?;
    let theirs_changes = backend.diff_trees(base_tree, input.theirs)?;

    let mut ours_renames: HashMap<&str, &str> = HashMap::new();
    for c in &ours_changes {
        if let TreeChange::Renamed { from, to, .. } = c {
            ours_renames.insert(from, to);
        }
    }

    let blob = |commit: &Oid, path: &str| backend.read_blob(commit, path);
    let base_blob = |path: &str| -> Result<Option<Vec<u8>>, GitError> {
        match input.base {
            Some(b) => blob(b, path),
            None => Ok(None),
        }
    };

    let env = MergeEnv {
        is_referenced: input.is_referenced,
        ..MergeEnv::new()
    };
    let tag: String = input.theirs.as_hex().chars().take(7).collect();

    let mut plan = MergePlan::default();
    // Final content per path, in a stable order; later entries win.
    let mut finals: Vec<(String, Option<Vec<u8>>)> = Vec::new();

    for change in &theirs_changes {
        // (base path, our path, their path, result path)
        let (base_path, ours_path, theirs_path, result_path): (
            Option<&str>,
            String,
            Option<&str>,
            String,
        ) = match change {
            TreeChange::Added { path } => (None, path.clone(), Some(path), path.clone()),
            TreeChange::Modified { path } => {
                let o = ours_renames.get(path.as_str()).copied().unwrap_or(path);
                (Some(path), o.to_string(), Some(path), o.to_string())
            }
            TreeChange::Deleted { path } => {
                let o = ours_renames.get(path.as_str()).copied().unwrap_or(path);
                (Some(path), o.to_string(), None, o.to_string())
            }
            TreeChange::Renamed { from, to, .. } => {
                let o = ours_renames.get(from.as_str()).copied().unwrap_or(from);
                let result = if ours_renames.contains_key(from.as_str()) {
                    o.to_string()
                } else {
                    to.clone()
                };
                (Some(from), o.to_string(), Some(to), result)
            }
        };
        for p in [
            Some(ours_path.as_str()),
            theirs_path,
            Some(result_path.as_str()),
        ]
        .into_iter()
        .flatten()
        {
            if !is_safe_relative(p) {
                return Err(GitError::other(format!("unsafe path in tree: `{p}`")));
            }
        }
        let base_content = match base_path {
            Some(p) => base_blob(p)?,
            None => None,
        };
        let ours_content = blob(input.ours, &ours_path)?;
        let theirs_content = match theirs_path {
            Some(p) => blob(input.theirs, p)?,
            None => None,
        };
        if let TreeChange::Renamed { from, to, .. } = change
            && ours_renames
                .get(from.as_str())
                .is_some_and(|o| *o != to.as_str())
        {
            plan.conflicts.push(ConflictRecord {
                id: String::new(),
                path: result_path.clone(),
                kind: ConflictType::RenameRename,
                block_key: None,
                breadcrumb: Vec::new(),
                base: Some(from.clone()),
                ours: Some(ours_path.clone()),
                theirs: Some(to.clone()),
            });
        }
        let ctx = FileCtx {
            path: &result_path,
            theirs_tag: &tag,
            env: &env,
        };
        let outcome = merge_file(
            &ctx,
            base_content.as_deref(),
            ours_content.as_deref(),
            theirs_content.as_deref(),
        );
        for (kind, page, b, o, t) in outcome.conflicts {
            let (block_key, breadcrumb) = page
                .map(|p| (p.block_key, p.breadcrumb))
                .unwrap_or_default();
            plan.conflicts.push(ConflictRecord {
                id: String::new(),
                path: result_path.clone(),
                kind,
                block_key,
                breadcrumb,
                base: b,
                ours: o,
                theirs: t,
            });
        }
        plan.notes.extend(outcome.notes);

        // A file moved by theirs (and not by ours) leaves its old path behind.
        if ours_path != result_path && ours_content.is_some() {
            finals.push((ours_path.clone(), None));
        }
        let at_result = blob(input.ours, &result_path)?;
        if outcome.content != at_result || ours_path != result_path {
            finals.push((result_path.clone(), outcome.content));
        }
        for (extra_path, bytes) in outcome.extra {
            if blob(input.ours, &extra_path)?.is_none() {
                finals.push((extra_path, Some(bytes)));
            }
        }
    }

    for (i, c) in plan.conflicts.iter_mut().enumerate() {
        c.id = format!("c-{:04}", i + 1);
    }

    // Collapse to the last decision per path; removals first so renames never collide.
    let mut order: Vec<String> = Vec::new();
    let mut last: HashMap<String, Option<Vec<u8>>> = HashMap::new();
    for (p, c) in finals {
        if !last.contains_key(&p) {
            order.push(p.clone());
        }
        last.insert(p, c);
    }
    let mut removals = Vec::new();
    let mut writes = Vec::new();
    for p in order {
        let content = last.remove(&p).flatten();
        let expected = blob(input.ours, &p)?;
        if content == expected {
            continue;
        }
        plan.touched.push(p.clone());
        match content {
            None => removals.push((p, expected)),
            Some(c) => writes.push((p, c, expected)),
        }
    }
    for (path, expected) in removals {
        plan.edits.push(TreeEdit::Remove { path: path.clone() });
        plan.changes.push(FileChange::Delete { path, expected });
    }
    for (path, content, expected) in writes {
        plan.edits.push(TreeEdit::Upsert {
            path: path.clone(),
            content: content.clone(),
        });
        plan.changes.push(FileChange::Write {
            path,
            content,
            expected,
        });
    }
    Ok(plan)
}

impl std::fmt::Debug for MergeInput<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MergeInput")
            .field("ours", &self.ours)
            .field("theirs", &self.theirs)
            .field("base", &self.base)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx<'a>(path: &'a str, env: &'a MergeEnv<'a>) -> FileCtx<'a> {
        FileCtx {
            path,
            theirs_tag: "abc1234",
            env,
        }
    }

    #[test]
    fn conflict_copy_names() {
        assert_eq!(
            conflict_copy_path("assets/pic.png", "abc1234"),
            "assets/pic (conflict-abc1234).png"
        );
        assert_eq!(conflict_copy_path("noext", "x"), "noext (conflict-x)");
    }

    #[test]
    fn policies_by_path() {
        let t = Some(b"x".as_slice());
        assert!(policy_for("pages/A.md", [t, t, t]) == Policy::Markdown);
        assert!(policy_for("logseq/config.edn", [t, t, t]) == Policy::Config);
        assert!(policy_for("logseq/custom.css", [t, t, t]) == Policy::Text);
        assert!(policy_for("whiteboards/a.edn", [t, t, t]) == Policy::WholeFile);
        assert!(policy_for("assets/a.png", [t, t, t]) == Policy::Binary);
        assert!(policy_for("logseq/bak/a.md", [t, t, t]) == Policy::Ignore);
        let bad = Some([0xff, 0xfe].as_slice());
        assert!(policy_for("pages/A.md", [t, bad, t]) == Policy::Binary);
    }

    #[test]
    fn unchanged_sides_short_circuit() {
        let env = MergeEnv::new();
        let c = ctx("pages/A.md", &env);
        let out = merge_file(&c, Some(b"- a\n"), Some(b"- a\n"), Some(b"- b\n"));
        assert_eq!(out.content.as_deref(), Some(b"- b\n".as_slice()));
        let out = merge_file(&c, Some(b"- a\n"), Some(b"- b\n"), Some(b"- a\n"));
        assert_eq!(out.content.as_deref(), Some(b"- b\n".as_slice()));
        assert!(out.conflicts.is_empty());
    }

    #[test]
    fn markdown_conflict_keeps_ours_without_markers() {
        let env = MergeEnv::new();
        let c = ctx("pages/A.md", &env);
        let out = merge_file(
            &c,
            Some(b"- Beta\n"),
            Some(b"- Beta ours\n"),
            Some(b"- Beta theirs\n"),
        );
        let text = String::from_utf8(out.content.unwrap_or_default()).unwrap_or_default();
        assert_eq!(text, "- Beta ours\n");
        assert!(!has_marker_line(&text));
        assert_eq!(out.conflicts.len(), 1);
    }

    #[test]
    fn config_and_text_use_line_merge() {
        let env = MergeEnv::new();
        let c = ctx("logseq/config.edn", &env);
        let out = merge_file(
            &c,
            Some(b"{:a 1\n :b 2\n :c 3}\n"),
            Some(b"{:a 10\n :b 2\n :c 3}\n"),
            Some(b"{:a 1\n :b 2\n :c 30}\n"),
        );
        assert_eq!(
            out.content.as_deref(),
            Some(b"{:a 10\n :b 2\n :c 30}\n".as_slice())
        );
        let out = merge_file(&c, Some(b"{:a 1}\n"), Some(b"{:a 2}\n"), Some(b"{:a 3}\n"));
        assert_eq!(out.content.as_deref(), Some(b"{:a 2}\n".as_slice()));
        assert_eq!(out.conflicts[0].0, ConflictType::Config);
    }

    #[test]
    fn blank_journal_is_unchanged() {
        assert!(is_blank_page("-\n") && is_blank_page("  \n") && is_blank_page(""));
        assert!(!is_blank_page("- hi\n"));
    }
}
