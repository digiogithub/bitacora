use std::path::PathBuf;

/// Errors produced by this crate.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// SQLite reported an error.
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// A filesystem operation on the index files failed.
    #[error("io error on {path}: {source}")]
    Io {
        /// Path involved.
        path: PathBuf,
        /// Underlying error.
        #[source]
        source: std::io::Error,
    },
    /// The platform data directory could not be determined.
    #[error("cannot determine the platform data directory")]
    NoDataDir,
    /// The graph root could not be resolved to an absolute path.
    #[error("cannot resolve graph root {path}: {source}")]
    GraphRoot {
        /// Graph root as given.
        path: PathBuf,
        /// Underlying error.
        #[source]
        source: std::io::Error,
    },
    /// The index location is inside the graph folder, which would violate ADR-005.
    #[error("index location {0} must not be inside the graph folder")]
    InsideGraph(PathBuf),
    /// The write connection was already taken.
    #[error("the write connection is already owned by another component")]
    WriterTaken,
    /// FTS5 (word or trigram tokenizer) is not available in this SQLite build.
    #[error("required SQLite feature missing: {0}")]
    MissingFeature(&'static str),
}

impl Error {
    pub(crate) fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}
