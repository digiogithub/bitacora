//! Transactions: atomic groups of ops with rollback and invariant checking
//! (`docs/design/block-editor.md` §3.3).

use std::collections::BTreeSet;
use std::time::Instant;

use uuid::Uuid;

use super::cmd::{Cmd, Refusal, plan_full};
use super::model::BlockId;
use super::op::{Op, OpError};
use super::workspace::Workspace;
use crate::graph::PageKey;

/// Identity of a committed transaction (monotonic per workspace).
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TxId(pub u64);

/// Editor cursor, restored by undo/redo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CursorState {
    /// Block holding the cursor.
    pub block: BlockId,
    /// Selection as byte offsets in the block text.
    pub selection: std::ops::Range<usize>,
}

/// Groups consecutive typing into one undo step (used by the history stack).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoalesceKey {
    /// Block being typed in.
    pub block: BlockId,
}

/// A committed group of applied ops (captured data filled in).
#[derive(Debug, Clone)]
pub struct Transaction {
    /// Identity.
    pub id: TxId,
    /// Human label ("Indent", "Typing"...).
    pub label: &'static str,
    /// The applied ops, in order.
    pub ops: Vec<Op>,
    /// Pages touched (they are dirty and go to the write queue).
    pub pages: Vec<PageKey>,
    /// Cursor before the transaction.
    pub cursor_before: Option<CursorState>,
    /// Cursor after the transaction.
    pub cursor_after: Option<CursorState>,
    /// Commit time.
    pub at: Instant,
    /// Typing-run key, when the transaction may be merged with its neighbours.
    pub coalesce: Option<CoalesceKey>,
}

impl Transaction {
    /// The ops that undo this transaction: inverses in reverse order.
    ///
    /// # Errors
    /// [`OpError::NotApplied`] when an op lacks captured data.
    pub fn inverse_ops(&self) -> Result<Vec<Op>, OpError> {
        self.ops.iter().rev().map(Op::inverse).collect()
    }
}

/// A tree invariant that does not hold.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum InvariantError {
    /// A block is missing from its parent's children, or listed twice, or unreachable (cycle).
    #[error("page {page:?}: tree is inconsistent ({what})")]
    Tree {
        /// Page.
        page: PageKey,
        /// What is wrong.
        what: &'static str,
    },
    /// The transaction made an `id::` uuid appear on two blocks.
    #[error("block uuid {0} would be used twice")]
    DuplicateUuid(Uuid),
}

