//! [`PandoService`]: lifecycle of the Pando integration for one graph session.
//!
//! The service is created by `bitacora-runtime` when a session opens and stopped in its ordered
//! shutdown. It owns a small tokio runtime (the rest of the session is synchronous), resolves the
//! endpoint according to the machine-local settings, probes `GET /health` periodically and
//! publishes [`PandoEvent`]s. Later stories use [`PandoService::client`],
//! [`PandoService::handle`] and [`PandoService::sink`] to run KB sync and agent runs.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::Receiver;
use std::time::Duration;

use bitacora_config::{PandoMode, PandoSettings, UrlRole, validate_pando_url};
use pando::{PandoClient, PandoConfig, Token};
use parking_lot::Mutex;
use tokio::sync::watch;

use crate::credentials::{PandoCredentials, TokenKind};
use crate::events::{EventSink, PandoEvent, PandoStatus};
use crate::managed::ManagedStatus;
use crate::supervisor::{McpAccess, Supervisor};

/// Default delay between health probes.
pub const DEFAULT_PROBE_INTERVAL: Duration = Duration::from_secs(15);

/// Everything the service needs to start.
#[derive(Clone)]
pub struct PandoOptions {
    /// Machine-local settings.
    pub settings: PandoSettings,
    /// Canonical graph folder; also the key of the per-graph consent.
    pub graph: PathBuf,
    /// Token source.
    pub credentials: PandoCredentials,
    /// Managed-mode supervisor; `None` makes managed mode unavailable (BIT-US-0141).
    pub supervisor: Option<Arc<dyn Supervisor>>,
    /// Delay between health probes.
    pub probe_interval: Duration,
    /// How agents reach Bitacora's MCP server (managed mode registers it as
    /// `[MCPServers.bitacora]`; ADR-031). `None` registers nothing.
    pub mcp: Option<McpAccess>,
}

impl std::fmt::Debug for PandoOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PandoOptions")
            .field("mode", &self.settings.mode)
            .field("enabled", &self.settings.enabled)
            .field("graph", &self.graph)
            .finish_non_exhaustive()
    }
}

impl PandoOptions {
    /// Options with system credentials, no supervisor and the default probe interval.
    #[must_use]
    pub fn new(settings: PandoSettings, graph: impl Into<PathBuf>) -> Self {
        Self {
            settings,
            graph: graph.into(),
            credentials: PandoCredentials::system(),
            supervisor: None,
            probe_interval: DEFAULT_PROBE_INTERVAL,
            mcp: None,
        }
    }

    /// The consent key of the graph (its path as text).
    #[must_use]
    pub fn graph_key(&self) -> String {
        self.graph.to_string_lossy().into_owned()
    }
}

/// Resolved, validated endpoints.
#[derive(Clone)]
pub struct Endpoints {
    /// REST transport settings (URL, token, timeouts).
    pub rest: PandoConfig,
    /// AG-UI base URL.
    pub agui_url: String,
    /// AG-UI token.
    pub agui_token: Option<Token>,
}

impl std::fmt::Debug for Endpoints {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Endpoints")
            .field("rest", &self.rest.base_url)
            .field("agui_url", &self.agui_url)
            .finish_non_exhaustive()
    }
}

struct Shared {
    status: Mutex<PandoStatus>,
    client: Mutex<Option<PandoClient>>,
    endpoints: Mutex<Option<Endpoints>>,
    events: EventSink,
}

impl Shared {
    fn set_status(&self, new: PandoStatus) {
        {
            let mut cur = self.status.lock();
            if *cur == new {
                return;
            }
            *cur = new.clone();
        }
        self.events.emit(&PandoEvent::Status(new));
    }
}

/// A cheap, cloneable view of a [`PandoService`] for background workers (they must not borrow the
/// service itself, which the session owns and stops).
#[derive(Clone)]
pub struct ServiceProbe {
    shared: Arc<Shared>,
}

impl std::fmt::Debug for ServiceProbe {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ServiceProbe")
    }
}

