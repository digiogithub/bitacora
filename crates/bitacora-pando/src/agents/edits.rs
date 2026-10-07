//! The `propose_edit` proposal: a constrained op list, validated against the current page and
//! converted to `bitacora-core` transactions (BIT-T-0457, BIT-SP-0011.R2).
//!
//! A proposal is *data* from an agent. It is parsed into [`Proposal`], validated against a
//! [`PageSnapshot`] by [`validate`] (unknown blocks, text that changed since the agent read it and
//! malformed content are refused) and shown to the user as a [`Preview`]. Only after an explicit
//! approval does [`QueueEditApplier`] re-validate it against the *current* page and commit it
//! through the core command queue with [`Source::Agent`]. Nothing in this module writes a file;
//! the single writer stays `bitacora-core` (rule 3).
//!
//! Blocks are addressed by their persisted `id::` uuid (the one MCP reads return). A block
//! without a persisted uuid cannot be targeted, and a page that is not loaded in the writer is
//! reported as [`EditError::PageNotLoaded`] so the caller can open it and retry.

use std::collections::HashSet;
use std::sync::Arc;

use bitacora_core::editor::{
    BlockId, Cmd, CommitError, Op, Refusal, Target, Transaction, text_is_representable,
};
use bitacora_core::graph::PageKey;
use bitacora_core::queue::{CommandQueue, PageSnapshot, QueueError, Request, Response, Source};
use bitacora_markdown::edit::properties::{get_property, set_property};
use serde::{Deserialize, Serialize};

/// Most operations one proposal may carry.
pub const MAX_OPS: usize = 50;
/// Longest text of one block, in bytes.
pub const MAX_TEXT_BYTES: usize = 20_000;

/// Where a moved block goes relative to the target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Place {
    /// Before the target, as its sibling.
    Before,
    /// After the target, as its sibling.
    After,
    /// First child of the target.
    FirstChild,
    /// Last child of the target.
    LastChild,
}

/// One requested change. Unknown `op` values fail the whole proposal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum EditOp {
    /// A new block: a sibling after `after_uuid`, or the last child of `parent_uuid`, or the last
    /// root block of the page when neither is given.
    InsertBlock {
        /// Text of the new block (no leading `- `).
        text: String,
        /// Parent of the new block.
        #[serde(default)]
        parent_uuid: Option<String>,
        /// Block the new one follows as a sibling.
        #[serde(default)]
        after_uuid: Option<String>,
    },
    /// Replaces the text of a block.
    UpdateBlock {
        /// Block.
        uuid: String,
        /// The text the agent read; a different current text makes the proposal stale.
        expected_text: String,
        /// New text.
        text: String,
    },
    /// Moves a block with its children.
    MoveBlock {
        /// Block.
        uuid: String,
        /// Reference block.
        target_uuid: String,
        /// Where relative to `target_uuid`.
        place: Place,
    },
    /// Deletes a block with its children.
    DeleteBlock {
        /// Block.
        uuid: String,
        /// The text the agent read (optional guard against stale deletes).
        #[serde(default)]
        expected_text: Option<String>,
    },
    /// Sets one property of a block.
    SetProperty {
        /// Block.
        uuid: String,
        /// Property key.
        key: String,
        /// Value (one line).
        value: String,
    },
}

/// A parsed `propose_edit` call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Proposal {
    /// One-line summary shown on the approval card.
    #[serde(default)]
    pub title: String,
    /// Title of the page every op belongs to.
    pub page: String,
    /// The changes, applied in order as one undoable group.
    pub ops: Vec<EditOp>,
}

/// Why a proposal cannot be shown or applied.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EditError {
    /// The arguments are not a valid proposal.
    #[error("invalid proposal: {0}")]
    Invalid(String),
    /// The page is not loaded in the writer.
    #[error("page `{0}` is not loaded; open it first")]
    PageNotLoaded(String),
    /// A block of the proposal does not exist (any more).
    #[error("block `{0}` was not found on the page")]
    UnknownBlock(String),
    /// The page changed since the agent read it.
    #[error("stale proposal: {0}")]
    Stale(String),
    /// The writer refused the change.
    #[error("the graph refused the edit: {0}")]
    Refused(String),
}

