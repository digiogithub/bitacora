//! Repository preparation: `.gitignore` merge, `.git/info/attributes` and repo-local identity
//! (design `git-sync-merge` 5.2, ADR-008). Nothing here touches global git config.

use std::io::Write;
use std::path::{Path, PathBuf};

/// Header line that introduces the entries Bitacora manages in `.gitignore`.
pub const GITIGNORE_HEADER: &str = "# Bitacora / Logseq volatile files";

/// Default `.gitignore` entries (design 5.2).
pub const GITIGNORE_ENTRIES: &[&str] = &[
    ".DS_Store",
    "Thumbs.db",
    "logseq/bak/",
    "logseq/.recycle/",
    "logseq/version-files/",
    "logseq/graphs-txid.edn",
    "logseq/pages-metadata.edn",
    "logseq/.bitacora/",
    ".trash/",
    "*~",
];

/// Lines Bitacora keeps in `.git/info/attributes` (never committed).
pub const ATTRIBUTES_LINES: &[&str] = &[
    "*.md merge=binary",
    "logseq/config.edn merge=binary",
    "* -text",
];

/// Errors from repository preparation.
#[derive(Debug, thiserror::Error)]
pub enum SetupError {
    /// Filesystem failure.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// Reading or writing the repo-local git config failed.
    #[error("git config error: {0}")]
    Config(String),
}

/// Repo-local commit identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    /// `user.name`.
    pub name: String,
    /// `user.email`.
    pub email: String,
}

impl Identity {
    /// The documented fallback: `Bitacora <device@hostname>`.
    pub fn fallback() -> Self {
        Self {
            name: "Bitacora".to_string(),
            email: format!("device@{}", hostname()),
        }
    }
}

/// Best-effort host name without extra dependencies.
pub fn hostname() -> String {
    for var in ["HOSTNAME", "COMPUTERNAME", "HOST"] {
        if let Ok(v) = std::env::var(var) {
            let v = v.trim().to_string();
            if !v.is_empty() {
                return sanitize_host(&v);
            }
        }
    }
    if let Ok(v) = std::fs::read_to_string("/etc/hostname") {
        let v = v.trim();
        if !v.is_empty() {
            return sanitize_host(v);
        }
    }
    if let Ok(out) = std::process::Command::new("hostname").output()
        && out.status.success()
    {
        let v = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !v.is_empty() {
            return sanitize_host(&v);
        }
    }
    "localhost".to_string()
}

fn sanitize_host(host: &str) -> String {
    host.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '.' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

/// Atomic replace: temp file in the same directory, fsync, rename.
pub(crate) fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(dir)?;
    let mut tmp = tempfile::NamedTempFile::new_in(dir)?;
    tmp.write_all(bytes)?;
    tmp.as_file().sync_all()?;
    tmp.persist(path).map_err(|e| e.error)?;
    Ok(())
}

/// Appends the lines of `wanted` missing from `existing`, preserving every existing byte.
///
/// `header` is added once before the first appended line when it is not already present. The
/// file's dominant line ending (CRLF vs LF) is kept. Returns `None` when nothing changes.
fn merge_lines(existing: &str, header: Option<&str>, wanted: &[&str]) -> Option<String> {
    let crlf = existing.contains("\r\n");
    let eol = if crlf { "\r\n" } else { "\n" };
    let present: std::collections::HashSet<&str> = existing.lines().map(str::trim).collect();
    let missing: Vec<&str> = wanted
        .iter()
        .copied()
        .filter(|w| !present.contains(w))
        .collect();
    if missing.is_empty() {
        return None;
    }
    let mut out = existing.to_string();
    if !out.is_empty() && !out.ends_with('\n') {
        out.push_str(eol);
    }
    if let Some(h) = header
        && !present.contains(h)
    {
        out.push_str(h);
        out.push_str(eol);
    }
    for line in missing {
        out.push_str(line);
        out.push_str(eol);
    }
    Some(out)
}

/// Writes or merges the default `.gitignore` in `graph`. Idempotent; user lines and their order
/// are preserved. Returns whether the file changed.
pub fn ensure_gitignore(graph: &Path) -> Result<bool, SetupError> {
    let path = graph.join(".gitignore");
    let existing = match std::fs::read(&path) {
        Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e.into()),
    };
    match merge_lines(&existing, Some(GITIGNORE_HEADER), GITIGNORE_ENTRIES) {
        Some(merged) => {
            atomic_write(&path, merged.as_bytes())?;
            Ok(true)
        }
        None => Ok(false),
    }
}

/// Resolves the real git directory of `graph` (`<graph>/.git`, following a `gitdir:` pointer file).
pub fn git_dir_of(graph: &Path) -> Option<PathBuf> {
    let dot_git = graph.join(".git");
    if dot_git.is_dir() {
        return Some(dot_git);
    }
    let text = std::fs::read_to_string(&dot_git).ok()?;
    let target = text.trim().strip_prefix("gitdir:")?.trim();
    let p = PathBuf::from(target);
    Some(if p.is_relative() { graph.join(p) } else { p })
}

/// Merges Bitacora's lines into `<git_dir>/info/attributes`. Returns whether the file changed.
pub fn ensure_attributes(git_dir: &Path) -> Result<bool, SetupError> {
    let path = git_dir.join("info").join("attributes");
    let existing = match std::fs::read(&path) {
        Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e.into()),
    };
    match merge_lines(&existing, None, ATTRIBUTES_LINES) {
        Some(merged) => {
            atomic_write(&path, merged.as_bytes())?;
            Ok(true)
        }
        None => Ok(false),
    }
}

pub(crate) fn cfg_err(e: impl std::fmt::Display) -> SetupError {
    SetupError::Config(e.to_string())
}

