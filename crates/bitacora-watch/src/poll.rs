//! Periodic mtime rescan used when the OS watcher is unavailable (inotify limit, network file
//! systems, ...).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, SystemTime};

use crate::ignore::IgnoreRules;
use crate::process::{Op, Processor, walk_into};

/// Change detector: remembers `(mtime, len)` per file between ticks.
pub(crate) struct Poller {
    root: PathBuf,
    ignore: IgnoreRules,
    seen: Option<HashMap<String, (Option<SystemTime>, u64)>>,
}

impl Poller {
    pub(crate) fn new(root: PathBuf, ignore: IgnoreRules) -> Self {
        Self {
            root,
            ignore,
            seen: None,
        }
    }

    /// One scan. The first call only records a baseline and returns no operations.
    pub(crate) fn tick(&mut self) -> Vec<Op> {
        let mut now = HashMap::new();
        walk_into(
            &self.root,
            &self.root.clone(),
            &self.ignore,
            &mut |rel, md| {
                now.insert(rel, (md.modified().ok(), md.len()));
            },
        );
        let mut ops = Vec::new();
        if let Some(prev) = &self.seen {
            let mut changed: Vec<&String> = now
                .iter()
                .filter(|(k, v)| prev.get(*k) != Some(*v))
                .map(|(k, _)| k)
                .chain(prev.keys().filter(|k| !now.contains_key(*k)))
                .collect();
            changed.sort();
            ops.extend(changed.into_iter().map(|k| Op::Touch(self.root.join(k))));
        }
        self.seen = Some(now);
        ops
    }
}

/// Starts the polling thread. Dropping the returned sender stops it.
pub(crate) fn spawn(
    mut poller: Poller,
    processor: Arc<Mutex<Processor>>,
    interval: Duration,
    trace: bool,
) -> Sender<()> {
    let (tx, rx) = mpsc::channel::<()>();
    // Baseline now so that changes made after the fallback started are detected.
    poller.tick();
    if trace {
        eprintln!("[watch poll] started, interval {interval:?}");
    }
    // A failed spawn only loses the fallback; the notice has already been emitted.
    let _ = thread::Builder::new()
        .name("bitacora-watch-poll".into())
        .spawn(move || {
            while let Err(RecvTimeoutError::Timeout) = rx.recv_timeout(interval) {
                let ops = poller.tick();
                let mut p = processor.lock().unwrap_or_else(PoisonError::into_inner);
                if trace {
                    eprintln!("[watch poll] tick: {} changed path(s) {ops:?}", ops.len());
                }
                p.apply(ops);
                let swept = p.sweep_missing();
                if trace {
                    eprintln!(
                        "[watch poll] sweep: {swept} missing of {} known",
                        p.known_len()
                    );
                }
            }
        });
    tx
}