impl ServiceProbe {
    /// The REST client once the endpoint is resolved.
    #[must_use]
    pub fn client(&self) -> Option<PandoClient> {
        self.shared.client.lock().clone()
    }

    /// Whether the last health probe succeeded.
    #[must_use]
    pub fn is_connected(&self) -> bool {
        matches!(*self.shared.status.lock(), PandoStatus::Connected { .. })
    }
}

/// A running (or inert) Pando integration for one graph.
pub struct PandoService {
    shared: Arc<Shared>,
    runtime: Option<tokio::runtime::Runtime>,
    stop_tx: watch::Sender<bool>,
    supervisor: Option<Arc<dyn Supervisor>>,
    graph: PathBuf,
    managed_started: Arc<std::sync::atomic::AtomicBool>,
    stopped: bool,
}

impl std::fmt::Debug for PandoService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PandoService")
            .field("status", &self.status())
            .finish_non_exhaustive()
    }
}

fn resolve_external(opts: &PandoOptions) -> Result<Endpoints, String> {
    let s = &opts.settings;
    let rest = validate_pando_url(UrlRole::Rest, &s.rest_url, s.allow_remote)
        .map_err(|e| e.to_string())?;
    let agui = validate_pando_url(UrlRole::Agui, &s.agui_url, s.allow_remote)
        .map_err(|e| e.to_string())?;
    let mut cfg = PandoConfig::new(rest.normalized);
    if let Some((t, _)) = opts.credentials.resolve(TokenKind::Rest) {
        cfg = cfg.with_token(t);
    }
    Ok(Endpoints {
        rest: cfg,
        agui_url: agui.normalized,
        agui_token: opts.credentials.resolve(TokenKind::Agui).map(|(t, _)| t),
    })
}

/// Managed endpoints are always loopback, whatever `allow_remote` says.
fn resolve_managed(
    opts: &PandoOptions,
    supervisor: &Arc<dyn Supervisor>,
) -> Result<Endpoints, String> {
    supervisor.set_mcp_access(&opts.graph, opts.mcp.clone());
    let m = supervisor.ensure_running(&opts.graph)?;
    let rest = validate_pando_url(UrlRole::Rest, &m.rest_url, false).map_err(|e| e.to_string())?;
    let agui = validate_pando_url(UrlRole::Agui, &m.agui_url, false).map_err(|e| e.to_string())?;
    let mut cfg = PandoConfig::new(rest.normalized);
    if let Some(t) = m.rest_token {
        cfg = cfg.with_token(t);
    }
    if let Some(pem) = m.ca_pem {
        cfg = cfg.with_root_certificate_pem(pem);
    }
    Ok(Endpoints {
        rest: cfg,
        agui_url: agui.normalized,
        agui_token: m.agui_token,
    })
}

impl PandoService {
    /// Starts the integration according to `opts`. Never fails: a problem becomes
    /// [`PandoStatus::Unavailable`] (and an event) so a bad Pando setup cannot stop a graph from
    /// opening.
    #[must_use]
    pub fn start(opts: PandoOptions) -> Self {
        let shared = Arc::new(Shared {
            status: Mutex::new(PandoStatus::Off),
            client: Mutex::new(None),
            endpoints: Mutex::new(None),
            events: EventSink::default(),
        });
        let (stop_tx, stop_rx) = watch::channel(false);
        let mut svc = Self {
            shared: Arc::clone(&shared),
            runtime: None,
            stop_tx,
            supervisor: opts.supervisor.clone(),
            graph: opts.graph.clone(),
            managed_started: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            stopped: false,
        };
        if !opts.settings.is_active() {
            return svc;
        }
        if !opts.settings.has_consent(&opts.graph_key()) {
            shared.set_status(PandoStatus::ConsentRequired);
            return svc;
        }
        if let Err(e) = opts.settings.validate() {
            shared.set_status(PandoStatus::Unavailable {
                reason: e.to_string(),
            });
            return svc;
        }
        let runtime = match tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .thread_name("bitacora-pando")
            .enable_all()
            .build()
        {
            Ok(rt) => rt,
            Err(e) => {
                shared.set_status(PandoStatus::Unavailable {
                    reason: format!("cannot start the async runtime: {e}"),
                });
                return svc;
            }
        };
        shared.set_status(PandoStatus::Starting);
        let started = Arc::clone(&svc.managed_started);
        runtime.spawn(run(shared, opts, stop_rx, started));
        svc.runtime = Some(runtime);
        svc
    }

