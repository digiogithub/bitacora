//! Runs the real `bitacora-askpass` helper binary against a loopback server, the way git does
//! (BIT-US-0176): environment from `AskpassServer::env`, prompt as argv[1], answer on stdout.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::process::Command;
use std::sync::Arc;

use bitacora_sync::askpass::{AskpassHandler, AskpassServer, ENV_MODE};

struct Fixed;
impl AskpassHandler for Fixed {
    fn answer(&self, prompt: &str) -> Option<String> {
        prompt
            .starts_with("Password")
            .then(|| "hunter2".to_string())
    }
}

fn run(server: &AskpassServer, prompt: &str) -> (bool, String) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_bitacora-askpass"));
    cmd.arg(prompt);
    for (k, v) in server.env() {
        cmd.env(k, v);
    }
    let out = cmd.output().expect("run helper");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).trim().to_string(),
    )
}

#[test]
fn helper_binary_roundtrips_through_the_server() {
    let server = AskpassServer::start(Arc::new(Fixed)).expect("server");
    assert!(server.env().iter().any(|(k, v)| k == ENV_MODE && v == "1"));
    assert_eq!(
        run(&server, "Password for 'https://u@h.example': "),
        (true, "hunter2".into())
    );
    // Cancelled prompt: non-zero exit, nothing on stdout.
    assert_eq!(
        run(&server, "Username for 'https://h.example': "),
        (false, String::new())
    );
}

#[test]
fn helper_without_environment_fails() {
    let out = Command::new(env!("CARGO_BIN_EXE_bitacora-askpass"))
        .arg("Password for 'x': ")
        .env_remove("BITACORA_ASKPASS_ADDR")
        .env_remove("BITACORA_ASKPASS_TOKEN")
        .output()
        .expect("run helper");
    assert!(!out.status.success());
}
