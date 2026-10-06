//! Agent write bridge: turns write tool calls into core commands on the single-writer queue
//! (BIT-SP-0007.R7, R9, R10, R12, R13; design `mcp-server.md` section 4; rule 3 of AGENTS.md).
//!
//! Every call becomes a *group* of core transactions submitted with `Source::Mcp`. A group is
//! all-or-nothing: if a step fails, the steps already committed are undone with their inverse
//! ops before the error is returned. The committed transactions of a group are handed to the
//! audit log, which can undo the whole call later by committing their inverses in reverse order
//! (one undo step for the user, `Request::Commit` of ready-made ops).
//!
//! Block identity: tools address blocks by `uuid`. A uuid is looked up first in the pages loaded
//! in core (blocks with a persisted `id::`, including those agents created), then in the index
//! (`id::` or generated uuid), in which case the page is loaded from disk and the block is found
//! by its document position and verified against the indexed text. A target without a persisted
//! `id::` gets its index uuid written as `id::` when the call edits it, so the uuid the agent
//! holds stays valid. Blocks created by agents always get an `id::`.

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::Arc;

use bitacora_config::EffectiveConfig;
use bitacora_core::date::Date;
use bitacora_core::editor::{
    BlockId as CoreId, Cmd, CommitError, Op, OpError, Refusal, Target, Transaction, new_page_path,
    text_is_representable,
};
use bitacora_core::graph::PageKey;
use bitacora_core::graph_path::GraphPath;
use bitacora_core::journal::{detect_journal, journal_page};
use bitacora_core::queue::{CommandQueue, PageSnapshot, QueueError, Request, Response, Source};
use bitacora_markdown::edit::properties::{get_property, remove_property, set_property};
use bitacora_markdown::edit::state::set_marker;

use crate::audit::{UndoData, UndoError};
use crate::policy::{READONLY_PROPERTY, WriteGate, WritePolicy, is_marked_readonly};
use crate::reader::{GraphReader, PageInfo};
use crate::render::{Code, ToolError};
use crate::status::{SyncState, SyncStatusProvider};

/// Task markers accepted by `set_task_status`.
pub(crate) const TASK_STATUSES: &[&str] = &[
    "TODO",
    "DOING",
    "DONE",
    "LATER",
    "NOW",
    "WAITING",
    "CANCELED",
    "CANCELLED",
];

/// Everything a write needs besides the queue.
pub(crate) struct Env<'a> {
    pub r: &'a dyn GraphReader,
    pub policy: &'a WritePolicy,
    pub gate: &'a dyn WriteGate,
    pub sync: &'a dyn SyncStatusProvider,
}

/// A block an agent asks to create.
#[derive(Debug, Clone)]
pub(crate) struct NewBlock {
    pub content: String,
    pub properties: Vec<(String, String)>,
    pub children: Vec<NewBlock>,
}

impl NewBlock {
    pub(crate) fn count(&self) -> usize {
        1 + self.children.iter().map(Self::count).sum::<usize>()
    }
}

/// Where a new block goes.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Place {
    /// Last top-level block of a page.
    RootLast,
    /// First top-level block of a page (the id is the current first root, if any).
    RootFirst(Option<CoreId>),
    /// Last child of a block.
    ChildLast(CoreId),
    /// First child of a block (the flag says whether it has children).
    ChildFirst(CoreId, bool),
    /// Next sibling.
    After(CoreId),
    /// Previous sibling.
    Before(CoreId),
}

/// Position argument of `insert_block` / `move_block`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Position {
    After,
    Before,
    FirstChild,
    LastChild,
}

/// One block touched by a call.
#[derive(Debug, Clone, serde::Serialize, schemars::JsonSchema)]
pub(crate) struct Affected {
    pub uuid: String,
    /// Version of the block after the call (absent for removed blocks).
    pub version: Option<String>,
    pub page: String,
}

/// What a write did.
#[derive(Debug, Default)]
pub(crate) struct Applied {
    pub txs: Vec<Transaction>,
    pub affected: Vec<Affected>,
    pub pages: Vec<String>,
    /// Canonical page name for page-level tools.
    pub page: Option<String>,
    /// File of the page, when known.
    pub file: Option<String>,
    pub created: bool,
    pub extra: serde_json::Value,
    /// Page versions after the call (see `CallInfo::fingerprint`).
    pub fingerprint: Vec<(PageKey, Option<u64>)>,
}

/// The write side of the server over core's command queue.
#[derive(Clone)]
pub struct QueueBridge {
    queue: CommandQueue,
    root: PathBuf,
    config: EffectiveConfig,
}

impl std::fmt::Debug for QueueBridge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QueueBridge")
            .field("root", &self.root)
            .finish_non_exhaustive()
    }
}

// ------------------------------------------------------------------ small helpers

fn internal(msg: impl Into<String>) -> ToolError {
    ToolError::new(Code::Internal, msg)
}

fn map_commit(c: CommitError) -> ToolError {
    match c {
        CommitError::Refused(r) => match r {
            Refusal::UnknownBlock(_) => ToolError::not_found("block"),
            Refusal::ReadOnly => ToolError::new(
                Code::ReadOnly,
                "the page is read-only (for example an .org page)",
            ),
            Refusal::Unrepresentable => ToolError::new(
                Code::InvalidContent,
                "the text cannot be stored in one block",
            ),
            other => ToolError::invalid(other.to_string()),
        },
        CommitError::Op { source, .. } => match source {
            OpError::Stale(_) | OpError::PageExists(_) | OpError::PathTaken(_) => {
                ToolError::new(Code::Conflict, format!("the graph changed: {source}"))
            }
            OpError::Unrepresentable => ToolError::new(
                Code::InvalidContent,
                "the text cannot be stored in one block",
            ),
            other => internal(other.to_string()),
        },
        other => internal(other.to_string()),
    }
}

