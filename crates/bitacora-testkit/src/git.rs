//! Temporary git repositories for sync tests, driven by the system `git` binary.
//!
//! Repositories get repo-local identity so tests do not depend on the host's git config.

// Test helpers fail loudly by design.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::Path;
use std::process::{Command, Stdio};

/// `true` when a system `git` is runnable.
pub fn git_available() -> bool {
    Command::new("git")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Runs `git <args>` in `dir`, returning trimmed stdout.
///
/// # Panics
/// Panics when git cannot run or exits non-zero; this is a test helper.
pub fn git(dir: &Path, args: &[&str]) -> String {
    let out = git_raw(dir, args);
    assert!(
        out.status.success(),
        "git {args:?} failed in {}: {}",
        dir.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// Runs `git <args>` in `dir` and returns the raw output without asserting success.
pub fn git_raw(dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("LC_ALL", "C")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .stdin(Stdio::null())
        .output()
        .expect("failed to spawn git")
}

/// Creates a bare repository with `main` as its default branch.
pub fn init_bare(dir: &Path) {
    std::fs::create_dir_all(dir).expect("create bare dir");
    git(dir, &["init", "--bare", "-b", "main", "."]);
}

/// Creates a normal repository on `main` with a repo-local identity.
pub fn init_repo(dir: &Path) {
    std::fs::create_dir_all(dir).expect("create repo dir");
    git(dir, &["init", "-b", "main", "."]);
    set_identity(dir, "Test User", "test@example.com");
}

/// Sets repo-local `user.name` / `user.email`.
pub fn set_identity(dir: &Path, name: &str, email: &str) {
    git(dir, &["config", "--local", "user.name", name]);
    git(dir, &["config", "--local", "user.email", email]);
}

/// Writes `content` to `rel` (creating parent directories) without committing.
pub fn write_file(dir: &Path, rel: &str, content: &str) {
    let path = dir.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create parent");
    }
    std::fs::write(path, content).expect("write file");
}

/// Writes a file, stages everything and commits; returns the new commit id.
pub fn commit_file(dir: &Path, rel: &str, content: &str, message: &str) -> String {
    write_file(dir, rel, content);
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "-m", message]);
    git(dir, &["rev-parse", "HEAD"])
}

/// Clones `url` into `dest` with repo-local identity configured.
pub fn clone_to(url: &str, dest: &Path) {
    let parent = dest.parent().expect("dest has parent");
    std::fs::create_dir_all(parent).expect("create parent");
    git(parent, &["clone", "-q", url, &dest.to_string_lossy()]);
    set_identity(dest, "Test User", "test@example.com");
}
