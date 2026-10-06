//! Shared harness: two devices syncing through a temp bare repository, against both backends.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bitacora_sync::backend::{CliConfig, select_backend};
use bitacora_sync::engine::{EngineConfig, SyncEngine, Timing};
use bitacora_sync::onboarding::{OnboardingConfig, clone_graph, enable_sync};
use bitacora_sync::repo_setup::Identity;
use bitacora_sync::state::MemoryMergeStore;
use bitacora_sync::writer::testing::DirGraphWriter;
use bitacora_sync::{GitDetection, detect_git};
use bitacora_testkit::git::{git, git_available, init_bare};
use tempfile::TempDir;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Hybrid,
    GixOnly,
}

pub const KINDS: [Kind; 2] = [Kind::Hybrid, Kind::GixOnly];

pub fn onboarding_config(kind: Kind, name: &str) -> OnboardingConfig {
    assert!(git_available(), "tests need a system git");
    let detection = match kind {
        Kind::Hybrid => detect_git(None),
        Kind::GixOnly => GitDetection::Missing,
    };
    assert!(
        kind == Kind::GixOnly || matches!(detection, GitDetection::Found { .. }),
        "git >= 2.38 required"
    );
    let mut c = OnboardingConfig::new(detection);
    c.identity = Some(Identity {
        name: name.to_string(),
        email: format!("{name}@example.com"),
    });
    c
}

/// Deterministic clock: sleeping advances time instead of blocking.
#[derive(Debug)]
pub struct ManualTiming {
    base: Instant,
    offset: Mutex<Duration>,
    pub sleeps: Mutex<Vec<Duration>>,
}

impl ManualTiming {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            base: Instant::now(),
            offset: Mutex::new(Duration::ZERO),
            sleeps: Mutex::new(Vec::new()),
        })
    }
    pub fn advance(&self, d: Duration) {
        *self.offset.lock().unwrap() += d;
    }
    pub fn recorded_sleeps(&self) -> Vec<Duration> {
        self.sleeps.lock().unwrap().clone()
    }
}

impl Timing for ManualTiming {
    fn now(&self) -> Instant {
        self.base + *self.offset.lock().unwrap()
    }
    fn unix_now(&self) -> i64 {
        let real = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        real + self.offset.lock().unwrap().as_secs() as i64
    }
    fn sleep(&self, d: Duration) {
        self.sleeps.lock().unwrap().push(d);
        self.advance(d);
    }
    fn jitter(&self, lo: Duration, hi: Duration) -> Duration {
        lo + (hi - lo) / 2
    }
}

pub struct Dev {
    pub dir: PathBuf,
    pub writer: Arc<DirGraphWriter>,
    pub timing: Arc<ManualTiming>,
    pub engine: SyncEngine,
}

impl Dev {
    pub fn read(&self, rel: &str) -> String {
        std::fs::read_to_string(self.dir.join(rel)).unwrap()
    }
    pub fn write(&self, rel: &str, content: &str) {
        let p = self.dir.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, content).unwrap();
    }
    pub fn git(&self, args: &[&str]) -> String {
        git(&self.dir, args)
    }
    pub fn head(&self) -> String {
        self.git(&["rev-parse", "HEAD"])
    }
}

pub fn make_dev(
    kind: Kind,
    dir: &Path,
    device: &str,
    mutate: impl FnOnce(&mut EngineConfig),
) -> Dev {
    let cfg = onboarding_config(kind, device);
    let backend = select_backend(&cfg.detection, dir, CliConfig::default()).unwrap();
    let writer = Arc::new(DirGraphWriter::new(dir));
    let timing = ManualTiming::new();
    let mut ec = EngineConfig::new(dir, device, "main");
    mutate(&mut ec);
    let engine = SyncEngine::new(
        backend,
        writer.clone(),
        Box::new(MemoryMergeStore::new()),
        ec,
        timing.clone(),
    );
    Dev {
        dir: dir.to_path_buf(),
        writer,
        timing,
        engine,
    }
}

pub struct World {
    pub tmp: TempDir,
    pub remote: PathBuf,
    pub kind: Kind,
}

impl World {
    pub fn new(kind: Kind) -> Self {
        assert!(git_available(), "tests need a system git");
        let tmp = tempfile::tempdir().unwrap();
        let remote = tmp.path().join("remote.git");
        init_bare(&remote);
        Self { tmp, remote, kind }
    }
    pub fn url(&self) -> String {
        self.remote.to_string_lossy().into_owned()
    }
    pub fn path(&self, name: &str) -> PathBuf {
        self.tmp.path().join(name)
    }
    pub fn remote_head(&self) -> String {
        git(&self.remote, &["rev-parse", "main"])
    }

    /// Device `a`: a fresh graph with `files`, connected with `enable_sync` (pushes the root).
    pub fn first_device(&self, files: &[(&str, &str)]) -> Dev {
        let dir = self.path("a");
        for (p, c) in files {
            let full = dir.join(p);
            std::fs::create_dir_all(full.parent().unwrap()).unwrap();
            std::fs::write(full, c).unwrap();
        }
        let cfg = onboarding_config(self.kind, "alice");
        enable_sync(&dir, &self.url(), "main", &cfg).unwrap();
        make_dev(self.kind, &dir, "alice", |_| {})
    }

    /// Device `b`: a clone of the remote.
    pub fn clone_device(&self) -> Dev {
        let dir = self.path("b");
        let cfg = onboarding_config(self.kind, "bob");
        clone_graph(&self.url(), &dir, &cfg).unwrap();
        make_dev(self.kind, &dir, "bob", |_| {})
    }
}

/// No `<<<<<<<`, `=======` or `>>>>>>>` line anywhere in the work tree (outside `.git`).
pub fn assert_no_markers(dir: &Path) {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        for e in std::fs::read_dir(dir).unwrap() {
            let e = e.unwrap();
            let p = e.path();
            if p.file_name().is_some_and(|n| n == ".git") {
                continue;
            }
            if p.is_dir() {
                walk(&p, out);
            } else {
                out.push(p);
            }
        }
    }
    let mut files = Vec::new();
    walk(dir, &mut files);
    for f in files {
        let Ok(text) = std::fs::read_to_string(&f) else {
            continue;
        };
        for line in text.lines() {
            assert!(
                !(line.starts_with("<<<<<<< ")
                    || line.starts_with(">>>>>>> ")
                    || line == "======="),
                "conflict marker in {}: {line}",
                f.display()
            );
        }
    }
}

/// Every state the engine visited follows the transition table of the design.
pub fn assert_history_legal(engine: &SyncEngine) {
    let mut prev = None;
    for s in engine.history() {
        if let Some(p) = prev {
            assert!(
                bitacora_sync::state::SyncState::can_transition(p, s),
                "illegal transition {p:?} -> {s:?}; history {:?}",
                engine.history()
            );
        }
        prev = Some(s);
    }
}
