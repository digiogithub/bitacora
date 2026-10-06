//! [`GraphWatcher`]: recursive `notify` watcher with per-path debouncing.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use notify::event::{ModifyKind, RenameMode};
use notify::{ErrorKind, EventKind, RecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{
    DebounceEventResult, DebouncedEvent, Debouncer, RecommendedCache, new_debouncer,
};

use crate::echo::EchoFilter;
use crate::ignore::IgnoreRules;
use crate::poll::{self, Poller};
use crate::process::{Op, Processor, Sink};
use crate::{Error, WatchEvent, WatchNotice};

/// Watcher tuning.
#[derive(Debug, Clone)]
pub struct WatchConfig {
    /// Per-path debounce window (default 100 ms).
    pub debounce: Duration,
    /// Interval of the mtime rescan fallback (default 5 s).
    pub poll_interval: Duration,
    /// Fall back to polling when the OS watch fails (default `true`).
    pub poll_fallback: bool,
    /// Skip the OS watcher and only poll (tests, network file systems).
    pub force_polling: bool,
}

impl Default for WatchConfig {
    fn default() -> Self {
        Self {
            debounce: Duration::from_millis(100),
            poll_interval: Duration::from_secs(5),
            poll_fallback: true,
            force_polling: false,
        }
    }
}

struct Shared {
    root: PathBuf,
    ignore: IgnoreRules,
    processor: Arc<Mutex<Processor>>,
    sink: Sink,
    poll_interval: Duration,
    /// Dropping the sender stops the polling thread.
    poll_stop: Mutex<Option<Sender<()>>>,
}

impl Shared {
    fn start_polling(&self, reason: &str) {
        let mut stop = self
            .poll_stop
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if stop.is_some() {
            return;
        }
        (self.sink)(WatchEvent::Notice(WatchNotice {
            message: format!(
                "{reason}; falling back to a rescan every {:?}",
                self.poll_interval
            ),
            fallback_polling: true,
        }));
        let poller = Poller::new(self.root.clone(), self.ignore.clone());
        *stop = Some(poll::spawn(
            poller,
            Arc::clone(&self.processor),
            self.poll_interval,
        ));
    }
}

/// Watches a graph folder and reports external changes as [`WatchEvent`]s. Dropping it stops
/// all threads.
pub struct GraphWatcher {
    shared: Arc<Shared>,
    _debouncer: Option<Debouncer<RecommendedWatcher, RecommendedCache>>,
}

impl std::fmt::Debug for GraphWatcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GraphWatcher")
            .field("root", &self.shared.root)
            .field("polling", &self.is_polling())
            .finish()
    }
}

impl GraphWatcher {
    /// Starts watching `root`.
    ///
    /// `echo` should be the filter fed by the writer; `sink` receives every event on a watcher
    /// thread (send to a channel and return quickly).
    ///
    /// # Errors
    /// [`Error::Root`] when `root` is not an accessible directory.
    pub fn start(
        root: impl Into<PathBuf>,
        ignore: IgnoreRules,
        echo: EchoFilter,
        cfg: WatchConfig,
        sink: impl Fn(WatchEvent) + Send + Sync + 'static,
    ) -> Result<Self, Error> {
        let root = root.into();
        let root = std::fs::canonicalize(&root).map_err(|source| Error::Root {
            path: root.clone(),
            source,
        })?;
        let sink: Sink = Arc::new(sink);
        let processor = Arc::new(Mutex::new(Processor::new(
            root.clone(),
            ignore.clone(),
            echo,
            Arc::clone(&sink),
        )));
        let shared = Arc::new(Shared {
            root: root.clone(),
            ignore,
            processor,
            sink,
            poll_interval: cfg.poll_interval,
            poll_stop: Mutex::new(None),
        });

        if cfg.force_polling {
            shared.start_polling("polling forced by configuration");
            return Ok(Self {
                shared,
                _debouncer: None,
            });
        }

        let handler_shared = Arc::clone(&shared);
        let allow_poll = cfg.poll_fallback;
        let handler = move |res: DebounceEventResult| handle(&handler_shared, allow_poll, res);
        let started = new_debouncer(cfg.debounce, None, handler).and_then(|mut d| {
            d.watch(&root, RecursiveMode::Recursive)?;
            Ok(d)
        });
        let debouncer = match started {
            Ok(d) => Some(d),
            Err(e) => {
                if allow_poll {
                    shared.start_polling(&format!("file watcher unavailable ({e})"));
                } else {
                    (shared.sink)(WatchEvent::Notice(WatchNotice {
                        message: format!("file watcher unavailable ({e})"),
                        fallback_polling: false,
                    }));
                }
                None
            }
        };
        Ok(Self {
            shared,
            _debouncer: debouncer,
        })
    }

