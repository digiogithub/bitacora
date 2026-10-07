//! Opt-in end-to-end test with a real `pando` on `PATH` (BIT-T-0434, BIT-US-0141):
//! `cargo test -p bitacora-runtime --test pando_real -- --ignored --nocapture`.
//!
//! A session with the MCP server and managed Pando starts the real `pando serve` from a generated
//! config in an isolated `XDG_CONFIG_HOME` (the user's own Pando setup is untouched). Pando
//! connects to Bitacora's MCP server with the `pando` token at startup, so its tool list shows the
//! `bitacora_*` read tools and none of the write tools; it also hosts the four Bitacora AG-UI
//! profiles.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use bitacora_config::{PandoMode, PandoSettings};
use bitacora_pando::{ManagedOptions, ManagedSupervisor, PandoCredentials};
use bitacora_runtime::{McpOptions, PandoOptions, PandoStatus, Session};

fn curl_json(ca: &Path, url: &str, token: &str) -> String {
    let out = Command::new("curl")
        .args(["-s", "--cacert"])
        .arg(ca)
        .args(["-H", &format!("X-Pando-Token: {token}"), url])
        .output()
        .expect("curl");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
#[ignore = "needs a real `pando` binary and curl on PATH"]
fn real_pando_discovers_the_read_only_bitacora_tools() {
    let tmp = tempfile::tempdir().unwrap();
    let graph = common::graph_with(tmp.path(), &[("pages/a.md", common::PAGE)]);
    let xdg = tmp.path().join("xdg");
    std::fs::create_dir_all(&xdg).unwrap();
    let ca = xdg.join("pando").join("tls").join("ca.crt");

    let mut mo = ManagedOptions::new("pando", tmp.path().join("cache").join("pando"));
    mo.ca_path = Some(ca.clone());
    mo.extra_env = vec![
        ("XDG_CONFIG_HOME".into(), xdg.display().to_string()),
        ("HOME".into(), tmp.path().display().to_string()),
    ];
    mo.timing.ready_timeout = Duration::from_secs(60);
    let supervisor = Arc::new(ManagedSupervisor::new(mo));

    let canonical = std::fs::canonicalize(&graph).unwrap();
    let mut settings = PandoSettings {
        enabled: true,
        mode: PandoMode::Managed,
        ..PandoSettings::default()
    };
    settings.grant_consent(&canonical.to_string_lossy(), 1);
    let mut po = PandoOptions::new(settings, canonical);
    po.credentials = PandoCredentials::new(None, |_| None);
    po.supervisor = Some(supervisor);

    let mut cfg = common::config(&graph, &tmp.path().join("data"));
    cfg.mcp = Some(McpOptions {
        config: bitacora_mcp::McpConfig {
            port: 0,
            stateful: false,
            ..bitacora_mcp::McpConfig::default()
        },
        token_path: tmp.path().join("tokens.json"),
        secrets: None,
    });
    cfg.pando = Some(po);
    let s = Session::open(cfg).unwrap();

    let pando = s.pando().unwrap();
    common::wait_for("pando connected", Duration::from_secs(90), || {
        matches!(s.pando_status(), PandoStatus::Connected { .. }).then_some(())
    });
    let ep = pando.endpoints().unwrap();
    let token = ep.rest.token.as_ref().unwrap().expose().to_owned();
    let tools = curl_json(&ca, &format!("{}/api/v1/tools", ep.rest.base_url), &token);
    eprintln!("pando tools: {tools}");
    for want in ["bitacora_get_page", "bitacora_search", "bitacora_backlinks"] {
        assert!(tools.contains(want), "{want} not discovered: {tools}");
    }
    for forbidden in ["bitacora_append_block", "bitacora_delete_page"] {
        assert!(!tools.contains(forbidden), "{forbidden} exposed: {tools}");
    }
    s.shutdown(Duration::from_secs(10));
}
