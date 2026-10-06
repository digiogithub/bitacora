//! Debounce scheduling for page writes (BIT-US-0063, BIT-SP-0005.R2/R7).
//!
//! [`WriteQueue`] is pure bookkeeping: callers pass the current time as a [`Duration`] since an
//! arbitrary epoch, so tests drive it with a fake clock. A page becomes *due* `debounce` after
//! its last edit but never later than `max_delay` after its first unwritten edit. Pages touched
//! by one transaction share a batch and are flushed together. Failed writes are retried with
//! exponential backoff (1 s doubling up to 60 s, BIT-SP-0005.R10).

use std::collections::BTreeMap;
use std::time::Duration;

use crate::graph::PageKey;

/// Debounce parameters.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct DebounceConfig {
    /// Quiet time after the last edit before a page is written.
    pub debounce: Duration,
    /// Longest a dirty page may wait while edits keep arriving.
    pub max_delay: Duration,
}

impl Default for DebounceConfig {
    fn default() -> Self {
        Self {
            debounce: Duration::from_millis(400),
            max_delay: Duration::from_secs(2),
        }
    }
}

/// First retry delay after a failed write.
pub const BACKOFF_START: Duration = Duration::from_secs(1);
/// Cap of the retry delay.
pub const BACKOFF_CAP: Duration = Duration::from_secs(60);

#[derive(Debug, Clone)]
struct Entry {
    first: Duration,
    last: Duration,
    batch: u64,
    not_before: Option<Duration>,
}

/// Pending pages and their deadlines.
#[derive(Debug, Clone)]
pub struct WriteQueue {
    cfg: DebounceConfig,
    pending: BTreeMap<PageKey, Entry>,
    attempts: BTreeMap<PageKey, u32>,
    next_batch: u64,
}

impl WriteQueue {
    /// Empty queue.
    #[must_use]
    pub fn new(cfg: DebounceConfig) -> Self {
        Self {
            cfg,
            pending: BTreeMap::new(),
            attempts: BTreeMap::new(),
            next_batch: 0,
        }
    }

    /// Schedules `keys` (one transaction) at time `now`; a page already pending is not
    /// duplicated, its debounce restarts and it joins the new batch.
    pub fn mark(&mut self, keys: &[PageKey], now: Duration) {
        self.next_batch += 1;
        let batch = self.next_batch;
        for k in keys {
            self.pending
                .entry(k.clone())
                .and_modify(|e| {
                    e.last = now;
                    e.batch = batch;
                })
                .or_insert(Entry {
                    first: now,
                    last: now,
                    batch,
                    not_before: None,
                });
        }
    }

    fn deadline_of(&self, e: &Entry) -> Duration {
        let d = (e.last + self.cfg.debounce).min(e.first + self.cfg.max_delay);
        e.not_before.map_or(d, |n| n.max(d))
    }

    /// Earliest time at which something is due.
    #[must_use]
    pub fn next_deadline(&self) -> Option<Duration> {
        self.pending.values().map(|e| self.deadline_of(e)).min()
    }

    /// Pages to write at `now`: every page past its deadline plus the pages that share a batch
    /// with one of them. They stay pending until [`WriteQueue::settle`].
    #[must_use]
    pub fn due(&self, now: Duration) -> Vec<PageKey> {
        let batches: Vec<u64> = self
            .pending
            .values()
            .filter(|e| self.deadline_of(e) <= now)
            .map(|e| e.batch)
            .collect();
        self.pending
            .iter()
            .filter(|(_, e)| batches.contains(&e.batch))
            .map(|(k, _)| k.clone())
            .collect()
    }

    /// Every pending page (flush on demand / shutdown).
    #[must_use]
    pub fn all(&self) -> Vec<PageKey> {
        self.pending.keys().cloned().collect()
    }

    /// Nothing pending.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    /// A page was written (or no longer needs writing).
    pub fn done(&mut self, key: &PageKey) {
        self.pending.remove(key);
        self.attempts.remove(key);
    }

