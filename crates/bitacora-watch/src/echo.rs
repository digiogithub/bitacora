//! Echo suppression of our own writes, keyed by `(path, content hash)`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use bitacora_core::editor::WrittenFile;
use bitacora_core::graph_path::GraphPath;

/// Default lifetime of an entry.
pub const DEFAULT_TTL: Duration = Duration::from_secs(5);

/// What we expect to observe for a path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Expect {
    Hash(blake3::Hash),
    Removed,
}

type Clock = Arc<dyn Fn() -> Instant + Send + Sync>;

struct Inner {
    entries: HashMap<(String, Expect), Instant>,
}

/// Remembers `(rel_path, hash)` of files Bitacora wrote (or deleted) so the watcher can drop
/// the resulting events. Cheap to clone: all clones share the same state.
///
/// Matching is by content hash, never by time alone: the TTL only bounds how long an entry is
/// kept. Entries are *not* consumed by a match, because one write can surface as several OS
/// events. An external write with different bytes is therefore never suppressed.
#[derive(Clone)]
pub struct EchoFilter {
    inner: Arc<Mutex<Inner>>,
    ttl: Duration,
    clock: Clock,
}

impl std::fmt::Debug for EchoFilter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EchoFilter")
            .field("ttl", &self.ttl)
            .finish()
    }
}

impl Default for EchoFilter {
    fn default() -> Self {
        Self::new()
    }
}

impl EchoFilter {
    /// Filter with the default 5 s TTL.
    #[must_use]
    pub fn new() -> Self {
        Self::with_clock(DEFAULT_TTL, Instant::now)
    }

    /// Filter with a custom TTL and an injectable clock (for tests).
    #[must_use]
    pub fn with_clock(ttl: Duration, clock: impl Fn() -> Instant + Send + Sync + 'static) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                entries: HashMap::new(),
            })),
            ttl,
            clock: Arc::new(clock),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn put(&self, rel: &str, e: Expect) {
        let now = (self.clock)();
        let mut g = self.lock();
        g.entries.retain(|_, exp| *exp > now);
        g.entries.insert((rel.to_owned(), e), now + self.ttl);
    }

    /// Records that we wrote `hash` to `rel`. Call right before the atomic rename.
    pub fn record_write(&self, rel: &str, hash: blake3::Hash) {
        self.put(rel, Expect::Hash(hash));
    }

    /// Records that we wrote `bytes` to `rel`.
    pub fn record_bytes(&self, rel: &str, bytes: &[u8]) {
        self.record_write(rel, blake3::hash(bytes));
    }

    /// Records a file reported by core's flush (`QueueEvent::Flushed` / `FilesApplied`).
    pub fn record_written_file(&self, w: &WrittenFile) {
        self.record_write(w.path.as_str(), w.hash);
    }

    /// Records that we deleted `path`.
    pub fn record_deleted(&self, path: &GraphPath) {
        self.put(path.as_str(), Expect::Removed);
    }

    /// Records that we deleted `rel` (string form).
    pub fn record_removal(&self, rel: &str) {
        self.put(rel, Expect::Removed);
    }

    fn is_live(&self, rel: &str, e: Expect) -> bool {
        let now = (self.clock)();
        let g = self.lock();
        g.entries
            .get(&(rel.to_owned(), e))
            .is_some_and(|exp| *exp > now)
    }

    /// Whether observing `hash` at `rel` is the echo of one of our writes.
    #[must_use]
    pub fn is_echo(&self, rel: &str, hash: blake3::Hash) -> bool {
        self.is_live(rel, Expect::Hash(hash))
    }

    /// Whether observing the removal of `rel` is the echo of one of our deletions.
    #[must_use]
    pub fn is_removal_echo(&self, rel: &str) -> bool {
        self.is_live(rel, Expect::Removed)
    }

    /// Number of live entries (diagnostics, tests).
    #[must_use]
    pub fn len(&self) -> usize {
        let now = (self.clock)();
        self.lock().entries.values().filter(|e| **e > now).count()
    }

    /// Whether no live entry exists.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manual() -> (EchoFilter, Arc<Mutex<Instant>>) {
        let t = Arc::new(Mutex::new(Instant::now()));
        let t2 = Arc::clone(&t);
        let f = EchoFilter::with_clock(Duration::from_secs(5), move || {
            *t2.lock().unwrap_or_else(PoisonError::into_inner)
        });
        (f, t)
    }

    fn advance(t: &Arc<Mutex<Instant>>, d: Duration) {
        *t.lock().unwrap_or_else(PoisonError::into_inner) += d;
    }

    #[test]
    fn own_write_is_echo_but_other_content_is_not() {
        let (f, t) = manual();
        f.record_bytes("pages/a.md", b"- mine\n");
        assert!(f.is_echo("pages/a.md", blake3::hash(b"- mine\n")));
        advance(&t, Duration::from_millis(100));
        assert!(!f.is_echo("pages/a.md", blake3::hash(b"- external\n")));
        assert!(!f.is_echo("pages/b.md", blake3::hash(b"- mine\n")));
        // Not consumed by a match.
        assert!(f.is_echo("pages/a.md", blake3::hash(b"- mine\n")));
    }

    #[test]
    fn entries_expire() {
        let (f, t) = manual();
        f.record_bytes("a.md", b"x");
        f.record_removal("b.md");
        assert_eq!(f.len(), 2);
        advance(&t, Duration::from_millis(4900));
        assert!(f.is_echo("a.md", blake3::hash(b"x")));
        assert!(f.is_removal_echo("b.md"));
        advance(&t, Duration::from_millis(200));
        assert!(!f.is_echo("a.md", blake3::hash(b"x")));
        assert!(!f.is_removal_echo("b.md"));
        assert!(f.is_empty());
    }

    #[test]
    fn clones_share_state() {
        let f = EchoFilter::new();
        let g = f.clone();
        g.record_bytes("a.md", b"x");
        assert!(f.is_echo("a.md", blake3::hash(b"x")));
    }
}
