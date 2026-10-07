//! Events the Pando service sends to the app over plain `std::sync::mpsc` channels.

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, PoisonError};

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
    /// Not usable; `reason` is safe to show (never contains a token).
    Unavailable {
        /// Human-readable cause.
        reason: String,
    },
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
}

/// Fan-out to every subscriber; dead receivers are dropped on the next emit.
#[derive(Clone, Default)]
pub struct EventSink(Arc<Mutex<Vec<Sender<PandoEvent>>>>);

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

    /// Sends `ev` to every live subscriber.
    pub fn emit(&self, ev: &PandoEvent) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .retain(|tx| tx.send(ev.clone()).is_ok());
    }
}
