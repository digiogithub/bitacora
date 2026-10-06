//! [`QueueGraphWriter`]: the sync engine's [`GraphWriter`] on top of core's command queue
//! (AGENTS.md rule 3). `acquire` flushes pending pages and blocks every other writer until the
//! lock is dropped; `apply` maps [`FileChange`] to core's [`FileEdit`].

use std::time::Duration;

use bitacora_core::queue::{CommandQueue, FileEdit, QueueError, QueueLock};
use bitacora_sync::writer::{FileChange, GraphLock, GraphWriter, WriterError};

/// How long `acquire` waits for the queue before reporting [`WriterError::Busy`].
pub const DEFAULT_ACQUIRE_TIMEOUT: Duration = Duration::from_secs(5);

/// Adapter implementing [`GraphWriter`] over a [`CommandQueue`].
#[derive(Debug, Clone)]
pub struct QueueGraphWriter {
    queue: CommandQueue,
    timeout: Duration,
}

impl QueueGraphWriter {
    /// Adapter with the default acquire timeout.
    #[must_use]
    pub fn new(queue: CommandQueue) -> Self {
        Self {
            queue,
            timeout: DEFAULT_ACQUIRE_TIMEOUT,
        }
    }

    /// Overrides the acquire timeout.
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

struct Lock(QueueLock);

fn map_error(e: QueueError) -> WriterError {
    match e {
        QueueError::Busy => WriterError::Busy,
        QueueError::Stale(p) => WriterError::Stale(p),
        other => WriterError::Failed(other.to_string()),
    }
}

fn edit_of(change: &FileChange) -> FileEdit {
    match change {
        FileChange::Write {
            path,
            content,
            expected,
        } => FileEdit::Write {
            path: path.clone(),
            content: content.clone(),
            expected: expected.clone(),
        },
        FileChange::Delete { path, expected } => FileEdit::Delete {
            path: path.clone(),
            expected: expected.clone(),
        },
    }
}

impl GraphLock for Lock {
    fn apply(&mut self, changes: &[FileChange]) -> Result<(), WriterError> {
        self.0
            .apply(changes.iter().map(edit_of).collect())
            .map_err(map_error)
    }
}

impl GraphWriter for QueueGraphWriter {
    fn acquire(&self) -> Result<Box<dyn GraphLock + '_>, WriterError> {
        let lock = self.queue.acquire(self.timeout).map_err(map_error)?;
        Ok(Box::new(Lock(lock)))
    }
}
