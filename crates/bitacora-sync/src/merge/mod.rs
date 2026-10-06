//! Tree-level merge: applies the per-path policies of design `git-sync-merge` 4.4 and 5 to the
//! difference between `base`, `ours` (HEAD) and `theirs` (the remote tip).
//!
//! The planner is read-only: it reads blobs through the [`GitBackend`] and returns a
//! [`MergePlan`] (tree edits for the merge commit, work-tree changes for the writer, conflicts as
//! data). It never writes files and never produces conflict markers; conflicting regions hold
//! "ours" (BIT-SP-0006.R8).
//!
//! Submodules: [`policy`] (path dispatch), [`edn`] (`config.edn`), [`files`] (text, whiteboard and
//! binary policies), [`renames`] (titles and link fix-up) and [`markers`] (external markers).

pub mod edn;
pub mod files;
pub mod markers;
pub mod policy;
pub mod renames;

use std::collections::HashMap;

use bitacora_merge::{
    ConflictKind, MergeEnv, NoteKind, PageConflict, Side, ensure_block_ids, merge_page,
};

use crate::backend::{GitBackend, GitError, Oid, TreeChange, TreeEdit};
use crate::commit_msg::parse_message;
use crate::writer::{FileChange, is_safe_relative};
use edn::{EdnMerge, merge_config};
use files::{conflict_copy_path, device_copy_path, iso_date, merge_text_lines};
use markers::{Regions, has_marker_line, split_markers};
use policy::{Policy, policy_for};

pub use bitacora_merge::Choice as Resolution;

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
    /// `logseq/config.edn` changed incompatibly on both sides (per key, or the whole file).
    Config,
    /// Overlapping line edits in a plain-text file (css, js, ...).
    Text,
    /// A file holds conflict markers written by another tool that could not be split.
    ExternalMarkers,
}

impl ConflictType {
    /// Stable name used in `merge-state.json`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Content => "content",
            Self::Property => "property",
            Self::DeleteVsModify => "delete_vs_modify",
            Self::FileDeleteVsModify => "file_delete_vs_modify",
            Self::RenameRename => "rename_rename",
            Self::Config => "config",
            Self::Text => "text",
            Self::ExternalMarkers => "external_markers",
        }
    }

    /// Inverse of [`ConflictType::as_str`].
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "content" => Self::Content,
            "property" => Self::Property,
            "delete_vs_modify" => Self::DeleteVsModify,
            "file_delete_vs_modify" => Self::FileDeleteVsModify,
            "rename_rename" => Self::RenameRename,
            "config" => Self::Config,
            "text" => Self::Text,
            "external_markers" => Self::ExternalMarkers,
            _ => return None,
        })
    }
}

/// One conflict, ready to be persisted and shown by the resolver (BIT-US-0053/0054).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConflictRecord {
    /// Stable id within one merge (`c-0001`, ...).
    pub id: String,
    /// Graph-relative path of the (surviving) file.
    pub path: String,
    /// Kind.
    pub kind: ConflictType,
    /// Conflicting field: `content`, a property key, `SCHEDULED`/`DEADLINE`, `block`, `page`
    /// (whole page), `config` (one config key), `file` (whole file).
    pub field: Option<String>,
    /// `id::` of the block, when the conflict is block-level.
    pub block_key: Option<String>,
    /// Ancestor titles followed by the block's first line; for `config` the key path; for
    /// `rename_rename` the two titles.
    pub breadcrumb: Vec<String>,
    /// Base value (text), when available.
    pub base: Option<String>,
    /// Our value.
    pub ours: Option<String>,
    /// Their value.
    pub theirs: Option<String>,
    /// For file-level delete-vs-modify: which side deleted the file.
    pub deleted_by: Option<Side>,
    /// A suggested follow-up (`alias:: ...` for a rename/rename conflict).
    pub suggestion: Option<String>,
    /// Our commit.
    pub ours_commit: Option<String>,
    /// Their commit.
    pub theirs_commit: Option<String>,
    /// Device that made their commit.
    pub theirs_author: Option<String>,
    /// Committer time of their commit (Unix seconds).
    pub theirs_time: Option<i64>,
    /// The user's choice, once made.
    pub resolution: Option<Resolution>,
}

impl ConflictRecord {
    /// Whether the user has not decided yet.
    pub fn is_unresolved(&self) -> bool {
        self.resolution.is_none()
    }

    /// Content hashes (base, ours, theirs) used to recognise this conflict again.
    pub fn hashes(&self) -> [String; 3] {
        [
            content_hash(self.base.as_deref()),
            content_hash(self.ours.as_deref()),
            content_hash(self.theirs.as_deref()),
        ]
    }
}

/// Stable 64-bit FNV-1a hash (hex) of an optional text; persisted, so it must not change.
pub fn content_hash(s: Option<&str>) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut feed = |b: u8| {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    };
    match s {
        None => feed(0),
        Some(s) => {
            feed(1);
            s.bytes().for_each(&mut feed);
        }
    }
    format!("{h:016x}")
}

