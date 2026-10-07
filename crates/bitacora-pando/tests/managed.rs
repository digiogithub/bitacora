//! Integration tests of the managed Pando supervisor with a fake `pando` shell script (no real
//! binary needed): readiness, generated config, crash and restart, failed state, adoption of a
//! running instance, orphan reaping and no process left after stop.
#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use bitacora_pando::managed::{
    InstanceDir, ManagedOptions, ManagedState, ManagedSupervisor, ProbeOutcome, Prober, Timing,
};
use bitacora_pando::{McpAccess, Supervisor};
use pando::Token;

const MCP_SECRET: &str = "bit_mcp_secret_value_1234567890";
const API_TOKEN: &str = "api-token-abcdef123456";

/// Fake `pando`: `--version` answers, `serve` records its environment, optionally crashes, and
/// otherwise signals readiness through a file and sleeps (exec, so the pid is the process).
fn fake_pando(dir: &Path, version: &str) -> PathBuf {
    let path = dir.join("pando");
    let script = format!(
        r#"#!/bin/sh
case "$1" in
--version) echo "pando version {version}"; exit 0 ;;
serve)
  rm -f ready
  echo "$$" > fake.pid
  echo "$PANDO_CONFIG_PARENT_SEARCH" > env.txt
  pwd > cwd.txt
  echo "$@" > args.txt
  grep -m1 '^Token' .pando.toml
  echo "serving, api token {API_TOKEN}"
  if [ -f crash-once ]; then rm -f crash-once; echo "boom" >&2; exit 3; fi
  if [ -f always-fail ] && [ ! -f ok ]; then exit 1; fi
  touch ready
  sleep 0.3
  echo "late line {API_TOKEN}"
  exec sleep 300 ;;
esac
exit 2
"#
    );
    std::fs::write(&path, script).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

/// Healthy when the fake wrote `ready` into the instance directory.
#[derive(Debug)]
struct FileProber {
    dir: PathBuf,
}

impl Prober for FileProber {
    fn probe(
        &self,
        _rest_url: &str,
        _ca: Option<&Path>,
        want_token: bool,
    ) -> Result<ProbeOutcome, String> {
        if !self.dir.join("ready").exists() {
            return Err("not ready".into());
        }
        Ok(ProbeOutcome {
            api_token: want_token.then(|| Token::new(API_TOKEN)),
            ca_pem: None,
        })
    }
}

fn fast() -> Timing {
    Timing {
        port_wait: Duration::from_millis(100),
        ready_timeout: Duration::from_secs(10),
        health_interval: Duration::from_millis(100),
        health_failures: 2,
        stop_timeout: Duration::from_secs(5),
        backoff_min: Duration::from_millis(50),
        backoff_max: Duration::from_millis(200),
        max_crashes: 3,
        crash_window: Duration::from_secs(60),
        poll: Duration::from_millis(20),
    }
}

struct Env {
    _tmp: tempfile::TempDir,
    graph: PathBuf,
    cache: PathBuf,
    bin: PathBuf,
}

impl Env {
    fn new(version: &str) -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let graph = tmp.path().join("graph");
        std::fs::create_dir(&graph).unwrap();
        let bin = fake_pando(tmp.path(), version);
        let cache = tmp.path().join("cache").join("pando");
        Self {
            _tmp: tmp,
            graph,
            cache,
            bin,
        }
    }

    fn dir(&self) -> InstanceDir {
        InstanceDir::new(&self.cache, &self.graph)
    }

    fn supervisor(&self) -> ManagedSupervisor {
        let mut o = ManagedOptions::new(&self.bin, &self.cache);
        o.scheme = "http";
        o.ca_path = None;
        o.timing = fast();
        o.prober = Arc::new(FileProber {
            dir: self.dir().path().to_path_buf(),
        });
        ManagedSupervisor::new(o)
    }
}

