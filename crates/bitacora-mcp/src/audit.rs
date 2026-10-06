//! Agent audit log (BIT-SP-0007.R7, R8; design `mcp-server.md` section 3, "Audit").
//!
//! One append-only JSONL record per tool call and per authentication failure: timestamp, token
//! name, client info, tool, an arguments hash and a short summary, affected block uuids, page
//! names and the result. **No note content and no token values** are ever stored: the summary
//! only shows non-content arguments verbatim and reports the size of the others.
//!
//! The file lives in the app data dir and rotates by size (`audit.jsonl`, `audit.jsonl.1`, ...);
//! the newest entries are always in `audit.jsonl`. Marking an entry as undone appends an
//! `undone` event line, so the file is never rewritten.
//!
//! Undo information (the committed core transactions of a call) is kept **in memory** for the
//! lifetime of the process: entries read back from disk after a restart are listed but cannot be
//! undone (their inverse ops were never persisted, because they hold note content).

use std::collections::{HashMap, VecDeque};
use std::fs::{File, OpenOptions};
use std::io::{BufRead as _, BufReader, Write as _};
use std::path::{Path, PathBuf};

use bitacora_core::editor::Transaction;
use bitacora_core::graph::PageKey;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

/// Default rotation threshold: 10 MiB per file.
pub const DEFAULT_MAX_BYTES: u64 = 10 * 1024 * 1024;
/// Default number of files kept (the live one plus four rotated).
pub const DEFAULT_MAX_FILES: usize = 5;
/// Records kept in memory for listing.
const RING: usize = 5_000;
/// Calls whose undo data is kept in memory.
const UNDO_KEPT: usize = 500;
/// File name of the live log.
pub const FILE_NAME: &str = "audit.jsonl";

/// Kind of a log line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditEvent {
    /// A tool call (read or write).
    Call,
    /// A request rejected by the token check.
    AuthFailure,
    /// An entry was undone (`undo_of` names it).
    Undone,
}

/// One audit record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditRecord {
    /// Unique, sortable id (`<unix ms>-<counter>`).
    pub id: String,
    /// Unix milliseconds.
    pub ts: i64,
    /// Line kind.
    pub event: AuditEvent,
    /// Token name (`None` for authentication failures).
    #[serde(default)]
    pub token: Option<String>,
    /// `clientInfo` of the MCP initialize handshake (`name/version`), when known.
    #[serde(default)]
    pub client: Option<String>,
    /// Tool name (or `auth`).
    #[serde(default)]
    pub tool: String,
    /// BLAKE3 (16 hex) of the arguments JSON.
    #[serde(default)]
    pub args_hash: String,
    /// Short, content-free rendering of the arguments.
    #[serde(default)]
    pub args: String,
    /// UUIDs of blocks the call created, changed, moved or removed.
    #[serde(default)]
    pub affected: Vec<String>,
    /// Display names of the pages touched.
    #[serde(default)]
    pub pages: Vec<String>,
    /// `ok` or the machine error code.
    #[serde(default)]
    pub result: String,
    /// The call wrote to the graph and its undo data is still available.
    #[serde(default)]
    pub undoable: bool,
    /// The call was written to the graph (whether or not it can still be undone).
    #[serde(default)]
    pub write: bool,
    /// Already undone.
    #[serde(default)]
    pub undone: bool,
    /// For [`AuditEvent::Undone`]: the entry that was undone.
    #[serde(default)]
    pub undo_of: Option<String>,
}

/// Listing filter.
#[derive(Debug, Clone, Default)]
pub struct AuditFilter {
    /// Only this token.
    pub token: Option<String>,
    /// Only this tool.
    pub tool: Option<String>,
    /// Only calls that wrote to the graph.
    pub writes_only: bool,
    /// Maximum entries (default 100).
    pub limit: Option<usize>,
}