    /// Current status.
    #[must_use]
    pub fn status(&self) -> PandoStatus {
        self.shared.status.lock().clone()
    }

    /// A receiver that first yields the current status, then every later event.
    #[must_use]
    pub fn subscribe(&self) -> Receiver<PandoEvent> {
        let rx = self.shared.events.subscribe();
        // Replayed through the sink so ordering with concurrent emits stays consistent enough:
        // a status change racing with this call yields at most a duplicate, never a gap.
        self.shared.events.emit(&PandoEvent::Status(self.status()));
        rx
    }

    /// A view of the service for background workers such as the semantic sync.
    #[must_use]
    pub fn probe(&self) -> ServiceProbe {
        ServiceProbe {
            shared: Arc::clone(&self.shared),
        }
    }

    /// Event sink for the sync and chat stories to publish progress and run events.
    #[must_use]
    pub fn sink(&self) -> EventSink {
        self.shared.events.clone()
    }

    /// The REST client once the endpoint is resolved (it may not be connected yet).
    #[must_use]
    pub fn client(&self) -> Option<PandoClient> {
        self.shared.client.lock().clone()
    }

    /// The resolved endpoints (for the AG-UI client).
    #[must_use]
    pub fn endpoints(&self) -> Option<Endpoints> {
        self.shared.endpoints.lock().clone()
    }

    /// Status of the managed instance (`None` outside managed mode or before it started).
    #[must_use]
    pub fn managed_status(&self) -> Option<ManagedStatus> {
        if !self
            .managed_started
            .load(std::sync::atomic::Ordering::SeqCst)
        {
            return None;
        }
        self.supervisor.as_ref()?.managed_status(&self.graph)
    }

    /// Restarts the managed instance (also clears a failed state).
    pub fn restart_managed(&self) {
        if let Some(s) = &self.supervisor {
            s.restart(&self.graph);
        }
    }

    /// Log file of the managed instance, for the "Open log" action.
    #[must_use]
    pub fn managed_log_path(&self) -> Option<PathBuf> {
        self.supervisor.as_ref()?.log_path(&self.graph)
    }

    /// Handle of the service's tokio runtime, to spawn sync and run tasks on.
    #[must_use]
    pub fn handle(&self) -> Option<tokio::runtime::Handle> {
        self.runtime.as_ref().map(|r| r.handle().clone())
    }

    /// Stops probing, asks the supervisor to stop the managed instance and shuts the runtime down
    /// within `budget`. Idempotent.
    pub fn stop(&mut self, budget: Duration) {
        if std::mem::replace(&mut self.stopped, true) {
            return;
        }
        let _ = self.stop_tx.send(true);
        if let Some(rt) = self.runtime.take() {
            rt.shutdown_timeout(budget);
        }
        if self
            .managed_started
            .load(std::sync::atomic::Ordering::SeqCst)
            && let Some(s) = &self.supervisor
        {
            s.stop(&self.graph);
        }
        *self.shared.client.lock() = None;
        self.shared.set_status(PandoStatus::Off);
    }
}

impl Drop for PandoService {
    fn drop(&mut self) {
        self.stop(Duration::from_secs(2));
    }
}

