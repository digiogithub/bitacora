//! `bitacora-watch`: File-system watcher built on `notify` with debouncing and echo suppression
//! of our own writes.
//!
//! * [`GraphWatcher`] watches a graph root recursively, debounces raw events (100 ms per path by
//!   default), re-reads the touched files, hashes them with BLAKE3 and emits [`WatchEvent`]s.
//! * [`IgnoreRules`] mirrors Logseq's directory-walk rules (via `bitacora_core::scan`), plus the
//!   `.bitacora-tmp` files of our own atomic writer and the config `:hidden` prefixes.
//! * [`EchoFilter`] remembers `(path, content hash)` pairs of our own writes for a few seconds so
//!   the resulting file-system events are dropped. Suppression is by **content hash**: an
//!   external write with different bytes right after our own write is still reported.
//! * When the OS watch cannot be established (inotify limit, unsupported file system) the watcher
//!   emits a [`WatchNotice`] and falls back to a periodic mtime rescan.
//!
//! The crate does not know the command queue. The application glue (a) feeds the filter from
//! `QueueEvent::Flushed` / `FilesApplied` with [`EchoFilter::record_written_file`] and
//! [`EchoFilter::record_deleted`], and (b) maps [`FileEvent`] to
//! `ExternalFileChanged` commands / the index `FsChange` type. [`WatchEvent::Rescan`] maps to
//! `FsChange::Overflow` (full reconcile).
//!
//! # Platform notes
//!
//! * **Linux (inotify)**: one watch per directory; `fs.inotify.max_user_watches` may be too low
//!   for big graphs (error `MaxFilesWatch`) -> polling fallback. Queue overflow is reported by
//!   `notify` as a rescan request -> [`WatchEvent::Rescan`].
//! * **macOS (FSEvents)**: events are coalesced by the OS with its own latency; the root is
//!   canonicalised because `/var` and `/tmp` are symlinks.
//! * **Windows (ReadDirectoryChangesW)**: buffer overflow is reported as a rescan request.
//!   Editors that save by delete+create are handled because every event is resolved by
//!   inspecting the file system after the debounce window.

mod echo;
mod ignore;
mod poll;
mod process;
mod watcher;

pub use echo::EchoFilter;
pub use ignore::IgnoreRules;
pub use watcher::{GraphWatcher, WatchConfig};

use std::sync::Arc;

/// Errors produced by this crate.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The graph root cannot be used.
    #[error("invalid graph root {path}: {source}")]
    Root {
        /// Offending path.
        path: std::path::PathBuf,
        /// Underlying error.
        source: std::io::Error,
    },
}

/// What happened to a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileEventKind {
    /// Created or modified (its content is in [`FileEvent::bytes`]).
    Upserted,
    /// Deleted.
    Removed,
    /// Moved from `from` (graph-relative) to [`FileEvent::rel_path`].
    Renamed {
        /// Previous graph-relative path.
        from: String,
    },
}

/// A debounced, hashed, echo-filtered change of one graph file.
#[derive(Debug, Clone)]
pub struct FileEvent {
    /// Graph-relative path, `/` separated.
    pub rel_path: String,
    /// What happened.
    pub kind: FileEventKind,
    /// BLAKE3 of `bytes` (`None` for removals).
    pub hash: Option<blake3::Hash>,
    /// Content read after the debounce window (`None` for removals).
    pub bytes: Option<Arc<[u8]>>,
}

/// Something the watcher wants the user or the app to know.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchNotice {
    /// Human-readable description.
    pub message: String,
    /// Whether the watcher switched to the periodic rescan fallback.
    pub fallback_polling: bool,
}

/// Output of [`GraphWatcher`].
#[derive(Debug, Clone)]
pub enum WatchEvent {
    /// A file changed.
    File(FileEvent),
    /// Events may have been lost (OS queue overflow): do a full rescan / reconcile.
    Rescan,
    /// Watcher problem notice.
    Notice(WatchNotice),
}

/// Crate name, used by smoke tests.
pub const CRATE_NAME: &str = "bitacora-watch";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_name_matches_package() {
        assert_eq!(CRATE_NAME, env!("CARGO_PKG_NAME"));
    }
}
