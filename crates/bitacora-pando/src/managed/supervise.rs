//! The process supervisor of a managed Pando (BIT-T-0437), modelled on git-in-track's
//! `internal/pando/supervisor` but written for this crate's blocking `Supervisor` seam.
//!
//! One [`Instance`] runs per graph: a supervision thread spawns `pando serve` with the instance
//! directory as working directory and `PANDO_CONFIG_PARENT_SEARCH=false`, waits for readiness,
//! health-checks it, restarts it after a crash with exponential backoff, marks the instance
//! failed after too many crashes and stops it (SIGTERM to the process group, then SIGKILL).
//! `state.json` is rewritten atomically on every change; the child's output goes to a rotated
//! log with the tokens redacted.

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead as _, BufReader};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Condvar, Mutex as StdMutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use pando::{PandoClient, PandoConfig, Token};
use parking_lot::Mutex;

use super::config::{self, ConfigInput};
use super::instance::{
    InstanceDir, LogSink, ManagedState, ManagedStatus, read_status, write_atomic, write_status,
};
use super::sys;
use crate::supervisor::{ManagedEndpoint, McpAccess, Supervisor};

/// Oldest Pando release Bitacora manages (the AG-UI profile schema and `/api/v1/token`).
pub const DEFAULT_MIN_VERSION: &str = "1.2.0";

/// Timings of the supervisor; tests shrink them.
#[derive(Debug, Clone)]
pub struct Timing {
    /// How long a start waits for the previous port to be released before choosing another.
    pub port_wait: Duration,
    /// Bound on the wait for a fresh child to pass its health check.
    pub ready_timeout: Duration,
    /// Pause between health checks of a ready child.
    pub health_interval: Duration,
    /// Consecutive failed checks that count as a crash.
    pub health_failures: u32,
    /// How long SIGTERM gets before SIGKILL.
    pub stop_timeout: Duration,
    /// First restart delay (doubles per crash in the window).
    pub backoff_min: Duration,
    /// Longest restart delay.
    pub backoff_max: Duration,
    /// Crashes inside [`crash_window`](Self::crash_window) that mark the instance failed.
    pub max_crashes: u32,
    /// Window of the crash count.
    pub crash_window: Duration,
    /// Poll step of the supervision loop.
    pub poll: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Self {
            port_wait: Duration::from_secs(2),
            ready_timeout: Duration::from_secs(30),
            health_interval: Duration::from_secs(30),
            health_failures: 3,
            stop_timeout: Duration::from_secs(10),
            backoff_min: Duration::from_secs(1),
            backoff_max: Duration::from_secs(60),
            max_crashes: 5,
            crash_window: Duration::from_secs(600),
            poll: Duration::from_millis(100),
        }
    }
}

/// What a successful health check learned.
#[derive(Debug, Default)]
pub struct ProbeOutcome {
    /// The API token of the server (when it was asked for).
    pub api_token: Option<Token>,
    /// PEM of the CA the server's certificate chains to.
    pub ca_pem: Option<Vec<u8>>,
}

/// Decides whether the child answers. The default [`HttpProber`] speaks to the real server.
pub trait Prober: Send + Sync + std::fmt::Debug {
    /// Checks the child serving REST at `rest_url`. `want_token` asks for the API token too.
    ///
    /// # Errors
    /// Why the child is not (yet) healthy.
    fn probe(
        &self,
        rest_url: &str,
        ca_path: Option<&Path>,
        want_token: bool,
    ) -> Result<ProbeOutcome, String>;
}

/// [`Prober`] over HTTP(S): `GET /health`, then `GET /api/v1/token` (loopback only).
#[derive(Debug, Default)]
pub struct HttpProber {
    runtime: Mutex<Option<tokio::runtime::Runtime>>,
}

