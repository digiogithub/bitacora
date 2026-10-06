//! Selective restore of a historical page version (BIT-US-0048, BIT-SP-0006.R22).
//!
//! A restore turns the current page back into (parts of) an older version using ordinary core
//! commands, so every step is a normal transaction: the single-writer rule holds, the page is
//! rewritten by core's block-preserving writer and the result is undoable
//! ([`RestoreReport::undo`]). Blocks are paired with the matcher of `bitacora-merge`, so a block
//! edited since is restored in place instead of duplicated.

use std::collections::HashMap;

use bitacora_core::editor::{BlockId, Cmd, IdGen, Op, Page, Target, Transaction};
use bitacora_core::graph::PageKey;
use bitacora_core::graph_path::GraphPath;
use bitacora_core::queue::{CommandQueue, QueueError, Request, Source};
use bitacora_sync::history::{BlockDiff, DiffKind, MergePage, PageDiff, diff_version};

use crate::session::RuntimeError;

/// Outcome of a restore.
#[derive(Debug, Default)]
pub struct RestoreReport {
    /// The committed transactions, in order.
    pub txs: Vec<Transaction>,
    /// Blocks (or the property block) brought back.
    pub restored: usize,
    /// Selected differences that could not be applied, with the reason.
    pub skipped: Vec<String>,
}

impl RestoreReport {
    /// Reverts the whole restore (one inverse transaction per applied one, last first).
    ///
    /// # Errors
    /// [`RuntimeError::Restore`] when the page changed since and an inverse no longer applies.
    pub fn undo(&self, queue: &CommandQueue) -> Result<(), RuntimeError> {
        for tx in self.txs.iter().rev() {
            let ops = tx
                .inverse_ops()
                .map_err(|e| RuntimeError::Restore(e.to_string()))?;
            queue
                .execute(
                    Source::Ui,
                    Request::Commit {
                        label: "Undo restore",
                        ops,
                    },
                )
                .map_err(restore_err)?;
        }
        Ok(())
    }
}

fn restore_err(e: QueueError) -> RuntimeError {
    RuntimeError::Restore(e.to_string())
}

/// Which differences to restore.
pub(crate) enum Selection<'a> {
    /// Everything that differs (including metadata and the page properties).
    Whole,
    /// The given differences of a previously computed [`PageDiff`].
    Blocks(&'a [BlockDiff]),
}

/// The loaded state a restore works on.
struct Ctx<'a> {
    queue: &'a CommandQueue,
    key: PageKey,
    /// Version blocks in document order: core text.
    ver_text: Vec<String>,
    ver: MergePage,
    /// Current block ids in document order.
    cur_ids: Vec<BlockId>,
    pairs: Vec<Option<usize>>,
    /// Version index -> block inserted by this restore.
    inserted: HashMap<usize, BlockId>,
    report: RestoreReport,
}

