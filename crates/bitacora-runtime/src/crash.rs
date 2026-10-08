//! Local crash reports with opt-in sharing (BIT-US-0111).
//!
//! A panic hook writes `<data_dir>/crashes/<timestamp>-<pid>.json` with the version, OS, thread,
//! message, location, backtrace and the tail of the newest log file. Nothing is uploaded: the
//! next start lists [`pending`] reports and lets the user view, copy, open a prefilled GitHub
//! issue (they review it in the browser and press Submit) or dismiss them.
//!
//! Privacy: graph folder paths are replaced by `<graph:NAME>` and the home directory by `~`
//! ([`Redactor`]); note content is never read by this module. Panic messages and log lines are
//! free text produced by code, so they are truncated, and the report is meant to be reviewed by
//! the user before sharing.
//!
//! Native crashes (GPU driver segfaults) cannot run the hook; they are covered by
//! [`abnormal_exit_report`], called when the instance lock finds the previous owner's sidecar
//! left behind (see [`crate::instance`]).

use std::backtrace::Backtrace;
use std::panic::PanicHookInfo;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

/// Number of log lines kept in a report.
pub const LOG_TAIL_LINES: usize = 80;
/// Longest panic message kept.
const MAX_MESSAGE: usize = 600;
/// Longest issue-URL body (browsers and GitHub reject very long URLs).
const MAX_ISSUE_BODY: usize = 5000;
/// Subfolder of dismissed reports.
const DISMISSED_DIR: &str = "dismissed";

/// Why a report exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CrashKind {
    /// A Rust panic.
    Panic,
    /// The previous session ended without a clean shutdown (native crash, kill, power loss).
    AbnormalExit,
}

/// One crash report as stored on disk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrashReport {
    /// Panic or abnormal exit.
    pub kind: CrashKind,
    /// Binary that crashed (`bitacora` / `bitacora-cli`).
    pub binary: String,
    /// Crate version.
    pub version: String,
    /// `std::env::consts::OS`.
    pub os: String,
    /// `std::env::consts::ARCH`.
    pub arch: String,
    /// RFC 3339 timestamp.
    pub timestamp: String,
    /// Process id of the crashed process.
    pub pid: u32,
    /// Thread name (panics).
    pub thread: Option<String>,
    /// Panic message (redacted, truncated).
    pub message: Option<String>,
    /// `file:line:column` of the panic.
    pub location: Option<String>,
    /// Backtrace (redacted).
    pub backtrace: Option<String>,
    /// Last log lines (redacted).
    pub log_tail: Vec<String>,
}

/// Replaces private paths in free text.
#[derive(Debug, Clone, Default)]
pub struct Redactor {
    roots: Vec<(String, String)>,
}

impl Redactor {
    /// A redactor for the given graph folders and (when known) the home directory.
    pub fn new(graph_roots: &[PathBuf], home: Option<&Path>) -> Self {
        let mut roots: Vec<(String, String)> = Vec::new();
        for root in graph_roots {
            let name = root
                .file_name()
                .map_or_else(|| "graph".to_owned(), |n| n.to_string_lossy().into_owned());
            let label = format!("<graph:{name}>");
            roots.push((root.to_string_lossy().into_owned(), label.clone()));
            // Log lines may carry the canonical spelling (macOS `/private/var`, Windows long
            // names) of a root that was configured through a symlink or short name.
            if let Ok(canon) = root.canonicalize() {
                roots.push((canon.to_string_lossy().into_owned(), label));
            }
        }
        // Longest first so a graph inside the home folder is matched before the home folder.
        if let Some(home) = home {
            roots.push((home.to_string_lossy().into_owned(), "~".to_owned()));
            if let Ok(canon) = home.canonicalize() {
                roots.push((canon.to_string_lossy().into_owned(), "~".to_owned()));
            }
        }
        roots.retain(|(from, _)| from.len() > 1);
        roots.sort_by_key(|(from, _)| std::cmp::Reverse(from.len()));
        Self { roots }
    }

    /// Applies every replacement.
    pub fn apply(&self, text: &str) -> String {
        let mut out = text.to_owned();
        for (from, to) in &self.roots {
            out = out.replace(from.as_str(), to);
        }
        out
    }
}

type RootsSource = Arc<dyn Fn() -> Vec<PathBuf> + Send + Sync>;

