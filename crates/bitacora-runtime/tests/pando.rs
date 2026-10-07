//! Pando service lifecycle inside a session (BIT-US-0135).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use bitacora_config::{PandoMode, PandoSettings};
use bitacora_pando::{ManagedEndpoint, McpAccess, PandoCredentials};
use bitacora_runtime::{McpOptions, PandoEvent, PandoOptions, PandoStatus, Session, Supervisor};

#[derive(Debug)]
struct Failing {
    stopped: Arc<AtomicBool>,
}

impl Supervisor for Failing {
    fn ensure_running(&self, _: &Path) -> Result<ManagedEndpoint, String> {
        Err("pando binary not found".into())
    }

    fn stop(&self, _: &Path) {
        self.stopped.store(true, Ordering::SeqCst);
    }
}

fn pando_opts(graph: &Path, active: bool, supervisor: Option<Arc<dyn Supervisor>>) -> PandoOptions {
    let canonical = std::fs::canonicalize(graph).unwrap();
    let mut settings = PandoSettings {
        enabled: active,
        mode: PandoMode::Managed,
        ..PandoSettings::default()
    };
    settings.grant_consent(&canonical.to_string_lossy(), 1);
    let mut o = PandoOptions::new(settings, canonical);
    o.credentials = PandoCredentials::new(None, |_| None);
    o.supervisor = supervisor;
    o
}

#[test]
fn session_without_pando_reports_off() {
    let dir = tempfile::tempdir().unwrap();
    let graph = common::graph_with(dir.path(), &[("pages/a.md", common::PAGE)]);
    let s = Session::open(common::config(&graph, &dir.path().join("data"))).unwrap();
    assert_eq!(s.pando_status(), PandoStatus::Off);
    assert!(s.pando().is_none() && s.pando_events().is_none());
    s.shutdown(Duration::from_secs(5));
}

#[test]
fn managed_failure_does_not_fail_open_and_shutdown_stops_the_supervisor() {
    let dir = tempfile::tempdir().unwrap();
    let graph = common::graph_with(dir.path(), &[("pages/a.md", common::PAGE)]);
    let stopped = Arc::new(AtomicBool::new(false));
    let mut cfg = common::config(&graph, &dir.path().join("data"));
    cfg.pando = Some(pando_opts(
        &graph,
        true,
        Some(Arc::new(Failing {
            stopped: Arc::clone(&stopped),
        })),
    ));
    let s = Session::open(cfg).unwrap();
    let rx = s.pando_events().unwrap();
    let status = common::wait_for("unavailable status", Duration::from_secs(10), || {
        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(PandoEvent::Status(st @ PandoStatus::Unavailable { .. })) => Some(st),
            _ => None,
        }
    });
    assert!(matches!(status, PandoStatus::Unavailable { reason } if reason.contains("not found")));
    let report = s.shutdown(Duration::from_secs(5));
    assert!(report.timed_out.is_empty(), "{:?}", report.timed_out);
    assert!(stopped.load(Ordering::SeqCst), "supervisor was stopped");
}

#[test]
fn disabled_settings_start_no_service_work() {
    let dir = tempfile::tempdir().unwrap();
    let graph = common::graph_with(dir.path(), &[("pages/a.md", common::PAGE)]);
    let mut cfg = common::config(&graph, &dir.path().join("data"));
    cfg.pando = Some(pando_opts(&graph, false, None));
    let s = Session::open(cfg).unwrap();
    assert_eq!(s.pando_status(), PandoStatus::Off);
    assert!(s.pando().unwrap().handle().is_none());
    s.shutdown(Duration::from_secs(5));
}

/// Records what the session hands to the supervisor.
#[derive(Debug, Default)]
struct Recording {
    access: std::sync::Mutex<Option<McpAccess>>,
}

#[derive(Debug)]
struct Shared(Arc<Recording>);

impl Supervisor for Shared {
    fn ensure_running(&self, _: &Path) -> Result<ManagedEndpoint, String> {
        Err("recording only".into())
    }
    fn stop(&self, _: &Path) {}
    fn set_mcp_access(&self, _: &Path, access: Option<McpAccess>) {
        *self.0.access.lock().unwrap() = access;
    }
}

fn open_with_mcp(
    dir: &Path,
    active: bool,
    agent_writes: bool,
    rec: &Arc<Recording>,
) -> (Session, std::path::PathBuf) {
    let graph = common::graph_with(dir, &[("pages/a.md", common::PAGE)]);
    let mut cfg = common::config(&graph, &dir.join("data"));
    let token_path = dir.join("tokens.json");
    cfg.mcp = Some(McpOptions {
        config: bitacora_mcp::McpConfig {
            port: 0,
            stateful: false,
            ..bitacora_mcp::McpConfig::default()
        },
        token_path: token_path.clone(),
        secrets: None,
    });
    let mut o = pando_opts(&graph, active, Some(Arc::new(Shared(Arc::clone(rec)))));
    let key = o.graph_key();
    if let Some(c) = o.settings.graphs.get_mut(&key) {
        c.agent_writes = agent_writes;
        c.exclusions = vec!["Private".into()];
    }
    cfg.pando = Some(o);
    (Session::open(cfg).unwrap(), token_path)
}

#[test]
fn session_mints_a_read_only_pando_token_and_registers_the_endpoint() {
    let dir = tempfile::tempdir().unwrap();
    let rec = Arc::new(Recording::default());
    let (s, token_path) = open_with_mcp(dir.path(), true, false, &rec);
    let tokens = s.mcp_tokens().unwrap();
    let infos = tokens.list();
    let pando = infos
        .iter()
        .find(|t| t.name == bitacora_mcp::PANDO_TOKEN_NAME)
        .expect("pando token minted");
    assert_eq!(pando.scopes, vec![bitacora_mcp::Scope::Read]);
    // The supervisor was told where the MCP server is and which secret to use.
    let access = common::wait_for("mcp access", Duration::from_secs(10), || {
        rec.access.lock().unwrap().clone()
    });
    assert_eq!(access.url, s.mcp_endpoint().unwrap());
    assert_eq!(
        Some(access.token_str().to_owned()),
        tokens.secret_of(bitacora_mcp::PANDO_TOKEN_NAME)
    );
    s.shutdown(Duration::from_secs(5));
    assert!(token_path.exists());
}

#[test]
fn write_grant_adds_the_write_scope_and_an_inactive_integration_mints_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let rec = Arc::new(Recording::default());
    let (s, _) = open_with_mcp(dir.path(), true, true, &rec);
    let t = s.mcp_tokens().unwrap();
    let scopes = t
        .list()
        .into_iter()
        .find(|t| t.name == bitacora_mcp::PANDO_TOKEN_NAME)
        .unwrap()
        .scopes;
    assert_eq!(
        scopes,
        vec![bitacora_mcp::Scope::Read, bitacora_mcp::Scope::Write]
    );
    s.shutdown(Duration::from_secs(5));

    let dir = tempfile::tempdir().unwrap();
    let rec = Arc::new(Recording::default());
    let (s, _) = open_with_mcp(dir.path(), false, false, &rec);
    assert!(
        s.mcp_tokens()
            .unwrap()
            .list()
            .iter()
            .all(|t| t.name != bitacora_mcp::PANDO_TOKEN_NAME)
    );
    assert!(rec.access.lock().unwrap().is_none());
    s.shutdown(Duration::from_secs(5));
}