impl Ctx<'_> {
    fn run(&mut self, label: &'static str, cmd: Cmd) -> Result<Transaction, RuntimeError> {
        let tx = self
            .queue
            .run(Source::Ui, label, cmd)
            .map_err(restore_err)?;
        self.report.txs.push(tx.clone());
        Ok(tx)
    }

    /// Current block standing for version block `old`.
    fn current_of(&self, old: usize) -> Option<BlockId> {
        self.inserted.get(&old).copied().or_else(|| {
            self.pairs
                .get(old)
                .copied()
                .flatten()
                .map(|n| self.cur_ids[n])
        })
    }

    fn first_child_of(&self, parent: BlockId) -> Option<BlockId> {
        let snap = self.queue.snapshot(&self.key)?;
        snap.blocks
            .iter()
            .find(|b| b.parent == Some(parent))
            .map(|b| b.id)
    }

    fn insert_removed(&mut self, old: usize) -> Result<(), RuntimeError> {
        let text = self.ver_text[old].clone();
        let parent_old = self.ver.blocks[old].parent;
        let parent = match parent_old {
            None => None,
            Some(p) => match self.current_of(p) {
                Some(id) => Some(id),
                None => {
                    self.report.skipped.push(format!(
                        "`{}`: its parent block is missing and was not selected",
                        first_line(&text)
                    ));
                    return Ok(());
                }
            },
        };
        let siblings = self.ver.children_of(parent_old);
        let pos = siblings.iter().position(|&s| s == old).unwrap_or(0);
        let prev = pos
            .checked_sub(1)
            .and_then(|i| self.current_of(siblings[i]));
        let tx = if let Some(after) = prev {
            self.run("Restore block", Cmd::InsertSibling { after, text })?
        } else {
            let first = match parent {
                Some(p) => self.first_child_of(p),
                None => self
                    .queue
                    .snapshot(&self.key)
                    .and_then(|s| s.blocks.iter().find(|b| b.parent.is_none()).map(|b| b.id)),
            };
            let tx = self.run(
                "Restore block",
                Cmd::InsertChild {
                    page: self.key.clone(),
                    parent,
                    text,
                },
            )?;
            if let (Some(first), Some(new)) = (first, inserted_id(&tx)) {
                // Appended last; move it before the first existing sibling.
                self.run(
                    "Restore block",
                    Cmd::MoveBlocks {
                        ids: vec![new],
                        target: Target::Before(first),
                    },
                )?;
            }
            tx
        };
        if let Some(id) = inserted_id(&tx) {
            self.inserted.insert(old, id);
            self.report.restored += 1;
        }
        Ok(())
    }

    fn remove_added(&mut self, new: usize, cur: &MergePage) -> Result<(), RuntimeError> {
        // Only a block whose whole subtree is new can go without losing older blocks.
        let mut stack = vec![new];
        while let Some(i) = stack.pop() {
            if self.pairs.contains(&Some(i)) {
                self.report.skipped.push(format!(
                    "`{}`: it holds blocks that exist in the old version",
                    first_line(&cur.blocks[new].content)
                ));
                return Ok(());
            }
            stack.extend(cur.blocks[i].children.iter().copied());
        }
        self.run(
            "Restore page",
            Cmd::DeleteBlocks {
                ids: vec![self.cur_ids[new]],
            },
        )?;
        self.report.restored += 1;
        Ok(())
    }
}

fn first_line(s: &str) -> String {
    s.lines().next().unwrap_or("").chars().take(60).collect()
}

fn inserted_id(tx: &Transaction) -> Option<BlockId> {
    tx.ops.iter().find_map(|op| match op {
        Op::InsertSubtree { subtree, .. } => Some(subtree.id),
        _ => None,
    })
}

