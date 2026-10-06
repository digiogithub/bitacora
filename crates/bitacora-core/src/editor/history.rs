//! Graph-wide undo/redo with typing coalescing and cursor restore (BIT-US-0039;
//! `docs/design/block-editor.md` §4).
//!
//! One stack for the whole graph, capped by entry count and by captured text. Undo commits the
//! inverse ops as an ordinary transaction, so the touched pages become dirty and are written like
//! any edit; the serializer then reproduces the original bytes (clean blocks keep their origin).
//! External reloads keep block ids, so history survives them; an entry whose blocks vanished
//! stops undo with [`HistoryError::Truncated`] and the stacks are cleared.

use std::collections::VecDeque;
use std::time::Duration;

use super::op::Op;
use super::tx::{CommitError, CursorState, Transaction};
use super::workspace::Workspace;

/// Caps and coalescing windows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HistoryConfig {
    /// Entries kept (default 1,000).
    pub max_entries: usize,
    /// Captured text kept in bytes (default 50 MB).
    pub max_bytes: usize,
    /// Typing within this gap of the previous keystroke joins the same undo step (1.5 s).
    pub typing_gap: Duration,
    /// A word boundary typed after a pause at least this long starts a new step (400 ms).
    pub word_pause: Duration,
}

impl Default for HistoryConfig {
    fn default() -> Self {
        Self {
            max_entries: 1_000,
            max_bytes: 50 * 1024 * 1024,
            typing_gap: Duration::from_millis(1_500),
            word_pause: Duration::from_millis(400),
        }
    }
}

/// Why an undo or redo did not happen.
#[derive(Debug, thiserror::Error)]
pub enum HistoryError {
    /// The undo stack is empty.
    #[error("nothing to undo")]
    NothingToUndo,
    /// The redo stack is empty.
    #[error("nothing to redo")]
    NothingToRedo,
    /// The graph changed (externally) so that an entry no longer fits; history was cleared.
    #[error("history truncated by external change")]
    Truncated(#[source] CommitError),
}

/// Result of an undo or redo.
#[derive(Debug, Clone)]
pub struct HistoryStep {
    /// The transaction that was committed (carries the touched pages).
    pub tx: Transaction,
    /// Where the editor cursor goes: `cursor_before` of the undone entry, `cursor_after` of the
    /// redone one.
    pub cursor: Option<CursorState>,
}

#[derive(Debug, Clone)]
struct Redo {
    /// The committed inverse; undoing it re-applies the original.
    inverse: Transaction,
    before: Option<CursorState>,
    after: Option<CursorState>,
}

/// The undo and redo stacks.
#[derive(Debug, Default)]
pub struct History {
    cfg: HistoryConfig,
    undo: VecDeque<Transaction>,
    redo: Vec<Redo>,
    bytes: usize,
    /// Set by undo/redo: the next typing run must not merge into an older entry.
    barrier: bool,
}

fn op_bytes(op: &Op) -> usize {
    use super::model::Subtree;
    fn st(s: &Subtree) -> usize {
        s.text.len() + 32 + s.children.iter().map(st).sum::<usize>()
    }
    match op {
        Op::InsertSubtree { subtree, .. } => st(subtree),
        Op::RemoveSubtree { captured, .. } => captured.as_ref().map_or(0, |(_, s)| st(s)),
        Op::SetText { before, after, .. } => before.len() + after.len(),
        Op::EditText {
            removed, inserted, ..
        } => removed.len() + inserted.len(),
        Op::SetPreamble { before, after, .. } => {
            before.as_ref().map_or(0, String::len) + after.as_ref().map_or(0, String::len)
        }
        Op::EditFile { before, after, .. } => before.len() + after.len(),
        Op::DeletePage { captured, .. } => captured
            .as_ref()
            .map_or(0, |p| p.blocks.values().map(|b| b.text.len() + 32).sum()),
        _ => 32,
    }
}

fn tx_bytes(tx: &Transaction) -> usize {
    tx.ops.iter().map(op_bytes).sum::<usize>() + 64
}

impl History {
    /// An empty history with the default caps.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// An empty history with custom caps.
    #[must_use]
    pub fn with_config(cfg: HistoryConfig) -> Self {
        Self {
            cfg,
            ..Self::default()
        }
    }

    /// Entries that can be undone.
    #[must_use]
    pub fn undo_len(&self) -> usize {
        self.undo.len()
    }

