//! Error and diagnostic types.

use std::fmt;
use std::path::PathBuf;

/// What went wrong while reading or editing EDN.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticKind {
    /// Malformed EDN (unbalanced delimiters, bad literal, ...).
    Syntax,
    /// A map or set holds the same key twice (Logseq rejects such a file).
    DuplicateKey,
    /// A map has a key without a value.
    OddMapEntries,
    /// The config root is not a map.
    NotAMap,
    /// The file could not be read.
    Io,
}

/// A located problem in an EDN document (`ConfigError` in the backlog wording).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// Category of the problem.
    pub kind: DiagnosticKind,
    /// Byte offset in the source.
    pub offset: usize,
    /// 1-based line.
    pub line: usize,
    /// 1-based column, counted in characters.
    pub column: usize,
    /// Human readable message.
    pub message: String,
    /// File the problem was found in, when read from disk.
    pub path: Option<PathBuf>,
}

impl Diagnostic {
    /// Builds a diagnostic for byte `offset` of `src`.
    pub fn at(src: &str, offset: usize, kind: DiagnosticKind, message: impl Into<String>) -> Self {
        let offset = offset.min(src.len());
        let before = src.get(..offset).unwrap_or("");
        let line = before.matches('\n').count() + 1;
        let line_start = before.rfind('\n').map_or(0, |i| i + 1);
        let column = before[line_start..].chars().count() + 1;
        Diagnostic {
            kind,
            offset,
            line,
            column,
            message: message.into(),
            path: None,
        }
    }

    /// Attaches the file the diagnostic belongs to.
    #[must_use]
    pub fn with_path(mut self, path: PathBuf) -> Self {
        self.path = Some(path);
        self
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(p) = &self.path {
            write!(f, "{}:", p.display())?;
        }
        write!(f, "{}:{}: {}", self.line, self.column, self.message)
    }
}

impl std::error::Error for Diagnostic {}

/// Errors produced by this crate.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The EDN text is invalid.
    #[error("invalid config: {0}")]
    Invalid(#[from] Diagnostic),
    /// Reading a config file failed.
    #[error("cannot read {path}: {source}")]
    Io {
        /// File that could not be read.
        path: PathBuf,
        /// Underlying error.
        source: std::io::Error,
    },
    /// An edit could not be applied.
    #[error("cannot edit config: {0}")]
    Edit(String),
}