/// Why an undo did not happen.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum UndoError {
    /// No such entry.
    #[error("no audit entry `{0}`")]
    NotFound(String),
    /// The entry did not write anything.
    #[error("entry `{0}` is not a write")]
    NotAWrite(String),
    /// Already undone.
    #[error("entry `{0}` was already undone")]
    AlreadyUndone(String),
    /// The inverse data is gone (the app restarted since the call).
    #[error("undo data for `{0}` is not available (the app restarted since the call)")]
    NotAvailable(String),
    /// The graph moved on: the blocks the call changed were edited since.
    #[error("the blocks changed since this call, so it cannot be undone safely")]
    Changed,
    /// The write pipeline refused or failed.
    #[error("undo failed: {0}")]
    Failed(String),
}

/// Content an in-flight tool call reports back to the audit layer.
#[derive(Debug, Default)]
pub(crate) struct CallInfo {
    pub affected: Vec<String>,
    pub pages: Vec<String>,
    pub txs: Vec<Transaction>,
    /// Version of every page the call touched, right after it ran: undo is refused once any of
    /// them moved on (the blocks changed since).
    pub fingerprint: Vec<(PageKey, Option<u64>)>,
    pub wrote: bool,
}

/// What undoing a call needs.
#[derive(Debug, Clone)]
pub(crate) struct UndoData {
    pub txs: Vec<Transaction>,
    pub fingerprint: Vec<(PageKey, Option<u64>)>,
}

struct Inner {
    records: VecDeque<AuditRecord>,
    file: Option<File>,
    size: u64,
    counter: u64,
    undo: HashMap<String, UndoData>,
    undo_order: VecDeque<String>,
}

/// The audit sink and its in-memory view.
pub struct AuditLog {
    dir: Option<PathBuf>,
    max_bytes: u64,
    max_files: usize,
    inner: Mutex<Inner>,
}

impl std::fmt::Debug for AuditLog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuditLog")
            .field("dir", &self.dir)
            .finish_non_exhaustive()
    }
}

fn now_ms() -> i64 {
    jiff::Timestamp::now().as_millisecond()
}

impl AuditLog {
    /// A log that only lives in memory (tests, embedding).
    #[must_use]
    pub fn in_memory() -> Self {
        Self::build(None, DEFAULT_MAX_BYTES, DEFAULT_MAX_FILES, Vec::new(), 0, 0)
    }

    /// Opens (creating) the log in `dir`, reading existing records back.
    ///
    /// # Errors
    /// I/O failures creating the directory or the file.
    pub fn open(dir: impl Into<PathBuf>) -> std::io::Result<Self> {
        Self::open_with(dir, DEFAULT_MAX_BYTES, DEFAULT_MAX_FILES)
    }

    /// Like [`AuditLog::open`] with explicit rotation limits.
    ///
    /// # Errors
    /// I/O failures creating the directory or the file.
    pub fn open_with(
        dir: impl Into<PathBuf>,
        max_bytes: u64,
        max_files: usize,
    ) -> std::io::Result<Self> {
        let dir = dir.into();
        std::fs::create_dir_all(&dir)?;
        let max_files = max_files.max(1);
        let mut loaded = Vec::new();
        // Oldest file first so later lines win.
        for n in (0..max_files).rev() {
            let path = file_of(&dir, n);
            if let Ok(f) = File::open(&path) {
                for line in BufReader::new(f).lines().map_while(Result::ok) {
                    if let Ok(r) = serde_json::from_str::<AuditRecord>(&line) {
                        loaded.push(r);
                    }
                }
            }
        }
        let live = file_of(&dir, 0);
        let file = OpenOptions::new().create(true).append(true).open(&live)?;
        let size = file.metadata()?.len();
        Ok(Self::build(Some(dir), max_bytes, max_files, loaded, size, 0).with_file(file))
    }