impl Prober for HttpProber {
    fn probe(
        &self,
        rest_url: &str,
        ca_path: Option<&Path>,
        want_token: bool,
    ) -> Result<ProbeOutcome, String> {
        let mut cfg = PandoConfig::new(rest_url)
            .with_timeout(Duration::from_secs(5))
            .with_connect_timeout(Duration::from_secs(2));
        let mut ca_pem = None;
        if rest_url.starts_with("https://") {
            let path = ca_path.ok_or("no CA certificate path for the HTTPS instance")?;
            let pem = std::fs::read(path).map_err(|e| {
                format!(
                    "Pando CA certificate {} not readable yet: {e}",
                    path.display()
                )
            })?;
            cfg = cfg.with_root_certificate_pem(pem.clone());
            ca_pem = Some(pem);
        }
        let client = PandoClient::new(cfg).map_err(|e| e.to_string())?;
        let mut slot = self.runtime.lock();
        if slot.is_none() {
            *slot = Some(
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|e| e.to_string())?,
            );
        }
        let Some(rt) = slot.as_ref() else {
            return Err("no runtime".into());
        };
        rt.block_on(async {
            client.info().await.map_err(|e| e.to_string())?;
            let api_token = if want_token {
                Some(client.fetch_api_token().await.map_err(|e| e.to_string())?)
            } else {
                None
            };
            Ok(ProbeOutcome { api_token, ca_pem })
        })
    }
}

/// Everything a [`ManagedSupervisor`] needs.
#[derive(Debug, Clone)]
pub struct ManagedOptions {
    /// The `pando` executable: an absolute path or a name looked up on `PATH`.
    pub binary: PathBuf,
    /// `<cache>/pando`: instance directories live below it.
    pub cache_root: PathBuf,
    /// Oldest acceptable `pando --version` (`None` accepts any).
    pub min_version: Option<String>,
    /// `https` for the real `pando serve`; tests use `http`.
    pub scheme: &'static str,
    /// The private CA file `pando serve` creates (default: `<pando config dir>/ca.crt`).
    pub ca_path: Option<PathBuf>,
    /// Adds `--debug` to the child.
    pub debug: bool,
    /// Extra environment of the child.
    pub extra_env: Vec<(String, String)>,
    /// Command prefix of the lifeline watchdog, used on hosts without a parent-death signal
    /// (macOS): the child is started as `<prefix...> <binary> <args...>` with the lifeline as
    /// descriptor 3. Binaries expose it with [`crate::managed::run_watchdog_fd3`].
    pub watchdog: Option<Vec<String>>,
    /// Timings.
    pub timing: Timing,
    /// Health checker.
    pub prober: Arc<dyn Prober>,
}

impl ManagedOptions {
    /// Defaults for `binary` with instances below `cache_root`.
    #[must_use]
    pub fn new(binary: impl Into<PathBuf>, cache_root: impl Into<PathBuf>) -> Self {
        Self {
            binary: binary.into(),
            cache_root: cache_root.into(),
            min_version: Some(DEFAULT_MIN_VERSION.to_owned()),
            scheme: "https",
            ca_path: default_ca_path(),
            debug: false,
            extra_env: Vec::new(),
            watchdog: None,
            timing: Timing::default(),
            prober: Arc::new(HttpProber::default()),
        }
    }
}

/// `<machine-local cache>/pando`, the root of every managed instance directory (never inside a
/// graph). Same `ProjectDirs` as the app's index cache.
#[must_use]
pub fn default_cache_root() -> Option<PathBuf> {
    directories::ProjectDirs::from("es", "Digio", "Bitacora").map(|d| d.cache_dir().join("pando"))
}

/// `$XDG_CONFIG_HOME/pando/tls/ca.crt` or `~/.config/pando/tls/ca.crt`, where `pando serve` keeps its CA.
#[must_use]
pub fn default_ca_path() -> Option<PathBuf> {
    let base = match std::env::var_os("XDG_CONFIG_HOME").filter(|v| !v.is_empty()) {
        Some(x) => PathBuf::from(x),
        None => PathBuf::from(std::env::var_os("HOME")?).join(".config"),
    };
    Some(base.join("pando").join("tls").join("ca.crt"))
}

/// Looks `binary` up on `PATH` (or checks it when it has a directory part).
#[must_use]
pub fn resolve_binary(binary: &Path) -> Option<PathBuf> {
    let usable = |p: &Path| p.is_file();
    if binary.components().count() > 1 || binary.is_absolute() {
        return usable(binary).then(|| binary.to_path_buf());
    }
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|d| d.join(binary))
        .find(|p| usable(p))
}

