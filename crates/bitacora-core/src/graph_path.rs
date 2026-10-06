//! `GraphPath`: graph-relative, NFC-normalised, `/`-separated path (§3.7).

use std::fmt;
use std::path::{Component, Path, PathBuf};

use unicode_normalization::UnicodeNormalization;

/// Invalid graph path.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GraphPathError {
    /// Empty path.
    #[error("empty graph path")]
    Empty,
    /// Absolute path or drive prefix.
    #[error("graph path must be relative: {0}")]
    Absolute(String),
    /// Contains `..` or `.` segments.
    #[error("graph path must not contain `.` or `..` segments: {0}")]
    Traversal(String),
    /// Path outside the graph root.
    #[error("path is outside the graph root: {0}")]
    Outside(String),
    /// Path is not valid UTF-8.
    #[error("path is not valid UTF-8: {0}")]
    NotUtf8(String),
}

/// A path relative to the graph root, always `/`-separated and NFC.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GraphPath(String);

impl GraphPath {
    /// Build from a relative string. `\` separators are accepted (Windows) and converted.
    pub fn new(rel: &str) -> Result<Self, GraphPathError> {
        let unified = rel.replace('\\', "/");
        if unified.is_empty() {
            return Err(GraphPathError::Empty);
        }
        if unified.starts_with('/') || unified.as_bytes().get(1) == Some(&b':') {
            return Err(GraphPathError::Absolute(rel.to_owned()));
        }
        let mut parts = Vec::new();
        for seg in unified.split('/') {
            match seg {
                "" => {}
                "." | ".." => return Err(GraphPathError::Traversal(rel.to_owned())),
                s => parts.push(s),
            }
        }
        if parts.is_empty() {
            return Err(GraphPathError::Empty);
        }
        Ok(Self(parts.join("/").nfc().collect()))
    }

    /// Build from an absolute path under `root`.
    pub fn from_abs(root: &Path, abs: &Path) -> Result<Self, GraphPathError> {
        let rel = abs
            .strip_prefix(root)
            .map_err(|_| GraphPathError::Outside(abs.display().to_string()))?;
        let mut parts = Vec::new();
        for c in rel.components() {
            match c {
                Component::Normal(s) => parts.push(
                    s.to_str()
                        .ok_or_else(|| GraphPathError::NotUtf8(abs.display().to_string()))?,
                ),
                _ => return Err(GraphPathError::Traversal(abs.display().to_string())),
            }
        }
        Self::new(&parts.join("/"))
    }

    /// The path as `/`-separated NFC text.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Absolute filesystem path under `root`.
    ///
    /// The on-disk spelling may differ from NFC (e.g. NFD names written by other tools); callers
    /// that must open an existing file should keep the scanner's original `PathBuf`.
    pub fn to_fs_path(&self, root: &Path) -> PathBuf {
        let mut p = root.to_path_buf();
        p.extend(self.0.split('/'));
        p
    }

    /// File name (last segment).
    pub fn file_name(&self) -> &str {
        self.0.rsplit('/').next().unwrap_or(&self.0)
    }

    /// Lower-case extension after the last `.` of the file name, if any.
    pub fn extension(&self) -> Option<&str> {
        let n = self.file_name();
        n.rfind('.').map(|i| &n[i + 1..])
    }
}

impl fmt::Display for GraphPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for GraphPath {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalises() {
        assert_eq!(
            GraphPath::new("pages\\a/b.md").expect("p").as_str(),
            "pages/a/b.md"
        );
        assert_eq!(
            GraphPath::new("pages/Cafe\u{301}.md").expect("p").as_str(),
            "pages/Caf\u{e9}.md"
        );
        assert_eq!(GraphPath::new("a//b").expect("p").as_str(), "a/b");
    }

    #[test]
    fn rejects() {
        assert_eq!(GraphPath::new(""), Err(GraphPathError::Empty));
        assert!(matches!(
            GraphPath::new("/a"),
            Err(GraphPathError::Absolute(_))
        ));
        assert!(matches!(
            GraphPath::new("C:/a"),
            Err(GraphPathError::Absolute(_))
        ));
        assert!(matches!(
            GraphPath::new("a/../b"),
            Err(GraphPathError::Traversal(_))
        ));
    }

    #[test]
    fn from_abs_and_back() {
        let root = Path::new("/g");
        let p = GraphPath::from_abs(root, Path::new("/g/pages/x.md")).expect("p");
        assert_eq!(p.as_str(), "pages/x.md");
        assert_eq!(p.to_fs_path(root), Path::new("/g/pages/x.md"));
        assert!(GraphPath::from_abs(root, Path::new("/h/x.md")).is_err());
        assert_eq!(p.file_name(), "x.md");
        assert_eq!(p.extension(), Some("md"));
    }
}
