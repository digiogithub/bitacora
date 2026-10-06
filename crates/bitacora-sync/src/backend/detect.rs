//! System git discovery and version check (ADR-020, ADR-022).

use std::fmt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Minimum supported system git version (ADR-022).
pub const MIN_GIT_VERSION: GitVersion = GitVersion {
    major: 2,
    minor: 38,
    patch: 0,
};

/// A parsed `git --version`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct GitVersion {
    /// Major component.
    pub major: u32,
    /// Minor component.
    pub minor: u32,
    /// Patch component (0 when absent).
    pub patch: u32,
}

impl fmt::Display for GitVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// Parses output such as `git version 2.43.0`, `git version 2.39.3 (Apple Git-145)` or
/// `git version 2.45.0.windows.1`.
pub fn parse_git_version(output: &str) -> Option<GitVersion> {
    let rest = output.trim().strip_prefix("git version ")?;
    let token = rest.split_whitespace().next()?;
    let mut parts = token.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    Some(GitVersion {
        major,
        minor,
        patch,
    })
}

/// Outcome of looking for a system git.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitDetection {
    /// A usable git (>= [`MIN_GIT_VERSION`]).
    Found {
        /// Executable path (or bare name resolved via `PATH`).
        path: PathBuf,
        /// Version.
        version: GitVersion,
    },
    /// A git exists but is too old.
    TooOld {
        /// Executable path.
        path: PathBuf,
        /// Version found.
        version: GitVersion,
    },
    /// No git found.
    Missing,
}

impl GitDetection {
    /// Classifies an already-known executable and version.
    pub fn from_version(path: PathBuf, version: GitVersion) -> Self {
        if version >= MIN_GIT_VERSION {
            Self::Found { path, version }
        } else {
            Self::TooOld { path, version }
        }
    }
}

/// Looks for git: the configured binary (`sync.git_binary`) first, otherwise `git` on `PATH`.
///
/// A configured binary that fails to run falls back to `PATH`; nothing found means
/// [`GitDetection::Missing`]. Never bundles or downloads git (ADR-020).
pub fn detect_git(configured: Option<&Path>) -> GitDetection {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(p) = configured {
        candidates.push(p.to_path_buf());
    }
    candidates.push(PathBuf::from("git"));
    let mut too_old = None;
    for candidate in candidates {
        let Some(version) = probe(&candidate) else {
            continue;
        };
        match GitDetection::from_version(candidate, version) {
            found @ GitDetection::Found { .. } => return found,
            old => too_old = too_old.or(Some(old)),
        }
    }
    too_old.unwrap_or(GitDetection::Missing)
}

fn probe(binary: &Path) -> Option<GitVersion> {
    let out = Command::new(binary)
        .arg("--version")
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    parse_git_version(&String::from_utf8_lossy(&out.stdout))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{ActiveBackend, backend_kind_for};

    #[test]
    fn parses_known_formats() {
        let v = |s| parse_git_version(s).unwrap();
        assert_eq!(v("git version 2.43.0\n").to_string(), "2.43.0");
        assert_eq!(v("git version 2.39.3 (Apple Git-145)").minor, 39);
        let w = v("git version 2.45.0.windows.1");
        assert_eq!((w.major, w.minor, w.patch), (2, 45, 0));
        assert_eq!(v("git version 2.38").patch, 0);
        assert!(parse_git_version("hello").is_none());
    }

    #[test]
    fn selection_by_version() {
        let p = PathBuf::from("git");
        let ver = |major, minor| GitVersion {
            major,
            minor,
            patch: 0,
        };
        let found = GitDetection::from_version(p.clone(), ver(2, 38));
        assert_eq!(backend_kind_for(&found), ActiveBackend::Hybrid);
        let old = GitDetection::from_version(p, ver(2, 37));
        assert!(matches!(old, GitDetection::TooOld { .. }));
        assert_eq!(backend_kind_for(&old), ActiveBackend::GixOnly);
        assert_eq!(
            backend_kind_for(&GitDetection::Missing),
            ActiveBackend::GixOnly
        );
    }

    #[test]
    fn missing_configured_binary_is_not_fatal() {
        let d = detect_git(Some(Path::new("/nonexistent/definitely-not-git")));
        // Falls back to PATH; either found or missing depending on the host, never a panic.
        assert!(matches!(
            d,
            GitDetection::Found { .. } | GitDetection::TooOld { .. } | GitDetection::Missing
        ));
    }
}
