//! Opt-in performance instrumentation and the in-app benchmark (BIT-T-0337).
//!
//! Disabled by default: [`mark`] and [`span`] cost one relaxed atomic load. With
//! `--perf-bench` (or `BITACORA_PERF=1`) they record startup marks and span durations, and the
//! driver in [`bench`] exercises the real workspace (open a 5,000-block page, scroll, type,
//! search, query widgets) and prints one `PERF_BENCH {json}` line before exiting. Every mark
//! is also a `tracing` event on target `bitacora::perf` (spans at debug level).

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use serde::Serialize;

pub mod bench;
pub mod frame;

pub use frame::FrameEnd;

static ENABLED: AtomicBool = AtomicBool::new(false);
static START: OnceLock<Instant> = OnceLock::new();
static MARKS: Mutex<Vec<(&'static str, f64)>> = Mutex::new(Vec::new());
static STAMPS: Mutex<Vec<(&'static str, Instant)>> = Mutex::new(Vec::new());
static SPANS: Mutex<BTreeMap<&'static str, Vec<f64>>> = Mutex::new(BTreeMap::new());

/// Anchors "process start" for the marks; call as early as possible.
pub fn init() {
    START.get_or_init(Instant::now);
    if std::env::var_os("BITACORA_PERF").is_some() {
        enable();
    }
}

/// Turns the recording on.
pub fn enable() {
    START.get_or_init(Instant::now);
    ENABLED.store(true, Ordering::Relaxed);
}

/// Whether recording is on.
pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// Milliseconds since [`init`].
pub fn since_start_ms() -> f64 {
    START
        .get()
        .map_or(0.0, |s| s.elapsed().as_secs_f64() * 1000.0)
}

/// Records that `name` happened now (the first time only).
pub fn mark(name: &'static str) {
    if !enabled() {
        return;
    }
    let ms = since_start_ms();
    if let Ok(mut marks) = MARKS.lock()
        && !marks.iter().any(|(n, _)| *n == name)
    {
        tracing::info!(target: "bitacora::perf", mark = name, ms);
        marks.push((name, ms));
    }
}

/// Remembers "now" as the latest time `name` happened (every call replaces the previous one).
pub fn stamp(name: &'static str) {
    if !enabled() {
        return;
    }
    if let Ok(mut stamps) = STAMPS.lock() {
        match stamps.iter_mut().find(|(n, _)| *n == name) {
            Some(slot) => slot.1 = Instant::now(),
            None => stamps.push((name, Instant::now())),
        }
    }
}

/// The latest [`stamp`] of `name`.
pub fn stamped(name: &str) -> Option<Instant> {
    STAMPS
        .lock()
        .ok()?
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, t)| *t)
}

/// When `name` was first marked, in ms since start.
pub fn marked(name: &str) -> Option<f64> {
    MARKS
        .lock()
        .ok()?
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, ms)| *ms)
}

/// Adds one duration sample to the span series `name`.
pub fn record(name: &'static str, ms: f64) {
    if !enabled() {
        return;
    }
    tracing::debug!(target: "bitacora::perf", span = name, ms);
    if let Ok(mut spans) = SPANS.lock() {
        spans.entry(name).or_default().push(ms);
    }
}

/// Times a scope: the duration is recorded when the guard drops. `None` (free) when disabled.
#[must_use]
pub fn span(name: &'static str) -> Option<Span> {
    enabled().then(|| Span {
        name,
        start: Instant::now(),
    })
}

/// Guard returned by [`span`].
#[derive(Debug)]
pub struct Span {
    name: &'static str,
    start: Instant,
}

impl Drop for Span {
    fn drop(&mut self) {
        record(self.name, self.start.elapsed().as_secs_f64() * 1000.0);
    }
}

/// Latency statistics in milliseconds (nearest-rank percentiles).
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct Stats {
    /// Number of samples.
    pub n: usize,
    /// Mean.
    pub mean: f64,
    /// Median.
    pub p50: f64,
    /// 95th percentile.
    pub p95: f64,
    /// 99th percentile.
    pub p99: f64,
    /// Maximum.
    pub max: f64,
}

/// Computes [`Stats`] of `samples`.
pub fn stats(samples: &[f64]) -> Stats {
    if samples.is_empty() {
        return Stats::default();
    }
    let mut sorted = samples.to_vec();
    sorted.sort_by(f64::total_cmp);
    let rank = |p: f64| {
        let ix = ((p / 100.0) * sorted.len() as f64).ceil() as usize;
        sorted[ix.clamp(1, sorted.len()) - 1]
    };
    Stats {
        n: sorted.len(),
        mean: sorted.iter().sum::<f64>() / sorted.len() as f64,
        p50: rank(50.0),
        p95: rank(95.0),
        p99: rank(99.0),
        max: sorted[sorted.len() - 1],
    }
}

/// Statistics of every recorded span series.
pub fn span_stats() -> BTreeMap<String, Stats> {
    SPANS
        .lock()
        .map(|s| s.iter().map(|(k, v)| ((*k).to_owned(), stats(v))).collect())
        .unwrap_or_default()
}

/// Forgets the samples of one span series (between benchmark phases).
pub fn clear_span(name: &'static str) {
    if let Ok(mut spans) = SPANS.lock() {
        spans.remove(name);
    }
}

/// Resident set size in KiB (Linux `/proc`; `None` elsewhere).
pub fn rss_kib() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|l| l.starts_with("VmRSS:"))?;
    line.split_whitespace().nth(1)?.parse().ok()
}

/// Anonymous resident memory in KiB (`RssAnon`: heap and stacks, without mapped files such as
/// the SQLite database); Linux only.
pub fn rss_anon_kib() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|l| l.starts_with("RssAnon:"))?;
    line.split_whitespace().nth(1)?.parse().ok()
}

/// Every mark so far.
pub fn marks() -> BTreeMap<String, f64> {
    MARKS
        .lock()
        .map(|m| m.iter().map(|(n, ms)| ((*n).to_owned(), *ms)).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentiles_are_nearest_rank() {
        let s: Vec<f64> = (1..=100).map(f64::from).collect();
        let st = stats(&s);
        assert_eq!(
            (st.n, st.p50, st.p95, st.p99, st.max),
            (100, 50.0, 95.0, 99.0, 100.0)
        );
        assert_eq!(stats(&[]), Stats::default());
    }
}