/// Graph folders to redact: a fixed set the app updates plus an optional live source (the app
/// passes its recent-graphs list, so graphs opened later are covered too).
#[derive(Clone, Default)]
pub struct GraphRoots {
    fixed: Arc<Mutex<Vec<PathBuf>>>,
    source: Option<RootsSource>,
}

impl std::fmt::Debug for GraphRoots {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GraphRoots").finish_non_exhaustive()
    }
}

impl GraphRoots {
    /// Adds a live source consulted whenever a report is built.
    #[must_use]
    pub fn with_source(
        mut self,
        source: impl Fn() -> Vec<PathBuf> + Send + Sync + 'static,
    ) -> Self {
        self.source = Some(Arc::new(source));
        self
    }

    /// Replaces the fixed set.
    pub fn set(&self, roots: Vec<PathBuf>) {
        if let Ok(mut guard) = self.fixed.lock() {
            *guard = roots;
        }
    }

    fn snapshot(&self) -> Vec<PathBuf> {
        let mut roots = self.fixed.lock().map(|g| g.clone()).unwrap_or_default();
        if let Some(source) = &self.source {
            roots.extend(source());
        }
        roots
    }
}

/// Where and for whom reports are written.
#[derive(Debug, Clone)]
pub struct CrashConfig {
    /// `<data_dir>/crashes`.
    pub dir: PathBuf,
    /// Folder of the rolling log files (`bitacora*.log`), if any.
    pub log_dir: Option<PathBuf>,
    /// `bitacora` or `bitacora-cli`.
    pub binary: String,
    /// Open graph folders, for redaction.
    pub graphs: GraphRoots,
}

fn home_dir() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|d| d.home_dir().to_path_buf())
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let mut cut: String = text.chars().take(max).collect();
    cut.push_str("... [truncated]");
    cut
}

/// The last `max` lines of the newest `*.log` file in `log_dir`.
pub fn read_log_tail(log_dir: &Path, max: usize) -> Vec<String> {
    let newest = std::fs::read_dir(log_dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "log"))
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .max();
    let Some((_, path)) = newest else {
        return Vec::new();
    };
    let Ok(bytes) = std::fs::read(path) else {
        return Vec::new();
    };
    let text = String::from_utf8_lossy(&bytes);
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(max)..]
        .iter()
        .map(|l| truncate(l, 400))
        .collect()
}

fn base_report(cfg: &CrashConfig, kind: CrashKind, pid: u32) -> CrashReport {
    let redactor = Redactor::new(&cfg.graphs.snapshot(), home_dir().as_deref());
    CrashReport {
        kind,
        binary: cfg.binary.clone(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        os: std::env::consts::OS.to_owned(),
        arch: std::env::consts::ARCH.to_owned(),
        timestamp: jiff::Timestamp::now().to_string(),
        pid,
        thread: None,
        message: None,
        location: None,
        backtrace: None,
        log_tail: cfg
            .log_dir
            .as_deref()
            .map(|d| read_log_tail(d, LOG_TAIL_LINES))
            .unwrap_or_default()
            .iter()
            .map(|l| redactor.apply(l))
            .collect(),
    }
}

/// Writes `report` into `dir` and returns the file path.
pub fn write_report(dir: &Path, report: &CrashReport) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let stamp: String = report
        .timestamp
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect();
    let path = dir.join(format!("{stamp}-{}.json", report.pid));
    let json = serde_json::to_vec_pretty(report).map_err(std::io::Error::other)?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, &path)?;
    Ok(path)
}

fn panic_report(cfg: &CrashConfig, info: &PanicHookInfo<'_>) -> CrashReport {
    let redactor = Redactor::new(&cfg.graphs.snapshot(), home_dir().as_deref());
    let mut report = base_report(cfg, CrashKind::Panic, std::process::id());
    let payload = info.payload();
    let message = payload
        .downcast_ref::<&str>()
        .map(|s| (*s).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "panic with a non-string payload".to_owned());
    report.message = Some(truncate(&redactor.apply(&message), MAX_MESSAGE));
    report.location = info
        .location()
        .map(|l| redactor.apply(&format!("{}:{}:{}", l.file(), l.line(), l.column())));
    report.thread = Some(
        std::thread::current()
            .name()
            .unwrap_or("<unnamed>")
            .to_owned(),
    );
    report.backtrace = Some(redactor.apply(&Backtrace::force_capture().to_string()));
    report
}