    /// Entries that can be redone.
    #[must_use]
    pub fn redo_len(&self) -> usize {
        self.redo.len()
    }

    /// Label of the entry undo would revert.
    #[must_use]
    pub fn next_undo_label(&self) -> Option<&'static str> {
        self.undo.back().map(|t| t.label)
    }

    /// Approximate captured text held, in bytes.
    #[must_use]
    pub fn bytes(&self) -> usize {
        self.bytes
    }

    /// Forgets everything (an entry no longer applies).
    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.bytes = 0;
    }

    fn word_boundary(tx: &Transaction) -> bool {
        matches!(tx.ops.first(), Some(Op::EditText { inserted, .. })
            if inserted.chars().next().is_some_and(|c| !c.is_alphanumeric()))
    }

    fn can_merge(&self, tx: &Transaction) -> bool {
        if self.barrier || tx.coalesce.is_none() {
            return false;
        }
        let Some(top) = self.undo.back() else {
            return false;
        };
        if top.coalesce != tx.coalesce {
            return false;
        }
        let gap = tx.at.saturating_duration_since(top.at);
        gap < self.cfg.typing_gap && !(gap >= self.cfg.word_pause && Self::word_boundary(tx))
    }

    /// Records a committed transaction: clears redo and merges typing runs. Transactions without
    /// ops are ignored.
    pub fn push(&mut self, tx: Transaction) {
        if tx.ops.is_empty() {
            return;
        }
        self.redo.clear();
        if self.can_merge(&tx) {
            if let Some(top) = self.undo.back_mut() {
                self.bytes += tx_bytes(&tx);
                top.ops.extend(tx.ops);
                for p in tx.pages {
                    if !top.pages.contains(&p) {
                        top.pages.push(p);
                    }
                }
                top.cursor_after = tx.cursor_after;
                top.at = tx.at;
            }
            return;
        }
        self.barrier = false;
        self.bytes += tx_bytes(&tx);
        self.undo.push_back(tx);
        while self.undo.len() > self.cfg.max_entries
            || (self.bytes > self.cfg.max_bytes && self.undo.len() > 1)
        {
            if let Some(old) = self.undo.pop_front() {
                self.bytes = self.bytes.saturating_sub(tx_bytes(&old));
            }
        }
    }

    /// Reverts the latest entry and restores its `cursor_before`.
    ///
    /// # Errors
    /// [`HistoryError::NothingToUndo`], or [`HistoryError::Truncated`] when the entry no longer
    /// fits the graph (the workspace is unchanged and the history is cleared).
    pub fn undo(&mut self, ws: &mut Workspace) -> Result<HistoryStep, HistoryError> {
        let tx = self.undo.pop_back().ok_or(HistoryError::NothingToUndo)?;
        self.bytes = self.bytes.saturating_sub(tx_bytes(&tx));
        match ws.undo(&tx) {
            Ok(mut inverse) => {
                self.barrier = true;
                inverse.cursor_before.clone_from(&tx.cursor_after);
                inverse.cursor_after.clone_from(&tx.cursor_before);
                let cursor = tx.cursor_before.clone();
                self.redo.push(Redo {
                    inverse: inverse.clone(),
                    before: tx.cursor_before,
                    after: tx.cursor_after,
                });
                Ok(HistoryStep {
                    tx: inverse,
                    cursor,
                })
            }
            Err(e) => {
                self.clear();
                Err(HistoryError::Truncated(e))
            }
        }
    }

    /// Re-applies the entry undo reverted and restores its `cursor_after`.
    ///
    /// # Errors
    /// [`HistoryError::NothingToRedo`], or [`HistoryError::Truncated`] as for undo.
    pub fn redo(&mut self, ws: &mut Workspace) -> Result<HistoryStep, HistoryError> {
        let r = self.redo.pop().ok_or(HistoryError::NothingToRedo)?;
        match ws.undo(&r.inverse) {
            Ok(mut tx) => {
                self.barrier = true;
                tx.cursor_before.clone_from(&r.before);
                tx.cursor_after.clone_from(&r.after);
                self.bytes += tx_bytes(&tx);
                self.undo.push_back(tx.clone());
                Ok(HistoryStep {
                    cursor: r.after,
                    tx,
                })
            }
            Err(e) => {
                self.clear();
                Err(HistoryError::Truncated(e))
            }
        }
    }
}