impl EditError {
    /// Stable machine code for the tool result sent back to the agent.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::Invalid(_) => "invalid_proposal",
            Self::PageNotLoaded(_) => "page_not_loaded",
            Self::UnknownBlock(_) => "unknown_block",
            Self::Stale(_) => "stale_proposal",
            Self::Refused(_) => "refused",
        }
    }
}

impl Proposal {
    /// Parses the JSON arguments of the tool call.
    ///
    /// # Errors
    /// [`EditError::Invalid`] when the arguments do not match the schema.
    pub fn parse(args: &serde_json::Value) -> Result<Self, EditError> {
        let p: Self =
            serde_json::from_value(args.clone()).map_err(|e| EditError::Invalid(e.to_string()))?;
        p.check_shape()?;
        Ok(p)
    }

    /// Context-free checks: sizes, content and property shape.
    fn check_shape(&self) -> Result<(), EditError> {
        let bad = |m: String| Err(EditError::Invalid(m));
        if self.page.trim().is_empty() {
            return bad("`page` must not be empty".into());
        }
        if self.ops.is_empty() {
            return bad("`ops` must not be empty".into());
        }
        if self.ops.len() > MAX_OPS {
            return bad(format!("at most {MAX_OPS} ops per proposal"));
        }
        for (i, op) in self.ops.iter().enumerate() {
            match op {
                EditOp::InsertBlock {
                    text,
                    parent_uuid,
                    after_uuid,
                } => {
                    check_text(text, i)?;
                    if parent_uuid.is_some() && after_uuid.is_some() {
                        return bad(format!(
                            "op {i}: give `parent_uuid` or `after_uuid`, not both"
                        ));
                    }
                }
                EditOp::UpdateBlock { text, .. } => check_text(text, i)?,
                EditOp::SetProperty { key, value, .. } => {
                    let k = key.trim();
                    let key_ok = !k.is_empty()
                        && k.chars()
                            .all(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.' | '/'));
                    if !key_ok {
                        return bad(format!("op {i}: invalid property key `{key}`"));
                    }
                    if matches!(
                        k.to_lowercase().replace('_', "-").as_str(),
                        "id" | "collapsed"
                    ) {
                        return bad(format!("op {i}: `{k}::` is managed by Bitacora"));
                    }
                    if value.contains('\n') || value.contains('\r') {
                        return bad(format!("op {i}: a property value is one line"));
                    }
                }
                EditOp::MoveBlock { .. } | EditOp::DeleteBlock { .. } => {}
            }
        }
        Ok(())
    }
}

fn check_text(text: &str, i: usize) -> Result<(), EditError> {
    let bad = |m: &str| Err(EditError::Invalid(format!("op {i}: {m}")));
    if text.trim().is_empty() {
        return bad("`text` must not be empty");
    }
    if text.len() > MAX_TEXT_BYTES {
        return bad("`text` is too long");
    }
    let first = text.trim_start().lines().next().unwrap_or_default();
    if first.starts_with("- ") || first == "-" {
        return bad("`text` is the text of ONE block without the leading `- `");
    }
    if !text_is_representable(&text.replace("\r\n", "\n")) {
        return bad("`text` would split into several blocks (a line starts like a list item)");
    }
    for key in ["id", "collapsed"] {
        if get_property(text, key).is_some() {
            return bad("`id::` and `collapsed::` are managed by Bitacora");
        }
    }
    Ok(())
}

/// What one op does, for the approval card's diff.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PreviewOp {
    /// `insert`, `update`, `move`, `delete` or `property`.
    pub kind: &'static str,
    /// Target block uuid, when the op has one.
    pub uuid: Option<String>,
    /// Text before the change.
    pub before: Option<String>,
    /// Text after the change.
    pub after: Option<String>,
}

/// The card content: what approving would change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Preview {
    /// Summary line (the agent's `title`, or a generated one).
    pub title: String,
    /// Page title.
    pub page: String,
    /// One entry per op, in order.
    pub ops: Vec<PreviewOp>,
}

/// An op with its blocks resolved against a snapshot.
#[derive(Debug, Clone)]
enum Resolved {
    Insert {
        text: String,
        parent: Option<BlockId>,
        after: Option<BlockId>,
    },
    Update {
        id: BlockId,
        before: String,
        after: String,
    },
    Move {
        id: BlockId,
        target: Target,
    },
    Delete {
        id: BlockId,
    },
    Property {
        id: BlockId,
        key: String,
        value: String,
    },
}

