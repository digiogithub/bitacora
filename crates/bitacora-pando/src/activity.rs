//! Machine-local, bounded, clearable log of what Bitacora did with Pando (BIT-SP-0009.R7).
//!
//! One JSON object per line in a file next to `pando.json`. It records counts, block uuids, run
//! ids and short fixed summaries only: never block text, page titles or tokens. The file keeps
//! the newest [`MAX_ENTRIES`] entries (older ones are dropped when it grows past
//! [`MAX_ENTRIES`] + [`SLACK`]) and [`ActivityLog::clear`] empties it.

use std::fs;
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

/// Entries kept after a compaction.
pub const MAX_ENTRIES: usize = 500;
/// Entries tolerated above [`MAX_ENTRIES`] before the file is rewritten.
pub const SLACK: usize = 100;
/// Most block uuids stored per entry; `count` still carries the real number.
pub const MAX_IDS: usize = 20;

/// What kind of thing happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityKind {
    /// The connection status changed.
    Status,
    /// A knowledge-base sync batch was acknowledged by Pando.
    Sync,
    /// An agent run started, finished, failed or was cancelled.
    Run,
    /// An approval card was resolved.
    Approval,
    /// An approved edit was applied to the graph.
    Edit,
}

/// One line of the log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivityEntry {
    /// Unix time in milliseconds.
    pub at_ms: i64,
    /// What happened.
    pub kind: ActivityKind,
    /// Short, content-free summary (a stable machine word such as `upserted`).
    pub summary: String,
    /// How many items were involved.
    #[serde(default)]
    pub count: u64,
    /// Block uuids (at most [`MAX_IDS`]) or a run / call id.
    #[serde(default)]
    pub ids: Vec<String>,
}

impl ActivityEntry {
    /// An entry stamped now; `ids` is truncated to [`MAX_IDS`].
    #[must_use]
    pub fn new(
        kind: ActivityKind,
        summary: impl Into<String>,
        count: u64,
        mut ids: Vec<String>,
    ) -> Self {
        ids.truncate(MAX_IDS);
        Self {
            at_ms: now_ms(),
            kind,
            summary: summary.into(),
            count,
            ids,
        }
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|d| i64::try_from(d.as_millis()).ok())
        .unwrap_or(0)
}

/// The file-backed log. Cheap to clone; every clone shares the lock.
#[derive(Clone)]
pub struct ActivityLog {
    path: Arc<PathBuf>,
    /// Known line count (`None` until the first write) behind the write lock.
    lock: Arc<Mutex<Option<usize>>>,
}

impl std::fmt::Debug for ActivityLog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ActivityLog")
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

impl ActivityLog {
    /// A log stored at `path` (created on the first record).
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: Arc::new(path.into()),
            lock: Arc::new(Mutex::new(None)),
        }
    }

    /// The log file next to a `pando.json` at `settings_file`.
    #[must_use]
    pub fn beside(settings_file: &Path) -> Self {
        Self::new(settings_file.with_file_name("pando-activity.jsonl"))
    }

    /// The file path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Appends `entry`, compacting the file when it grew past its bound.
    ///
    /// # Errors
    /// I/O failures; callers treat the log as best-effort.
    pub fn record(&self, entry: &ActivityEntry) -> io::Result<()> {
        let mut guard = self.lock.lock();
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }
        let mut line = serde_json::to_string(entry).map_err(io::Error::other)?;
        line.push('\n');
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&*self.path)?;
        file.write_all(line.as_bytes())?;
        let count = match *guard {
            Some(n) => n + 1,
            None => self.read_all().len(),
        };
        if count > MAX_ENTRIES + SLACK {
            let all = self.read_all();
            let keep = &all[all.len().saturating_sub(MAX_ENTRIES)..];
            let mut text = String::new();
            for e in keep {
                text.push_str(&serde_json::to_string(e).map_err(io::Error::other)?);
                text.push('\n');
            }
            let tmp = self.path.with_extension("jsonl.tmp");
            fs::write(&tmp, text)?;
            fs::rename(&tmp, &*self.path)?;
            *guard = Some(keep.len());
        } else {
            *guard = Some(count);
        }
        Ok(())
    }

    /// Records `entry`, ignoring failures.
    pub fn record_best_effort(&self, entry: &ActivityEntry) {
        if let Err(e) = self.record(entry) {
            tracing::debug!(error = %e, "pando activity log write failed");
        }
    }

    fn read_all(&self) -> Vec<ActivityEntry> {
        let Ok(text) = fs::read_to_string(&*self.path) else {
            return Vec::new();
        };
        text.lines()
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect()
    }

    /// The newest `limit` entries, newest first. Unreadable lines are skipped.
    #[must_use]
    pub fn read(&self, limit: usize) -> Vec<ActivityEntry> {
        let _guard = self.lock.lock();
        let mut all = self.read_all();
        all.reverse();
        all.truncate(limit);
        all
    }

    /// Deletes every entry.
    ///
    /// # Errors
    /// I/O failures other than a missing file.
    pub fn clear(&self) -> io::Result<()> {
        let mut guard = self.lock.lock();
        *guard = Some(0);
        match fs::remove_file(&*self.path) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn entry(n: u64) -> ActivityEntry {
        ActivityEntry::new(ActivityKind::Sync, "upserted", n, vec![format!("u{n}")])
    }

    #[test]
    fn records_reads_newest_first_and_clears() {
        let dir = tempfile::tempdir().unwrap();
        let log = ActivityLog::beside(&dir.path().join("pando.json"));
        assert!(log.read(10).is_empty());
        for n in 1..=3 {
            log.record(&entry(n)).unwrap();
        }
        let got = log.read(2);
        assert_eq!(got.iter().map(|e| e.count).collect::<Vec<_>>(), [3, 2]);
        log.clear().unwrap();
        assert!(log.read(10).is_empty());
        log.clear().unwrap();
        log.record(&entry(9)).unwrap();
        assert_eq!(log.read(10).len(), 1);
    }

    #[test]
    fn stays_bounded() {
        let dir = tempfile::tempdir().unwrap();
        let log = ActivityLog::new(dir.path().join("a.jsonl"));
        let total = u64::try_from(MAX_ENTRIES + SLACK + 50).unwrap();
        for n in 0..total {
            log.record(&entry(n)).unwrap();
        }
        let all = log.read(usize::MAX);
        assert!(all.len() <= MAX_ENTRIES + SLACK);
        assert_eq!(all[0].count, total - 1, "the newest entry survives");
    }

    #[test]
    fn ids_are_capped_and_garbage_lines_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.jsonl");
        std::fs::write(&p, "not json\n").unwrap();
        let log = ActivityLog::new(&p);
        let ids = (0..100).map(|i| i.to_string()).collect();
        log.record(&ActivityEntry::new(ActivityKind::Sync, "x", 100, ids))
            .unwrap();
        let got = log.read(10);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].ids.len(), MAX_IDS);
        assert_eq!(got[0].count, 100);
    }
}
