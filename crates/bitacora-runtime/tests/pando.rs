//! Pando service lifecycle inside a session (BIT-US-0135).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use bitacora_config::{PandoMode, PandoSettings};
use bitacora_pando::{ManagedEndpoint, PandoCredentials};
use bitacora_runtime::{PandoEvent, PandoOptions, PandoStatus, Session, Supervisor};

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
