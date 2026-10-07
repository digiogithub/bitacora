//! The per-graph instance directory of a managed Pando: layout, atomic file writes, `state.json`
//! and the redacted, rotated log (BIT-US-0141, design `pando-integration.md` section 6).
//!
//! Everything lives under `<cache>/pando/<key>/` (mode `0700` on unix); nothing is ever written
//! inside the graph folder.

use std::collections::VecDeque;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

/// Generated Pando configuration (rewritten on every child start).
pub const CONFIG_FILE: &str = ".pando.toml";
/// The instance's API token (written once the child answers; `0600`).
pub const TOKEN_FILE: &str = "token";
/// [`ManagedStatus`] as JSON, rewritten atomically on every change.
pub const STATE_FILE: &str = "state.json";
/// Lock held while a supervisor owns the directory.
pub const LOCK_FILE: &str = "supervisor.lock";
/// Child stdout and stderr, tokens redacted.
pub const LOG_FILE: &str = "pando.log";

/// Rotate the log at this size.
pub const LOG_MAX_BYTES: u64 = 10 * 1024 * 1024;
/// Rotated log files kept (`pando.log.1`, `pando.log.2`).
pub const LOG_KEEP: usize = 2;
/// Lines of recent output kept for the status.
pub const TAIL_LINES: usize = 20;

/// Directory key of a graph: its folder name (sanitised) plus a short hash of the full path, so
/// two graphs with the same name cannot collide.
#[must_use]
pub fn instance_key(graph: &Path) -> String {
    let name: String = graph
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .take(32)
        .collect();
    let name = if name.trim_matches('-').is_empty() {
        "graph".to_owned()
    } else {
        name
    };
    let hash = blake3::hash(graph.to_string_lossy().as_bytes()).to_hex();
    format!("{name}-{}", &hash.as_str()[..8])
}

/// Where the files of one instance live.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstanceDir {
    dir: PathBuf,
}

impl InstanceDir {
    /// `<cache_root>/<instance key of graph>`.
    #[must_use]
    pub fn new(cache_root: &Path, graph: &Path) -> Self {
        Self {
            dir: cache_root.join(instance_key(graph)),
        }
    }

    /// The directory itself (the child's working directory).
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.dir
    }

    /// `.pando.toml`.
    #[must_use]
    pub fn config_path(&self) -> PathBuf {
        self.dir.join(CONFIG_FILE)
    }

    /// `token`.
    #[must_use]
    pub fn token_path(&self) -> PathBuf {
        self.dir.join(TOKEN_FILE)
    }

    /// `state.json`.
    #[must_use]
    pub fn state_path(&self) -> PathBuf {
        self.dir.join(STATE_FILE)
    }

    /// `supervisor.lock`.
    #[must_use]
    pub fn lock_path(&self) -> PathBuf {
        self.dir.join(LOCK_FILE)
    }

    /// `pando.log`.
    #[must_use]
    pub fn log_path(&self) -> PathBuf {
        self.dir.join(LOG_FILE)
    }

    /// Creates the directory with mode `0700`.
    ///
    /// # Errors
    /// I/O failure.
    pub fn ensure(&self) -> io::Result<()> {
        fs::create_dir_all(&self.dir)?;
        set_mode(&self.dir, 0o700)
    }
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
}

#[cfg(not(unix))]
fn set_mode(_path: &Path, _mode: u32) -> io::Result<()> {
    Ok(())
}