/// A proposal validated against one version of a page.
#[derive(Debug, Clone)]
pub struct Validated {
    page: PageKey,
    /// Snapshot version it was validated against.
    pub version: u64,
    ops: Vec<Resolved>,
    /// What approving would do.
    pub preview: Preview,
    /// Uuids of the blocks the proposal touches (for the audit record).
    pub affected: Vec<String>,
}

fn find<'a>(
    snap: &'a PageSnapshot,
    uuid: &str,
) -> Result<&'a bitacora_core::queue::SnapshotBlock, EditError> {
    let want = uuid.trim().to_lowercase();
    snap.blocks
        .iter()
        .find(|b| b.uuid.is_some_and(|u| u.to_string() == want))
        .ok_or_else(|| EditError::UnknownBlock(uuid.to_owned()))
}

/// Validates `proposal` against the current content of its page.
///
/// # Errors
/// [`EditError::UnknownBlock`] for a block that is not on the page, [`EditError::Stale`] when an
/// `expected_text` no longer matches, [`EditError::Invalid`] for conflicting ops (the same block
/// deleted and edited, a move into its own subtree).
pub fn validate(proposal: &Proposal, snap: &PageSnapshot) -> Result<Validated, EditError> {
    proposal.check_shape()?;
    let mut ops = Vec::with_capacity(proposal.ops.len());
    let mut preview = Vec::with_capacity(proposal.ops.len());
    let mut affected = Vec::new();
    let mut deleted: HashSet<String> = HashSet::new();
    let mut edited: HashSet<String> = HashSet::new();
    for (i, op) in proposal.ops.iter().enumerate() {
        match op {
            EditOp::InsertBlock {
                text,
                parent_uuid,
                after_uuid,
            } => {
                let parent = parent_uuid.as_deref().map(|u| find(snap, u)).transpose()?;
                let after = after_uuid.as_deref().map(|u| find(snap, u)).transpose()?;
                ops.push(Resolved::Insert {
                    text: text.trim_end().to_owned(),
                    parent: parent.map(|b| b.id),
                    after: after.map(|b| b.id),
                });
                preview.push(PreviewOp {
                    kind: "insert",
                    uuid: None,
                    before: None,
                    after: Some(text.trim_end().to_owned()),
                });
            }
            EditOp::UpdateBlock {
                uuid,
                expected_text,
                text,
            } => {
                let b = find(snap, uuid)?;
                if normalize(&b.text) != normalize(expected_text) {
                    return Err(EditError::Stale(format!(
                        "block `{uuid}` changed since the agent read it"
                    )));
                }
                // `id::` and `collapsed::` survive an edit: they are not part of the agent's text.
                let mut after = text.trim_end().replace("\r\n", "\n");
                for key in ["id", "collapsed"] {
                    if let Some(v) = get_property(&b.text, key) {
                        after = set_property(&after, key, &v);
                    }
                }
                edited.insert(uuid.to_lowercase());
                affected.push(uuid.to_lowercase());
                preview.push(PreviewOp {
                    kind: "update",
                    uuid: Some(uuid.to_lowercase()),
                    before: Some(b.text.clone()),
                    after: Some(after.clone()),
                });
                ops.push(Resolved::Update {
                    id: b.id,
                    before: b.text.clone(),
                    after,
                });
            }
            EditOp::MoveBlock {
                uuid,
                target_uuid,
                place,
            } => {
                let b = find(snap, uuid)?;
                let t = find(snap, target_uuid)?;
                if b.id == t.id || is_descendant(snap, t.id, b.id) {
                    return Err(EditError::Invalid(format!(
                        "op {i}: cannot move a block into itself or its own subtree"
                    )));
                }
                let target = match place {
                    Place::Before => Target::Before(t.id),
                    Place::After => Target::After(t.id),
                    Place::FirstChild => Target::FirstChild(t.id),
                    Place::LastChild => Target::LastChild(t.id),
                };
                edited.insert(uuid.to_lowercase());
                affected.push(uuid.to_lowercase());
                preview.push(PreviewOp {
                    kind: "move",
                    uuid: Some(uuid.to_lowercase()),
                    before: Some(b.text.clone()),
                    after: None,
                });
                ops.push(Resolved::Move { id: b.id, target });
            }
            EditOp::DeleteBlock {
                uuid,
                expected_text,
            } => {
                let b = find(snap, uuid)?;
                if let Some(exp) = expected_text
                    && normalize(&b.text) != normalize(exp)
                {
                    return Err(EditError::Stale(format!(
                        "block `{uuid}` changed since the agent read it"
                    )));
                }
                deleted.insert(uuid.to_lowercase());
                affected.push(uuid.to_lowercase());
                preview.push(PreviewOp {
                    kind: "delete",
                    uuid: Some(uuid.to_lowercase()),
                    before: Some(b.text.clone()),
                    after: None,
                });
                ops.push(Resolved::Delete { id: b.id });
            }
            EditOp::SetProperty { uuid, key, value } => {
                let b = find(snap, uuid)?;
                let after = set_property(&b.text, key.trim(), value.trim());
                edited.insert(uuid.to_lowercase());
                affected.push(uuid.to_lowercase());
                preview.push(PreviewOp {
                    kind: "property",
                    uuid: Some(uuid.to_lowercase()),
                    before: Some(b.text.clone()),
                    after: Some(after),
                });
                ops.push(Resolved::Property {
                    id: b.id,
                    key: key.trim().to_owned(),
                    value: value.trim().to_owned(),
                });
            }
        }
    }
    if let Some(u) = deleted.iter().find(|u| edited.contains(*u)) {
        return Err(EditError::Invalid(format!(
            "block `{u}` is both deleted and changed in one proposal"
        )));
    }
    affected.sort();
    affected.dedup();
    let title = if proposal.title.trim().is_empty() {
        format!("{} change(s) to {}", proposal.ops.len(), proposal.page)
    } else {
        proposal.title.trim().to_owned()
    };
    Ok(Validated {
        page: snap.key.clone(),
        version: snap.version,
        ops,
        preview: Preview {
            title,
            page: proposal.page.clone(),
            ops: preview,
        },
        affected,
    })
}

