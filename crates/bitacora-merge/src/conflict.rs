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

/// A block id replaced by another during `id::` union: every `((from))` in the merge result must be
/// rewritten to `((to))`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdRewrite {
    /// The losing id.
    pub from: String,
    /// The winning id.
    pub to: String,
}