/// Why a commit failed. The workspace is back to its pre-commit state except for
/// [`CommitError::RollbackFailed`].
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CommitError {
    /// An op failed to apply.
    #[error("op #{index} failed: {source}")]
    Op {
        /// Index of the failing op.
        index: usize,
        /// The cause.
        source: OpError,
    },
    /// The command was refused by its planner.
    #[error("refused: {0}")]
    Refused(#[from] Refusal),
    /// The result violated an invariant and was rolled back.
    #[error(transparent)]
    Invariant(#[from] InvariantError),
    /// Rolling back failed: the in-memory state may be inconsistent; reload the pages.
    #[error("rollback failed: {0}")]
    RollbackFailed(OpError),
}

impl Workspace {
    /// Applies `ops` atomically: all or nothing, with invariants checked on the touched pages.
    ///
    /// # Errors
    /// [`CommitError`]; on `Op` and `Invariant` errors the applied prefix has been rolled back.
    pub fn commit(
        &mut self,
        label: &'static str,
        ops: Vec<Op>,
    ) -> Result<Transaction, CommitError> {
        self.begin_tx();
        let mut applied: Vec<Op> = Vec::with_capacity(ops.len());
        for (index, mut op) in ops.into_iter().enumerate() {
            match op.apply(self) {
                Ok(()) => applied.push(op),
                Err(source) => {
                    self.rollback(&applied)?;
                    return Err(CommitError::Op { index, source });
                }
            }
        }
        if let Err(e) = self.check_invariants() {
            self.rollback(&applied)?;
            return Err(e.into());
        }
        self.next_tx += 1;
        Ok(Transaction {
            id: TxId(self.next_tx),
            label,
            ops: applied,
            pages: self.touched.iter().cloned().collect(),
            cursor_before: None,
            cursor_after: None,
            at: Instant::now(),
            coalesce: None,
        })
    }

    /// Plans `cmd` (pure) and commits the resulting ops.
    ///
    /// # Errors
    /// [`CommitError::Refused`] when the planner refuses; otherwise as [`Workspace::commit`].
    pub fn run(&mut self, label: &'static str, cmd: &Cmd) -> Result<Transaction, CommitError> {
        let planned = plan_full(self, cmd)?;
        let before = cursor_before(self, cmd);
        let mut tx = self.commit(label, planned.ops)?;
        tx.cursor_before = before;
        tx.cursor_after = planned.cursor_after;
        tx.coalesce = match cmd {
            Cmd::EditText { id, .. } => Some(CoalesceKey { block: *id }),
            _ => None,
        };
        Ok(tx)
    }

    /// Commits the inverse of `tx` (one undo step). Redo is `undo` of the returned transaction.
    ///
    /// # Errors
    /// As [`Workspace::commit`], plus [`CommitError::Op`] when the graph moved on and an inverse
    /// no longer fits.
    pub fn undo(&mut self, tx: &Transaction) -> Result<Transaction, CommitError> {
        let ops = tx
            .inverse_ops()
            .map_err(|source| CommitError::Op { index: 0, source })?;
        self.commit(tx.label, ops)
    }

    fn rollback(&mut self, applied: &[Op]) -> Result<(), CommitError> {
        for op in applied.iter().rev() {
            let mut inv = op.inverse().map_err(CommitError::RollbackFailed)?;
            inv.apply(self).map_err(CommitError::RollbackFailed)?;
        }
        Ok(())
    }

    /// Checks the tree invariants of the pages touched by the running transaction: parent links
    /// agree with child lists, every block is reachable exactly once from the roots (so the tree
    /// is acyclic and each block has one parent slot), the global index matches, and no uuid was
    /// duplicated by the transaction. Duplicates already present when a page was loaded (user
    /// data) are tolerated.
    ///
    /// # Errors
    /// The first violated invariant.
    pub fn check_invariants(&self) -> Result<(), InvariantError> {
        let touched: BTreeSet<PageKey> = self.touched.clone();
        for key in &touched {
            let Some(page) = self.page(key) else { continue };
            let err = |what| InvariantError::Tree {
                page: key.clone(),
                what,
            };
            let mut seen = std::collections::HashSet::new();
            let mut stack: Vec<(Option<BlockId>, BlockId)> =
                page.roots.iter().map(|r| (None, *r)).collect();
            while let Some((parent, id)) = stack.pop() {
                if !seen.insert(id) {
                    return Err(err("block reachable twice (cycle or shared child)"));
                }
                let b = page.block(id).ok_or_else(|| err("dangling child id"))?;
                if b.parent != parent {
                    return Err(err("parent link disagrees with child list"));
                }
                if self.locate(id) != Some(key) {
                    return Err(err("global block index out of sync"));
                }
                stack.extend(b.children.iter().map(|c| (Some(id), *c)));
            }
            if seen.len() != page.blocks.len() {
                return Err(err("unreachable blocks"));
            }
        }
        if let Some(u) = self.new_duplicate_uuids().into_iter().next() {
            debug_assert!(self.uuid_count(&u) > 1);
            return Err(InvariantError::DuplicateUuid(u));
        }
        Ok(())
    }
}

/// Editor cursor before a command, for the commands that depend on the caret.
fn cursor_before(ws: &Workspace, cmd: &Cmd) -> Option<CursorState> {
    let at =
        |block: BlockId, selection: std::ops::Range<usize>| Some(CursorState { block, selection });
    match cmd {
        Cmd::Enter { id, cursor, .. } | Cmd::SplitBlock { id, cursor } => at(*id, cursor.clone()),
        Cmd::InsertNewline { id, at: r } => at(*id, r.clone()),
        Cmd::EditText { id, range, .. } => at(*id, range.clone()),
        Cmd::PasteText { target, cursor, .. } => at(*target, cursor.clone()),
        Cmd::InsertBlockRef { target, range, .. } => at(*target, range.clone()),
        Cmd::InsertTemplate {
            target, trigger, ..
        } => at(*target, trigger.clone()),
        Cmd::MergeWithPrevious { id } | Cmd::OutdentEmptyLast { id } => at(*id, 0..0),
        Cmd::MergeNext { id } => {
            let end = ws.block(*id).map_or(0, |b| b.text.len());
            at(*id, end..end)
        }
        _ => None,
    }
}