fn map_queue(e: QueueError) -> ToolError {
    match e {
        QueueError::Commit(c) => map_commit(c),
        QueueError::Stale(p) => ToolError::new(
            Code::Conflict,
            format!("`{p}` changed on disk; re-read and retry"),
        ),
        QueueError::PageDirty(_) => ToolError::new(
            Code::Conflict,
            "the page has unsaved edits and was not reloaded",
        ),
        QueueError::Invalid(m) => ToolError::invalid(m),
        QueueError::Rename(r) => ToolError::invalid(r.to_string()),
        QueueError::PageConflicted(p) => ToolError::new(
            Code::BlockInConflict,
            format!("`{p:?}` has an unresolved conflict with the file on disk"),
        ),
        QueueError::Busy | QueueError::Closed | QueueError::Store(_) => internal(e.to_string()),
    }
}

/// `blake3(uuid, text)[..16]`, the same formula as the index reader's block `version`.
pub(crate) fn version_of(uuid: &str, text: &str) -> String {
    let mut h = blake3::Hasher::new();
    h.update(uuid.as_bytes());
    h.update(text.as_bytes());
    h.finalize().to_hex()[..16].to_owned()
}

/// A random UUID version 4 in canonical lower-case form.
pub(crate) fn new_uuid() -> Result<String, ToolError> {
    let mut b = [0u8; 16];
    getrandom::fill(&mut b).map_err(|e| internal(format!("random source unavailable: {e}")))?;
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let hex: String = b.iter().map(|x| format!("{x:02x}")).collect();
    Ok(format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    ))
}

const RESERVED_KEYS: &[&str] = &["id", "collapsed"];

fn reserved(key: &str) -> bool {
    let k = key.trim().to_lowercase().replace('_', "-");
    RESERVED_KEYS.contains(&k.as_str())
}

/// Validates the content of one block (BIT-T-0201).
pub(crate) fn validate_content(content: &str) -> Result<String, ToolError> {
    let text = content.replace("\r\n", "\n");
    let invalid = |m: &str| ToolError::new(Code::InvalidContent, m);
    if text.trim().is_empty() {
        return Err(invalid("`content` must not be empty"));
    }
    let first = text.trim_start().lines().next().unwrap_or_default();
    if first.starts_with("- ") || first == "-" {
        return Err(invalid(
            "`content` is the text of ONE block without the leading `- `; use `blocks` for several",
        ));
    }
    if !text_is_representable(&text) {
        return Err(invalid(
            "`content` would split into several blocks (a line starts like a list item); use `blocks`",
        ));
    }
    for key in RESERVED_KEYS {
        if get_property(&text, key).is_some() {
            return Err(invalid(&format!(
                "the `{key}::` property is managed by Bitacora and cannot be set in content"
            )));
        }
    }
    Ok(text.trim_end().to_owned())
}

/// Validates a property key/value pair.
pub(crate) fn validate_property(key: &str, value: &str) -> Result<(), ToolError> {
    let key = key.trim();
    if key.is_empty() || key.chars().any(|c| c.is_whitespace() || c == ':') || value.contains('\n')
    {
        return Err(ToolError::new(
            Code::InvalidContent,
            "a property key must be a single word without `:` and its value one line",
        ));
    }
    if reserved(key) {
        return Err(ToolError::new(
            Code::InvalidContent,
            format!("the `{key}::` property is managed by Bitacora"),
        ));
    }
    Ok(())
}

/// Text of a new block: content, properties, then the persistent `id::`.
fn new_block_text(nb: &NewBlock, uuid: &str) -> String {
    let mut t = nb.content.clone();
    for (k, v) in &nb.properties {
        t = set_property(&t, k, v);
    }
    set_property(&t, "id", uuid)
}

/// Case-insensitive replacement of page references to `old` by `new`:
/// `[[old]]`, `#[[old]]` and `#old` (the latter only when `old` is a single word).
pub(crate) fn rewrite_links(text: &str, old: &str, new: &str) -> String {
    fn replace_ci(text: &str, needle: &str, with: &str, word_end: bool) -> String {
        let lower = text.to_lowercase();
        let needle_l = needle.to_lowercase();
        if lower.len() != text.len() {
            // Case folding changed byte lengths: only exact matches are rewritten.
            return text.replace(needle, with);
        }
        let mut out = String::with_capacity(text.len());
        let mut i = 0;
        while let Some(rel) = lower[i..].find(&needle_l) {
            let at = i + rel;
            let end = at + needle_l.len();
            let boundary_ok = !word_end
                || lower[end..]
                    .chars()
                    .next()
                    .is_none_or(|c| !(c.is_alphanumeric() || c == '_' || c == '-' || c == '/'));
            let start_ok = !word_end
                || lower[..at]
                    .chars()
                    .next_back()
                    .is_none_or(|c| !(c.is_alphanumeric() || c == '_'));
            out.push_str(&text[i..at]);
            if boundary_ok && start_ok && text.is_char_boundary(at) && text.is_char_boundary(end) {
                out.push_str(with);
            } else {
                out.push_str(&text[at..end]);
            }
            i = end;
        }
        out.push_str(&text[i..]);
        out
    }
    let mut t = replace_ci(text, &format!("[[{old}]]"), &format!("[[{new}]]"), false);
    if !old.contains(char::is_whitespace) {
        let with = if new.contains(char::is_whitespace) {
            format!("#[[{new}]]")
        } else {
            format!("#{new}")
        };
        t = replace_ci(&t, &format!("#{old}"), &with, true);
    }
    t
}

// ------------------------------------------------------------------ groups

struct Group<'a> {
    q: &'a CommandQueue,
    txs: Vec<Transaction>,
    affected: Vec<Affected>,
    pages: BTreeSet<String>,
}