/// First `x.y.z` in `text` (a leading `v` is ignored).
#[must_use]
pub fn parse_version(text: &str) -> Option<(u64, u64, u64)> {
    for word in text.split(|c: char| c.is_whitespace() || c == ',' || c == '(' || c == ')') {
        let w = word.trim_start_matches(['v', 'V']);
        let mut it = w.split('.');
        let (Some(a), Some(b), Some(c)) = (it.next(), it.next(), it.next()) else {
            continue;
        };
        let num = |s: &str| {
            let d: String = s.chars().take_while(char::is_ascii_digit).collect();
            d.parse::<u64>().ok()
        };
        if let (Some(a), Some(b), Some(c)) = (num(a), num(b), num(c)) {
            return Some((a, b, c));
        }
    }
    None
}

/// Runs `binary --version` and checks it against `min`. Returns the version line.
fn check_binary(
    binary: &Path,
    min: Option<&str>,
    env: &[(String, String)],
) -> Result<String, String> {
    let mut cmd = Command::new(binary);
    cmd.arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    for (k, v) in env {
        cmd.env(k, v);
    }
    let mut child =
        spawn_retrying(&mut cmd).map_err(|e| format!("cannot run {}: {e}", binary.display()))?;
    let end = Instant::now() + Duration::from_secs(10);
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) if Instant::now() < end => std::thread::sleep(Duration::from_millis(20)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("`{} --version` did not answer", binary.display()));
            }
            Err(e) => return Err(e.to_string()),
        }
    };
    let mut out = String::new();
    if let Some(mut so) = child.stdout.take() {
        let _ = std::io::Read::read_to_string(&mut so, &mut out);
    }
    if !status.success() {
        return Err(format!(
            "`{} --version` failed ({status})",
            binary.display()
        ));
    }
    let line = out.lines().next().unwrap_or("").trim().to_owned();
    let found =
        parse_version(&line).ok_or_else(|| format!("cannot read a version from `{line}`"))?;
    if let Some(min) = min {
        let floor = parse_version(min).ok_or_else(|| format!("invalid minimum version `{min}`"))?;
        if found < floor {
            return Err(format!(
                "Pando {line} is older than the required {min}; update Pando"
            ));
        }
    }
    Ok(line)
}

/// `spawn`, retrying a few times on `ETXTBSY`: a binary that was just written (an update, a
/// download) can be briefly busy because another thread's `fork` still holds its write
/// descriptor.
fn spawn_retrying(cmd: &mut Command) -> std::io::Result<Child> {
    let mut attempts = 0;
    loop {
        match cmd.spawn() {
            Err(e) if e.raw_os_error() == Some(26) && attempts < 10 => {
                attempts += 1;
                std::thread::sleep(Duration::from_millis(30));
            }
            other => return other,
        }
    }
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|d| i64::try_from(d.as_secs()).ok())
        .unwrap_or(0)
}