/// Writes `bytes` to `path` atomically: temp file in the same directory, fsync, rename. Files are
/// created `0600` (they may carry a credential).
///
/// # Errors
/// I/O failure (the destination is left untouched).
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let dir = path
        .parent()
        .ok_or_else(|| io::Error::other("path has no parent"))?;
    let tmp = dir.join(format!(
        ".{}.tmp{}",
        path.file_name()
            .map(|n| n.to_string_lossy())
            .unwrap_or_default(),
        std::process::id()
    ));
    let result = (|| {
        let mut f = File::create(&tmp)?;
        set_mode(&tmp, 0o600)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

/// Lifecycle state of a managed instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManagedState {
    /// Nothing runs.
    #[default]
    Stopped,
    /// The child was started and is not ready yet.
    Starting,
    /// The child answers health checks.
    Ready,
    /// The child ended (crash or requested restart) and a new one is coming.
    Restarting,
    /// Too many crashes in the window; retried only on an explicit restart.
    Failed,
}

/// What the settings page shows and what other processes read from `state.json`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ManagedStatus {
    /// Lifecycle state.
    pub state: ManagedState,
    /// Child pid while one runs.
    pub pid: Option<u32>,
    /// Pid of the Bitacora process that supervises the instance.
    pub supervisor_pid: Option<u32>,
    /// REST base URL of the running child.
    pub rest_url: Option<String>,
    /// AG-UI base URL of the running child.
    pub agui_url: Option<String>,
    /// REST port of the last child, tried first on the next start.
    pub last_rest_port: Option<u16>,
    /// AG-UI port of the last child.
    pub last_agui_port: Option<u16>,
    /// Crashes inside the crash window.
    pub crashes: u32,
    /// Why the last run ended or could not start.
    pub last_error: Option<String>,
    /// Last lines of the child's output (tokens redacted).
    pub last_output: Vec<String>,
    /// `pando --version` line.
    pub version: Option<String>,
    /// Path of the binary in use.
    pub binary: Option<String>,
    /// Unix seconds of the last state change.
    pub since: i64,
}

/// Reads `state.json` of an instance directory.
///
/// # Errors
/// Missing or unreadable file.
pub fn read_status(dir: &InstanceDir) -> io::Result<ManagedStatus> {
    let bytes = fs::read(dir.state_path())?;
    serde_json::from_slice(&bytes).map_err(io::Error::other)
}

/// Writes `state.json` atomically.
///
/// # Errors
/// I/O failure.
pub fn write_status(dir: &InstanceDir, status: &ManagedStatus) -> io::Result<()> {
    let bytes = serde_json::to_vec_pretty(status).map_err(io::Error::other)?;
    write_atomic(&dir.state_path(), &bytes)
}

/// Replacement text for a secret in logs.
pub const REDACTED: &str = "[redacted]";

/// Line-oriented log sink: redacts registered secrets, rotates at [`LOG_MAX_BYTES`] and keeps the
/// last [`TAIL_LINES`] lines for the status. Clones share one file.
#[derive(Clone)]
pub struct LogSink {
    inner: Arc<Mutex<SinkInner>>,
}

impl std::fmt::Debug for LogSink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LogSink").finish_non_exhaustive()
    }
}

struct SinkInner {
    path: PathBuf,
    file: File,
    size: u64,
    max_bytes: u64,
    secrets: Vec<String>,
    tail: VecDeque<String>,
}

impl LogSink {
    /// Opens (appending) the log of `dir`.
    ///
    /// # Errors
    /// I/O failure.
    pub fn open(dir: &InstanceDir, secrets: Vec<String>) -> io::Result<Self> {
        Self::open_with(dir.log_path(), secrets, LOG_MAX_BYTES)
    }

    /// Like [`open`](Self::open) with an explicit rotation size (tests).
    ///
    /// # Errors
    /// I/O failure.
    pub fn open_with(path: PathBuf, secrets: Vec<String>, max_bytes: u64) -> io::Result<Self> {
        let file = open_log(&path)?;
        let size = file.metadata().map(|m| m.len()).unwrap_or(0);
        Ok(Self {
            inner: Arc::new(Mutex::new(SinkInner {
                path,
                file,
                size,
                max_bytes,
                secrets: secrets.into_iter().filter(|s| s.len() >= 8).collect(),
                tail: VecDeque::new(),
            })),
        })
    }

    /// Registers one more secret to redact (for example the API token once it is known).
    pub fn add_secret(&self, secret: &str) {
        if secret.len() >= 8 {
            self.inner.lock().secrets.push(secret.to_owned());
        }
    }

