//! Events the Pando service sends to the app over plain `std::sync::mpsc` channels.

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use crate::activity::{ActivityEntry, ActivityKind, ActivityLog};

/// Connection state of the integration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PandoStatus {
    /// Disabled, mode off, or stopped.
    Off,
    /// Enabled but this graph has no consent yet; nothing is contacted.
    ConsentRequired,
    /// Resolving the endpoint (starting the managed server) and probing.
    Starting,
    /// The server answered its health probe.
    Connected {
        /// Server version string.
        version: String,
    },
    /// The server answered but rejected the token (401/403).
    Unauthorized,
    /// The server is older than the minimum Bitacora needs.
    TooOld {
        /// Version the server reported.
        version: String,
        /// Minimum version required.
        min: String,
    },
    /// Not usable; `reason` is safe to show (never contains a token).
    Unavailable {
        /// Human-readable cause.
        reason: String,
    },
}

impl PandoStatus {
    /// A stable, content-free word for the status (activity log, tests).
    #[must_use]
    pub fn word(&self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::ConsentRequired => "consent_required",
            Self::Starting => "connecting",
            Self::Connected { .. } => "ok",
            Self::Unauthorized => "unauthorized",
            Self::TooOld { .. } => "too_old",
            Self::Unavailable { .. } => "unreachable",
        }
    }
}

/// Progress of a knowledge-base sync (emitted by the sync story, BIT-US-0131+).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncProgress {
    /// Documents processed so far.
    pub done: u64,
    /// Total documents, when known.
    pub total: Option<u64>,
}

/// One event of an agent run (filled in by the AG-UI chat story, BIT-US-0137+).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunEvent {
    /// Run identifier.
    pub run_id: String,
    /// Event kind name (AG-UI event type).
    pub kind: String,
    /// Event payload as JSON text.
    pub data: String,
}

/// Everything the app can receive from the service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PandoEvent {
    /// The status changed.
    Status(PandoStatus),
    /// KB sync progress.
    SyncProgress(SyncProgress),
    /// Agent run event.
    Run(RunEvent),
    /// Something worth listing in the activity log (already written to it when a log is set).
    Activity(ActivityEntry),
}

/// Fan-out to every subscriber; dead receivers are dropped on the next emit.
#[derive(Clone, Default)]
pub struct EventSink(
    Arc<Mutex<Vec<Sender<PandoEvent>>>>,
    Arc<OnceLock<ActivityLog>>,
);

impl std::fmt::Debug for EventSink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("EventSink")
    }
}

impl EventSink {
    /// A new receiver; it only sees later events.
    #[must_use]
    pub fn subscribe(&self) -> Receiver<PandoEvent> {
        let (tx, rx) = mpsc::channel();
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(tx);
        rx
    }

    /// Attaches the activity log; status changes and [`PandoEvent::Activity`] events are written
    /// to it from now on. Only the first call has an effect.
    pub fn set_log(&self, log: ActivityLog) {
        let _ = self.1.set(log);
    }

    /// Writes `entry` to the activity log (when set) and announces it.
    pub fn record(&self, entry: ActivityEntry) {
        self.emit(&PandoEvent::Activity(entry));
    }

    /// Sends `ev` to every live subscriber.
    pub fn emit(&self, ev: &PandoEvent) {
        if let Some(log) = self.1.get() {
            match ev {
                PandoEvent::Activity(e) => log.record_best_effort(e),
                PandoEvent::Status(s) => {
                    log.record_best_effort(&ActivityEntry::new(
                        ActivityKind::Status,
                        s.word(),
                        0,
                        Vec::new(),
                    ));
                }
                _ => {}
            }
        }
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .retain(|tx| tx.send(ev.clone()).is_ok());
    }
}
