//! Machine-local review cache and the optional daily schedule (BIT-T-0463, BIT-SP-0011.R4).
//!
//! Reviews are *derived* data: they are never written into the graph (so never synced) and live
//! in the machine-local cache directory the caller passes in (the same place as the semantic
//! ledger). The key is the graph id, the day range and a fingerprint of the journal content the
//! agent may see ([`super::review::content_hash`]), so editing a journal makes its cached review
//! stale and an untouched one is served without a new run. The file is a small JSON document
//! written atomically; a missing or corrupt file is just an empty cache.
//!
//! [`DailySchedule`] decides when the app, while it runs, should review today's journal on its
//! own: once per day after a configured time, only when the integration is connected and the
//! graph consented. It is pure (it takes the time as an argument), so tests use a fake clock.

use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use super::AgentError;
use super::review::{Review, ReviewRange};

/// File name inside the cache directory.
pub const CACHE_FILE: &str = "agent-reviews.json";
/// Entries kept; the oldest are dropped first.
pub const MAX_ENTRIES: usize = 200;

/// Wall-clock time: Unix seconds and the local calendar day.
pub trait WallClock: Send + Sync {
    /// Unix seconds.
    fn now_unix(&self) -> i64;
    /// Local day as `yyyyMMdd` and minutes since local midnight.
    fn local_day_minute(&self) -> (i64, u32);
}

/// The system clock in the local time zone.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemWallClock;

impl WallClock for SystemWallClock {
    fn now_unix(&self) -> i64 {
        jiff::Timestamp::now().as_second()
    }