    fn build(
        dir: Option<PathBuf>,
        max_bytes: u64,
        max_files: usize,
        loaded: Vec<AuditRecord>,
        size: u64,
        counter: u64,
    ) -> Self {
        let mut records: VecDeque<AuditRecord> = VecDeque::new();
        for r in loaded {
            if r.event == AuditEvent::Undone {
                if let Some(target) = &r.undo_of
                    && let Some(t) = records.iter_mut().find(|x| &x.id == target)
                {
                    t.undone = true;
                    t.undoable = false;
                }
                continue;
            }
            let mut r = r;
            r.undoable = false;
            if records.len() >= RING {
                records.pop_front();
            }
            records.push_back(r);
        }
        Self {
            dir,
            max_bytes: max_bytes.max(64),
            max_files: max_files.max(1),
            inner: Mutex::new(Inner {
                records,
                file: None,
                size,
                counter,
                undo: HashMap::new(),
                undo_order: VecDeque::new(),
            }),
        }
    }

    fn with_file(self, file: File) -> Self {
        self.inner.lock().file = Some(file);
        self
    }

    /// Records a finished tool call and returns its id. `info` carries what the call changed.
    pub(crate) fn record_call(&self, base: CallBase, info: CallInfo, result: &str) -> String {
        let mut g = self.inner.lock();
        let (id, ts) = next_id(&mut g);
        let undoable = info.wrote && !info.txs.is_empty() && result == "ok";
        let rec = AuditRecord {
            id: id.clone(),
            ts,
            event: AuditEvent::Call,
            token: base.token,
            client: base.client,
            tool: base.tool,
            args_hash: base.args_hash,
            args: base.args,
            affected: info.affected,
            pages: info.pages,
            result: result.to_owned(),
            undoable,
            write: info.wrote && result == "ok",
            undone: false,
            undo_of: None,
        };
        if undoable {
            g.undo.insert(
                id.clone(),
                UndoData {
                    txs: info.txs,
                    fingerprint: info.fingerprint,
                },
            );
            g.undo_order.push_back(id.clone());
            while g.undo_order.len() > UNDO_KEPT {
                if let Some(old) = g.undo_order.pop_front() {
                    g.undo.remove(&old);
                    if let Some(r) = g.records.iter_mut().find(|r| r.id == old) {
                        r.undoable = false;
                    }
                }
            }
        }
        self.append(&mut g, &rec);
        push_record(&mut g, rec);
        id
    }

    /// Records a request rejected by the token check.
    pub fn record_auth_failure(&self, detail: &str) {
        let mut g = self.inner.lock();
        let (id, ts) = next_id(&mut g);
        let rec = AuditRecord {
            id,
            ts,
            event: AuditEvent::AuthFailure,
            token: None,
            client: None,
            tool: "auth".to_owned(),
            args_hash: String::new(),
            args: detail.to_owned(),
            affected: Vec::new(),
            pages: Vec::new(),
            result: "UNAUTHORIZED".to_owned(),
            undoable: false,
            write: false,
            undone: false,
            undo_of: None,
        };
        self.append(&mut g, &rec);
        push_record(&mut g, rec);
    }

    /// Entries newest first.
    #[must_use]
    pub fn list(&self, filter: &AuditFilter) -> Vec<AuditRecord> {
        let g = self.inner.lock();
        g.records
            .iter()
            .rev()
            .filter(|r| {
                filter
                    .token
                    .as_ref()
                    .is_none_or(|t| r.token.as_ref() == Some(t))
                    && filter.tool.as_ref().is_none_or(|t| &r.tool == t)
                    && (!filter.writes_only || r.write)
            })
            .take(filter.limit.unwrap_or(100))
            .cloned()
            .collect()
    }

