//! `bitacora-cli sync --graph`: one cycle through the runtime against a temp bare remote.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::process::Command;

use bitacora_sync::detect_git;
use bitacora_sync::onboarding::{OnboardingConfig, enable_sync};
use bitacora_sync::repo_setup::Identity;
use bitacora_testkit::git::{git, git_available, init_bare};

#[test]
fn sync_commits_and_pushes_local_edits() {
    assert!(git_available());
    let tmp = tempfile::tempdir().unwrap();
    let remote = tmp.path().join("remote.git");
    init_bare(&remote);
    let graph = tmp.path().join("graph");
    std::fs::create_dir_all(graph.join("pages")).unwrap();
    std::fs::write(graph.join("pages/p.md"), "- one\n").unwrap();
    let mut cfg = OnboardingConfig::new(detect_git(None));
    cfg.identity = Some(Identity {
        name: "cli".into(),
        email: "cli@example.com".into(),
    });
    enable_sync(&graph, &remote.to_string_lossy(), "main", &cfg).unwrap();

    std::fs::write(graph.join("pages/p.md"), "- one\n- two\n").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_bitacora-cli"))
        .args(["sync", "--graph"])
        .arg(&graph)
        .arg("--data-dir")
        .arg(tmp.path().join("data"))
        .args(["--device", "cli-test"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        git(&graph, &["rev-parse", "HEAD"]),
        git(&remote, &["rev-parse", "main"])
    );
    assert_eq!(git(&remote, &["show", "main:pages/p.md"]), "- one\n- two");
}