/// A free loopback port, trying `preferred` first for up to `wait`, never one of `avoid`.
fn pick_port(preferred: Option<u16>, wait: Duration, avoid: &[u16]) -> Result<u16, String> {
    if let Some(p) = preferred.filter(|p| !avoid.contains(p)) {
        let end = Instant::now() + wait;
        loop {
            if TcpListener::bind(("127.0.0.1", p)).is_ok() {
                return Ok(p);
            }
            if Instant::now() >= end {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    for _ in 0..8 {
        let l = TcpListener::bind(("127.0.0.1", 0))
            .map_err(|e| format!("no free loopback port: {e}"))?;
        let p = l.local_addr().map_err(|e| e.to_string())?.port();
        drop(l);
        if !avoid.contains(&p) {
            return Ok(p);
        }
    }
    Err("no free loopback port".into())
}

enum Cmd {
    Restart,
    Stop,
}

struct Shared {
    dir: InstanceDir,
    status: StdMutex<ManagedStatus>,
    cv: Condvar,
    token: Mutex<Option<Token>>,
    ca_pem: Mutex<Option<Vec<u8>>>,
}

impl Shared {
    fn update(&self, f: impl FnOnce(&mut ManagedStatus)) {
        let mut g = self
            .status
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let prev = g.state;
        f(&mut g);
        if g.state != prev {
            g.since = now_unix();
        }
        if let Err(e) = write_status(&self.dir, &g) {
            tracing::warn!("cannot write the Pando state file: {e}");
        }
        drop(g);
        self.cv.notify_all();
    }

    fn snapshot(&self) -> ManagedStatus {
        self.status
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    fn endpoint(&self) -> Option<ManagedEndpoint> {
        let st = self.snapshot();
        if st.state != ManagedState::Ready {
            return None;
        }
        let token = self.token.lock().clone();
        Some(ManagedEndpoint {
            rest_url: st.rest_url?,
            agui_url: st.agui_url?,
            rest_token: token.clone(),
            agui_token: token,
            ca_pem: self.ca_pem.lock().clone(),
        })
    }
}

/// One supervised Pando.
struct Instance {
    shared: Arc<Shared>,
    ctl: Sender<Cmd>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl Instance {
    fn finished(&self) -> bool {
        self.thread
            .lock()
            .as_ref()
            .is_none_or(JoinHandle::is_finished)
    }

    fn stop(&self) {
        let _ = self.ctl.send(Cmd::Stop);
        if let Some(t) = self.thread.lock().take() {
            let _ = t.join();
        }
    }
}

/// Real [`Supervisor`]: a managed `pando serve` per graph (ADR-029, BIT-US-0141).
pub struct ManagedSupervisor {
    opts: Arc<ManagedOptions>,
    instances: Mutex<HashMap<PathBuf, Arc<Instance>>>,
    mcp: Arc<Mutex<HashMap<PathBuf, McpAccess>>>,
    chat_models: Arc<Mutex<HashMap<PathBuf, Vec<String>>>>,
}

impl std::fmt::Debug for ManagedSupervisor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ManagedSupervisor")
            .field("binary", &self.opts.binary)
            .finish_non_exhaustive()
    }
}

impl ManagedSupervisor {
    /// A supervisor with `opts`.
    #[must_use]
    pub fn new(opts: ManagedOptions) -> Self {
        Self {
            opts: Arc::new(opts),
            instances: Mutex::new(HashMap::new()),
            mcp: Arc::new(Mutex::new(HashMap::new())),
            chat_models: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// A supervisor with the defaults: the `pando` found on `PATH` (or `binary`, a configured
    /// path) and instances under [`default_cache_root`].
    ///
    /// # Errors
    /// No cache directory can be determined on this host.
    pub fn with_defaults(binary: Option<&str>) -> Result<Self, String> {
        let root = default_cache_root().ok_or("cannot determine the cache directory")?;
        let binary = binary.filter(|b| !b.trim().is_empty()).unwrap_or("pando");
        Ok(Self::new(ManagedOptions::new(binary, root)))
    }

    /// The instance directory of `graph`.
    #[must_use]
    pub fn instance_dir(&self, graph: &Path) -> InstanceDir {
        InstanceDir::new(&self.opts.cache_root, graph)
    }

    fn start_instance(&self, graph: &Path) -> Result<Arc<Instance>, StartError> {
        if !sys::SUPPORTED {
            return Err(StartError::Failed(
                "managed Pando is not supported on this platform yet; use external mode".into(),
            ));
        }
        let dir = self.instance_dir(graph);
        dir.ensure().map_err(|e| {
            StartError::Failed(format!("cannot create the instance directory: {e}"))
        })?;
        let lock = File::options()
            .create(true)
            .truncate(false)
            .write(true)
            .open(dir.lock_path())
            .map_err(|e| StartError::Failed(format!("cannot open the supervisor lock: {e}")))?;
        match lock.try_lock() {
            Ok(()) => {}
            Err(std::fs::TryLockError::WouldBlock) => return Err(StartError::Locked(dir)),
            Err(std::fs::TryLockError::Error(e)) => {
                return Err(StartError::Failed(format!("cannot lock the instance: {e}")));
            }
        }
        let shared = Arc::new(Shared {
            dir: dir.clone(),
            status: StdMutex::new(ManagedStatus::default()),
            cv: Condvar::new(),
            token: Mutex::new(None),
            ca_pem: Mutex::new(None),
        });
        // Keep the ports of the previous run: clients that remember them keep working.
        let previous = read_status(&dir).ok();
        let fail = |shared: &Shared, msg: String| {
            shared.update(|s| {
                s.state = ManagedState::Failed;
                s.last_error = Some(msg.clone());
                s.supervisor_pid = Some(std::process::id());
            });
            StartError::Failed(msg)
        };
        if let Some(prev) = &previous {
            shared.update(|s| {
                s.last_rest_port = prev.last_rest_port;
                s.last_agui_port = prev.last_agui_port;
            });
            self.reap_orphan(&dir, prev);
        }
        let binary = match resolve_binary(&self.opts.binary) {
            Some(b) => b,
            None => {
                let hint = if std::env::var_os("FLATPAK_ID").is_some() {
                    " (inside Flatpak the host's pando is not reachable unless it is exposed to the sandbox)"
                } else {
                    ""
                };
                return Err(fail(
                    &shared,
                    format!(
                        "the `{}` binary was not found{hint}; install Pando or set its path",
                        self.opts.binary.display()
                    ),
                ));
            }
        };
        let version = match check_binary(
            &binary,
            self.opts.min_version.as_deref(),
            &self.opts.extra_env,
        ) {
            Ok(v) => v,
            Err(e) => return Err(fail(&shared, e)),
        };
        shared.update(|s| {
            s.state = ManagedState::Starting;
            s.supervisor_pid = Some(std::process::id());
            s.version = Some(version);
            s.binary = Some(binary.display().to_string());
            s.last_error = None;
            s.crashes = 0;
        });
        let mcp_secret = self.mcp.lock().get(graph).map(|m| m.token_str().to_owned());
        let sink = LogSink::open(&dir, mcp_secret.into_iter().collect())
            .map_err(|e| StartError::Failed(format!("cannot open the log: {e}")))?;
        let (tx, rx) = mpsc::channel();
        let worker = Worker {
            opts: Arc::clone(&self.opts),
            graph: graph.to_path_buf(),
            mcp: Arc::clone(&self.mcp),
            chat_models: Arc::clone(&self.chat_models),
            binary,
            shared: Arc::clone(&shared),
            sink,
            rx,
            crashes: Vec::new(),
            _lock: lock,
        };
        let handle = std::thread::Builder::new()
            .name("bitacora-pando-supervisor".into())
            .spawn(move || worker.run())
            .map_err(|e| StartError::Failed(format!("cannot start the supervisor thread: {e}")))?;
        Ok(Arc::new(Instance {
            shared,
            ctl: tx,
            thread: Mutex::new(Some(handle)),
        }))
    }

    /// Ends a child that a dead supervisor left behind (state file names a live pid that runs in
    /// this instance directory).
    fn reap_orphan(&self, dir: &InstanceDir, prev: &ManagedStatus) {
        let Some(pid) = prev.pid.filter(|p| *p != std::process::id()) else {
            return;
        };
        if !sys::pid_alive(pid) {
            return;
        }
        let binary = prev
            .binary
            .as_deref()
            .map(Path::new)
            .unwrap_or(&self.opts.binary);
        if sys::is_instance_process(pid, dir.path(), binary) {
            tracing::warn!(pid, "ending a managed Pando left by a previous supervisor");
            sys::kill_orphan(pid, self.opts.timing.stop_timeout);
        }
    }

    /// Another process supervises the instance: use its endpoint when it is ready.
    fn adopt(&self, dir: &InstanceDir) -> Result<ManagedEndpoint, String> {
        let st = read_status(dir).map_err(|_| {
            "another Bitacora process manages this Pando instance and has not published its state"
                .to_owned()
        })?;
        let alive = st.pid.is_some_and(sys::pid_alive);
        if st.state != ManagedState::Ready || !alive {
            return Err(
                "another Bitacora process manages this Pando instance and it is not ready".into(),
            );
        }
        let token = std::fs::read_to_string(dir.token_path())
            .ok()
            .map(|t| Token::new(t.trim()))
            .filter(|t| !t.expose().is_empty());
        let ca_pem = self
            .opts
            .ca_path
            .as_ref()
            .and_then(|p| std::fs::read(p).ok());
        Ok(ManagedEndpoint {
            rest_url: st.rest_url.ok_or("no REST URL in the shared state")?,
            agui_url: st.agui_url.ok_or("no AG-UI URL in the shared state")?,
            rest_token: token.clone(),
            agui_token: token,
            ca_pem,
        })
    }
}

enum StartError {
    Locked(InstanceDir),
    Failed(String),
}

impl Supervisor for ManagedSupervisor {
    fn ensure_running(&self, graph: &Path) -> Result<ManagedEndpoint, String> {
        let instance = {
            let mut map = self.instances.lock();
            match map.get(graph).filter(|i| !i.finished()).cloned() {
                Some(i) => i,
                None => match self.start_instance(graph) {
                    Ok(i) => {
                        map.insert(graph.to_path_buf(), Arc::clone(&i));
                        i
                    }
                    Err(StartError::Locked(dir)) => return self.adopt(&dir),
                    Err(StartError::Failed(e)) => return Err(e),
                },
            }
        };
        let shared = &instance.shared;
        let deadline = Instant::now() + self.opts.timing.ready_timeout + Duration::from_secs(5);
        let mut guard = shared
            .status
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        loop {
            match guard.state {
                ManagedState::Ready => break,
                ManagedState::Failed => {
                    return Err(guard
                        .last_error
                        .clone()
                        .unwrap_or_else(|| "managed Pando failed".into()));
                }
                ManagedState::Stopped => return Err("managed Pando is stopped".into()),
                ManagedState::Starting | ManagedState::Restarting => {}
            }
            let Some(left) = deadline.checked_duration_since(Instant::now()) else {
                return Err("managed Pando did not become ready in time".into());
            };
            guard = shared
                .cv
                .wait_timeout(guard, left)
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .0;
        }
        drop(guard);
        shared
            .endpoint()
            .ok_or_else(|| "managed Pando has no endpoint yet".into())
    }

    fn stop(&self, graph: &Path) {
        let instance = self.instances.lock().remove(graph);
        if let Some(i) = instance {
            i.stop();
        }
    }

    fn set_mcp_access(&self, graph: &Path, access: Option<McpAccess>) {
        let mut map = self.mcp.lock();
        match access {
            Some(a) => map.insert(graph.to_path_buf(), a),
            None => map.remove(graph),
        };
    }

    fn set_chat_models(&self, graph: &Path, models: Vec<String>) {
        self.chat_models.lock().insert(graph.to_path_buf(), models);
    }

    fn managed_status(&self, graph: &Path) -> Option<ManagedStatus> {
        if let Some(i) = self.instances.lock().get(graph) {
            return Some(i.shared.snapshot());
        }
        read_status(&self.instance_dir(graph)).ok()
    }

    fn restart(&self, graph: &Path) {
        if let Some(i) = self.instances.lock().get(graph) {
            let _ = i.ctl.send(Cmd::Restart);
        }
    }

    fn log_path(&self, graph: &Path) -> Option<PathBuf> {
        let p = self.instance_dir(graph).log_path();
        p.exists().then_some(p)
    }
}

impl Drop for ManagedSupervisor {
    fn drop(&mut self) {
        let all: Vec<_> = self.instances.lock().drain().map(|(_, i)| i).collect();
        for i in all {
            i.stop();
        }
    }
}

enum End {
    Stop,
    Restart,
    Crash(String),
}

struct Worker {
    opts: Arc<ManagedOptions>,
    graph: PathBuf,
    mcp: Arc<Mutex<HashMap<PathBuf, McpAccess>>>,
    chat_models: Arc<Mutex<HashMap<PathBuf, Vec<String>>>>,
    binary: PathBuf,
    shared: Arc<Shared>,
    sink: LogSink,
    rx: Receiver<Cmd>,
    crashes: Vec<Instant>,
    _lock: File,
}

impl Worker {
    fn run(mut self) {
        let mut avoid: Vec<u16> = Vec::new();
        loop {
            let end = self.run_once(&mut avoid);
            match end {
                End::Stop => break,
                End::Restart => {
                    self.shared.update(|s| s.state = ManagedState::Restarting);
                    self.crashes.clear();
                }
                End::Crash(reason) => {
                    let (delay, failed) = self.record_crash(&reason);
                    tracing::warn!(reason, "managed Pando ended");
                    if failed {
                        tracing::error!("managed Pando marked failed");
                        match self.rx.recv() {
                            Ok(Cmd::Restart) => {
                                self.crashes.clear();
                                self.shared.update(|s| {
                                    s.state = ManagedState::Restarting;
                                    s.crashes = 0;
                                });
                                continue;
                            }
                            Ok(Cmd::Stop) | Err(_) => break,
                        }
                    }
                    match self.rx.recv_timeout(delay) {
                        Ok(Cmd::Stop) | Err(RecvTimeoutError::Disconnected) => break,
                        Ok(Cmd::Restart) => self.crashes.clear(),
                        Err(RecvTimeoutError::Timeout) => {}
                    }
                }
            }
        }
        self.shared.update(|s| {
            s.state = ManagedState::Stopped;
            s.pid = None;
            s.rest_url = None;
            s.agui_url = None;
        });
        // Dropping `self` releases the lock file.
    }

    fn record_crash(&mut self, reason: &str) -> (Duration, bool) {
        let t = &self.opts.timing;
        let now = Instant::now();
        self.crashes
            .retain(|c| now.duration_since(*c) < t.crash_window);
        self.crashes.push(now);
        let n = self.crashes.len();
        let failed = n >= usize::try_from(t.max_crashes).unwrap_or(usize::MAX);
        let tail = self.sink.tail();
        self.shared.update(|s| {
            s.pid = None;
            s.rest_url = None;
            s.agui_url = None;
            s.crashes = u32::try_from(n).unwrap_or(u32::MAX);
            s.last_error = Some(reason.to_owned());
            s.last_output = tail;
            s.state = if failed {
                ManagedState::Failed
            } else {
                ManagedState::Restarting
            };
        });
        let mut delay = t.backoff_min;
        for _ in 1..n {
            if delay >= t.backoff_max {
                break;
            }
            delay = delay.saturating_mul(2);
        }
        (delay.min(t.backoff_max), failed)
    }

    fn write_config(&self, agui_port: u16) -> Result<(), String> {
        let mcp = self.mcp.lock().get(&self.graph).cloned();
        let chat_models = self
            .chat_models
            .lock()
            .get(&self.graph)
            .cloned()
            .unwrap_or_default();
        let rendered = config::render(&ConfigInput {
            agui_port,
            mcp: mcp.as_ref(),
            chat_models: &chat_models,
        });
        for (rel, content) in rendered.files {
            let dest = self.shared.dir.path().join(&rel);
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("create {rel} directory: {e}"))?;
            }
            write_atomic(&dest, content.as_bytes()).map_err(|e| format!("write {rel}: {e}"))?;
        }
        Ok(())
    }

    fn spawn(&self, rest_port: u16, agui_port: u16) -> Result<(Child, sys::Lifeline), String> {
        let mut args: Vec<String> = vec![
            "serve".into(),
            "--host".into(),
            "127.0.0.1".into(),
            "--port".into(),
            rest_port.to_string(),
            "--agui-port".into(),
            agui_port.to_string(),
        ];
        if self.opts.debug {
            args.push("--debug".into());
        }
        let watchdog = self
            .opts
            .watchdog
            .as_ref()
            .filter(|w| !w.is_empty() && !sys::HAS_PDEATHSIG);
        let mut cmd = match watchdog {
            Some(prefix) => {
                let mut c = Command::new(&prefix[0]);
                c.args(&prefix[1..]).arg(&self.binary);
                c
            }
            None => Command::new(&self.binary),
        };
        cmd.args(&args)
            .current_dir(self.shared.dir.path())
            .env("PANDO_CONFIG_PARENT_SEARCH", "false")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (k, v) in &self.opts.extra_env {
            cmd.env(k, v);
        }
        let mut lifeline = sys::Lifeline::new().map_err(|e| format!("prepare pando: {e}"))?;
        sys::prepare(&mut cmd, watchdog.map(|_| &lifeline));
        let child = spawn_retrying(&mut cmd).map_err(|e| format!("start pando: {e}"))?;
        lifeline.started();
        Ok((child, lifeline))
    }

    fn pump(&self, child: &mut Child) {
        let sink = self.sink.clone();
        if let Some(out) = child.stdout.take() {
            let s = sink.clone();
            std::thread::spawn(move || {
                for l in BufReader::new(out).lines().map_while(Result::ok) {
                    s.line(&l);
                }
            });
        }
        if let Some(err) = child.stderr.take() {
            std::thread::spawn(move || {
                for l in BufReader::new(err).lines().map_while(Result::ok) {
                    sink.line(&l);
                }
            });
        }
    }

    fn stop_child(&self, child: &mut Child) {
        let pid = child.id();
        sys::signal_group(pid, false);
        let end = Instant::now() + self.opts.timing.stop_timeout;
        while Instant::now() < end {
            if matches!(child.try_wait(), Ok(Some(_))) {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        sys::signal_group(pid, true);
        let _ = child.kill();
        let _ = child.wait();
    }

    /// Waits one poll step for a control command; `Some(end)` when the run must end.
    fn control(&self) -> Option<End> {
        match self.rx.recv_timeout(self.opts.timing.poll) {
            Ok(Cmd::Stop) | Err(RecvTimeoutError::Disconnected) => Some(End::Stop),
            Ok(Cmd::Restart) => Some(End::Restart),
            Err(RecvTimeoutError::Timeout) => None,
        }
    }

    fn run_once(&mut self, avoid: &mut Vec<u16>) -> End {
        let t = self.opts.timing.clone();
        let prev = self.shared.snapshot();
        let rest_port = match pick_port(prev.last_rest_port, t.port_wait, avoid) {
            Ok(p) => p,
            Err(e) => return End::Crash(e),
        };
        let agui_port = match pick_port(prev.last_agui_port, t.port_wait, &[rest_port]) {
            Ok(p) => p,
            Err(e) => return End::Crash(e),
        };
        if let Err(e) = self.write_config(agui_port) {
            return End::Crash(e);
        }
        let (mut child, _lifeline) = match self.spawn(rest_port, agui_port) {
            Ok(c) => c,
            Err(e) => return End::Crash(e),
        };
        self.pump(&mut child);
        let scheme = self.opts.scheme;
        let rest_url = format!("{scheme}://127.0.0.1:{rest_port}");
        let agui_url = format!("{scheme}://127.0.0.1:{agui_port}");
        self.shared.update(|s| {
            s.state = ManagedState::Starting;
            s.pid = Some(child.id());
            s.last_rest_port = Some(rest_port);
            s.last_agui_port = Some(agui_port);
        });

        // Ready check.
        let deadline = Instant::now() + t.ready_timeout;
        let mut last_err;
        let outcome = loop {
            if let Some(end) = self.control() {
                self.stop_child(&mut child);
                return end;
            }
            if let Ok(Some(status)) = child.try_wait() {
                return End::Crash(format!("pando exited while starting: {status}"));
            }
            match self
                .opts
                .prober
                .probe(&rest_url, self.opts.ca_path.as_deref(), true)
            {
                Ok(o) => break o,
                Err(e) => last_err = e,
            }
            if Instant::now() >= deadline {
                self.stop_child(&mut child);
                // Pando silently moves to another port when the requested one is busy.
                avoid.clear();
                avoid.push(rest_port);
                return End::Crash(format!(
                    "pando was not healthy on port {rest_port} within {:?}: {last_err}",
                    t.ready_timeout
                ));
            }
        };
        avoid.clear();
        if let Some(tok) = &outcome.api_token {
            self.sink.add_secret(tok.expose());
            if let Err(e) = write_atomic(
                &self.shared.dir.token_path(),
                format!("{}\n", tok.expose()).as_bytes(),
            ) {
                tracing::warn!("cannot write the Pando token file: {e}");
            }
        }
        *self.shared.token.lock() = outcome.api_token;
        *self.shared.ca_pem.lock() = outcome.ca_pem;
        self.shared.update(|s| {
            s.state = ManagedState::Ready;
            s.rest_url = Some(rest_url.clone());
            s.agui_url = Some(agui_url.clone());
            s.last_error = None;
        });

        // Monitor.
        let mut next_check = Instant::now() + t.health_interval;
        let mut fails = 0_u32;
        loop {
            if let Some(end) = self.control() {
                self.stop_child(&mut child);
                return end;
            }
            if let Ok(Some(status)) = child.try_wait() {
                return End::Crash(format!("pando exited: {status}"));
            }
            if Instant::now() < next_check {
                continue;
            }
            next_check = Instant::now() + t.health_interval;
            match self
                .opts
                .prober
                .probe(&rest_url, self.opts.ca_path.as_deref(), false)
            {
                Ok(_) => fails = 0,
                Err(e) => {
                    fails += 1;
                    if fails >= t.health_failures {
                        self.stop_child(&mut child);
                        return End::Crash(format!("{fails} health checks failed in a row: {e}"));
                    }
                }
            }
        }
    }
}