fn wait_until(what: &str, mut f: impl FnMut() -> bool) {
    let end = Instant::now() + Duration::from_secs(15);
    while Instant::now() < end {
        if f() {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("timed out waiting for {what}");
}

fn pid_alive(pid: u32) -> bool {
    Path::new(&format!("/proc/{pid}")).exists()
        && !std::fs::read_to_string(format!("/proc/{pid}/stat"))
            .unwrap_or_default()
            .contains(") Z")
}

fn read_pid(dir: &InstanceDir) -> u32 {
    std::fs::read_to_string(dir.path().join("fake.pid"))
        .unwrap()
        .trim()
        .parse()
        .unwrap()
}

#[test]
fn starts_ready_with_generated_config_and_private_files() {
    let env = Env::new("v1.2.11");
    let sup = env.supervisor();
    sup.set_mcp_access(
        &env.graph,
        Some(McpAccess::new("http://127.0.0.1:7878/mcp", MCP_SECRET)),
    );
    let ep = sup.ensure_running(&env.graph).unwrap();
    assert!(ep.rest_url.starts_with("http://127.0.0.1:"), "{ep:?}");
    assert!(ep.agui_url.starts_with("http://127.0.0.1:"));
    assert_ne!(ep.rest_url, ep.agui_url);
    assert_eq!(ep.rest_token.as_ref().unwrap().expose(), API_TOKEN);
    assert_eq!(ep.agui_token.as_ref().unwrap().expose(), API_TOKEN);

    let dir = env.dir();
    let read = |f: &str| std::fs::read_to_string(dir.path().join(f)).unwrap();
    // The child runs in the instance dir with the parent search switched off.
    assert_eq!(read("env.txt").trim(), "false");
    assert_eq!(
        std::fs::canonicalize(read("cwd.txt").trim()).unwrap(),
        std::fs::canonicalize(dir.path()).unwrap()
    );
    let args = read("args.txt");
    assert!(args.contains("serve --host 127.0.0.1 --port "), "{args}");
    assert!(args.contains("--agui-port"));
    // Generated config: valid TOML with the MCP registration, no storage keys.
    let cfg: toml::Table = read(".pando.toml").parse().unwrap();
    assert_eq!(
        cfg["MCPServers"]["bitacora"]["Auth"]["Token"].as_str(),
        Some(MCP_SECRET)
    );
    assert!(cfg.get("Remembrances").is_none() && cfg.get("Data").is_none());
    assert!(dir.path().join("agents/personas/bitacora-chat.md").exists());
    // Private files and a redacted log.
    let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(dir.path()), 0o700);
    assert_eq!(mode(&dir.config_path()), 0o600);
    assert_eq!(mode(&dir.token_path()), 0o600);
    wait_until("log lines", || {
        std::fs::read_to_string(dir.log_path()).is_ok_and(|l| l.contains("late line"))
    });
    let log = std::fs::read_to_string(dir.log_path()).unwrap();
    // The MCP token is redacted from the start; the API token once it is known.
    assert!(!log.contains(MCP_SECRET), "{log}");
    assert!(log.contains("late line [redacted]"), "{log}");
    assert!(log.contains("[redacted]"));
    // state.json mirrors the status.
    let st = sup.managed_status(&env.graph).unwrap();
    assert_eq!(st.state, ManagedState::Ready);
    assert!(st.version.unwrap().contains("1.2.11"));
    let on_disk: bitacora_pando::ManagedStatus =
        serde_json::from_slice(&std::fs::read(dir.state_path()).unwrap()).unwrap();
    assert_eq!(on_disk.state, ManagedState::Ready);
    // Nothing was written inside the graph.
    assert_eq!(std::fs::read_dir(&env.graph).unwrap().count(), 0);
    assert!(sup.log_path(&env.graph).is_some());
    sup.stop(&env.graph);
}

#[test]
fn crash_restarts_with_backoff_and_becomes_ready_again() {
    let env = Env::new("1.2.11");
    let sup = env.supervisor();
    let dir = env.dir();
    dir.ensure().unwrap();
    std::fs::write(dir.path().join("crash-once"), "").unwrap();
    let ep = sup.ensure_running(&env.graph).unwrap();
    assert!(ep.rest_url.starts_with("http://"));
    let st = sup.managed_status(&env.graph).unwrap();
    assert_eq!(st.state, ManagedState::Ready);
    assert_eq!(st.crashes, 1);
    wait_until("crash output in the log", || {
        std::fs::read_to_string(dir.log_path()).is_ok_and(|l| l.contains("boom"))
    });
    sup.stop(&env.graph);
}

#[test]
fn too_many_crashes_mark_failed_until_an_explicit_restart() {
    let env = Env::new("1.2.11");
    let sup = env.supervisor();
    let dir = env.dir();
    dir.ensure().unwrap();
    std::fs::write(dir.path().join("always-fail"), "").unwrap();
    let err = sup.ensure_running(&env.graph).unwrap_err();
    assert!(err.contains("exited"), "{err}");
    let st = sup.managed_status(&env.graph).unwrap();
    assert_eq!(st.state, ManagedState::Failed);
    assert_eq!(st.crashes, 3);
    // Still failed after the backoff would have elapsed: no retry on its own.
    std::thread::sleep(Duration::from_millis(400));
    assert_eq!(
        sup.managed_status(&env.graph).unwrap().state,
        ManagedState::Failed
    );
    // Fix the cause and restart.
    std::fs::write(dir.path().join("ok"), "").unwrap();
    sup.restart(&env.graph);
    wait_until("ready after restart", || {
        sup.managed_status(&env.graph).unwrap().state == ManagedState::Ready
    });
    assert!(sup.ensure_running(&env.graph).is_ok());
    sup.stop(&env.graph);
}

#[test]
fn stop_and_drop_leave_no_process_behind() {
    let env = Env::new("1.2.11");
    let sup = env.supervisor();
    sup.ensure_running(&env.graph).unwrap();
    let pid = read_pid(&env.dir());
    assert!(pid_alive(pid));
    sup.stop(&env.graph);
    wait_until("child to end", || !pid_alive(pid));
    assert_eq!(
        sup.managed_status(&env.graph).unwrap().state,
        ManagedState::Stopped
    );
    // Stopping twice is fine, and a stopped instance can be started again.
    sup.stop(&env.graph);
    sup.ensure_running(&env.graph).unwrap();
    let pid2 = read_pid(&env.dir());
    drop(sup);
    wait_until("child to end after drop", || !pid_alive(pid2));
}

#[test]
fn old_or_missing_binary_is_reported_without_spawning_serve() {
    let env = Env::new("1.0.3");
    let sup = env.supervisor();
    let err = sup.ensure_running(&env.graph).unwrap_err();
    assert!(err.contains("older than the required"), "{err}");
    assert_eq!(
        sup.managed_status(&env.graph).unwrap().state,
        ManagedState::Failed
    );
    assert!(!env.dir().path().join("fake.pid").exists());

    let mut o = ManagedOptions::new("definitely-not-a-pando", &env.cache);
    o.timing = fast();
    let err = ManagedSupervisor::new(o)
        .ensure_running(&env.graph)
        .unwrap_err();
    assert!(err.contains("was not found"), "{err}");
}

#[test]
fn a_second_process_adopts_the_running_instance() {
    let env = Env::new("1.2.11");
    let a = env.supervisor();
    let ep_a = a.ensure_running(&env.graph).unwrap();
    // Same cache, same graph: the lock is held, so the second supervisor adopts.
    let b = env.supervisor();
    let ep_b = b.ensure_running(&env.graph).unwrap();
    assert_eq!(ep_a.rest_url, ep_b.rest_url);
    assert_eq!(ep_b.rest_token.as_ref().unwrap().expose(), API_TOKEN);
    // Stopping the adopter must not end the owner's child.
    b.stop(&env.graph);
    let pid = read_pid(&env.dir());
    assert!(pid_alive(pid));
    a.stop(&env.graph);
    wait_until("child to end", || !pid_alive(pid));
}

#[cfg(target_os = "linux")]
#[test]
fn an_orphan_of_a_dead_supervisor_is_ended_on_start() {
    use std::os::unix::process::CommandExt as _;
    let env = Env::new("1.2.11");
    let dir = env.dir();
    dir.ensure().unwrap();
    let mut orphan = std::process::Command::new("sleep")
        .arg("300")
        .current_dir(dir.path())
        .process_group(0)
        .spawn()
        .unwrap();
    let st = bitacora_pando::ManagedStatus {
        state: ManagedState::Ready,
        pid: Some(orphan.id()),
        supervisor_pid: Some(999_999),
        binary: Some("sleep".into()),
        ..bitacora_pando::ManagedStatus::default()
    };
    std::fs::write(dir.state_path(), serde_json::to_vec(&st).unwrap()).unwrap();
    let sup = env.supervisor();
    sup.ensure_running(&env.graph).unwrap();
    wait_until("orphan to be ended", || {
        orphan.try_wait().unwrap().is_some()
    });
    sup.stop(&env.graph);
}

#[test]
fn the_lifeline_watchdog_ends_its_child_when_the_supervisor_side_closes() {
    use std::fs::File;
    use std::os::fd::OwnedFd;
    let (reader, writer) = std::io::pipe().unwrap();
    let lifeline = File::from(OwnedFd::from(reader));
    let handle = std::thread::spawn(move || {
        bitacora_pando::managed::run_watchdog(lifeline, "sleep", &["300".to_owned()])
    });
    std::thread::sleep(Duration::from_millis(200));
    assert!(
        !handle.is_finished(),
        "the watchdog must keep running while the pipe is open"
    );
    drop(writer);
    wait_until("watchdog to end its child", || handle.is_finished());
    assert!(handle.join().unwrap().is_ok());
}

/// Optional smoke test against a real `pando` on `PATH` (`cargo test -p bitacora-pando --test
/// managed -- --ignored`). Uses an isolated `XDG_CONFIG_HOME`, so the user's own Pando
/// configuration, CA and database are untouched.
#[test]
#[ignore = "needs a real `pando` binary on PATH"]
fn real_pando_serves_https_with_its_private_ca_and_hands_out_its_token() {
    let tmp = tempfile::tempdir().unwrap();
    let graph = tmp.path().join("graph");
    std::fs::create_dir(&graph).unwrap();
    let cache = tmp.path().join("cache").join("pando");
    let xdg = tmp.path().join("xdg");
    std::fs::create_dir(&xdg).unwrap();
    let mut o = ManagedOptions::new("pando", &cache);
    o.ca_path = Some(xdg.join("pando").join("tls").join("ca.crt"));
    o.extra_env = vec![
        ("XDG_CONFIG_HOME".into(), xdg.display().to_string()),
        ("HOME".into(), tmp.path().display().to_string()),
    ];
    o.timing.ready_timeout = Duration::from_secs(60);
    let sup = ManagedSupervisor::new(o);
    sup.set_mcp_access(
        &graph,
        Some(McpAccess::new("http://127.0.0.1:1/mcp", MCP_SECRET)),
    );
    let ep = sup.ensure_running(&graph).unwrap();
    assert!(ep.rest_url.starts_with("https://127.0.0.1:"));
    assert!(ep.rest_token.is_some() && ep.ca_pem.is_some());
    // The AG-UI listener answers over the same private CA with the instance token, and the
    // generated profiles were accepted by Pando.
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let cfg = pando::PandoConfig::new(ep.rest_url.clone())
        .with_token(ep.rest_token.clone().unwrap())
        .with_root_certificate_pem(ep.ca_pem.clone().unwrap());
    let client = pando::PandoClient::new(cfg).unwrap();
    let agui = client.agui_with(
        pando::agui::AguiOptions::default()
            .with_base_url(ep.agui_url.clone())
            .with_token(ep.agui_token.clone().unwrap()),
    );
    let health = rt.block_on(agui.healthz()).unwrap();
    let info = rt.block_on(agui.info()).unwrap();
    eprintln!("agui info: {info:?}");
    assert_eq!(health.status, "ok");
    let dir = InstanceDir::new(&cache, &graph);
    eprintln!("instance dir listing:");
    for e in std::fs::read_dir(dir.path()).unwrap() {
        eprintln!("  {:?}", e.unwrap().file_name());
    }
    sup.stop(&graph);
}