    /// One entry.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<AuditRecord> {
        self.inner
            .lock()
            .records
            .iter()
            .find(|r| r.id == id)
            .cloned()
    }

    /// Checks that `id` can be undone and returns its transactions (newest last).
    ///
    /// # Errors
    /// [`UndoError`] explaining why not.
    pub(crate) fn undo_data(&self, id: &str) -> Result<UndoData, UndoError> {
        let g = self.inner.lock();
        let rec = g
            .records
            .iter()
            .find(|r| r.id == id)
            .ok_or_else(|| UndoError::NotFound(id.to_owned()))?;
        if !rec.write {
            return Err(UndoError::NotAWrite(id.to_owned()));
        }
        if rec.undone {
            return Err(UndoError::AlreadyUndone(id.to_owned()));
        }
        g.undo
            .get(id)
            .cloned()
            .ok_or_else(|| UndoError::NotAvailable(id.to_owned()))
    }

    /// Marks `id` undone and appends the `undone` event.
    pub(crate) fn mark_undone(&self, id: &str) {
        let mut g = self.inner.lock();
        g.undo.remove(id);
        if let Some(r) = g.records.iter_mut().find(|r| r.id == id) {
            r.undone = true;
            r.undoable = false;
        }
        let (new_id, ts) = next_id(&mut g);
        let rec = AuditRecord {
            id: new_id,
            ts,
            event: AuditEvent::Undone,
            token: None,
            client: None,
            tool: "undo".to_owned(),
            args_hash: String::new(),
            args: String::new(),
            affected: Vec::new(),
            pages: Vec::new(),
            result: "ok".to_owned(),
            undoable: false,
            write: false,
            undone: false,
            undo_of: Some(id.to_owned()),
        };
        self.append(&mut g, &rec);
    }

    fn append(&self, g: &mut Inner, rec: &AuditRecord) {
        let Some(dir) = &self.dir else {
            return;
        };
        let Ok(mut line) = serde_json::to_string(rec) else {
            return;
        };
        line.push('\n');
        let len = line.len() as u64;
        if g.size > 0 && g.size + len > self.max_bytes {
            g.file = None;
            rotate(dir, self.max_files);
            g.file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(file_of(dir, 0))
                .ok();
            g.size = 0;
        }
        if let Some(f) = g.file.as_mut() {
            if f.write_all(line.as_bytes())
                .and_then(|()| f.flush())
                .is_ok()
            {
                g.size += len;
            } else {
                tracing::warn!("cannot append to the MCP audit log");
            }
        }
    }
}

fn push_record(g: &mut Inner, rec: AuditRecord) {
    if g.records.len() >= RING {
        g.records.pop_front();
    }
    g.records.push_back(rec);
}

fn next_id(g: &mut Inner) -> (String, i64) {
    g.counter += 1;
    let ts = now_ms();
    (format!("{ts}-{}", g.counter), ts)
}

fn file_of(dir: &Path, n: usize) -> PathBuf {
    if n == 0 {
        dir.join(FILE_NAME)
    } else {
        dir.join(format!("{FILE_NAME}.{n}"))
    }
}

/// `audit.jsonl.(n-1)` is dropped, the others shift up by one, the live file becomes `.1`.
fn rotate(dir: &Path, max_files: usize) {
    let _ = std::fs::remove_file(file_of(dir, max_files.saturating_sub(1)));
    for n in (1..max_files.saturating_sub(1)).rev() {
        let _ = std::fs::rename(file_of(dir, n), file_of(dir, n + 1));
    }
    if max_files > 1 {
        let _ = std::fs::rename(file_of(dir, 0), file_of(dir, 1));
    } else {
        let _ = std::fs::remove_file(file_of(dir, 0));
    }
}

/// Fields known before a call runs.
#[derive(Debug, Clone)]
pub(crate) struct CallBase {
    pub token: Option<String>,
    pub client: Option<String>,
    pub tool: String,
    pub args_hash: String,
    pub args: String,
}

/// Argument keys whose values are shown verbatim in the summary: identifiers and switches, never
/// note content.
const SHOWN_KEYS: &[&str] = &[
    "uuid",
    "target_uuid",
    "name",
    "new_name",
    "page",
    "position",
    "status",
    "key",
    "graph",
    "limit",
    "kind",
    "if_exists",
    "update_links",
    "expected_version",
    "journal",
    "create_if_missing",
    "include_children",
    "include_parents",
    "from",
    "to",
    "cursor",
];