async fn run(
    shared: Arc<Shared>,
    opts: PandoOptions,
    mut stop: watch::Receiver<bool>,
    managed_started: Arc<std::sync::atomic::AtomicBool>,
) {
    let resolved = match opts.settings.mode {
        PandoMode::External => resolve_external(&opts),
        PandoMode::Managed => match opts.supervisor.clone() {
            None => Err("managed Pando is not available in this build; use external mode".into()),
            Some(sup) => {
                // The supervisor may block (process spawn, readiness wait).
                let o = opts.clone();
                managed_started.store(true, std::sync::atomic::Ordering::SeqCst);
                match tokio::task::spawn_blocking(move || resolve_managed(&o, &sup)).await {
                    Ok(r) => r,
                    Err(e) => Err(format!("supervisor task failed: {e}")),
                }
            }
        },
        PandoMode::Off => return,
    };
    let endpoints = match resolved {
        Ok(e) => e,
        Err(reason) => {
            shared.set_status(PandoStatus::Unavailable { reason });
            return;
        }
    };
    let mut client = match install(&shared, endpoints) {
        Ok(c) => c,
        Err(reason) => {
            shared.set_status(PandoStatus::Unavailable { reason });
            return;
        }
    };
    let managed = opts.settings.mode == PandoMode::Managed;
    loop {
        match client.info().await {
            Ok(info) => shared.set_status(PandoStatus::Connected {
                version: info.version,
            }),
            Err(e) => {
                shared.set_status(PandoStatus::Unavailable {
                    reason: e.to_string(),
                });
                // A managed instance that was restarted may listen elsewhere with a new token:
                // ask the supervisor again and switch clients when the endpoint changed.
                if managed && let Some(sup) = opts.supervisor.clone() {
                    let o = opts.clone();
                    if let Ok(Ok(fresh)) =
                        tokio::task::spawn_blocking(move || resolve_managed(&o, &sup)).await
                        && endpoint_changed(&shared, &fresh)
                        && let Ok(c) = install(&shared, fresh)
                    {
                        client = c;
                    }
                }
            }
        }
        tokio::select! {
            () = tokio::time::sleep(opts.probe_interval) => {}
            _ = stop.changed() => return,
        }
    }
}

/// Builds the client for `endpoints` and publishes both.
fn install(shared: &Shared, endpoints: Endpoints) -> Result<PandoClient, String> {
    let client = PandoClient::new(endpoints.rest.clone()).map_err(|e| e.to_string())?;
    *shared.endpoints.lock() = Some(endpoints);
    *shared.client.lock() = Some(client.clone());
    Ok(client)
}