    /// Appends one line (without the trailing newline).
    pub fn line(&self, line: &str) {
        let mut g = self.inner.lock();
        let mut text = line.to_owned();
        for s in &g.secrets {
            text = text.replace(s.as_str(), REDACTED);
        }
        g.tail.push_back(text.clone());
        while g.tail.len() > TAIL_LINES {
            g.tail.pop_front();
        }
        if g.size >= g.max_bytes {
            g.rotate();
        }
        let n = text.len() as u64 + 1;
        if writeln!(g.file, "{text}").is_ok() {
            g.size += n;
        }
    }

    /// The last lines written.
    #[must_use]
    pub fn tail(&self) -> Vec<String> {
        self.inner.lock().tail.iter().cloned().collect()
    }
}

fn open_log(path: &Path) -> io::Result<File> {
    let f = OpenOptions::new().create(true).append(true).open(path)?;
    set_mode(path, 0o600)?;
    Ok(f)
}

impl SinkInner {
    fn rotate(&mut self) {
        let numbered = |i: usize| PathBuf::from(format!("{}.{i}", self.path.display()));
        let _ = fs::remove_file(numbered(LOG_KEEP));
        for i in (1..LOG_KEEP).rev() {
            let _ = fs::rename(numbered(i), numbered(i + 1));
        }
        let _ = fs::rename(&self.path, numbered(1));
        if let Ok(f) = open_log(&self.path) {
            self.file = f;
            self.size = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;

    #[test]
    fn key_is_stable_sanitised_and_path_dependent() {
        let a = instance_key(Path::new("/home/me/My Notes"));
        assert!(a.starts_with("My-Notes-"), "{a}");
        assert_eq!(a, instance_key(Path::new("/home/me/My Notes")));
        assert_ne!(a, instance_key(Path::new("/other/My Notes")));
        assert!(instance_key(Path::new("/")).starts_with("graph-"));
    }

    #[test]
    fn atomic_write_replaces_and_is_private() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = InstanceDir::new(tmp.path(), Path::new("/g"));
        dir.ensure().unwrap();
        write_atomic(&dir.token_path(), b"one").unwrap();
        write_atomic(&dir.token_path(), b"two").unwrap();
        assert_eq!(fs::read(dir.token_path()).unwrap(), b"two");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let m = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
            assert_eq!(m(dir.path()), 0o700);
            assert_eq!(m(&dir.token_path()), 0o600);
        }
        let leftovers = fs::read_dir(dir.path())
            .unwrap()
            .filter(|e| {
                e.as_ref()
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .contains(".tmp")
            })
            .count();
        assert_eq!(leftovers, 0);
    }

    #[test]
    fn status_roundtrips() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = InstanceDir::new(tmp.path(), Path::new("/g"));
        dir.ensure().unwrap();
        let st = ManagedStatus {
            state: ManagedState::Ready,
            pid: Some(7),
            rest_url: Some("https://127.0.0.1:1".into()),
            ..ManagedStatus::default()
        };
        write_status(&dir, &st).unwrap();
        assert_eq!(read_status(&dir).unwrap(), st);
    }

    #[test]
    fn log_redacts_rotates_and_keeps_a_tail() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("pando.log");
        let sink = LogSink::open_with(path.clone(), vec!["supersecret-token".into()], 200).unwrap();
        sink.line("auth supersecret-token ok");
        sink.add_secret("late-secret-value");
        sink.line("late-secret-value leaked?");
        let text = fs::read_to_string(&path).unwrap();
        assert!(!text.contains("supersecret-token") && !text.contains("late-secret-value"));
        assert!(text.contains(REDACTED));
        for i in 0..40 {
            sink.line(&format!("filler line number {i} to force rotation"));
        }
        assert!(tmp.path().join("pando.log.1").exists());
        assert!(!tmp.path().join("pando.log.3").exists());
        let tail = sink.tail();
        assert_eq!(tail.len(), TAIL_LINES);
        assert!(tail.last().unwrap().contains("39"));
    }
}