/// Restores `selection` of `version` into the page at `rel`.
pub(crate) fn restore(
    queue: &CommandQueue,
    root: &std::path::Path,
    key: &PageKey,
    rel: &str,
    version: &str,
    selection: &Selection<'_>,
) -> Result<RestoreReport, RuntimeError> {
    let path = GraphPath::new(rel).map_err(|_| RuntimeError::BadPath(rel.to_owned()))?;
    let current =
        std::fs::read_to_string(path.to_fs_path(root)).map_err(|source| RuntimeError::Read {
            path: rel.to_owned(),
            source,
        })?;
    let diff: PageDiff = diff_version(version, &current);
    let ver = MergePage::parse(version);
    let cur = MergePage::parse(&current);

    // Core's view of both versions, in the same document order as the merge pages.
    let ids = IdGen::default();
    let ver_page = Page::load(key.clone(), "restore", None, version.as_bytes(), &ids);
    let ver_text: Vec<String> = ver_page
        .dfs()
        .into_iter()
        .filter_map(|id| ver_page.block(id).map(|b| b.text.clone()))
        .collect();
    let snap = queue
        .snapshot(key)
        .ok_or_else(|| RuntimeError::Restore(format!("page `{rel}` is not loaded")))?;
    if ver_text.len() != ver.blocks.len() || snap.blocks.len() != cur.blocks.len() {
        return Err(RuntimeError::Restore(
            "the page structure is out of sync with the file; reload the page and retry".into(),
        ));
    }
    let mut ctx = Ctx {
        queue,
        key: key.clone(),
        ver_text,
        ver,
        cur_ids: snap.blocks.iter().map(|b| b.id).collect(),
        pairs: diff.pairs.clone(),
        inserted: HashMap::new(),
        report: RestoreReport::default(),
    };

    let (wanted, preamble): (Vec<&BlockDiff>, bool) = match selection {
        Selection::Whole => (diff.blocks.iter().collect(), diff.preamble_changed),
        Selection::Blocks(sel) => {
            let mut out = Vec::new();
            for s in *sel {
                match diff.blocks.iter().find(|d| same_diff(d, s)) {
                    Some(d) => out.push(d),
                    None => ctx
                        .report
                        .skipped
                        .push("a selected difference no longer applies (the page changed)".into()),
                }
            }
            (out, false)
        }
    };

    // 1. Edit changed blocks in place.
    for d in wanted
        .iter()
        .filter(|d| matches!(d.kind, DiffKind::Changed | DiffKind::MetaOnly))
    {
        let (Some(old), Some(new)) = (d.old, d.new) else {
            continue;
        };
        let text = ctx.ver_text[old].clone();
        match ctx.run(
            "Restore block",
            Cmd::SetText {
                id: ctx.cur_ids[new],
                text,
            },
        ) {
            Ok(_) => ctx.report.restored += 1,
            Err(e) => ctx.report.skipped.push(e.to_string()),
        }
    }
    // 2. Re-insert removed blocks, parents before children (version order).
    let mut removed: Vec<usize> = wanted
        .iter()
        .filter(|d| d.kind == DiffKind::Removed)
        .filter_map(|d| d.old)
        .collect();
    removed.sort_unstable();
    for old in removed {
        ctx.insert_removed(old)?;
    }
    // 3. Drop blocks that did not exist in the old version (page restore or explicit choice);
    // deepest and last first so ids stay valid.
    let mut added: Vec<usize> = wanted
        .iter()
        .filter(|d| d.kind == DiffKind::Added)
        .filter_map(|d| d.new)
        .collect();
    added.sort_unstable_by(|a, b| b.cmp(a));
    let mut gone: Vec<usize> = Vec::new();
    for new in added {
        // A block inside an already removed subtree is gone with it.
        let mut p = cur.blocks[new].parent;
        let mut inside = false;
        while let Some(i) = p {
            if gone.contains(&i) {
                inside = true;
                break;
            }
            p = cur.blocks[i].parent;
        }
        if inside {
            continue;
        }
        let before = ctx.report.restored;
        ctx.remove_added(new, &cur)?;
        if ctx.report.restored > before {
            gone.push(new);
        }
    }
    // 4. Page properties.
    if preamble {
        let current_pre = queue.snapshot(key).and_then(|s| s.preamble.clone());
        if current_pre != ver_page.preamble {
            let tx = queue
                .execute(
                    Source::Ui,
                    Request::Commit {
                        label: "Restore page properties",
                        ops: vec![Op::SetPreamble {
                            page: key.clone(),
                            before: current_pre,
                            after: ver_page.preamble.clone(),
                        }],
                    },
                )
                .map_err(restore_err)?;
            if let bitacora_core::queue::Response::Committed(tx) = tx {
                ctx.report.txs.push(tx);
                ctx.report.restored += 1;
            }
        }
    }
    Ok(ctx.report)
}

fn same_diff(a: &BlockDiff, b: &BlockDiff) -> bool {
    a.kind == b.kind
        && a.old == b.old
        && a.new == b.new
        && a.old_text == b.old_text
        && a.new_text == b.new_text
}