fn normalize(s: &str) -> String {
    s.replace("\r\n", "\n").trim_end().to_owned()
}

fn is_descendant(snap: &PageSnapshot, id: BlockId, ancestor: BlockId) -> bool {
    let mut cur = snap
        .blocks
        .iter()
        .find(|b| b.id == id)
        .and_then(|b| b.parent);
    while let Some(p) = cur {
        if p == ancestor {
            return true;
        }
        cur = snap
            .blocks
            .iter()
            .find(|b| b.id == p)
            .and_then(|b| b.parent);
    }
    false
}

/// What an applied proposal changed; the handle for undo and the audit record.
#[derive(Debug, Clone)]
pub struct AppliedEdit {
    /// The committed core transactions, in order.
    pub txs: Vec<Transaction>,
    /// Page title.
    pub page: String,
    /// Uuids of the touched blocks.
    pub affected: Vec<String>,
    /// Id the audit log gave the write, when an [`AuditSink`] recorded it.
    pub audit_id: Option<String>,
}

/// Applies approved proposals. The app (or tests) implement it; production uses
/// [`QueueEditApplier`].
pub trait EditApplier: Send + Sync {
    /// Validates `proposal` against the current page for the approval card.
    ///
    /// # Errors
    /// See [`validate`] and [`EditError::PageNotLoaded`].
    fn preview(&self, proposal: &Proposal) -> Result<Preview, EditError>;

    /// Re-validates and commits an *approved* proposal as one undoable group.
    ///
    /// # Errors
    /// [`EditError::Stale`] and friends when the page moved on, [`EditError::Refused`] when the
    /// writer refused. A failed group leaves the graph unchanged.
    fn apply(&self, proposal: &Proposal) -> Result<AppliedEdit, EditError>;
}

/// A block as the index sees it (the uuid agents read over MCP).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexedBlock {
    /// Index uuid: the `id::` value, or one the index generated for a block without `id::`.
    pub uuid: String,
    /// Raw block text.
    pub text: String,
}