/// Installs the panic hook. The previous hook still runs afterwards (stderr message, abort
/// policy). Report-writing failures are swallowed: a crash handler must not panic.
pub fn install_panic_hook(cfg: CrashConfig) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let report = panic_report(&cfg, info);
        if let Ok(path) = write_report(&cfg.dir, &report) {
            eprintln!("{}: crash report saved to {}", cfg.binary, path.display());
        }
        previous(info);
    }));
}

/// A report for a previous session that ended without a clean shutdown, unless a panic report
/// of the same pid already exists in `cfg.dir`.
pub fn abnormal_exit_report(cfg: &CrashConfig, previous_pid: u32) -> Option<PathBuf> {
    let already = pending(&cfg.dir)
        .into_iter()
        .any(|(_, r)| r.pid == previous_pid);
    if already {
        return None;
    }
    let report = base_report(cfg, CrashKind::AbnormalExit, previous_pid);
    write_report(&cfg.dir, &report).ok()
}

/// Reports not yet reviewed, oldest first.
pub fn pending(dir: &Path) -> Vec<(PathBuf, CrashReport)> {
    let mut found: Vec<(PathBuf, CrashReport)> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| {
            let report = serde_json::from_slice(&std::fs::read(e.path()).ok()?).ok()?;
            Some((e.path(), report))
        })
        .collect();
    found.sort_by(|a, b| a.0.cmp(&b.0));
    found
}

/// Moves a report out of the pending list (kept on disk under `dismissed/`).
pub fn dismiss(path: &Path) -> std::io::Result<()> {
    let dir = path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(DISMISSED_DIR);
    std::fs::create_dir_all(&dir)?;
    let name = path.file_name().unwrap_or_default();
    std::fs::rename(path, dir.join(name))
}

impl CrashReport {
    /// Human-readable text, also used as the clipboard content.
    pub fn to_text(&self) -> String {
        let mut out = format!(
            "{} {} on {} {} ({:?}, pid {}) at {}\n",
            self.binary, self.version, self.os, self.arch, self.kind, self.pid, self.timestamp
        );
        if let Some(thread) = &self.thread {
            out.push_str(&format!("thread: {thread}\n"));
        }
        if let Some(message) = &self.message {
            out.push_str(&format!("message: {message}\n"));
        }
        if let Some(location) = &self.location {
            out.push_str(&format!("location: {location}\n"));
        }
        if let Some(backtrace) = &self.backtrace {
            out.push_str(&format!("\nbacktrace:\n{backtrace}\n"));
        }
        if !self.log_tail.is_empty() {
            out.push_str("\nlast log lines:\n");
            out.push_str(&self.log_tail.join("\n"));
            out.push('\n');
        }
        out
    }

    /// Prefilled `issues/new` URL on `repository`. The body carries a trimmed report; the user
    /// reads and edits it in the browser and decides whether to submit.
    pub fn issue_url(&self, repository: &str) -> String {
        let title = match (&self.kind, &self.message) {
            (CrashKind::Panic, Some(m)) => {
                format!("Crash: {}", truncate(m.lines().next().unwrap_or(""), 80))
            }
            _ => "Crash: previous session ended abnormally".to_owned(),
        };
        let mut body = format!(
            "**Version:** {} ({} {})\n**Kind:** {:?}\n",
            self.version, self.os, self.arch, self.kind
        );
        if let Some(m) = &self.message {
            body.push_str(&format!("**Message:** {m}\n"));
        }
        if let Some(l) = &self.location {
            body.push_str(&format!("**Location:** {l}\n"));
        }
        body.push_str("\n<details><summary>Backtrace and log tail</summary>\n\n```\n");
        let detail = format!(
            "{}\n{}",
            self.backtrace.as_deref().unwrap_or(""),
            self.log_tail.join("\n")
        );
        body.push_str(&truncate(
            &detail,
            MAX_ISSUE_BODY.saturating_sub(body.len()),
        ));
        body.push_str("\n```\n</details>\n\nWhat were you doing when it crashed?\n");
        format!(
            "{}/issues/new?title={}&body={}&labels=crash",
            repository.trim_end_matches('/'),
            percent_encode(&title),
            percent_encode(&body)
        )
    }
}