/// Sets repo-local `user.name` / `user.email` in `<git_dir>/config` (never global config).
///
/// With `Some(identity)` the values are set. With `None`, values already present locally are kept
/// and missing ones are filled from [`Identity::fallback`]. Returns the identity in effect.
pub fn ensure_identity(
    git_dir: &Path,
    identity: Option<&Identity>,
) -> Result<Identity, SetupError> {
    let path = git_dir.join("config");
    let mut file = if path.exists() {
        gix::config::File::from_path_no_includes(path.clone(), gix::config::Source::Local)
            .map_err(cfg_err)?
    } else {
        gix::config::File::new(gix::config::file::Metadata::from(
            gix::config::Source::Local,
        ))
    };
    let existing_name = file.string("user.name").map(|v| v.to_string());
    let existing_email = file.string("user.email").map(|v| v.to_string());
    let fallback = Identity::fallback();
    let effective = match identity {
        Some(id) => id.clone(),
        None => Identity {
            name: existing_name.clone().unwrap_or(fallback.name),
            email: existing_email.clone().unwrap_or(fallback.email),
        },
    };
    let changed = existing_name.as_deref() != Some(effective.name.as_str())
        || existing_email.as_deref() != Some(effective.email.as_str());
    if changed {
        file.set_raw_value_by("user", None, "name", effective.name.as_str())
            .map_err(cfg_err)?;
        file.set_raw_value_by("user", None, "email", effective.email.as_str())
            .map_err(cfg_err)?;
        let mut buf = Vec::new();
        file.write_to(&mut buf)?;
        atomic_write(&path, &buf)?;
    }
    Ok(effective)
}

/// Loads `<git_dir>/config` as a repo-local config file, applies `f`, and writes it back
/// atomically when `f` returns `true`.
pub(crate) fn update_config(
    git_dir: &Path,
    f: impl FnOnce(&mut gix::config::File) -> Result<bool, SetupError>,
) -> Result<(), SetupError> {
    let path = git_dir.join("config");
    let mut file = if path.exists() {
        gix::config::File::from_path_no_includes(path.clone(), gix::config::Source::Local)
            .map_err(cfg_err)?
    } else {
        gix::config::File::new(gix::config::file::Metadata::from(
            gix::config::Source::Local,
        ))
    };
    if f(&mut file)? {
        let mut buf = Vec::new();
        file.write_to(&mut buf)?;
        atomic_write(&path, &buf)?;
    }
    Ok(())
}

/// Runs every preparation step for the graph at `graph` (which must already be a git repository).
pub fn prepare_repo(graph: &Path, identity: Option<&Identity>) -> Result<Identity, SetupError> {
    let git_dir = git_dir_of(graph)
        .ok_or_else(|| SetupError::Config("graph is not a git repository".into()))?;
    ensure_gitignore(graph)?;
    ensure_attributes(&git_dir)?;
    ensure_identity(&git_dir, identity)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gitignore_created_idempotent() {
        let d = tempfile::tempdir().unwrap();
        assert!(ensure_gitignore(d.path()).unwrap());
        let first = std::fs::read_to_string(d.path().join(".gitignore")).unwrap();
        assert!(first.starts_with(GITIGNORE_HEADER));
        assert!(first.contains("logseq/.bitacora/\n"));
        assert!(!ensure_gitignore(d.path()).unwrap());
        assert_eq!(
            std::fs::read_to_string(d.path().join(".gitignore")).unwrap(),
            first
        );
    }

    #[test]
    fn gitignore_merge_preserves_user_lines_and_order() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join(".gitignore"), "zzz\n.DS_Store\naaa").unwrap();
        assert!(ensure_gitignore(d.path()).unwrap());
        let out = std::fs::read_to_string(d.path().join(".gitignore")).unwrap();
        assert!(out.starts_with("zzz\n.DS_Store\naaa\n# Bitacora"));
        assert_eq!(out.matches(".DS_Store").count(), 1);
        assert!(out.contains("logseq/bak/\n"));
        assert!(!ensure_gitignore(d.path()).unwrap());
    }

    #[test]
    fn gitignore_keeps_crlf() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join(".gitignore"), "a\r\nb\r\n").unwrap();
        ensure_gitignore(d.path()).unwrap();
        let out = std::fs::read_to_string(d.path().join(".gitignore")).unwrap();
        assert!(out.starts_with("a\r\nb\r\n# Bitacora"));
        assert!(!out.replace("\r\n", "").contains('\n'));
    }

    #[test]
    fn attributes_written_and_merged() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(d.path().join("info")).unwrap();
        std::fs::write(
            d.path().join("info/attributes"),
            "*.png binary\n*.md merge=binary",
        )
        .unwrap();
        assert!(ensure_attributes(d.path()).unwrap());
        let out = std::fs::read_to_string(d.path().join("info/attributes")).unwrap();
        assert!(out.starts_with("*.png binary\n*.md merge=binary\n"));
        for line in ATTRIBUTES_LINES {
            assert_eq!(out.lines().filter(|l| l == line).count(), 1, "{line}");
        }
        assert!(!ensure_attributes(d.path()).unwrap());
    }

    #[test]
    fn identity_set_kept_and_fallback() {
        let d = tempfile::tempdir().unwrap();
        let fb = ensure_identity(d.path(), None).unwrap();
        assert_eq!(fb.name, "Bitacora");
        assert!(fb.email.starts_with("device@"));
        let custom = Identity {
            name: "Jo".into(),
            email: "jo@example.com".into(),
        };
        ensure_identity(d.path(), Some(&custom)).unwrap();
        // None keeps what is there.
        assert_eq!(ensure_identity(d.path(), None).unwrap(), custom);
        let text = std::fs::read_to_string(d.path().join("config")).unwrap();
        assert!(text.contains("jo@example.com"));
    }
}