/// What the app gives [`QueueEditApplier`] to reach pages that are not loaded yet and blocks
/// that have no persisted `id::`.
pub trait PageResolver: Send + Sync {
    /// Loads the page into the writer when it exists in the graph. Never creates a page.
    /// `true` when the page is (now) loaded.
    fn ensure_loaded(&self, page: &str) -> bool;

    /// The index's blocks of the page in outline order, without the page-properties pre-block.
    fn indexed_blocks(&self, page: &str) -> Vec<IndexedBlock>;
}

/// Gives the blocks of `snap` that have no persisted `id::` the uuid the index (and so the agent)
/// knows them by. A block is matched by position and only when its text is identical, so an index
/// that lags behind the writer never aliases another block. The result lives in memory only:
/// nothing is ever written into the user's file for it (rule 1).
fn with_index_uuids(snap: &PageSnapshot, indexed: &[IndexedBlock]) -> PageSnapshot {
    let mut out = snap.clone();
    for (b, ix) in out.blocks.iter_mut().zip(indexed) {
        if b.uuid.is_none()
            && normalize(&b.text) == normalize(&ix.text)
            && let Ok(u) = ix.uuid.parse()
        {
            b.uuid = Some(u);
        }
    }
    out
}

/// Records applied agent edits in the agent audit log (BIT-SP-0011.R2).
pub trait AuditSink: Send + Sync {
    /// Records `edit`; returns the audit entry id (used with the log's undo).
    fn record_edit(&self, summary: &str, edit: &AppliedEdit) -> Option<String>;
}

/// [`EditApplier`] over the core command queue, tagged [`Source::Agent`].
#[derive(Clone)]
pub struct QueueEditApplier {
    queue: CommandQueue,
    audit: Option<Arc<dyn AuditSink>>,
    resolver: Option<Arc<dyn PageResolver>>,
}

impl std::fmt::Debug for QueueEditApplier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("QueueEditApplier")
    }
}

impl QueueEditApplier {
    /// An applier over `queue`.
    #[must_use]
    pub fn new(queue: CommandQueue) -> Self {
        Self {
            queue,
            audit: None,
            resolver: None,
        }
    }

    /// Records every applied edit in `audit`.
    #[must_use]
    pub fn with_audit(mut self, audit: Arc<dyn AuditSink>) -> Self {
        self.audit = Some(audit);
        self
    }

    /// Loads pages on demand and addresses blocks without `id::` through `resolver`.
    #[must_use]
    pub fn with_resolver(mut self, resolver: Arc<dyn PageResolver>) -> Self {
        self.resolver = Some(resolver);
        self
    }

    fn snapshot(&self, page: &str) -> Result<Arc<PageSnapshot>, EditError> {
        let key = PageKey::from_title(page);
        let mut snap = self.queue.snapshot(&key);
        if snap.is_none()
            && let Some(r) = &self.resolver
            && r.ensure_loaded(page)
        {
            snap = self.queue.snapshot(&key);
        }
        let snap = snap.ok_or_else(|| EditError::PageNotLoaded(page.to_owned()))?;
        Ok(match &self.resolver {
            Some(r) if snap.blocks.iter().any(|b| b.uuid.is_none()) => {
                Arc::new(with_index_uuids(&snap, &r.indexed_blocks(page)))
            }
            _ => snap,
        })
    }

    fn commit_op(&self, label: &'static str, ops: Vec<Op>) -> Result<Transaction, EditError> {
        match self
            .queue
            .execute(Source::Agent, Request::Commit { label, ops })
            .map_err(map_queue)?
        {
            Response::Committed(tx) => Ok(tx),
            _ => Err(EditError::Refused("unexpected response".into())),
        }
    }

    fn run_cmd(&self, label: &'static str, cmd: Cmd) -> Result<Option<Transaction>, EditError> {
        match self.queue.run(Source::Agent, label, cmd) {
            Ok(tx) => Ok(Some(tx)),
            Err(QueueError::Commit(CommitError::Refused(Refusal::NoChange))) => Ok(None),
            Err(e) => Err(map_queue(e)),
        }
    }