fn percent_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 2);
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(dir: &Path, log_dir: Option<PathBuf>) -> CrashConfig {
        CrashConfig {
            dir: dir.join("crashes"),
            log_dir,
            binary: "bitacora-test".into(),
            graphs: GraphRoots::default(),
        }
    }

    #[test]
    fn redactor_hides_graph_and_home_paths() {
        let r = Redactor::new(
            &[PathBuf::from("/home/ann/notes/work")],
            Some(Path::new("/home/ann")),
        );
        assert_eq!(
            r.apply("open /home/ann/notes/work/pages/x.md from /home/ann/.cache"),
            "open <graph:work>/pages/x.md from ~/.cache"
        );
    }

    #[cfg(unix)]
    #[test]
    fn redactor_also_hides_the_canonical_spelling_of_a_symlinked_root() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let real = tmp.path().join("real").join("diary");
        std::fs::create_dir_all(&real).expect("real");
        let link = tmp.path().join("link");
        std::os::unix::fs::symlink(tmp.path().join("real"), &link).expect("symlink");
        let canon = real.canonicalize().expect("canon");
        let r = Redactor::new(&[link.join("diary")], None);
        let out = r.apply(&format!("open {}/pages/x.md", canon.display()));
        assert_eq!(out, "open <graph:diary>/pages/x.md");
    }

    #[test]
    fn log_tail_takes_last_lines_of_newest_log() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let body: String = (0..200).map(|i| format!("line {i}\n")).collect();
        std::fs::write(tmp.path().join("bitacora.2026-01-01.log"), body).expect("write");
        let tail = read_log_tail(tmp.path(), 5);
        assert_eq!(tail.len(), 5);
        assert_eq!(tail[4], "line 199");
        assert!(read_log_tail(&tmp.path().join("missing"), 5).is_empty());
    }

    #[test]
    fn panic_hook_writes_a_redacted_report() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let logs = tmp.path().join("logs");
        std::fs::create_dir_all(&logs).expect("logs");
        std::fs::write(
            logs.join("bitacora.log"),
            "opened /secret/graphs/diary ok\n",
        )
        .expect("log");
        let config = cfg(tmp.path(), Some(logs));
        config
            .graphs
            .set(vec![PathBuf::from("/secret/graphs/diary")]);
        // Installing a global panic hook would race other tests, so the report is built directly.
        let mut report = base_report(&config, CrashKind::Panic, 4242);
        report.message = Some("boom".into());
        let path = write_report(&config.dir, &report).expect("write");
        let (_, loaded) = pending(&config.dir).into_iter().next().expect("pending");
        assert_eq!(loaded, report);
        assert_eq!(loaded.log_tail, vec!["opened <graph:diary> ok".to_owned()]);
        assert!(path.starts_with(&config.dir));
        dismiss(&path).expect("dismiss");
        assert!(pending(&config.dir).is_empty());
        assert!(
            config
                .dir
                .join(DISMISSED_DIR)
                .read_dir()
                .expect("dir")
                .count()
                == 1
        );
    }

    #[test]
    fn abnormal_exit_is_deduplicated_against_panic_reports() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let config = cfg(tmp.path(), None);
        let panic = base_report(&config, CrashKind::Panic, 77);
        write_report(&config.dir, &panic).expect("write");
        assert!(abnormal_exit_report(&config, 77).is_none());
        let path = abnormal_exit_report(&config, 78).expect("new report");
        assert!(path.exists());
        assert_eq!(pending(&config.dir).len(), 2);
    }

    #[test]
    fn issue_url_is_encoded_and_bounded() {
        let mut report = base_report(&cfg(Path::new("/x"), None), CrashKind::Panic, 1);
        report.message = Some("index out of bounds & more".into());
        report.backtrace = Some("frame\n".repeat(5000));
        let url = report.issue_url("https://github.com/digiogithub/bitacora/");
        assert!(
            url.starts_with("https://github.com/digiogithub/bitacora/issues/new?title=Crash%3A")
        );
        assert!(!url.contains(' ') && !url.contains('\n'));
        assert!(url.len() < 20_000, "url too long: {}", url.len());
    }
}
