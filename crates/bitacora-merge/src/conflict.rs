//! The conflict model produced by field-level merging (`docs/design/git-sync-merge.md` §4.5).

/// What kind of conflict a field produced. Metadata never conflicts (ADR-009).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConflictKind {
    /// Overlapping edits of the block text (or a marker/priority collision).
    Content,
    /// The same user property, `SCHEDULED:` or `DEADLINE:` changed differently on both sides.
    Property,
    /// One side deleted the block, the other modified it (raised by the structural merge).
    DeleteVsModify,
}

/// One unresolved field of a block. The merged output holds the **ours** value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    /// Kind of conflict.
    pub kind: ConflictKind,
    /// `content`, the normalised property key, or `SCHEDULED` / `DEADLINE`.
    pub field: String,
    /// Base value (`None`: absent in base).
    pub base: Option<String>,
    /// Our value (`None`: removed on our side).
    pub ours: Option<String>,
    /// Their value (`None`: removed on their side).
    pub theirs: Option<String>,
}

/// A conflict located in a page: the field conflict plus where the block lives, for the UI and
/// the persisted merge state (`docs/design/git-sync-merge.md` §4.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageConflict {
    /// `id::` of the block, or `None` for blocks without a usable id (see `breadcrumb`).
    pub block_key: Option<String>,
    /// Titles of the ancestors (outermost first) followed by the block's own first line.
    pub breadcrumb: Vec<String>,
    /// The conflicting field. The merge output holds the ours value (or the surviving block).
    pub conflict: Conflict,
}

/// What an informational note is about. Notes are never conflicts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NoteKind {
    /// Both sides moved the block to different places: ours was taken.
    CompetingMove,
    /// Both sides reordered siblings differently: ours was taken.
    CompetingReorder,
    /// The parent of a new or modified block was deleted: it was re-attached to an ancestor.
    Reattached,
    /// A move would have created a cycle: the block was put at the top level.
    CycleBroken,
    /// `((from))` references were rewritten to `((to))` after an `id::` union.
    IdRewritten,
}

/// An informational note about an automatic decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    /// What happened.
    pub kind: NoteKind,
    /// Titles of the block's ancestors followed by its first line.
    pub breadcrumb: Vec<String>,
    /// Human-readable text.
    pub message: String,
}

/// A block id replaced by another during `id::` union: every `((from))` in the merge result must be
/// rewritten to `((to))`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdRewrite {
    /// The losing id.
    pub from: String,
    /// The winning id.
    pub to: String,
}