    fn rollback(&self, txs: &[Transaction]) {
        let mut ops = Vec::new();
        for tx in txs.iter().rev() {
            match tx.inverse_ops() {
                Ok(o) => ops.extend(o),
                Err(e) => {
                    tracing::warn!(error = %e, "cannot invert an agent edit while rolling back");
                    return;
                }
            }
        }
        if ops.is_empty() {
            return;
        }
        if let Err(e) = self.queue.execute(
            Source::Agent,
            Request::Commit {
                label: "Agent edit (rolled back)",
                ops,
            },
        ) {
            tracing::warn!(error = %e, "rolling back an agent edit failed");
        }
    }

    fn run_resolved(&self, v: &Validated, op: &Resolved) -> Result<Option<Transaction>, EditError> {
        match op {
            Resolved::Insert {
                text,
                parent,
                after,
            } => match after {
                Some(after) => self.run_cmd(
                    "Agent: insert block",
                    Cmd::InsertSibling {
                        after: *after,
                        text: text.clone(),
                    },
                ),
                None => self.run_cmd(
                    "Agent: insert block",
                    Cmd::InsertChild {
                        page: v.page.clone(),
                        parent: *parent,
                        text: text.clone(),
                    },
                ),
            },
            Resolved::Update { id, before, after } => self
                .commit_op(
                    "Agent: update block",
                    vec![Op::SetText {
                        id: *id,
                        before: before.clone(),
                        after: after.clone(),
                    }],
                )
                .map(Some),
            Resolved::Move { id, target } => self.run_cmd(
                "Agent: move block",
                Cmd::MoveBlocks {
                    ids: vec![*id],
                    target: *target,
                },
            ),
            Resolved::Delete { id } => {
                self.run_cmd("Agent: delete block", Cmd::DeleteBlocks { ids: vec![*id] })
            }
            Resolved::Property { id, key, value } => self.run_cmd(
                "Agent: set property",
                Cmd::SetProperty {
                    id: *id,
                    key: key.clone(),
                    value: value.clone(),
                },
            ),
        }
    }
}

fn map_queue(e: QueueError) -> EditError {
    match e {
        QueueError::Commit(CommitError::Op {
            source: bitacora_core::editor::OpError::Stale(m),
            ..
        }) => EditError::Stale(m.to_owned()),
        other => EditError::Refused(other.to_string()),
    }
}

impl EditApplier for QueueEditApplier {
    fn preview(&self, proposal: &Proposal) -> Result<Preview, EditError> {
        let snap = self.snapshot(&proposal.page)?;
        validate(proposal, &snap).map(|v| v.preview)
    }

