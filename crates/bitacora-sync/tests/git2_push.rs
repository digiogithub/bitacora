//! ADR-023: the gix-only backend pushes through libgit2.
#![cfg(feature = "git2-push")]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use bitacora_sync::backend::GitError;
use bitacora_sync::credentials::{
    ChainProvider, Credential, CredentialRequest, MemoryStore, PromptAnswer, PromptHandler, Secret,
};
use bitacora_sync::{GitBackend, GixBackend};
use bitacora_testkit::git::{clone_to, commit_file, git, init_bare, init_repo};

fn seeded(tmp: &std::path::Path) -> (std::path::PathBuf, std::path::PathBuf) {
    let remote = tmp.join("remote.git");
    init_bare(&remote);
    let a = tmp.join("a");
    init_repo(&a);
    git(&a, &["remote", "add", "origin", remote.to_str().unwrap()]);
    commit_file(&a, "pages/a.md", "- hello\n", "seed");
    (remote, a)
}

#[test]
fn libgit2_push_to_bare_remote() {
    let tmp = tempfile::tempdir().unwrap();
    let (remote, a) = seeded(tmp.path());
    let backend = GixBackend::open(&a).unwrap();
    assert!(backend.push_via_libgit2("origin", "main").unwrap().pushed);
    let head = git(&a, &["rev-parse", "HEAD"]);
    assert_eq!(git(&remote, &["rev-parse", "refs/heads/main"]), head);
    assert_eq!(git(&a, &["rev-parse", "refs/remotes/origin/main"]), head);
    // Second push is a no-op.
    assert!(!backend.push_via_libgit2("origin", "main").unwrap().pushed);
    // A new commit goes through as a fast-forward.
    let h2 = commit_file(&a, "pages/b.md", "- b\n", "second");
    assert!(backend.push_via_libgit2("origin", "main").unwrap().pushed);
    assert_eq!(git(&remote, &["rev-parse", "refs/heads/main"]), h2);
}

#[test]
fn libgit2_push_is_never_forced() {
    let tmp = tempfile::tempdir().unwrap();
    let (remote, a) = seeded(tmp.path());
    git(&a, &["push", "-q", "origin", "main"]);
    let b = tmp.path().join("b");
    clone_to(remote.to_str().unwrap(), &b);
    commit_file(&b, "pages/b.md", "- b\n", "b");
    git(&b, &["push", "-q", "origin", "main"]);
    let before = git(&remote, &["rev-parse", "refs/heads/main"]);
    commit_file(&a, "pages/a2.md", "- a2\n", "a2");
    let err = GixBackend::open(&a)
        .unwrap()
        .push_via_libgit2("origin", "main")
        .unwrap_err();
    assert!(matches!(err, GitError::NonFastForward), "{err:?}");
    assert_eq!(git(&remote, &["rev-parse", "refs/heads/main"]), before);
}

struct Scripted(AtomicUsize);

impl PromptHandler for Scripted {
    fn prompt(&self, _: &CredentialRequest) -> Option<PromptAnswer> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Some(PromptAnswer {
            credential: Credential {
                username: "alice".into(),
                secret: Secret::new("tok3n"),
            },
            remember: false,
        })
    }
}

/// An HTTP server that always answers 401 and records the `Authorization` headers it sees.
#[test]
fn https_push_asks_the_credential_provider_and_reports_auth_failure() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let seen: Arc<Mutex<Vec<String>>> = Arc::default();
    let stop = Arc::new(AtomicBool::new(false));
    let server = {
        let (seen, stop) = (seen.clone(), stop.clone());
        thread::spawn(move || {
            for stream in listener.incoming() {
                if stop.load(Ordering::SeqCst) {
                    break;
                }
                let Ok(mut s) = stream else { continue };
                let mut buf = vec![0u8; 8192];
                let n = s.read(&mut buf).unwrap_or(0);
                let text = String::from_utf8_lossy(&buf[..n]).to_string();
                for line in text.lines() {
                    if line.to_ascii_lowercase().starts_with("authorization:") {
                        seen.lock().unwrap().push(line.to_string());
                    }
                }
                let _ = s.write_all(
                    b"HTTP/1.1 401 Unauthorized\r\nWWW-Authenticate: Basic realm=\"t\"\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                );
            }
        })
    };

    let tmp = tempfile::tempdir().unwrap();
    let (_, a) = seeded(tmp.path());
    git(
        &a,
        &[
            "remote",
            "set-url",
            "origin",
            &format!("http://127.0.0.1:{port}/r.git"),
        ],
    );
    let prompts = Arc::new(Scripted(AtomicUsize::new(0)));
    let provider = Arc::new(ChainProvider::new(
        Arc::new(MemoryStore::new()),
        Some(prompts.clone()),
    ));
    let backend = GixBackend::open(&a).unwrap().with_credentials(provider);
    let err = backend.push("origin", "main").unwrap_err();
    assert!(matches!(err, GitError::Auth { .. }), "{err:?}");
    assert!(prompts.0.load(Ordering::SeqCst) >= 1);
    let headers = seen.lock().unwrap().clone();
    // base64("alice:tok3n")
    assert!(
        headers.iter().any(|h| h.contains("YWxpY2U6dG9rM24=")),
        "{headers:?}"
    );

    stop.store(true, Ordering::SeqCst);
    let _ = std::net::TcpStream::connect(("127.0.0.1", port));
    server.join().unwrap();
}

#[test]
fn unreachable_https_remote_is_a_network_error() {
    let tmp = tempfile::tempdir().unwrap();
    let (_, a) = seeded(tmp.path());
    git(
        &a,
        &["remote", "set-url", "origin", "http://127.0.0.1:1/r.git"],
    );
    let err = GixBackend::open(&a)
        .unwrap()
        .push("origin", "main")
        .unwrap_err();
    assert!(matches!(err, GitError::Network(_)), "{err:?}");
}