    /// Whether the mtime rescan fallback is active.
    #[must_use]
    pub fn is_polling(&self) -> bool {
        self.shared
            .poll_stop
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_some()
    }
}

impl Drop for GraphWatcher {
    fn drop(&mut self) {
        // Stops the polling thread; the debouncer stops itself when dropped.
        self.shared
            .poll_stop
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
    }
}

fn handle(shared: &Shared, allow_poll: bool, res: DebounceEventResult) {
    match res {
        Ok(events) => {
            let (rescan, ops) = plan(events);
            if rescan {
                (shared.sink)(WatchEvent::Rescan);
            }
            if !ops.is_empty() {
                shared
                    .processor
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .apply(ops);
            }
        }
        Err(errors) => {
            for e in errors {
                if matches!(e.kind, ErrorKind::MaxFilesWatch) && allow_poll {
                    shared.start_polling("OS file watch limit reached");
                } else {
                    (shared.sink)(WatchEvent::Notice(WatchNotice {
                        message: format!("file watcher error: {e}"),
                        fallback_polling: false,
                    }));
                }
            }
        }
    }
}

/// Turns debounced events into a rescan flag plus the operations to apply.
///
/// A rescan request (inotify overflow, Windows buffer overflow, FSEvents `MustScanSubDirs`)
/// asks the consumer for a full reconcile. FSEvents attaches the affected directory to that
/// request, and the debouncer keeps only one such request per batch, so the path is also walked
/// here: a directory created with files in it is reported even when the OS coalesced the
/// per-file events into a single "scan this directory" hint.
fn plan(events: Vec<DebouncedEvent>) -> (bool, Vec<Op>) {
    let mut rescan = false;
    let mut ops = Vec::new();
    let mut touched = HashSet::new();
    for ev in events {
        if ev.need_rescan() {
            rescan = true;
            for p in &ev.paths {
                if touched.insert(p.clone()) {
                    ops.push(Op::Touch(p.clone()));
                }
            }
            continue;
        }
        match ev.kind {
            EventKind::Access(_) => {}
            EventKind::Modify(ModifyKind::Name(RenameMode::Both)) if ev.paths.len() == 2 => {
                ops.push(Op::Rename {
                    from: ev.paths[0].clone(),
                    to: ev.paths[1].clone(),
                });
            }
            _ => {
                for p in &ev.paths {
                    if touched.insert(p.clone()) {
                        ops.push(Op::Touch(p.clone()));
                    }
                }
            }
        }
    }
    (rescan, ops)
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::Event;
    use notify::event::Flag;
    use std::time::Instant;

    #[test]
    fn rescan_request_with_a_path_also_touches_that_path() {
        let ev = Event::new(EventKind::Other)
            .set_flag(Flag::Rescan)
            .add_path(PathBuf::from("/g/pages/sub"));
        let (rescan, ops) = plan(vec![DebouncedEvent::new(ev, Instant::now())]);
        assert!(rescan);
        assert_eq!(ops, [Op::Touch(PathBuf::from("/g/pages/sub"))]);
    }

    #[test]
    fn rescan_request_without_a_path_only_flags_a_rescan() {
        let ev = Event::new(EventKind::Other).set_flag(Flag::Rescan);
        let (rescan, ops) = plan(vec![DebouncedEvent::new(ev, Instant::now())]);
        assert!(rescan);
        assert!(ops.is_empty());
    }
}