    fn apply(&self, proposal: &Proposal) -> Result<AppliedEdit, EditError> {
        let snap = self.snapshot(&proposal.page)?;
        let v = validate(proposal, &snap)?;
        let mut txs: Vec<Transaction> = Vec::new();
        for op in &v.ops {
            match self.run_resolved(&v, op) {
                Ok(Some(tx)) => txs.push(tx),
                Ok(None) => {}
                Err(e) => {
                    self.rollback(&txs);
                    return Err(e);
                }
            }
        }
        let mut applied = AppliedEdit {
            txs,
            page: proposal.page.clone(),
            affected: v.affected.clone(),
            audit_id: None,
        };
        if let Some(sink) = &self.audit {
            applied.audit_id = sink.record_edit(&v.preview.title, &applied);
        }
        Ok(applied)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use bitacora_core::editor::{MemStore, Workspace};
    use bitacora_core::graph_path::GraphPath;
    use bitacora_core::queue::QueueConfig;
    use serde_json::json;

    const A: &str = "11111111-1111-4111-8111-111111111111";
    const B: &str = "22222222-2222-4222-8222-222222222222";

    fn queue() -> (CommandQueue, bitacora_core::queue::QueueJoin) {
        let (q, join) = CommandQueue::spawn(
            Workspace::new(),
            Box::new(MemStore::default()),
            QueueConfig {
                debounce: None,
                ..QueueConfig::default()
            },
        );
        q.execute(
            Source::Ui,
            Request::LoadPage {
                key: PageKey::from_title("Notes"),
                title: "Notes".into(),
                path: GraphPath::new("pages/Notes.md").ok(),
                bytes: format!("- alpha\n  id:: {A}\n- beta\n  id:: {B}\n- plain\n").into_bytes(),
            },
        )
        .expect("load");
        (q, join)
    }

    fn proposal(ops: serde_json::Value) -> Proposal {
        Proposal::parse(&json!({"title": "t", "page": "Notes", "ops": ops})).expect("parse")
    }

    fn texts(q: &CommandQueue) -> Vec<String> {
        q.snapshot(&PageKey::from_title("Notes"))
            .expect("snap")
            .blocks
            .iter()
            .map(|b| b.text.clone())
            .collect()
    }

    #[test]
    fn parse_rejects_bad_shapes() {
        for bad in [
            json!({"page": "", "ops": [{"op": "delete_block", "uuid": A}]}),
            json!({"page": "p", "ops": []}),
            json!({"page": "p", "ops": [{"op": "explode", "uuid": A}]}),
            json!({"page": "p", "ops": [{"op": "insert_block", "text": "- x"}]}),
            json!({"page": "p", "ops": [{"op": "insert_block", "text": "a\n- b"}]}),
            json!({"page": "p", "ops": [{"op": "insert_block", "text": "x\nid:: 1"}]}),
            json!({"page": "p", "ops": [{"op": "set_property", "uuid": A, "key": "id", "value": "x"}]}),
            json!({"page": "p", "ops": [{"op": "set_property", "uuid": A, "key": "a b", "value": "x"}]}),
            json!({"page": "p", "ops": [{"op": "insert_block", "text": "x",
                   "parent_uuid": A, "after_uuid": B}]}),
        ] {
            assert!(Proposal::parse(&bad).is_err(), "{bad}");
        }
        let many: Vec<_> = (0..=MAX_OPS)
            .map(|_| json!({"op": "delete_block", "uuid": A}))
            .collect();
        assert!(Proposal::parse(&json!({"page": "p", "ops": many})).is_err());
    }

    #[test]
    fn stale_unknown_and_conflicting_proposals_are_rejected() {
        let (q, join) = queue();
        let applier = QueueEditApplier::new(q.clone());
        let stale = proposal(json!([{"op": "update_block", "uuid": A,
            "expected_text": "old text", "text": "new"}]));
        assert!(matches!(applier.preview(&stale), Err(EditError::Stale(_))));
        let unknown = proposal(json!([{"op": "delete_block",
            "uuid": "33333333-3333-4333-8333-333333333333"}]));
        assert!(matches!(
            applier.apply(&unknown),
            Err(EditError::UnknownBlock(_))
        ));
        let both = proposal(json!([
            {"op": "delete_block", "uuid": A},
            {"op": "set_property", "uuid": A, "key": "k", "value": "v"}]));
        assert!(matches!(applier.preview(&both), Err(EditError::Invalid(_))));
        let into_self = proposal(json!([{"op": "move_block", "uuid": A,
            "target_uuid": A, "place": "after"}]));
        assert!(matches!(
            applier.preview(&into_self),
            Err(EditError::Invalid(_))
        ));
        let mut missing = proposal(json!([{"op": "delete_block", "uuid": A}]));
        missing.page = "Nowhere".into();
        assert!(matches!(
            applier.preview(&missing),
            Err(EditError::PageNotLoaded(_))
        ));
        assert_eq!(texts(&q).len(), 3, "nothing was written");
        drop(q);
        let _ = join.shutdown();
    }

    #[test]
    fn approved_proposal_is_one_group_and_undoable() {
        let (q, join) = queue();
        let applier = QueueEditApplier::new(q.clone());
        let p = proposal(json!([
            {"op": "update_block", "uuid": A, "expected_text": format!("alpha\nid:: {A}"),
             "text": "ALPHA"},
            {"op": "insert_block", "text": "gamma", "after_uuid": B},
            {"op": "set_property", "uuid": B, "key": "status", "value": "open"},
            {"op": "move_block", "uuid": B, "target_uuid": A, "place": "before"},
        ]));
        let preview = applier.preview(&p).expect("preview");
        assert_eq!(preview.ops.len(), 4);
        assert_eq!(preview.ops[0].kind, "update");
        let before = texts(&q);
        let applied = applier.apply(&p).expect("apply");
        assert_eq!(applied.txs.len(), 4);
        let after = texts(&q);
        assert!(
            after
                .iter()
                .any(|t| t.starts_with("ALPHA") && t.contains(A)),
            "{after:?}"
        );
        assert!(after.iter().any(|t| t == "gamma"));
        assert!(after.iter().any(|t| t.contains("status:: open")));
        // The writer attributed the group to the agent.
        assert!(q.audit().iter().any(|a| a.source == Source::Agent));
        // Undo the group in reverse order, as the audit log does.
        let mut ops = Vec::new();
        for tx in applied.txs.iter().rev() {
            ops.extend(tx.inverse_ops().expect("inverse"));
        }
        q.execute(Source::Ui, Request::Commit { label: "undo", ops })
            .expect("undo");
        assert_eq!(texts(&q), before);
        drop(q);
        let _ = join.shutdown();
    }

    #[test]
    fn a_stale_update_is_refused_and_leaves_the_page_untouched() {
        let (q, join) = queue();
        let applier = QueueEditApplier::new(q.clone());
        let p = proposal(json!([
            {"op": "insert_block", "text": "first"},
            {"op": "update_block", "uuid": A, "expected_text": format!("alpha\nid:: {A}"),
             "text": "x"},
        ]));
        let snap = q.snapshot(&PageKey::from_title("Notes")).expect("snap");
        // The user edits the block after the card was shown.
        q.run(
            Source::Ui,
            "edit",
            Cmd::SetText {
                id: snap.blocks[0].id,
                text: format!("changed\nid:: {A}"),
            },
        )
        .expect("edit");
        let before = texts(&q);
        assert!(matches!(applier.apply(&p), Err(EditError::Stale(_))));
        assert_eq!(texts(&q), before);
        drop(q);
        let _ = join.shutdown();
    }

    struct FakeResolver {
        queue: CommandQueue,
        plain_uuid: String,
    }

    impl PageResolver for FakeResolver {
        fn ensure_loaded(&self, page: &str) -> bool {
            if page != "Lazy" {
                return false;
            }
            self.queue
                .execute(
                    Source::Ui,
                    Request::LoadPage {
                        key: PageKey::from_title("Lazy"),
                        title: "Lazy".into(),
                        path: GraphPath::new("pages/Lazy.md").ok(),
                        bytes: b"- one\n- two\n".to_vec(),
                    },
                )
                .is_ok()
        }

        fn indexed_blocks(&self, page: &str) -> Vec<IndexedBlock> {
            let row = |u: &str, t: &str| IndexedBlock {
                uuid: u.into(),
                text: t.into(),
            };
            match page {
                "Lazy" => vec![row(&self.plain_uuid, "one"), row(B, "stale index text")],
                _ => Vec::new(),
            }
        }
    }

    #[test]
    fn resolver_loads_pages_and_addresses_blocks_without_id() {
        let (q, join) = queue();
        let uuid = "44444444-4444-4444-8444-444444444444".to_owned();
        let applier = QueueEditApplier::new(q.clone()).with_resolver(Arc::new(FakeResolver {
            queue: q.clone(),
            plain_uuid: uuid.clone(),
        }));
        let mut p = proposal(json!([{"op": "update_block", "uuid": uuid,
            "expected_text": "one", "text": "ONE"}]));
        p.page = "Lazy".into();
        // The page was not loaded; the resolver loads it and maps the index uuid.
        let preview = applier.preview(&p).expect("preview");
        assert_eq!(preview.ops[0].before.as_deref(), Some("one"));
        applier.apply(&p).expect("apply");
        let snap = q.snapshot(&PageKey::from_title("Lazy")).expect("snap");
        assert_eq!(snap.blocks[0].text, "ONE");
        assert!(
            snap.blocks.iter().all(|b| b.uuid.is_none()),
            "no id:: is written for the user"
        );
        // A block whose text differs from the index (lagging index) is not addressable.
        let mut bad = proposal(json!([{"op": "delete_block", "uuid": B}]));
        bad.page = "Lazy".into();
        assert!(matches!(
            applier.preview(&bad),
            Err(EditError::UnknownBlock(_))
        ));
        // Unknown pages stay not loaded.
        let mut none = proposal(json!([{"op": "delete_block", "uuid": A}]));
        none.page = "Nowhere".into();
        assert!(matches!(
            applier.preview(&none),
            Err(EditError::PageNotLoaded(_))
        ));
        drop(applier);
        drop(q);
        let _ = join.shutdown();
    }
}
