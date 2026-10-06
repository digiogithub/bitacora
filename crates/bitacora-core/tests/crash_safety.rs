//! Crash safety of the atomic writer (BIT-T-0321): a child process rewrites a file in a tight
//! loop and is killed at random moments; the target must always hold a complete old or a
//! complete new version, never a mix or a truncation.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::Path;
use std::process::Command;
use std::time::Duration;

use bitacora_core::editor::fsio::{atomic_write, cleanup_stale_tmp};

const SIZE: usize = 1 << 20;
const ENV: &str = "BITACORA_CRASH_TARGET";

/// Entry point of the child process (does nothing in a normal test run).
#[test]
fn crash_child() {
    let Ok(target) = std::env::var(ENV) else {
        return;
    };
    let target = Path::new(&target);
    let mut flip = false;
    loop {
        let byte = if flip { b'A' } else { b'B' };
        atomic_write(target, &vec![byte; SIZE]).expect("child write");
        flip = !flip;
    }
}

fn assert_whole(target: &Path) {
    let bytes = std::fs::read(target).expect("target exists");
    assert_eq!(bytes.len(), SIZE, "truncated or grown file");
    let first = bytes[0];
    assert!(first == b'A' || first == b'B', "unexpected content");
    assert!(bytes.iter().all(|b| *b == first), "mixed old/new content");
}

#[test]
fn killing_the_writer_mid_write_never_corrupts_the_target() {
    let dir = tempfile::tempdir().expect("tmp");
    let target = dir.path().join("page.md");
    atomic_write(&target, &vec![b'A'; SIZE]).expect("seed");
    let exe = std::env::current_exe().expect("exe");
    let mut seed: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut leftovers = 0;
    for _ in 0..40 {
        seed = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let delay = 5 + (seed >> 33) % 40;
        let mut child = Command::new(&exe)
            .args(["--exact", "crash_child", "--nocapture", "--test-threads=1"])
            .env(ENV, &target)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("spawn child");
        std::thread::sleep(Duration::from_millis(delay));
        child.kill().expect("kill");
        child.wait().expect("wait");
        assert_whole(&target);
        leftovers += cleanup_stale_tmp(dir.path());
    }
    // Whatever was interrupted left at most temp files, which start-up cleanup removes.
    let _ = leftovers;
    assert!(cleanup_stale_tmp(dir.path()) == 0);
    assert_whole(&target);
}