fn endpoint_changed(shared: &Shared, fresh: &Endpoints) -> bool {
    shared.endpoints.lock().as_ref().is_none_or(|cur| {
        cur.rest.base_url != fresh.rest.base_url
            || cur.agui_url != fresh.agui_url
            || cur.rest.token != fresh.rest.token
            || cur.agui_token != fresh.agui_token
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;
    use crate::credentials::{MemoryBackend, SecretBackend};
    use crate::supervisor::ManagedEndpoint;
    use std::net::SocketAddr;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Instant;

    /// A tiny `/health` server on its own thread; returns its address.
    fn health_server() -> SocketAddr {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            rt.block_on(async {
                let app = axum::Router::new().route(
                    "/health",
                    axum::routing::get(|| async { axum::Json(serde_health("9.9.9")) }),
                );
                let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
                tx.send(l.local_addr().unwrap()).unwrap();
                axum::serve(l, app).await.unwrap();
            });
        });
        rx.recv().unwrap()
    }

    fn serde_health(version: &str) -> std::collections::BTreeMap<&'static str, String> {
        std::collections::BTreeMap::from([("version", version.to_owned())])
    }

    fn settings(mode: PandoMode, url: &str, graph: &std::path::Path) -> PandoSettings {
        let mut s = PandoSettings {
            enabled: true,
            mode,
            rest_url: url.to_owned(),
            agui_url: url.to_owned(),
            ..PandoSettings::default()
        };
        s.grant_consent(&graph.to_string_lossy(), 1);
        s
    }

    fn opts(s: PandoSettings, graph: &std::path::Path) -> PandoOptions {
        let mem: Arc<dyn SecretBackend> = Arc::new(MemoryBackend::default());
        PandoOptions {
            settings: s,
            graph: graph.to_path_buf(),
            credentials: PandoCredentials::new(Some(mem), |_| None),
            supervisor: None,
            probe_interval: Duration::from_millis(50),
            mcp: None,
        }
    }

    fn wait_for(
        rx: &Receiver<PandoEvent>,
        pred: impl Fn(&PandoStatus) -> bool,
    ) -> Option<PandoStatus> {
        let end = Instant::now() + Duration::from_secs(10);
        while let Some(left) = end.checked_duration_since(Instant::now()) {
            match rx.recv_timeout(left) {
                Ok(PandoEvent::Status(s)) if pred(&s) => return Some(s),
                Ok(_) => {}
                Err(_) => return None,
            }
        }
        None
    }

    #[test]
    fn default_settings_start_nothing() {
        let g = PathBuf::from("/g");
        let svc = PandoService::start(opts(PandoSettings::default(), &g));
        assert_eq!(svc.status(), PandoStatus::Off);
        assert!(svc.client().is_none() && svc.handle().is_none());
    }

    #[test]
    fn missing_consent_blocks_connection() {
        let g = PathBuf::from("/g");
        let mut s = settings(PandoMode::External, "http://127.0.0.1:9", &g);
        s.revoke_consent("/g");
        let svc = PandoService::start(opts(s, &g));
        assert_eq!(svc.status(), PandoStatus::ConsentRequired);
        assert!(svc.handle().is_none());
    }

    #[test]
    fn invalid_or_remote_urls_are_unavailable_without_connecting() {
        let g = PathBuf::from("/g");
        let svc = PandoService::start(opts(
            settings(PandoMode::External, "http://pando.example.com", &g),
            &g,
        ));
        assert!(
            matches!(svc.status(), PandoStatus::Unavailable { .. }),
            "{:?}",
            svc.status()
        );
        assert!(svc.handle().is_none());
    }

    #[test]
    fn external_mode_connects_and_stops_cleanly() {
        let g = PathBuf::from("/g");
        let addr = health_server();
        let mut svc = PandoService::start(opts(
            settings(PandoMode::External, &format!("http://{addr}"), &g),
            &g,
        ));
        let rx = svc.subscribe();
        let s = wait_for(&rx, |s| matches!(s, PandoStatus::Connected { .. })).unwrap();
        assert_eq!(
            s,
            PandoStatus::Connected {
                version: "9.9.9".into()
            }
        );
        assert!(svc.client().is_some() && svc.endpoints().is_some());
        svc.stop(Duration::from_secs(2));
        assert_eq!(svc.status(), PandoStatus::Off);
        assert!(svc.client().is_none());
        svc.stop(Duration::from_secs(2)); // idempotent
    }

    #[test]
    fn unreachable_server_reports_unavailable() {
        let g = PathBuf::from("/g");
        // Reserve a port and free it so nothing listens there.
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let svc = PandoService::start(opts(
            settings(PandoMode::External, &format!("http://127.0.0.1:{port}"), &g),
            &g,
        ));
        let rx = svc.subscribe();
        assert!(wait_for(&rx, |s| matches!(s, PandoStatus::Unavailable { .. })).is_some());
    }

    #[derive(Debug)]
    struct FakeSupervisor {
        url: String,
        stopped: Arc<AtomicBool>,
    }

    impl Supervisor for FakeSupervisor {
        fn ensure_running(&self, _: &std::path::Path) -> Result<ManagedEndpoint, String> {
            Ok(ManagedEndpoint {
                rest_url: self.url.clone(),
                agui_url: self.url.clone(),
                rest_token: Some(Token::new("generated")),
                agui_token: None,
                ca_pem: None,
            })
        }
        fn stop(&self, _: &std::path::Path) {
            self.stopped.store(true, Ordering::SeqCst);
        }
    }

    #[test]
    fn managed_mode_without_supervisor_is_unavailable() {
        let g = PathBuf::from("/g");
        let svc = PandoService::start(opts(
            settings(PandoMode::Managed, "http://127.0.0.1:1", &g),
            &g,
        ));
        let rx = svc.subscribe();
        let s = wait_for(&rx, |s| matches!(s, PandoStatus::Unavailable { .. })).unwrap();
        assert!(matches!(s, PandoStatus::Unavailable { reason } if reason.contains("managed")));
    }

    #[test]
    fn managed_mode_uses_the_supervisor_and_stops_it() {
        let g = PathBuf::from("/g");
        let addr = health_server();
        let stopped = Arc::new(AtomicBool::new(false));
        let mut o = opts(settings(PandoMode::Managed, "http://127.0.0.1:1", &g), &g);
        o.supervisor = Some(Arc::new(FakeSupervisor {
            url: format!("http://{addr}"),
            stopped: Arc::clone(&stopped),
        }));
        let mut svc = PandoService::start(o);
        let rx = svc.subscribe();
        assert!(wait_for(&rx, |s| matches!(s, PandoStatus::Connected { .. })).is_some());
        svc.stop(Duration::from_secs(2));
        assert!(stopped.load(Ordering::SeqCst));
    }

    #[test]
    fn managed_endpoint_outside_loopback_is_refused() {
        let g = PathBuf::from("/g");
        let mut o = opts(settings(PandoMode::Managed, "http://127.0.0.1:1", &g), &g);
        o.settings.allow_remote = true;
        o.supervisor = Some(Arc::new(FakeSupervisor {
            url: "https://pando.example.com".into(),
            stopped: Arc::new(AtomicBool::new(false)),
        }));
        let svc = PandoService::start(o);
        let rx = svc.subscribe();
        assert!(wait_for(&rx, |s| matches!(s, PandoStatus::Unavailable { .. })).is_some());
    }

    /// First answer is a dead port (the instance crashed), later answers the live one.
    #[derive(Debug)]
    struct MovingSupervisor {
        live: String,
        calls: std::sync::atomic::AtomicUsize,
        mcp: Mutex<Option<McpAccess>>,
    }

    impl Supervisor for MovingSupervisor {
        fn ensure_running(&self, _: &std::path::Path) -> Result<ManagedEndpoint, String> {
            let n = self.calls.fetch_add(1, Ordering::SeqCst);
            let url = if n == 0 {
                "http://127.0.0.1:1".to_owned()
            } else {
                self.live.clone()
            };
            Ok(ManagedEndpoint {
                rest_url: url.clone(),
                agui_url: url,
                rest_token: None,
                agui_token: None,
                ca_pem: None,
            })
        }
        fn stop(&self, _: &std::path::Path) {}
        fn set_mcp_access(&self, _: &std::path::Path, access: Option<McpAccess>) {
            *self.mcp.lock() = access;
        }
    }

    #[test]
    fn managed_service_follows_a_restarted_instance_and_registers_mcp() {
        let g = PathBuf::from("/g");
        let addr = health_server();
        let sup = Arc::new(MovingSupervisor {
            live: format!("http://{addr}"),
            calls: std::sync::atomic::AtomicUsize::new(0),
            mcp: Mutex::new(None),
        });
        let mut o = opts(settings(PandoMode::Managed, "http://127.0.0.1:1", &g), &g);
        o.mcp = Some(McpAccess::new(
            "http://127.0.0.1:9/mcp",
            "bit_secret_token_value",
        ));
        o.supervisor = Some(sup.clone());
        let svc = PandoService::start(o);
        let rx = svc.subscribe();
        assert!(wait_for(&rx, |s| matches!(s, PandoStatus::Connected { .. })).is_some());
        assert_eq!(
            sup.mcp.lock().as_ref().map(|m| m.url.clone()).as_deref(),
            Some("http://127.0.0.1:9/mcp")
        );
        assert!(
            svc.endpoints()
                .unwrap()
                .rest
                .base_url
                .contains(&addr.to_string())
        );
    }
}