/// Hash and summary of tool arguments, without any note content.
pub(crate) fn summarize_args(
    args: Option<&serde_json::Map<String, serde_json::Value>>,
) -> (String, String) {
    let Some(args) = args else {
        return (hash16(b"{}"), String::new());
    };
    let canonical = serde_json::to_string(args).unwrap_or_default();
    let hash = hash16(canonical.as_bytes());
    let mut parts = Vec::new();
    let mut keys: Vec<&String> = args.keys().collect();
    keys.sort();
    for k in keys {
        let v = &args[k];
        let shown = if SHOWN_KEYS.contains(&k.as_str()) {
            match v {
                serde_json::Value::String(s) => truncate(s, 80),
                other => truncate(&other.to_string(), 80),
            }
        } else {
            match v {
                serde_json::Value::String(s) => format!("<{} chars>", s.chars().count()),
                serde_json::Value::Array(a) => format!("<{} items>", a.len()),
                serde_json::Value::Object(o) => format!("<{} keys>", o.len()),
                serde_json::Value::Null => "null".to_owned(),
                other => other.to_string(),
            }
        };
        parts.push(format!("{k}={shown}"));
    }
    (hash, parts.join(" "))
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_owned()
    } else {
        let cut: String = s.chars().take(max).collect();
        format!("{cut}...")
    }
}

fn hash16(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex()[..16].to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base(tool: &str) -> CallBase {
        CallBase {
            token: Some("t".into()),
            client: None,
            tool: tool.into(),
            args_hash: "h".into(),
            args: "a".into(),
        }
    }

    #[test]
    fn summary_never_contains_content() {
        let v = serde_json::json!({"uuid": "u1", "content": "secret note text", "blocks": [1, 2]});
        let (hash, summary) = summarize_args(v.as_object());
        assert_eq!(hash.len(), 16);
        assert!(summary.contains("uuid=u1"));
        assert!(summary.contains("content=<16 chars>"));
        assert!(summary.contains("blocks=<2 items>"));
        assert!(!summary.contains("secret"));
    }

    #[test]
    fn rotation_keeps_newest_and_bounds_files() {
        let tmp = tempfile::tempdir().expect("tmp");
        let log = AuditLog::open_with(tmp.path(), 400, 3).expect("open");
        for i in 0..40 {
            log.record_call(
                CallBase {
                    args: format!("n={i}"),
                    ..base("search")
                },
                CallInfo::default(),
                "ok",
            );
        }
        let files: Vec<_> = std::fs::read_dir(tmp.path())
            .expect("dir")
            .map(|e| e.expect("e").file_name().to_string_lossy().into_owned())
            .collect();
        assert!(files.len() <= 3, "{files:?}");
        let live = std::fs::read_to_string(tmp.path().join(FILE_NAME)).expect("live");
        assert!(live.contains("n=39"), "newest entry is in the live file");
        let again = AuditLog::open_with(tmp.path(), 400, 3).expect("reopen");
        assert!(
            again
                .list(&AuditFilter::default())
                .iter()
                .any(|r| r.args == "n=39")
        );
    }

    #[test]
    fn undone_events_survive_a_restart() {
        let tmp = tempfile::tempdir().expect("tmp");
        let log = AuditLog::open(tmp.path()).expect("open");
        let info = CallInfo {
            wrote: true,
            ..CallInfo::default()
        };
        let id = log.record_call(base("update_block"), info, "ok");
        // No transactions: not undoable.
        assert!(!log.get(&id).expect("rec").undoable);
        log.mark_undone(&id);
        drop(log);
        let again = AuditLog::open(tmp.path()).expect("reopen");
        assert!(again.get(&id).expect("rec").undone);
        assert_eq!(
            again.undo_data(&id).unwrap_err(),
            UndoError::AlreadyUndone(id)
        );
    }
}