/// A resolution remembered across recomputed merges (block-level rerere, BIT-SP-0006.R21),
/// keyed by path, block key, field and the content hashes of the three sides.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoEntry {
    /// File.
    pub path: String,
    /// Conflict kind.
    pub kind: ConflictType,
    /// Block key (or config key path joined by a space).
    pub block_key: Option<String>,
    /// Field.
    pub field: Option<String>,
    /// Hash of the base value.
    pub base_hash: String,
    /// Hash of our value when the choice was made.
    pub ours_hash: String,
    /// Hash of their value.
    pub theirs_hash: String,
    /// Hash of the text that replaced the conflict (an `Edit`), when it is known.
    pub result_hash: Option<String>,
    /// The choice.
    pub resolution: Resolution,
}

impl MemoEntry {
    /// The memo of resolving `record` with `resolution`.
    pub fn of(record: &ConflictRecord, resolution: &Resolution) -> Self {
        let [base_hash, ours_hash, theirs_hash] = record.hashes();
        let result_hash = match resolution {
            Resolution::Edit(s) => Some(content_hash(Some(s))),
            Resolution::Theirs => Some(theirs_hash.clone()),
            _ => None,
        };
        Self {
            path: record.path.clone(),
            kind: record.kind,
            block_key: record.block_key.clone(),
            field: record.field.clone(),
            base_hash,
            ours_hash,
            theirs_hash,
            result_hash,
            resolution: resolution.clone(),
        }
    }

    /// Whether this entry answers `record`: same identity, same base and theirs, and our side is
    /// what it was (or already is the remembered result).
    pub fn matches(&self, record: &ConflictRecord) -> bool {
        let [b, o, t] = record.hashes();
        self.path == record.path
            && self.kind == record.kind
            && self.block_key == record.block_key
            && self.field == record.field
            && self.base_hash == b
            && self.theirs_hash == t
            && (self.ours_hash == o || self.result_hash.as_deref() == Some(o.as_str()))
    }
}

/// Where the index says a block lives (for the `id::` write of BIT-SP-0006.R14).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockLocation {
    /// Page file of the block.
    pub path: String,
    /// First line of the block's content (marker included), used to find it in the file.
    pub first_line: String,
}

/// Result of planning a merge.
#[derive(Debug, Clone, Default)]
pub struct MergePlan {
    /// Edits to apply on top of the `ours` tree to obtain the merged tree.
    pub edits: Vec<TreeEdit>,
    /// The same changes as work-tree mutations (for the graph writer).
    pub changes: Vec<FileChange>,
    /// Conflicts; the plan keeps "ours" for each of them. Conflicts answered by the memo carry
    /// their resolution already.
    pub conflicts: Vec<ConflictRecord>,
    /// Informational notes about automatic decisions.
    pub notes: Vec<String>,
    /// Paths whose content changes in the merged tree.
    pub touched: Vec<String>,
}

type BlockLocator<'a> = &'a dyn Fn(&str) -> Option<BlockLocation>;

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
    /// Index lookup of the block that carries a uuid (R14: `id::` for newly referenced blocks).
    pub locate_block: Option<BlockLocator<'a>>,
    /// Default journal template text, so a template-only journal counts as unchanged.
    pub template: Option<&'a str>,
    /// Resolutions remembered from earlier computations of this merge.
    pub memo: &'a [MemoEntry],
}

impl<'a> MergeInput<'a> {
    /// Input without index lookups, template or memo.
    pub fn new(
        backend: &'a dyn GitBackend,
        ours: &'a Oid,
        theirs: &'a Oid,
        base: Option<&'a Oid>,
        is_referenced: &'a dyn Fn(&str) -> bool,
    ) -> Self {
        Self {
            backend,
            ours,
            theirs,
            base,
            is_referenced,
            locate_block: None,
            template: None,
            memo: &[],
        }
    }
}

fn text(b: Option<&[u8]>) -> Option<String> {
    b.and_then(|b| std::str::from_utf8(b).ok())
        .map(str::to_owned)
}

/// A conflict before it gets its id and commit metadata.
#[derive(Debug, Clone, Default)]
struct Raw {
    kind: Option<ConflictType>,
    field: Option<String>,
    block_key: Option<String>,
    breadcrumb: Vec<String>,
    base: Option<String>,
    ours: Option<String>,
    theirs: Option<String>,
    deleted_by: Option<Side>,
    suggestion: Option<String>,
}

impl Raw {
    fn file(
        kind: ConflictType,
        base: Option<String>,
        ours: Option<String>,
        theirs: Option<String>,
    ) -> Self {
        Self {
            kind: Some(kind),
            field: Some("file".to_owned()),
            base,
            ours,
            theirs,
            ..Self::default()
        }
    }
}

struct FileOutcome {
    /// Final content at the result path (`None`: deleted).
    content: Option<Vec<u8>>,
    /// Extra files to create (conflict copies).
    extra: Vec<(String, Vec<u8>)>,
    conflicts: Vec<Raw>,
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

    fn text(content: String) -> Self {
        Self::take(Some(content.as_bytes()))
    }
}

struct FileCtx<'a> {
    path: &'a str,
    theirs_tag: &'a str,
    theirs_device: &'a str,
    theirs_date: &'a str,
    env: &'a MergeEnv<'a>,
}

/// What the three versions of a file look like after resolving external conflict markers.
enum Normalized {
    /// All sides usable (markers were split and merged where present).
    Ok(String, String, String, Vec<String>),
    /// Some side has markers that cannot be split.
    Malformed,
}