impl Group<'_> {
    fn cmd(&mut self, label: &'static str, cmd: Cmd) -> Result<Transaction, ToolError> {
        let tx = self.q.run(Source::Mcp, label, cmd).map_err(map_queue)?;
        self.txs.push(tx.clone());
        Ok(tx)
    }

    /// Like [`Group::cmd`] but a command that changes nothing is `Ok(None)`.
    fn cmd_or_noop(
        &mut self,
        label: &'static str,
        cmd: Cmd,
    ) -> Result<Option<Transaction>, ToolError> {
        match self.q.run(Source::Mcp, label, cmd) {
            Ok(tx) => {
                self.txs.push(tx.clone());
                Ok(Some(tx))
            }
            Err(QueueError::Commit(CommitError::Refused(Refusal::NoChange))) => Ok(None),
            Err(e) => Err(map_queue(e)),
        }
    }

    fn ops(&mut self, label: &'static str, ops: Vec<Op>) -> Result<Transaction, ToolError> {
        match self
            .q
            .execute(Source::Mcp, Request::Commit { label, ops })
            .map_err(map_queue)?
        {
            Response::Committed(tx) => {
                self.txs.push(tx.clone());
                Ok(tx)
            }
            _ => Err(internal("unexpected response from the writer")),
        }
    }

    /// Undoes the committed steps (best effort; failures are logged).
    fn rollback(&mut self) {
        let mut ops = Vec::new();
        for tx in self.txs.iter().rev() {
            match tx.inverse_ops() {
                Ok(o) => ops.extend(o),
                Err(e) => {
                    tracing::warn!(error = %e, "cannot invert an agent write while rolling back");
                    return;
                }
            }
        }
        if ops.is_empty() {
            return;
        }
        if let Err(e) = self.q.execute(
            Source::Mcp,
            Request::Commit {
                label: "Agent edit (rolled back)",
                ops,
            },
        ) {
            tracing::warn!(error = %e, "rolling back an agent write failed");
        }
        self.txs.clear();
    }

    fn touch(&mut self, page: &str) {
        self.pages.insert(page.to_owned());
    }
}

struct Resolved {
    key: PageKey,
    id: CoreId,
    snap: Arc<PageSnapshot>,
    text: String,
    uuid: String,
    persisted: bool,
    page: String,
}

fn inserted_id(tx: &Transaction) -> Option<CoreId> {
    tx.ops.iter().find_map(|o| match o {
        Op::InsertSubtree { subtree, .. } => Some(subtree.id),
        _ => None,
    })
}

fn conflict_with_current(uuid: &str, text: &str, why: &str) -> ToolError {
    ToolError::new(Code::Conflict, why).with_extra(serde_json::json!({
        "current": {
            "uuid": uuid,
            "content": text,
            "version": version_of(uuid, text),
        }
    }))
}

impl QueueBridge {
    /// A bridge over the graph's command queue. `root` is the canonical graph folder and
    /// `config` its effective config (page paths, journal names).
    #[must_use]
    pub fn new(queue: CommandQueue, root: impl Into<PathBuf>, config: EffectiveConfig) -> Self {
        Self {
            queue,
            root: root.into(),
            config,
        }
    }