    fn local_day_minute(&self) -> (i64, u32) {
        let z = jiff::Zoned::now();
        let day = i64::from(z.year()) * 10_000 + i64::from(z.month()) * 100 + i64::from(z.day());
        let minute =
            u32::try_from(z.hour()).unwrap_or(0) * 60 + u32::try_from(z.minute()).unwrap_or(0);
        (day, minute)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Entry {
    created: i64,
    review: Review,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct File {
    #[serde(default)]
    entries: BTreeMap<String, Entry>,
}

/// The cache. Cloning shares the same store.
#[derive(Clone)]
pub struct ReviewCache {
    path: PathBuf,
    clock: Arc<dyn WallClock>,
    state: Arc<Mutex<Option<File>>>,
}

impl std::fmt::Debug for ReviewCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReviewCache")
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

fn key(graph_id: &str, range: &ReviewRange, content_hash: &str) -> String {
    let mut h = blake3::Hasher::new();
    h.update(graph_id.as_bytes());
    h.update(&[0]);
    h.update(&range.from.to_le_bytes());
    h.update(&range.to.to_le_bytes());
    h.update(content_hash.as_bytes());
    h.finalize().to_hex().to_string()
}

impl ReviewCache {
    /// A cache stored in `dir` (the machine-local cache directory, never the graph).
    #[must_use]
    pub fn new(dir: impl AsRef<Path>) -> Self {
        Self::with_clock(dir, Arc::new(SystemWallClock))
    }

    /// Like [`ReviewCache::new`] with an explicit clock (tests).
    #[must_use]
    pub fn with_clock(dir: impl AsRef<Path>, clock: Arc<dyn WallClock>) -> Self {
        Self {
            path: dir.as_ref().join(CACHE_FILE),
            clock,
            state: Arc::new(Mutex::new(None)),
        }
    }

    /// The file the cache lives in.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    fn load(&self) -> File {
        std::fs::read(&self.path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    /// The cached review for exactly this graph, range and content, if any.
    #[must_use]
    pub fn get(&self, graph_id: &str, range: &ReviewRange, content_hash: &str) -> Option<Review> {
        let mut g = self.state.lock();
        let file = g.get_or_insert_with(|| self.load());
        file.entries
            .get(&key(graph_id, range, content_hash))
            .map(|e| e.review.clone())
    }

    /// Stores a review.
    ///
    /// # Errors
    /// [`AgentError::Cache`] when the file cannot be written (the in-memory copy is kept).
    pub fn put(
        &self,
        graph_id: &str,
        range: &ReviewRange,
        content_hash: &str,
        review: &Review,
    ) -> Result<(), AgentError> {
        let mut g = self.state.lock();
        let file = g.get_or_insert_with(|| self.load());
        file.entries.insert(
            key(graph_id, range, content_hash),
            Entry {
                created: self.clock.now_unix(),
                review: review.clone(),
            },
        );
        while file.entries.len() > MAX_ENTRIES {
            let oldest = file
                .entries
                .iter()
                .min_by_key(|(_, e)| e.created)
                .map(|(k, _)| k.clone());
            match oldest {
                Some(k) => file.entries.remove(&k),
                None => break,
            };
        }
        let bytes = serde_json::to_vec(&*file).map_err(|e| AgentError::Cache(e.to_string()))?;
        write_atomic(&self.path, &bytes).map_err(|e| AgentError::Cache(e.to_string()))
    }

    /// Number of cached reviews.
    #[must_use]
    pub fn len(&self) -> usize {
        let mut g = self.state.lock();
        g.get_or_insert_with(|| self.load()).entries.len()
    }

    /// No review is cached.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Forgets every review of `graph_id` (used when the user revokes consent).
    ///
    /// # Errors
    /// [`AgentError::Cache`] when the file cannot be written.
    pub fn clear(&self) -> Result<(), AgentError> {
        let mut g = self.state.lock();
        *g = Some(File::default());
        match std::fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(AgentError::Cache(e.to_string())),
        }
    }
}

/// Temp file in the same directory, fsync, rename.
fn write_atomic(target: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let dir = target.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(dir)?;
    let tmp = dir.join(format!(
        ".{}.{}.tmp",
        target
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("cache"),
        std::process::id()
    ));
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, target).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

/// When the app should review today on its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DailySchedule {
    /// The user switched the schedule on (off by default).
    pub enabled: bool,
    /// Minutes since local midnight from which the review may run.
    pub at_minute: u32,
    /// Day (`yyyyMMdd`) of the last scheduled run.
    pub last_run_day: Option<i64>,
}

/// What must hold for a scheduled run besides the time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScheduleGate {
    /// Pando is connected.
    pub connected: bool,
    /// The graph consented to agent access.
    pub consent: bool,
    /// No review run is in flight.
    pub idle: bool,
}

impl DailySchedule {
    /// A disabled schedule at 18:00.
    #[must_use]
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            at_minute: 18 * 60,
            last_run_day: None,
        }
    }

    /// Range to review now (today), when a run is due; `None` otherwise. Marks the day as run so
    /// the next call returns `None` until tomorrow; call [`DailySchedule::retry`] when the run
    /// failed to try again later today.
    pub fn due(&mut self, clock: &dyn WallClock, gate: ScheduleGate) -> Option<ReviewRange> {
        if !self.enabled || !gate.connected || !gate.consent || !gate.idle {
            return None;
        }
        let (day, minute) = clock.local_day_minute();
        if minute < self.at_minute || self.last_run_day == Some(day) {
            return None;
        }
        self.last_run_day = Some(day);
        Some(ReviewRange::day(day))
    }

    /// Lets today's run be attempted again.
    pub fn retry(&mut self) {
        self.last_run_day = None;
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicI64, AtomicU32, Ordering};

    struct Fake {
        unix: AtomicI64,
        day: AtomicI64,
        minute: AtomicU32,
    }

    impl Fake {
        fn at(day: i64, minute: u32) -> Self {
            Self {
                unix: AtomicI64::new(1_000),
                day: AtomicI64::new(day),
                minute: AtomicU32::new(minute),
            }
        }
    }

    impl WallClock for Fake {
        fn now_unix(&self) -> i64 {
            self.unix.fetch_add(1, Ordering::SeqCst)
        }
        fn local_day_minute(&self) -> (i64, u32) {
            (
                self.day.load(Ordering::SeqCst),
                self.minute.load(Ordering::SeqCst),
            )
        }
    }

    fn review(s: &str) -> Review {
        Review {
            summary: s.into(),
            themes: vec!["t".into()],
            mood: None,
            pending_tasks: Vec::new(),
            next_actions: Vec::new(),
        }
    }

    #[test]
    fn hit_miss_and_persistence_across_instances() {
        let dir = tempfile::tempdir().unwrap();
        let range = ReviewRange::day(20_261_007);
        let c = ReviewCache::new(dir.path());
        assert!(c.get("g", &range, "h1").is_none());
        c.put("g", &range, "h1", &review("one")).unwrap();
        assert_eq!(c.get("g", &range, "h1").unwrap().summary, "one");
        // Content, range and graph are all part of the key.
        assert!(c.get("g", &range, "h2").is_none());
        assert!(c.get("g", &ReviewRange::day(20_261_008), "h1").is_none());
        assert!(c.get("other", &range, "h1").is_none());
        // A second instance reads the file.
        let again = ReviewCache::new(dir.path());
        assert_eq!(again.get("g", &range, "h1").unwrap().summary, "one");
        assert_eq!(again.path(), dir.path().join(CACHE_FILE));
        again.clear().unwrap();
        assert!(ReviewCache::new(dir.path()).is_empty());
    }

    #[test]
    fn corrupt_file_is_an_empty_cache_and_oldest_entries_go_first() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(CACHE_FILE), b"{not json").unwrap();
        let c = ReviewCache::with_clock(dir.path(), Arc::new(Fake::at(1, 0)));
        assert!(c.is_empty());
        for i in 0..(MAX_ENTRIES + 5) {
            c.put("g", &ReviewRange::day(1), &format!("h{i}"), &review("x"))
                .unwrap();
        }
        assert_eq!(c.len(), MAX_ENTRIES);
        assert!(c.get("g", &ReviewRange::day(1), "h0").is_none());
        assert!(
            c.get("g", &ReviewRange::day(1), &format!("h{}", MAX_ENTRIES + 4))
                .is_some()
        );
    }

    const GO: ScheduleGate = ScheduleGate {
        connected: true,
        consent: true,
        idle: true,
    };

    #[test]
    fn schedule_runs_once_a_day_after_its_time() {
        let clock = Fake::at(20_261_007, 17 * 60);
        let mut s = DailySchedule {
            enabled: true,
            at_minute: 18 * 60,
            last_run_day: None,
        };
        assert!(s.due(&clock, GO).is_none(), "before the time");
        clock.minute.store(18 * 60, Ordering::SeqCst);
        assert_eq!(s.due(&clock, GO), Some(ReviewRange::day(20_261_007)));
        assert!(s.due(&clock, GO).is_none(), "already ran today");
        s.retry();
        assert!(s.due(&clock, GO).is_some());
        // Next day.
        clock.day.store(20_261_008, Ordering::SeqCst);
        clock.minute.store(19 * 60, Ordering::SeqCst);
        assert_eq!(s.due(&clock, GO), Some(ReviewRange::day(20_261_008)));
    }

    #[test]
    fn schedule_respects_switch_consent_status_and_busy() {
        let clock = Fake::at(20_261_007, 23 * 60);
        let mut s = DailySchedule {
            enabled: false,
            ..DailySchedule::disabled()
        };
        assert!(s.due(&clock, GO).is_none(), "off by default");
        s.enabled = true;
        for gate in [
            ScheduleGate {
                connected: false,
                ..GO
            },
            ScheduleGate {
                consent: false,
                ..GO
            },
            ScheduleGate { idle: false, ..GO },
        ] {
            assert!(s.due(&clock, gate).is_none(), "{gate:?}");
            assert!(
                s.last_run_day.is_none(),
                "a blocked check does not use up the day"
            );
        }
        assert!(s.due(&clock, GO).is_some());
    }
}