fn normalize_one(
    policy: Policy,
    s: &str,
    env: &MergeEnv<'_>,
    notes: &mut Vec<String>,
) -> Option<String> {
    match split_markers(s) {
        Regions::None => Some(s.to_owned()),
        Regions::Malformed => None,
        Regions::WellFormed { base, ours, theirs } => {
            let merged = if policy == Policy::Markdown {
                merge_page(&base, &ours, &theirs, env).output
            } else if policy == Policy::Config {
                match merge_config(&base, &ours, &theirs) {
                    EdnMerge::Merged { text, conflicts } if conflicts.is_empty() => text,
                    _ => return None,
                }
            } else {
                let m = merge_text_lines(&base, &ours, &theirs);
                if m.conflict {
                    return None;
                }
                m.text
            };
            if has_marker_line(&merged) {
                return None;
            }
            notes.push("conflict markers written by another tool were merged".to_owned());
            Some(merged)
        }
    }
}

fn normalize_markers(policy: Policy, b: &str, o: &str, t: &str, env: &MergeEnv<'_>) -> Normalized {
    let mut notes = Vec::new();
    match (
        normalize_one(policy, b, env, &mut notes),
        normalize_one(policy, o, env, &mut notes),
        normalize_one(policy, t, env, &mut notes),
    ) {
        (Some(b), Some(o), Some(t)) => Normalized::Ok(b, o, t, notes),
        _ => Normalized::Malformed,
    }
}