    /// A page conflicts with an external change: stop retrying until it is edited again or the
    /// conflict is resolved.
    pub fn parked(&mut self, key: &PageKey) {
        self.pending.remove(key);
        self.attempts.remove(key);
    }

    /// A write failed: retry later with exponential backoff. Returns the delay.
    pub fn failed(&mut self, key: &PageKey, now: Duration) -> Duration {
        let n = self.attempts.entry(key.clone()).or_insert(0);
        *n += 1;
        let shift = (*n - 1).min(16);
        let delay = BACKOFF_START
            .checked_mul(1u32 << shift)
            .map_or(BACKOFF_CAP, |d| d.min(BACKOFF_CAP));
        self.next_batch += 1;
        let batch = self.next_batch;
        self.pending.insert(
            key.clone(),
            Entry {
                first: now,
                last: now,
                batch,
                not_before: Some(now + delay),
            },
        );
        delay
    }

    /// Number of failed attempts recorded for `key`.
    #[must_use]
    pub fn attempts(&self, key: &PageKey) -> u32 {
        self.attempts.get(key).copied().unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn k(s: &str) -> PageKey {
        PageKey::from_title(s)
    }
    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn debounce_restarts_on_each_edit() {
        let mut q = WriteQueue::new(DebounceConfig::default());
        q.mark(&[k("a")], ms(0));
        assert_eq!(q.next_deadline(), Some(ms(400)));
        q.mark(&[k("a")], ms(300));
        assert_eq!(q.next_deadline(), Some(ms(700)));
        assert!(q.due(ms(699)).is_empty());
        assert_eq!(q.due(ms(700)), vec![k("a")]);
    }

    #[test]
    fn max_delay_caps_continuous_typing() {
        let mut q = WriteQueue::new(DebounceConfig::default());
        for t in (0..=1900).step_by(100) {
            q.mark(&[k("a")], ms(t));
        }
        // Last edit at 1900 would wait until 2300; the cap is first + 2 s.
        assert_eq!(q.next_deadline(), Some(ms(2000)));
        assert_eq!(q.due(ms(2000)), vec![k("a")]);
    }

    #[test]
    fn dedup_per_page_and_batches_flush_together() {
        let mut q = WriteQueue::new(DebounceConfig::default());
        q.mark(&[k("a"), k("b")], ms(0));
        q.mark(&[k("a")], ms(10));
        q.mark(&[k("c"), k("a")], ms(20));
        assert_eq!(q.all().len(), 3);
        // b is a batch of its own now (a moved to the later batch), due at 400.
        assert_eq!(q.due(ms(400)), vec![k("b")]);
        // a and c share the last batch.
        assert_eq!(q.due(ms(420)), vec![k("a"), k("b"), k("c")]);
        q.done(&k("a"));
        q.done(&k("b"));
        q.done(&k("c"));
        assert!(q.is_empty());
        assert_eq!(q.next_deadline(), None);
    }

    #[test]
    fn failures_back_off_1s_to_60s() {
        let mut q = WriteQueue::new(DebounceConfig::default());
        let key = k("a");
        let delays: Vec<u64> = (0..9).map(|_| q.failed(&key, ms(0)).as_secs()).collect();
        assert_eq!(delays, vec![1, 2, 4, 8, 16, 32, 60, 60, 60]);
        assert_eq!(q.attempts(&key), 9);
        q.done(&key);
        assert_eq!(q.attempts(&key), 0);
    }

    #[test]
    fn backoff_is_not_shortened_by_new_edits() {
        let mut q = WriteQueue::new(DebounceConfig::default());
        let key = k("a");
        q.failed(&key, ms(0));
        q.mark(std::slice::from_ref(&key), ms(100));
        assert_eq!(q.next_deadline(), Some(ms(1000)));
    }

    #[test]
    fn parked_pages_are_dropped() {
        let mut q = WriteQueue::new(DebounceConfig::default());
        q.mark(&[k("a")], ms(0));
        q.parked(&k("a"));
        assert!(q.is_empty());
    }
}