    fn group<F>(&self, f: F) -> Result<Applied, ToolError>
    where
        F: FnOnce(&mut Group<'_>) -> Result<Applied, ToolError>,
    {
        let mut g = Group {
            q: &self.queue,
            txs: Vec::new(),
            affected: Vec::new(),
            pages: BTreeSet::new(),
        };
        match f(&mut g) {
            Ok(mut a) => {
                a.txs = std::mem::take(&mut g.txs);
                a.affected = std::mem::take(&mut g.affected);
                a.pages = std::mem::take(&mut g.pages).into_iter().collect();
                a.fingerprint = self.fingerprint(&a.txs);
                Ok(a)
            }
            Err(e) => {
                g.rollback();
                Err(e)
            }
        }
    }

    /// Current version of every page the transactions touched.
    fn fingerprint(&self, txs: &[Transaction]) -> Vec<(PageKey, Option<u64>)> {
        let keys: BTreeSet<&PageKey> = txs.iter().flat_map(|t| t.pages.iter()).collect();
        keys.into_iter()
            .map(|k| (k.clone(), self.queue.snapshot(k).map(|s| page_hash(&s))))
            .collect()
    }

    // -------------------------------------------------------------- pages and blocks lookup

    fn load_page(&self, info: &PageInfo) -> Result<(PageKey, Arc<PageSnapshot>), ToolError> {
        let key = PageKey::from_title(&info.original_name);
        if let Some(s) = self.queue.snapshot(&key) {
            return Ok((key, s));
        }
        let rel = info.file.as_deref().ok_or_else(|| {
            ToolError::not_found(format!("file of page `{}`", info.original_name))
        })?;
        self.load_file(&key, &info.original_name, rel)
    }

    fn load_file(
        &self,
        key: &PageKey,
        title: &str,
        rel: &str,
    ) -> Result<(PageKey, Arc<PageSnapshot>), ToolError> {
        let path = GraphPath::new(rel).map_err(|e| ToolError::invalid(e.to_string()))?;
        let bytes = std::fs::read(path.to_fs_path(&self.root))
            .map_err(|e| internal(format!("cannot read `{rel}`: {e}")))?;
        self.queue
            .execute(
                Source::Mcp,
                Request::LoadPage {
                    key: key.clone(),
                    title: title.to_owned(),
                    path: Some(path),
                    bytes,
                },
            )
            .map_err(map_queue)?;
        let snap = self
            .queue
            .snapshot(key)
            .ok_or_else(|| internal("page did not load"))?;
        Ok((key.clone(), snap))
    }

    fn resolve(&self, env: &Env<'_>, uuid_in: &str) -> Result<Resolved, ToolError> {
        let uuid = uuid_in.trim().to_ascii_lowercase();
        if uuid.is_empty() {
            return Err(ToolError::invalid("`uuid` must not be empty"));
        }
        for key in self.queue.snapshot_keys() {
            let Some(snap) = self.queue.snapshot(&key) else {
                continue;
            };
            let found = snap
                .blocks
                .iter()
                .find(|b| b.uuid.is_some_and(|u| u.to_string() == uuid))
                .cloned();
            if let Some(b) = found {
                return Ok(Resolved {
                    key,
                    id: b.id,
                    text: b.text,
                    uuid,
                    persisted: true,
                    page: snap.title.clone(),
                    snap,
                });
            }
        }
        let info = env
            .r
            .block(&uuid)?
            .ok_or_else(|| ToolError::not_found(format!("block `{uuid}`")))?;
        let pos = env
            .r
            .block_position(&uuid)?
            .ok_or_else(|| ToolError::not_found(format!("block `{uuid}`")))?;
        let page = env
            .r
            .page(&info.page)?
            .ok_or_else(|| ToolError::not_found(format!("page `{}`", info.page)))?;
        let (key, snap) = self.load_page(&page)?;
        // The index can lag behind the editable page (earlier writes not indexed yet): trust the
        // document position only when the text agrees, else look for the one block with this text.
        let same =
            |b: &bitacora_core::queue::SnapshotBlock| b.text.trim_end() == info.content.trim_end();
        let found = snap.blocks.get(pos).filter(|b| same(b)).or_else(|| {
            let mut hits = snap.blocks.iter().filter(|b| same(b));
            match (hits.next(), hits.next()) {
                (Some(b), None) => Some(b),
                _ => None,
            }
        });
        let Some(b) = found else {
            return Err(conflict_with_current(
                &uuid,
                &info.content,
                "the block changed since it was indexed; re-read and retry",
            ));
        };
        Ok(Resolved {
            key,
            id: b.id,
            text: b.text.clone(),
            uuid,
            persisted: b.uuid.is_some(),
            page: snap.title.clone(),
            snap: Arc::clone(&snap),
        })
    }

    fn check_version(res: &Resolved, expected: Option<&str>) -> Result<(), ToolError> {
        match expected {
            Some(e) if !e.trim().is_empty() && e.trim() != version_of(&res.uuid, &res.text) => {
                Err(conflict_with_current(
                    &res.uuid,
                    &res.text,
                    "`expected_version` does not match the current block; re-read it",
                ))
            }
            _ => Ok(()),
        }
    }

    fn guard_page(
        &self,
        env: &Env<'_>,
        page: &str,
        block: Option<&str>,
        block_level: bool,
    ) -> Result<(), ToolError> {
        let protected = |why: &str| {
            ToolError::new(
                Code::ProtectedPage,
                format!("page `{page}` is protected from agent writes ({why})"),
            )
        };
        if env.policy.in_protected_namespace(page) {
            return Err(protected("protected namespace"));
        }
        if let Ok(Some(p)) = env.r.page(page)
            && is_marked_readonly(p.properties.iter())
        {
            return Err(protected(READONLY_PROPERTY));
        }
        if let Some(s) = self.queue.snapshot(&PageKey::from_title(page))
            && s.preamble
                .as_deref()
                .and_then(|p| get_property(p, READONLY_PROPERTY))
                .is_some_and(|v| v.trim().eq_ignore_ascii_case("true"))
        {
            return Err(protected(READONLY_PROPERTY));
        }
        if block_level {
            let st = env.sync.status();
            if st.state == SyncState::Conflicted
                && st
                    .conflict_pages
                    .iter()
                    .any(|n| n.eq_ignore_ascii_case(page))
            {
                return Err(ToolError::new(
                    Code::BlockInConflict,
                    format!("page `{page}` has a pending sync conflict"),
                ));
            }
            if let Some(u) = block
                && let Some(ms) = env.gate.busy(page, u)
            {
                return Err(ToolError::new(
                    Code::BlockBusy,
                    "the block is being edited in the app",
                )
                .with_extra(serde_json::json!({ "retry_after_ms": ms })));
            }
        }
        Ok(())
    }

    // -------------------------------------------------------------- insertion

    fn insert_tree(
        &self,
        g: &mut Group<'_>,
        key: &PageKey,
        page: &str,
        place: Place,
        nb: &NewBlock,
    ) -> Result<CoreId, ToolError> {
        let uuid = new_uuid()?;
        let text = new_block_text(nb, &uuid);
        let label = "Agent: insert block";
        let id = match place {
            Place::RootLast => {
                let tx = g.cmd(
                    label,
                    Cmd::InsertChild {
                        page: key.clone(),
                        parent: None,
                        text: text.clone(),
                    },
                )?;
                inserted_id(&tx).ok_or_else(|| internal("insert produced no block"))?
            }
            Place::RootFirst(first) => {
                let tx = g.cmd(
                    label,
                    Cmd::InsertChild {
                        page: key.clone(),
                        parent: None,
                        text: text.clone(),
                    },
                )?;
                let id = inserted_id(&tx).ok_or_else(|| internal("insert produced no block"))?;
                if let Some(f) = first {
                    g.cmd(
                        label,
                        Cmd::MoveBlocks {
                            ids: vec![id],
                            target: Target::Before(f),
                        },
                    )?;
                }
                id
            }
            Place::ChildLast(parent) => {
                let tx = g.cmd(
                    label,
                    Cmd::InsertChild {
                        page: key.clone(),
                        parent: Some(parent),
                        text: text.clone(),
                    },
                )?;
                inserted_id(&tx).ok_or_else(|| internal("insert produced no block"))?
            }
            Place::ChildFirst(parent, has_children) => {
                let tx = g.cmd(
                    label,
                    Cmd::InsertChild {
                        page: key.clone(),
                        parent: Some(parent),
                        text: text.clone(),
                    },
                )?;
                let id = inserted_id(&tx).ok_or_else(|| internal("insert produced no block"))?;
                if has_children {
                    g.cmd(
                        label,
                        Cmd::MoveBlocks {
                            ids: vec![id],
                            target: Target::FirstChild(parent),
                        },
                    )?;
                }
                id
            }
            Place::After(after) => {
                let tx = g.cmd(
                    label,
                    Cmd::InsertSibling {
                        after,
                        text: text.clone(),
                    },
                )?;
                inserted_id(&tx).ok_or_else(|| internal("insert produced no block"))?
            }
            Place::Before(t) => {
                let tx = g.cmd(
                    label,
                    Cmd::InsertSibling {
                        after: t,
                        text: text.clone(),
                    },
                )?;
                let id = inserted_id(&tx).ok_or_else(|| internal("insert produced no block"))?;
                g.cmd(
                    label,
                    Cmd::MoveBlocks {
                        ids: vec![id],
                        target: Target::Before(t),
                    },
                )?;
                id
            }
        };
        g.touch(page);
        g.affected.push(Affected {
            version: Some(version_of(&uuid, &text)),
            uuid,
            page: page.to_owned(),
        });
        for child in &nb.children {
            self.insert_tree(g, key, page, Place::ChildLast(id), child)?;
        }
        Ok(id)
    }

    /// Inserts `blocks` starting at `first`; later blocks follow their predecessor.
    fn insert_many(
        &self,
        g: &mut Group<'_>,
        key: &PageKey,
        page: &str,
        first: Place,
        blocks: &[NewBlock],
    ) -> Result<(), ToolError> {
        let mut prev: Option<CoreId> = None;
        for (i, nb) in blocks.iter().enumerate() {
            let place = match (i, first, prev) {
                (0, p, _) => p,
                (_, Place::RootLast, _) => Place::RootLast,
                (_, Place::ChildLast(p), _) => Place::ChildLast(p),
                (_, _, Some(p)) => Place::After(p),
                (_, p, None) => p,
            };
            prev = Some(self.insert_tree(g, key, page, place, nb)?);
        }
        Ok(())
    }

    /// Finds or creates the page; returns its key, canonical title and whether it was created.
    fn ensure_page(
        &self,
        env: &Env<'_>,
        g: &mut Group<'_>,
        name: &str,
    ) -> Result<(PageKey, String, bool), ToolError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(ToolError::invalid("page name must not be empty"));
        }
        let name = if name.eq_ignore_ascii_case("today") {
            let day = env.r.today();
            let date = Date::new(
                i32::try_from(day / 10_000).unwrap_or(1970),
                u8::try_from((day / 100) % 100).unwrap_or(1),
                u8::try_from(day % 100).unwrap_or(1),
            )
            .ok_or_else(|| internal("invalid local date"))?;
            journal_page(date, &self.config).title
        } else {
            name.to_owned()
        };
        if let Some(info) = env.r.page(&name)?
            && info.file.is_some()
        {
            self.guard_page(env, &info.original_name, None, false)?;
            let (key, _) = self.load_page(&info)?;
            return Ok((key, info.original_name, false));
        }
        let key = PageKey::from_title(&name);
        if let Some(s) = self.queue.snapshot(&key) {
            self.guard_page(env, &s.title, None, false)?;
            return Ok((key, s.title.clone(), false));
        }
        self.guard_page(env, &name, None, false)?;
        let path =
            new_page_path(&name, &self.config).map_err(|e| ToolError::invalid(e.to_string()))?;
        if path.to_fs_path(&self.root).exists() {
            // Present on disk but not indexed yet.
            let (key, snap) = self.load_file(&key, &name, path.as_str())?;
            return Ok((key, snap.title.clone(), false));
        }
        g.ops(
            "Agent: create page",
            vec![Op::CreatePage {
                page: key.clone(),
                title: name.clone(),
                path: Some(path),
                restored: None,
            }],
        )?;
        g.touch(&name);
        Ok((key, name, true))
    }

    fn page_file(&self, key: &PageKey) -> Option<String> {
        self.queue
            .snapshot(key)
            .and_then(|s| s.path.as_ref().map(|p| p.as_str().to_owned()))
    }

    // -------------------------------------------------------------- tools

    /// `create_page`.
    pub(crate) fn create_page(
        &self,
        env: &Env<'_>,
        name: &str,
        properties: &[(String, String)],
        blocks: &[NewBlock],
        error_if_exists: bool,
    ) -> Result<Applied, ToolError> {
        let name_t = name.trim();
        if name_t.is_empty() {
            return Err(ToolError::invalid("`name` must not be empty"));
        }
        let existing = env.r.page(name_t)?.filter(|p| p.file.is_some());
        if let Some(info) = existing {
            if error_if_exists {
                return Err(ToolError::new(
                    Code::Conflict,
                    format!("page `{}` already exists", info.original_name),
                )
                .with_extra(serde_json::json!({ "page": info.original_name })));
            }
            return Ok(Applied {
                page: Some(info.original_name),
                file: info.file,
                extra: serde_json::Value::Null,
                ..Applied::default()
            });
        }
        self.group(|g| {
            let (key, title, created) = self.ensure_page(env, g, name_t)?;
            if !created && error_if_exists {
                return Err(ToolError::new(
                    Code::Conflict,
                    format!("page `{title}` already exists"),
                ));
            }
            for (k, v) in properties {
                g.cmd_or_noop(
                    "Agent: set page property",
                    Cmd::SetPageProperty {
                        page: key.clone(),
                        key: k.clone(),
                        value: v.clone(),
                    },
                )?;
            }
            self.insert_many(g, &key, &title, Place::RootLast, blocks)?;
            g.touch(&title);
            Ok(Applied {
                file: self.page_file(&key),
                page: Some(title),
                created,
                ..Applied::default()
            })
        })
    }

    /// `append_block` / `prepend_block`.
    pub(crate) fn add_to_page(
        &self,
        env: &Env<'_>,
        page: &str,
        blocks: &[NewBlock],
        prepend: bool,
    ) -> Result<Applied, ToolError> {
        self.group(|g| {
            let (key, title, created) = self.ensure_page(env, g, page)?;
            let first = if prepend {
                let first_root = self
                    .queue
                    .snapshot(&key)
                    .and_then(|s| s.blocks.iter().find(|b| b.parent.is_none()).map(|b| b.id));
                Place::RootFirst(first_root)
            } else {
                Place::RootLast
            };
            self.insert_many(g, &key, &title, first, blocks)?;
            Ok(Applied {
                file: self.page_file(&key),
                page: Some(title),
                created,
                ..Applied::default()
            })
        })
    }

    /// `insert_block`.
    pub(crate) fn insert_block(
        &self,
        env: &Env<'_>,
        target: &str,
        position: Position,
        blocks: &[NewBlock],
    ) -> Result<Applied, ToolError> {
        let res = self.resolve(env, target)?;
        self.guard_page(env, &res.page, Some(&res.uuid), true)?;
        let has_children = res.snap.blocks.iter().any(|b| b.parent == Some(res.id));
        let place = match position {
            Position::After => Place::After(res.id),
            Position::Before => Place::Before(res.id),
            Position::FirstChild => Place::ChildFirst(res.id, has_children),
            Position::LastChild => Place::ChildLast(res.id),
        };
        self.group(|g| {
            self.insert_many(g, &res.key, &res.page, place, blocks)?;
            Ok(Applied {
                page: Some(res.page.clone()),
                ..Applied::default()
            })
        })
    }

    fn edit_text(
        &self,
        g: &mut Group<'_>,
        res: &Resolved,
        new_text: String,
        label: &'static str,
    ) -> Result<(), ToolError> {
        let tx = g.cmd_or_noop(
            label,
            Cmd::SetText {
                id: res.id,
                text: new_text.clone(),
            },
        )?;
        g.touch(&res.page);
        let text = if tx.is_some() {
            new_text
        } else {
            res.text.clone()
        };
        g.affected.push(Affected {
            version: Some(version_of(&res.uuid, &text)),
            uuid: res.uuid.clone(),
            page: res.page.clone(),
        });
        Ok(())
    }

    /// Ensures the lazily persisted `id::` of an edited block.
    fn with_id(res: &Resolved, text: String) -> String {
        if res.persisted {
            text
        } else {
            set_property(&text, "id", &res.uuid)
        }
    }

    /// `update_block`.
    pub(crate) fn update_block(
        &self,
        env: &Env<'_>,
        uuid: &str,
        content: &str,
        properties: &[(String, String)],
        expected: Option<&str>,
    ) -> Result<Applied, ToolError> {
        let content = validate_content(content)?;
        let res = self.resolve(env, uuid)?;
        self.guard_page(env, &res.page, Some(&res.uuid), true)?;
        Self::check_version(&res, expected)?;
        let mut text = content;
        for key in RESERVED_KEYS {
            if let Some(v) = get_property(&res.text, key) {
                text = set_property(&text, key, &v);
            }
        }
        for (k, v) in properties {
            text = set_property(&text, k, v);
        }
        let text = Self::with_id(&res, text);
        self.group(|g| {
            self.edit_text(g, &res, text, "Agent: update block")?;
            Ok(Applied {
                page: Some(res.page.clone()),
                ..Applied::default()
            })
        })
    }

    /// `set_block_property` (`value = Some`) and `remove_block_property` (`None`).
    pub(crate) fn set_property(
        &self,
        env: &Env<'_>,
        uuid: &str,
        key: &str,
        value: Option<&str>,
    ) -> Result<Applied, ToolError> {
        validate_property(key, value.unwrap_or(""))?;
        let res = self.resolve(env, uuid)?;
        self.guard_page(env, &res.page, Some(&res.uuid), true)?;
        let text = match value {
            Some(v) => set_property(&res.text, key, v),
            None => remove_property(&res.text, key),
        };
        let text = Self::with_id(&res, text);
        self.group(|g| {
            self.edit_text(g, &res, text, "Agent: edit property")?;
            Ok(Applied {
                page: Some(res.page.clone()),
                ..Applied::default()
            })
        })
    }

    /// `set_task_status`.
    pub(crate) fn set_task_status(
        &self,
        env: &Env<'_>,
        uuid: &str,
        status: &str,
    ) -> Result<Applied, ToolError> {
        let status_u = status.trim().to_ascii_uppercase();
        let marker = if status_u.is_empty() || status_u == "NONE" {
            None
        } else if TASK_STATUSES.contains(&status_u.as_str()) {
            Some(status_u.as_str())
        } else {
            return Err(ToolError::invalid(format!(
                "`status` must be one of {} or `none`",
                TASK_STATUSES.join(", ")
            )));
        };
        let res = self.resolve(env, uuid)?;
        self.guard_page(env, &res.page, Some(&res.uuid), true)?;
        let text = Self::with_id(&res, set_marker(&res.text, marker));
        self.group(|g| {
            self.edit_text(g, &res, text, "Agent: set task status")?;
            Ok(Applied {
                page: Some(res.page.clone()),
                ..Applied::default()
            })
        })
    }

    /// `move_block`.
    pub(crate) fn move_block(
        &self,
        env: &Env<'_>,
        uuid: &str,
        target: &str,
        position: Position,
        expected: Option<&str>,
    ) -> Result<Applied, ToolError> {
        let res = self.resolve(env, uuid)?;
        let dest = self.resolve(env, target)?;
        self.guard_page(env, &res.page, Some(&res.uuid), true)?;
        self.guard_page(env, &dest.page, Some(&dest.uuid), true)?;
        Self::check_version(&res, expected)?;
        let t = match position {
            Position::After => Target::After(dest.id),
            Position::Before => Target::Before(dest.id),
            Position::FirstChild => Target::FirstChild(dest.id),
            Position::LastChild => Target::LastChild(dest.id),
        };
        self.group(|g| {
            g.cmd(
                "Agent: move block",
                Cmd::MoveBlocks {
                    ids: vec![res.id],
                    target: t,
                },
            )?;
            g.touch(&res.page);
            g.touch(&dest.page);
            g.affected.push(Affected {
                version: Some(version_of(&res.uuid, &res.text)),
                uuid: res.uuid.clone(),
                page: dest.page.clone(),
            });
            Ok(Applied {
                page: Some(dest.page.clone()),
                ..Applied::default()
            })
        })
    }

    /// `remove_block`.
    pub(crate) fn remove_block(
        &self,
        env: &Env<'_>,
        uuid: &str,
        expected: Option<&str>,
    ) -> Result<Applied, ToolError> {
        let res = self.resolve(env, uuid)?;
        self.guard_page(env, &res.page, Some(&res.uuid), true)?;
        Self::check_version(&res, expected)?;
        self.group(|g| {
            g.cmd(
                "Agent: remove block",
                Cmd::DeleteBlocks { ids: vec![res.id] },
            )?;
            g.touch(&res.page);
            g.affected.push(Affected {
                version: None,
                uuid: res.uuid.clone(),
                page: res.page.clone(),
            });
            Ok(Applied {
                page: Some(res.page.clone()),
                ..Applied::default()
            })
        })
    }

    /// `delete_page`: the file goes to `logseq/.recycle/` at the next flush (core behaviour).
    pub(crate) fn delete_page(&self, env: &Env<'_>, name: &str) -> Result<Applied, ToolError> {
        let info = env
            .r
            .page(name.trim())?
            .filter(|p| p.file.is_some())
            .ok_or_else(|| ToolError::not_found(format!("page `{name}`")))?;
        self.guard_page(env, &info.original_name, None, true)?;
        let (key, _) = self.load_page(&info)?;
        self.group(|g| {
            g.cmd("Agent: delete page", Cmd::DeletePage { page: key })?;
            g.touch(&info.original_name);
            Ok(Applied {
                page: Some(info.original_name.clone()),
                file: info.file.clone(),
                ..Applied::default()
            })
        })
    }

    /// `rename_page`: the page moves to the new title's file (the old file goes to
    /// `logseq/.recycle/`), references are rewritten, all in one undoable group.
    pub(crate) fn rename_page(
        &self,
        env: &Env<'_>,
        name: &str,
        new_name: &str,
        update_links: bool,
    ) -> Result<Applied, ToolError> {
        let new_name = new_name.trim();
        if new_name.is_empty() {
            return Err(ToolError::invalid("`new_name` must not be empty"));
        }
        let info = env
            .r
            .page(name.trim())?
            .filter(|p| p.file.is_some())
            .ok_or_else(|| ToolError::not_found(format!("page `{name}`")))?;
        if detect_journal(&info.original_name, &self.config).is_some()
            || detect_journal(new_name, &self.config).is_some()
        {
            return Err(ToolError::invalid("journal pages cannot be renamed"));
        }
        let old = info.original_name.clone();
        if let Some(other) = env.r.page(new_name)?
            && other.file.is_some()
            && !other.original_name.eq_ignore_ascii_case(&old)
        {
            return Err(ToolError::new(
                Code::Conflict,
                format!("page `{}` already exists", other.original_name),
            ));
        }
        self.guard_page(env, &old, None, true)?;
        self.guard_page(env, new_name, None, false)?;
        let new_path =
            new_page_path(new_name, &self.config).map_err(|e| ToolError::invalid(e.to_string()))?;
        let (old_key, snap) = self.load_page(&info)?;
        let new_key = PageKey::from_title(new_name);
        // Pages that reference the old title.
        let mut referrers: Vec<String> = Vec::new();
        if update_links {
            for grp in env.r.linked_references(&info)? {
                if !grp.page.eq_ignore_ascii_case(&old) && !referrers.contains(&grp.page) {
                    referrers.push(grp.page);
                }
            }
        }
        self.group(|g| {
            g.cmd("Agent: rename page", Cmd::DeletePage { page: old_key })?;
            g.ops(
                "Agent: rename page",
                vec![Op::CreatePage {
                    page: new_key.clone(),
                    title: new_name.to_owned(),
                    path: Some(new_path),
                    restored: None,
                }],
            )?;
            if snap.preamble.is_some() {
                let after = snap.preamble.as_deref().map(|p| {
                    if update_links {
                        rewrite_links(p, &old, new_name)
                    } else {
                        p.to_owned()
                    }
                });
                g.ops(
                    "Agent: rename page",
                    vec![Op::SetPreamble {
                        page: new_key.clone(),
                        before: None,
                        after,
                    }],
                )?;
            }
            let mut ids: HashMap<CoreId, CoreId> = HashMap::new();
            for b in &snap.blocks {
                let parent = match b.parent {
                    Some(p) => Some(
                        *ids.get(&p)
                            .ok_or_else(|| internal("inconsistent page tree"))?,
                    ),
                    None => None,
                };
                let text = if update_links {
                    rewrite_links(&b.text, &old, new_name)
                } else {
                    b.text.clone()
                };
                let tx = g.cmd(
                    "Agent: rename page",
                    Cmd::InsertChild {
                        page: new_key.clone(),
                        parent,
                        text,
                    },
                )?;
                let id = inserted_id(&tx).ok_or_else(|| internal("insert produced no block"))?;
                ids.insert(b.id, id);
            }
            g.touch(&old);
            g.touch(new_name);
            let mut rewritten = Vec::new();
            let mut skipped = Vec::new();
            for page in &referrers {
                if self.guard_page(env, page, None, true).is_err() {
                    skipped.push(page.clone());
                    continue;
                }
                let Some(pinfo) = env.r.page(page)?.filter(|p| p.file.is_some()) else {
                    continue;
                };
                let (pkey, psnap) = self.load_page(&pinfo)?;
                let mut changed = false;
                for b in &psnap.blocks {
                    let text = rewrite_links(&b.text, &old, new_name);
                    if text != b.text {
                        g.cmd("Agent: rewrite links", Cmd::SetText { id: b.id, text })?;
                        changed = true;
                    }
                }
                let _ = pkey;
                if changed {
                    g.touch(&pinfo.original_name);
                    rewritten.push(pinfo.original_name);
                }
            }
            Ok(Applied {
                page: Some(new_name.to_owned()),
                file: Some(new_path_str(&self.config, new_name)),
                extra: serde_json::json!({
                    "renamed_from": old,
                    "rewritten_pages": rewritten,
                    "skipped_pages": skipped,
                }),
                ..Applied::default()
            })
        })
    }

    // -------------------------------------------------------------- undo

    /// Commits the inverse of the call's transactions (newest first) as one undo step. Refused
    /// with [`UndoError::Changed`] when any touched page changed since the call.
    pub(crate) fn undo(&self, data: &UndoData) -> Result<(), UndoError> {
        for (key, version) in &data.fingerprint {
            if self.queue.snapshot(key).map(|s| page_hash(&s)) != *version {
                return Err(UndoError::Changed);
            }
        }
        let mut ops = Vec::new();
        for tx in data.txs.iter().rev() {
            ops.extend(
                tx.inverse_ops()
                    .map_err(|e| UndoError::Failed(e.to_string()))?,
            );
        }
        if ops.is_empty() {
            return Ok(());
        }
        match self.queue.execute(
            Source::Mcp,
            Request::Commit {
                label: "Undo agent edit",
                ops,
            },
        ) {
            Ok(_) => Ok(()),
            Err(QueueError::Commit(CommitError::Op { .. })) => Err(UndoError::Changed),
            Err(e) => Err(UndoError::Failed(e.to_string())),
        }
    }
}