/// Taking their side verbatim, except that marker regions written by another tool are split and
/// merged first (R8: markers never reach the work tree through a merge).
fn sanitize_taken(
    ctx: &FileCtx<'_>,
    base: Option<&[u8]>,
    ours: Option<&[u8]>,
    theirs: Option<&[u8]>,
) -> FileOutcome {
    let Some(t) = theirs else {
        return FileOutcome::take(theirs);
    };
    let policy = policy_for(ctx.path, [base, ours, theirs]);
    let marked = matches!(
        policy,
        Policy::Markdown | Policy::TextLine | Policy::Config | Policy::WholeFile
    ) && std::str::from_utf8(t).is_ok_and(has_marker_line);
    if !marked {
        return FileOutcome::take(theirs);
    }
    let ts = text(Some(t)).unwrap_or_default();
    let mut notes = Vec::new();
    match normalize_one(policy, &ts, ctx.env, &mut notes) {
        Some(clean) => {
            let mut out = FileOutcome::text(clean);
            out.notes = notes;
            out
        }
        None => {
            let mut out = FileOutcome::take(ours);
            out.conflicts.push(Raw::file(
                ConflictType::ExternalMarkers,
                text(base),
                text(ours),
                Some(ts),
            ));
            out
        }
    }
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
        return sanitize_taken(ctx, base, ours, theirs);
    }
    // Both sides changed differently.
    let policy = policy_for(ctx.path, [base, ours, theirs]);
    if policy == Policy::Ignore {
        return FileOutcome::take(ours);
    }
    let (o, t) = match (ours, theirs) {
        (Some(o), Some(t)) => (o, t),
        // Deleted on one side, modified on the other. The default is "restore": the modified
        // file survives and the conflict stays open until the user confirms or deletes it.
        (None, Some(t)) => {
            let mut out = FileOutcome::take(Some(t));
            let mut raw = Raw::file(
                ConflictType::FileDeleteVsModify,
                text(base),
                None,
                text(Some(t)),
            );
            raw.deleted_by = Some(Side::Ours);
            out.conflicts.push(raw);
            out.notes.push(format!(
                "`{}` was deleted here but edited remotely: it was restored with their changes",
                ctx.path
            ));
            return out;
        }
        (Some(o), None) => {
            let mut out = FileOutcome::take(Some(o));
            let mut raw = Raw::file(
                ConflictType::FileDeleteVsModify,
                text(base),
                text(Some(o)),
                None,
            );
            raw.deleted_by = Some(Side::Theirs);
            out.conflicts.push(raw);
            return out;
        }
        (None, None) => return FileOutcome::take(None),
    };
    match policy {
        Policy::Markdown => merge_markdown(ctx, base, o, t),
        Policy::TextLine => merge_text_policy(ctx, base, o, t),
        Policy::Config => merge_config_policy(ctx, base, o, t),
        Policy::WholeFile => {
            // One side changed: handled above. Both changed: keep ours, save theirs next to it.
            let mut out = FileOutcome::take(Some(o));
            let copy = device_copy_path(ctx.path, ctx.theirs_device, ctx.theirs_date);
            out.notes.push(format!(
                "`{}` changed on both sides; their version was saved as `{copy}`",
                ctx.path
            ));
            out.extra.push((copy, t.to_vec()));
            out
        }
        Policy::Binary => {
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

fn utf8_triple(base: Option<&[u8]>, o: &[u8], t: &[u8]) -> (String, String, String) {
    (
        text(base).unwrap_or_default(),
        text(Some(o)).unwrap_or_default(),
        text(Some(t)).unwrap_or_default(),
    )
}

fn external_markers(o: &[u8], base: Option<&[u8]>, t: &[u8]) -> FileOutcome {
    let mut out = FileOutcome::take(Some(o));
    out.conflicts.push(Raw::file(
        ConflictType::ExternalMarkers,
        text(base),
        text(Some(o)),
        text(Some(t)),
    ));
    out
}

fn merge_text_policy(ctx: &FileCtx<'_>, base: Option<&[u8]>, o: &[u8], t: &[u8]) -> FileOutcome {
    let (b, os, ts) = utf8_triple(base, o, t);
    let (b, os, ts, mut notes) = match normalize_markers(Policy::TextLine, &b, &os, &ts, ctx.env) {
        Normalized::Ok(b, o, t, n) => (b, o, t, n),
        Normalized::Malformed => return external_markers(o, base, t),
    };
    let m = merge_text_lines(&b, &os, &ts);
    if m.conflict {
        let mut out = FileOutcome::take(Some(o));
        out.conflicts
            .push(Raw::file(ConflictType::Text, Some(b), Some(os), Some(ts)));
        return out;
    }
    notes.push(format!("`{}` merged line by line", ctx.path));
    let mut out = FileOutcome::text(m.text);
    out.notes = notes;
    out
}

fn merge_config_policy(ctx: &FileCtx<'_>, base: Option<&[u8]>, o: &[u8], t: &[u8]) -> FileOutcome {
    let (b, os, ts) = utf8_triple(base, o, t);
    if has_marker_line(&b) || has_marker_line(&os) || has_marker_line(&ts) {
        return external_markers(o, base, t);
    }
    // Line endings: compare and edit in ours' style.
    match merge_config(&b, &os, &ts) {
        EdnMerge::Unparsable => {
            let mut out = FileOutcome::take(Some(o));
            out.conflicts
                .push(Raw::file(ConflictType::Config, Some(b), Some(os), Some(ts)));
            out
        }
        EdnMerge::Merged { text, conflicts } => {
            let mut out = FileOutcome::text(text);
            for c in conflicts {
                out.conflicts.push(Raw {
                    kind: Some(ConflictType::Config),
                    field: Some("config".to_owned()),
                    block_key: Some(c.path.join(" ")),
                    breadcrumb: c.path,
                    base: c.base,
                    ours: c.ours,
                    theirs: c.theirs,
                    ..Raw::default()
                });
            }
            out.notes.push(format!("`{}` merged key by key", ctx.path));
            out
        }
    }
}

fn merge_markdown(ctx: &FileCtx<'_>, base: Option<&[u8]>, o: &[u8], t: &[u8]) -> FileOutcome {
    let (b, os, ts) = utf8_triple(base, o, t);
    let (b, os, ts, mut notes) = match normalize_markers(Policy::Markdown, &b, &os, &ts, ctx.env) {
        Normalized::Ok(b, o, t, n) => (b, o, t, n),
        Normalized::Malformed => return external_markers(o, base, t),
    };
    let r = merge_page(&b, &os, &ts, ctx.env);
    if has_marker_line(&r.output) && !has_marker_line(&os) {
        // Defensive: never let marker lines reach a graph file.
        return external_markers(o, base, t);
    }
    let mut out = FileOutcome::text(r.output);
    for c in r.conflicts {
        out.conflicts.push(raw_from_page(c));
    }
    for n in r.notes {
        let label = match n.kind {
            NoteKind::IdRewritten => "ids",
            _ => "structure",
        };
        notes.push(format!("`{}` ({label}): {}", ctx.path, n.message));
    }
    out.notes = notes;
    out
}

fn raw_from_page(c: PageConflict) -> Raw {
    let kind = match c.conflict.kind {
        ConflictKind::Content => ConflictType::Content,
        ConflictKind::Property => ConflictType::Property,
        ConflictKind::DeleteVsModify => ConflictType::DeleteVsModify,
    };
    Raw {
        kind: Some(kind),
        field: Some(c.conflict.field),
        block_key: c.block_key,
        breadcrumb: c.breadcrumb,
        base: c.conflict.base,
        ours: c.conflict.ours,
        theirs: c.conflict.theirs,
        ..Raw::default()
    }
}

/// Result of [`repair_external_markers`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarkerRepair {
    /// The text has no marker lines.
    Clean,
    /// Regions were split and merged: `text` is clean, `conflicts` the remaining ones.
    Repaired {
        /// The marker-free text.
        text: String,
        /// Conflicts that still need the user (no ids yet).
        conflicts: Vec<ConflictRecord>,
    },
    /// Markers present but unbalanced: the file is left untouched.
    Malformed,
}

/// Splits well-formed marker regions of a work-tree file into base/ours/theirs, merges them with
/// the file's own policy and returns the clean text (R8). Never returns marker lines.
pub fn repair_external_markers(path: &str, content: &str, env: &MergeEnv<'_>) -> MarkerRepair {
    if !has_marker_line(content) {
        return MarkerRepair::Clean;
    }
    let policy = policy_for(path, [Some(content.as_bytes()), None, None]);
    let Regions::WellFormed { base, ours, theirs } = split_markers(content) else {
        return MarkerRepair::Malformed;
    };
    let ctx = FileCtx {
        path,
        theirs_tag: "external",
        theirs_device: "external",
        theirs_date: "",
        env,
    };
    let outcome = match policy {
        Policy::Markdown => merge_markdown(
            &ctx,
            Some(base.as_bytes()),
            ours.as_bytes(),
            theirs.as_bytes(),
        ),
        Policy::TextLine => merge_text_policy(
            &ctx,
            Some(base.as_bytes()),
            ours.as_bytes(),
            theirs.as_bytes(),
        ),
        _ => return MarkerRepair::Malformed,
    };
    let Some(Ok(clean)) = outcome.content.map(String::from_utf8) else {
        return MarkerRepair::Malformed;
    };
    if has_marker_line(&clean) {
        return MarkerRepair::Malformed;
    }
    let conflicts = outcome
        .conflicts
        .into_iter()
        .filter(|r| r.kind != Some(ConflictType::ExternalMarkers))
        .map(|r| record_of(path, r, &CommitMeta::default()))
        .collect();
    MarkerRepair::Repaired {
        text: clean,
        conflicts,
    }
}

#[derive(Default)]
struct CommitMeta {
    ours: Option<String>,
    theirs: Option<String>,
    author: Option<String>,
    time: Option<i64>,
}

fn record_of(path: &str, raw: Raw, meta: &CommitMeta) -> ConflictRecord {
    ConflictRecord {
        id: String::new(),
        path: path.to_owned(),
        kind: raw.kind.unwrap_or(ConflictType::Content),
        field: raw.field,
        block_key: raw.block_key,
        breadcrumb: raw.breadcrumb,
        base: raw.base,
        ours: raw.ours,
        theirs: raw.theirs,
        deleted_by: raw.deleted_by,
        suggestion: raw.suggestion,
        ours_commit: meta.ours.clone(),
        theirs_commit: meta.theirs.clone(),
        theirs_author: meta.author.clone(),
        theirs_time: meta.time,
        resolution: None,
    }
}

/// A page renamed on one side only: links to the old title must follow.
struct RenamedPage {
    old: String,
    new: String,
    by_ours: bool,
}

/// Plans the merge of `theirs` into `ours`. Pure with respect to the repository and work tree.
pub fn plan_merge(input: &MergeInput<'_>) -> Result<MergePlan, GitError> {
    let backend = input.backend;
    let empty = Oid::empty_tree();
    let base_tree = input.base.unwrap_or(&empty);
    let ours_changes = backend.diff_trees(base_tree, input.ours)?;
    let theirs_changes = backend.diff_trees(base_tree, input.theirs)?;

    let mut ours_renames: HashMap<&str, &str> = HashMap::new();
    let mut theirs_renames: HashMap<&str, &str> = HashMap::new();
    for c in &ours_changes {
        if let TreeChange::Renamed { from, to, .. } = c {
            ours_renames.insert(from, to);
        }
    }
    for c in &theirs_changes {
        if let TreeChange::Renamed { from, to, .. } = c {
            theirs_renames.insert(from, to);
        }
    }

    let blob = |commit: &Oid, path: &str| backend.read_blob(commit, path);
    let base_blob = |path: &str| -> Result<Option<Vec<u8>>, GitError> {
        match input.base {
            Some(b) => blob(b, path),
            None => Ok(None),
        }
    };

    // The newer writer wins last-writer-wins metadata; their device and date name conflict copies.
    let ours_info = backend.commit_info(input.ours)?;
    let theirs_info = backend.commit_info(input.theirs)?;
    let theirs_device = parse_message(&theirs_info.message)
        .device
        .unwrap_or_else(|| "remote".to_owned());
    let theirs_date = iso_date(theirs_info.committer_time);
    let meta = CommitMeta {
        ours: Some(input.ours.as_hex().to_owned()),
        theirs: Some(input.theirs.as_hex().to_owned()),
        author: Some(theirs_device.clone()),
        time: Some(theirs_info.committer_time),
    };
    let env = MergeEnv {
        is_referenced: input.is_referenced,
        prefer: if theirs_info.committer_time > ours_info.committer_time {
            Side::Theirs
        } else {
            Side::Ours
        },
        template: input.template,
        ..MergeEnv::new()
    };
    let tag: String = input.theirs.as_hex().chars().take(7).collect();
    // Page titles of renamed files follow the graph's `:file/name-format`.
    let config_text =
        blob(input.ours, "logseq/config.edn")?.and_then(|b| String::from_utf8(b).ok());
    let title_cfg = bitacora_config::EffectiveConfig::from_texts(None, config_text.as_deref());

    let mut plan = MergePlan::default();
    let mut raws: Vec<(String, Raw)> = Vec::new();
    let mut renamed: Vec<RenamedPage> = Vec::new();
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
        if let TreeChange::Renamed { from, to, .. } = change {
            match ours_renames.get(from.as_str()) {
                Some(o) if *o != to.as_str() => {
                    let ours_title = renames::page_title(
                        &ours_path,
                        &text(ours_content.as_deref()).unwrap_or_default(),
                        &title_cfg,
                    );
                    let theirs_title = renames::page_title(
                        to,
                        &text(theirs_content.as_deref()).unwrap_or_default(),
                        &title_cfg,
                    );
                    raws.push((
                        result_path.clone(),
                        Raw {
                            kind: Some(ConflictType::RenameRename),
                            field: Some("file".to_owned()),
                            breadcrumb: vec![ours_title, theirs_title.clone()],
                            base: Some(from.clone()),
                            ours: Some(ours_path.clone()),
                            theirs: Some(to.clone()),
                            suggestion: Some(format!("alias:: [[{theirs_title}]]")),
                            ..Raw::default()
                        },
                    ));
                }
                Some(_) => {} // the same rename on both sides
                None if renames::is_page_path(from) && renames::is_page_path(to) => {
                    let old = renames::page_title(
                        from,
                        &text(base_content.as_deref()).unwrap_or_default(),
                        &title_cfg,
                    );
                    let new = renames::page_title(
                        to,
                        &text(theirs_content.as_deref()).unwrap_or_default(),
                        &title_cfg,
                    );
                    if old != new {
                        renamed.push(RenamedPage {
                            old,
                            new,
                            by_ours: false,
                        });
                    }
                }
                None => {}
            }
        }
        let ctx = FileCtx {
            path: &result_path,
            theirs_tag: &tag,
            theirs_device: &theirs_device,
            theirs_date: &theirs_date,
            env: &env,
        };
        let outcome = merge_file(
            &ctx,
            base_content.as_deref(),
            ours_content.as_deref(),
            theirs_content.as_deref(),
        );
        for raw in outcome.conflicts {
            raws.push((result_path.clone(), raw));
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

    // Pages renamed on our side only: remember the title change for the link fix-up.
    for c in &ours_changes {
        if let TreeChange::Renamed { from, to, .. } = c
            && !theirs_renames.contains_key(from.as_str())
            && renames::is_page_path(from)
            && renames::is_page_path(to)
        {
            let old = renames::page_title(
                from,
                &text(base_blob(from)?.as_deref()).unwrap_or_default(),
                &title_cfg,
            );
            let new = renames::page_title(
                to,
                &text(blob(input.ours, to)?.as_deref()).unwrap_or_default(),
                &title_cfg,
            );
            if old != new {
                renamed.push(RenamedPage {
                    old,
                    new,
                    by_ours: true,
                });
            }
        }
    }

    // Collapse to the last decision per path.
    let mut order: Vec<String> = Vec::new();
    let mut last: HashMap<String, Option<Vec<u8>>> = HashMap::new();
    for (p, c) in finals {
        if !last.contains_key(&p) {
            order.push(p.clone());
        }
        last.insert(p, c);
    }
    let view =
        |last: &HashMap<String, Option<Vec<u8>>>, p: &str| -> Result<Option<Vec<u8>>, GitError> {
            match last.get(p) {
                Some(c) => Ok(c.clone()),
                None => blob(input.ours, p),
            }
        };
    let utf8 = |b: Option<Vec<u8>>| b.and_then(|b| String::from_utf8(b).ok());

    // Post-merge fix-up: links to a renamed page that the other side introduced.
    let mut candidates: Vec<String> = order
        .iter()
        .filter(|p| p.ends_with(".md") && last.get(*p).is_some_and(Option::is_some))
        .cloned()
        .collect();
    for c in &ours_changes {
        let p = match c {
            TreeChange::Added { path } | TreeChange::Modified { path } => path,
            TreeChange::Renamed { to, .. } => to,
            TreeChange::Deleted { .. } => continue,
        };
        if p.ends_with(".md") && !candidates.contains(p) {
            candidates.push(p.clone());
        }
    }
    for r in &renamed {
        for p in &candidates {
            // Our renames only matter for what their side brought; theirs for what ours brought.
            let relevant = if r.by_ours {
                last.contains_key(p)
            } else {
                true
            };
            if !relevant {
                continue;
            }
            let Some(current) = utf8(view(&last, p)?) else {
                continue;
            };
            if let Some(fixed) = renames::rewrite_links(&current, &r.old, &r.new) {
                if !last.contains_key(p) {
                    order.push(p.clone());
                }
                last.insert(p.clone(), Some(fixed.into_bytes()));
                plan.notes.push(format!(
                    "links to [[{}]] were rewritten to [[{}]] in `{p}`",
                    r.old, r.new
                ));
            }
        }
    }

    // R14: write `id::` for blocks the merge result newly references.
    if let Some(locate) = input.locate_block {
        let mut wants: HashMap<String, Vec<(String, String)>> = HashMap::new();
        for p in order.clone() {
            if !p.ends_with(".md") {
                continue;
            }
            let Some(merged) = utf8(last.get(&p).cloned().flatten()) else {
                continue;
            };
            let before = utf8(blob(input.ours, &p)?).unwrap_or_default();
            let known = renames::block_refs(&before);
            for uuid in renames::block_refs(&merged) {
                if known.contains(&uuid) {
                    continue;
                }
                if let Some(loc) = locate(&uuid) {
                    wants
                        .entry(loc.path)
                        .or_default()
                        .push((uuid, loc.first_line));
                }
            }
        }
        let mut paths: Vec<_> = wants.into_iter().collect();
        paths.sort();
        for (p, w) in paths {
            if !p.ends_with(".md") || !is_safe_relative(&p) {
                continue;
            }
            let Some(current) = utf8(view(&last, &p)?) else {
                continue;
            };
            if let Some((fixed, ids)) = ensure_block_ids(&current, &w) {
                if !last.contains_key(&p) {
                    order.push(p.clone());
                }
                last.insert(p.clone(), Some(fixed.into_bytes()));
                plan.notes.push(format!(
                    "`id::` written for referenced blocks in `{p}`: {}",
                    ids.join(", ")
                ));
            }
        }
    }

    // Records: ids, commit metadata, remembered resolutions.
    for (i, (path, raw)) in raws.into_iter().enumerate() {
        let mut rec = record_of(&path, raw, &meta);
        rec.id = format!("c-{:04}", i + 1);
        if let Some(m) = input.memo.iter().find(|m| m.matches(&rec)) {
            rec.resolution = Some(m.resolution.clone());
        }
        plan.conflicts.push(rec);
    }

    // Removals first so renames never collide (this also orders case-only renames safely).
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
            theirs_device: "phone",
            theirs_date: "2026-10-06",
            env,
        }
    }

    fn content(o: &FileOutcome) -> String {
        String::from_utf8(o.content.clone().unwrap_or_default()).unwrap_or_default()
    }

    #[test]
    fn unchanged_sides_short_circuit() {
        let env = MergeEnv::new();
        let c = ctx("pages/A.md", &env);
        let out = merge_file(&c, Some(b"- a\n"), Some(b"- a\n"), Some(b"- b\n"));
        assert_eq!(content(&out), "- b\n");
        let out = merge_file(&c, Some(b"- a\n"), Some(b"- b\n"), Some(b"- a\n"));
        assert_eq!(content(&out), "- b\n");
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
        assert_eq!(content(&out), "- Beta ours\n");
        assert!(!has_marker_line(&content(&out)));
        assert_eq!(out.conflicts.len(), 1);
        assert_eq!(out.conflicts[0].kind, Some(ConflictType::Content));
        assert_eq!(out.conflicts[0].field.as_deref(), Some("content"));
    }

    #[test]
    fn config_merges_per_key_and_reports_same_key_conflicts() {
        let env = MergeEnv::new();
        let c = ctx("logseq/config.edn", &env);
        let out = merge_file(
            &c,
            Some(b"{:a 1\n :b 2\n :c 3}\n"),
            Some(b";; mine\n{:a 10\n :b 2\n :c 3}\n"),
            Some(b"{:a 1\n :b 2\n :c 30}\n"),
        );
        assert_eq!(content(&out), ";; mine\n{:a 10\n :b 2\n :c 30}\n");
        assert!(out.conflicts.is_empty());
        let out = merge_file(&c, Some(b"{:a 1}\n"), Some(b"{:a 2}\n"), Some(b"{:a 3}\n"));
        assert_eq!(content(&out), "{:a 2}\n");
        assert_eq!(out.conflicts[0].kind, Some(ConflictType::Config));
        assert_eq!(out.conflicts[0].breadcrumb, vec!["a".to_owned()]);
        assert_eq!(out.conflicts[0].theirs.as_deref(), Some("3"));
        // An unparsable side conflicts as a whole file, keeping ours.
        let out = merge_file(&c, Some(b"{:a 1}\n"), Some(b"{:a 2}\n"), Some(b"{:a "));
        assert_eq!(content(&out), "{:a 2}\n");
        assert_eq!(out.conflicts[0].field.as_deref(), Some("file"));
    }

    #[test]
    fn text_files_use_line_diff3() {
        let env = MergeEnv::new();
        let c = ctx("logseq/custom.css", &env);
        let out = merge_file(
            &c,
            Some(b"a\nb\nc\n"),
            Some(b"A\nb\nc\n"),
            Some(b"a\nb\nC\n"),
        );
        assert_eq!(content(&out), "A\nb\nC\n");
        let out = merge_file(&c, Some(b"a\n"), Some(b"b\n"), Some(b"c\n"));
        assert_eq!(content(&out), "b\n");
        assert_eq!(out.conflicts[0].kind, Some(ConflictType::Text));
    }

    #[test]
    fn whiteboard_and_binary_collisions_keep_both() {
        let env = MergeEnv::new();
        let out = merge_file(
            &ctx("whiteboards/w.edn", &env),
            Some(b"{:a 1}"),
            Some(b"{:a 2}"),
            Some(b"{:a 3}"),
        );
        assert_eq!(content(&out), "{:a 2}");
        assert_eq!(
            out.extra[0].0,
            "whiteboards/w (conflict phone 2026-10-06).edn"
        );
        assert_eq!(out.extra[0].1, b"{:a 3}");
        let out = merge_file(
            &ctx("assets/diagram_1696000000.png", &env),
            Some(b"0"),
            Some(b"1"),
            Some(b"2"),
        );
        assert_eq!(
            out.extra[0].0,
            "assets/diagram_1696000000 (conflict-abc1234).png"
        );
        assert!(out.conflicts.is_empty());
        // One side changed: taken without a copy.
        let out = merge_file(
            &ctx("assets/a.png", &env),
            Some(b"0"),
            Some(b"0"),
            Some(b"2"),
        );
        assert_eq!(out.content.as_deref(), Some(b"2".as_slice()));
        assert!(out.extra.is_empty());
    }

    #[test]
    fn delete_versus_modify_restores_the_modified_file() {
        let env = MergeEnv::new();
        let c = ctx("pages/Q.md", &env);
        // We deleted, they modified: restored.
        let out = merge_file(&c, Some(b"- q\n"), None, Some(b"- q2\n"));
        assert_eq!(content(&out), "- q2\n");
        assert_eq!(
            out.conflicts[0].kind,
            Some(ConflictType::FileDeleteVsModify)
        );
        assert_eq!(out.conflicts[0].deleted_by, Some(Side::Ours));
        // They deleted, we modified: ours stays.
        let out = merge_file(&c, Some(b"- q\n"), Some(b"- q2\n"), None);
        assert_eq!(content(&out), "- q2\n");
        assert_eq!(out.conflicts[0].deleted_by, Some(Side::Theirs));
        // Deleted and untouched on the other side: plain deletion.
        let out = merge_file(&c, Some(b"- q\n"), Some(b"- q\n"), None);
        assert!(out.content.is_none() && out.conflicts.is_empty());
    }

    #[test]
    fn external_markers_are_split_and_never_survive() {
        let env = MergeEnv::new();
        let c = ctx("pages/A.md", &env);
        let marked = b"- shared\n<<<<<<< HEAD\n- mine\n=======\n- theirs\n>>>>>>> origin/main\n";
        let out = merge_file(
            &c,
            Some(b"- shared\n"),
            Some(b"- shared\n- ours\n"),
            Some(marked),
        );
        let text = content(&out);
        assert!(!has_marker_line(&text), "{text}");
        assert!(
            text.contains("- mine") && text.contains("- theirs") && text.contains("- ours"),
            "{text}"
        );
        // Unbalanced markers become an external_markers conflict and ours is kept.
        let out = merge_file(
            &c,
            Some(b"- a\n"),
            Some(b"- b\n"),
            Some(b"- a\n<<<<<<< x\n- c\n"),
        );
        assert_eq!(content(&out), "- b\n");
        assert_eq!(out.conflicts[0].kind, Some(ConflictType::ExternalMarkers));
    }

    #[test]
    fn repair_splits_work_tree_regions() {
        let env = MergeEnv::new();
        let text = "- shared\n<<<<<<< HEAD\n- mine\n=======\n- theirs\n>>>>>>> origin/main\n";
        let MarkerRepair::Repaired { text, conflicts } =
            repair_external_markers("pages/P.md", text, &env)
        else {
            panic!("not repaired");
        };
        assert_eq!(text, "- shared\n- mine\n- theirs\n");
        assert!(conflicts.is_empty());
        assert_eq!(
            repair_external_markers("pages/P.md", "- ok\n", &env),
            MarkerRepair::Clean
        );
        assert_eq!(
            repair_external_markers("pages/P.md", "<<<<<<< x\n- a\n", &env),
            MarkerRepair::Malformed
        );
        assert_eq!(
            repair_external_markers("assets/a.png", "<<<<<<< x\n=======\n>>>>>>> y\n", &env),
            MarkerRepair::Malformed
        );
    }

    mod no_markers {
        use super::*;
        use proptest::prelude::*;

        const LINES: [&str; 12] = [
            "- a",
            "- b",
            "  - child",
            "  x:: 1",
            "<<<<<<< HEAD",
            "=======",
            ">>>>>>> origin/main",
            "||||||| base",
            "- [[Old]]",
            "a {}",
            "{:a 1}",
            "",
        ];
        const PATHS: [&str; 5] = [
            "pages/P.md",
            "journals/2026_10_06.md",
            "logseq/custom.css",
            "logseq/config.edn",
            "whiteboards/w.edn",
        ];

        fn text() -> impl Strategy<Value = String> {
            proptest::collection::vec(0..LINES.len(), 0..7).prop_map(|ix| {
                let mut s = String::new();
                for i in ix {
                    s.push_str(LINES[i]);
                    s.push('\n');
                }
                s
            })
        }

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(400))]
            /// BIT-SP-0006.R8: a merge never introduces marker lines, whatever the inputs look like
            /// (including inputs that already carry markers from another tool).
            #[test]
            fn merge_output_never_gains_marker_lines(
                path in 0..PATHS.len(),
                b in text(),
                o in text(),
                t in text(),
            ) {
                let env = MergeEnv::new();
                let c = ctx(PATHS[path], &env);
                let out = merge_file(&c, Some(b.as_bytes()), Some(o.as_bytes()), Some(t.as_bytes()));
                let merged = String::from_utf8(out.content.clone().unwrap_or_default()).unwrap_or_default();
                prop_assert!(
                    !has_marker_line(&merged) || merged == o,
                    "markers in output of {}: {merged:?} (b={b:?} o={o:?} t={t:?})", PATHS[path]
                );
                // Work-tree repair of a file with markers is also marker-free.
                if let MarkerRepair::Repaired { text, .. } = repair_external_markers(PATHS[path], &o, &env) {
                    prop_assert!(!has_marker_line(&text), "{text:?}");
                }
            }
        }
    }

    #[test]
    fn memo_matches_by_hashes() {
        let rec = ConflictRecord {
            id: "c-0001".into(),
            path: "pages/P.md".into(),
            kind: ConflictType::Content,
            field: Some("content".into()),
            block_key: Some("u1".into()),
            breadcrumb: vec![],
            base: Some("b".into()),
            ours: Some("o".into()),
            theirs: Some("t".into()),
            deleted_by: None,
            suggestion: None,
            ours_commit: None,
            theirs_commit: None,
            theirs_author: None,
            theirs_time: None,
            resolution: None,
        };
        let memo = MemoEntry::of(&rec, &Resolution::Edit("both".into()));
        assert!(memo.matches(&rec));
        // Ours became the edited text: still the same conflict.
        let mut edited = rec.clone();
        edited.ours = Some("both".into());
        assert!(memo.matches(&edited));
        // Theirs changed again: the conflict reappears.
        let mut again = rec.clone();
        again.theirs = Some("t2".into());
        assert!(!memo.matches(&again));
    }
}