/// Content hash of a page snapshot (structure and text); unlike `PageSnapshot::version` it does
/// not change when a flush republishes the page.
fn page_hash(s: &PageSnapshot) -> u64 {
    let mut h = blake3::Hasher::new();
    h.update(s.preamble.as_deref().unwrap_or_default().as_bytes());
    for b in &s.blocks {
        h.update(&b.id.get().to_le_bytes());
        h.update(&b.parent.map_or(0, |p| p.get()).to_le_bytes());
        h.update(&(b.depth as u64).to_le_bytes());
        h.update(b.text.as_bytes());
        h.update(&[0]);
    }
    let d = h.finalize();
    let b = d.as_bytes();
    u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]])
}

fn new_path_str(cfg: &EffectiveConfig, name: &str) -> String {
    new_page_path(name, cfg)
        .map(|p| p.as_str().to_owned())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_validation() {
        assert!(validate_content("hello world").is_ok());
        assert!(validate_content("line one\nline two").is_ok());
        assert!(validate_content("  \n").is_err());
        assert!(validate_content("- bullet").is_err());
        assert!(validate_content("a\n- b").is_err());
        assert!(validate_content("a\nid:: 11111111-1111-4111-8111-111111111111").is_err());
        assert!(validate_content("a\ncollapsed:: true").is_err());
        assert!(validate_content("a\nstatus:: open").is_ok());
    }

    #[test]
    fn property_validation() {
        assert!(validate_property("status", "open").is_ok());
        assert!(validate_property("id", "x").is_err());
        assert!(validate_property("Collapsed", "true").is_err());
        assert!(validate_property("a b", "x").is_err());
        assert!(validate_property("a", "x\ny").is_err());
    }

    #[test]
    fn link_rewriting() {
        assert_eq!(
            rewrite_links("see [[Old Page]] and [[old page]]", "Old Page", "New"),
            "see [[New]] and [[New]]"
        );
        assert_eq!(
            rewrite_links("tag #old and #older", "old", "new"),
            "tag #new and #older"
        );
        assert_eq!(rewrite_links("#old", "old", "new name"), "#[[new name]]");
        assert_eq!(
            rewrite_links("plain old text", "old", "new"),
            "plain old text"
        );
    }

    #[test]
    fn uuid_shape() {
        let u = new_uuid().expect("uuid");
        assert_eq!(u.len(), 36);
        assert_eq!(&u[14..15], "4");
    }

    // ---- gates against a real queue over an in-memory store

    use crate::policy::OpenGate;
    use crate::reader::StaticGraphReader;
    use crate::status::{SyncStatus, SyncStatusProvider};
    use bitacora_core::editor::{MemStore, Workspace};
    use bitacora_core::queue::QueueConfig;

    const ID: &str = "11111111-1111-4111-8111-111111111111";

    struct Busy;
    impl WriteGate for Busy {
        fn busy(&self, page: &str, block: &str) -> Option<u64> {
            (page == "t" && block == ID).then_some(750)
        }
    }

    struct Conflicted;
    impl SyncStatusProvider for Conflicted {
        fn status(&self) -> SyncStatus {
            SyncStatus {
                state: SyncState::Conflicted,
                conflict_count: 1,
                conflict_pages: vec!["T".into()],
                ..SyncStatus::default()
            }
        }
    }

    fn bridge_with_page() -> (QueueBridge, bitacora_core::queue::QueueJoin) {
        let (queue, join) = CommandQueue::spawn(
            Workspace::new(),
            Box::new(MemStore::default()),
            QueueConfig {
                debounce: None,
                ..QueueConfig::default()
            },
        );
        queue
            .execute(
                Source::Ui,
                Request::LoadPage {
                    key: PageKey::from_title("t"),
                    title: "t".into(),
                    path: GraphPath::new("pages/t.md").ok(),
                    bytes: format!("- a\n  id:: {ID}\n").into_bytes(),
                },
            )
            .expect("load");
        let cfg = EffectiveConfig::load(std::path::Path::new("/nonexistent"), None);
        (QueueBridge::new(queue, "/nonexistent", cfg), join)
    }

    #[test]
    fn busy_and_conflicted_blocks_refuse_writes_and_write_nothing() {
        let (b, join) = bridge_with_page();
        let reader = StaticGraphReader::new("g", "/g");
        let policy = WritePolicy::new(true, true, Vec::new());
        let sync = crate::status::DisabledSync;

        let busy = Env {
            r: &reader,
            policy: &policy,
            gate: &Busy,
            sync: &sync,
        };
        let e = b
            .update_block(&busy, ID, "changed", &[], None)
            .expect_err("busy");
        assert_eq!(e.code, Code::BlockBusy);
        assert_eq!(e.extra.expect("extra")["retry_after_ms"], 750);

        let conflicted = Env {
            r: &reader,
            policy: &policy,
            gate: &OpenGate,
            sync: &Conflicted,
        };
        let e = b
            .update_block(&conflicted, ID, "changed", &[], None)
            .expect_err("conflict");
        assert_eq!(e.code, Code::BlockInConflict);

        // Nothing was committed; with the gates open the same call goes through.
        assert!(b.queue.audit().is_empty());
        let open = Env {
            r: &reader,
            policy: &policy,
            gate: &OpenGate,
            sync: &sync,
        };
        let done = b.update_block(&open, ID, "changed", &[], None).expect("ok");
        assert_eq!(done.txs.len(), 1);
        assert_eq!(b.queue.audit().len(), 1);
        drop(join);
    }
}
