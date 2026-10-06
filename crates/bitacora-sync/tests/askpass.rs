//! The askpass bridge end to end: system git -> `bitacora-askpass` helper -> loopback server ->
//! credential provider (BIT-T-0290, BIT-T-0374).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use bitacora_sync::askpass::{AskpassServer, ProviderAskpass};
use bitacora_sync::credentials::{
    ChainProvider, Credential, CredentialRequest, MemoryStore, PromptAnswer, PromptHandler, Secret,
};
use bitacora_sync::{CliBackend, CliConfig, GitDetection, detect_git};
use bitacora_testkit::git::{git, init_repo};

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

#[test]
fn system_git_prompts_through_the_app() {
    let GitDetection::Found { path, .. } = detect_git(None) else {
        eprintln!("skipped: no system git");
        return;
    };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let seen: Arc<Mutex<Vec<String>>> = Arc::default();
    let stop = Arc::new(AtomicBool::new(false));
    let http = {
        let (seen, stop) = (seen.clone(), stop.clone());
        thread::spawn(move || {
            for stream in listener.incoming() {
                if stop.load(Ordering::SeqCst) {
                    break;
                }
                let Ok(mut s) = stream else { continue };
                let mut buf = vec![0u8; 8192];
                let n = s.read(&mut buf).unwrap_or(0);
                for line in String::from_utf8_lossy(&buf[..n]).lines() {
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

    let prompts = Arc::new(Scripted(AtomicUsize::new(0)));
    let chain = ChainProvider::new(Arc::new(MemoryStore::new()), Some(prompts.clone()));
    let server = AskpassServer::start(Arc::new(ProviderAskpass::new(Arc::new(chain)))).unwrap();

    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("r");
    init_repo(&repo);
    git(
        &repo,
        &[
            "remote",
            "add",
            "origin",
            &format!("http://127.0.0.1:{port}/r.git"),
        ],
    );
    let cli = CliBackend::new(
        &repo,
        path,
        CliConfig {
            askpass: Some(PathBuf::from(env!("CARGO_BIN_EXE_bitacora-askpass"))),
            askpass_env: server.env(),
            ..CliConfig::default()
        },
    );
    // The server always answers 401, so the call fails, but git must have asked the app once and
    // sent the credentials.
    assert!(cli.ls_remote("origin", "main").is_err());
    assert_eq!(prompts.0.load(Ordering::SeqCst), 1);
    let headers = seen.lock().unwrap().clone();
    assert!(
        headers.iter().any(|h| h.contains("YWxpY2U6dG9rM24=")),
        "{headers:?}"
    );

    stop.store(true, Ordering::SeqCst);
    let _ = std::net::TcpStream::connect(("127.0.0.1", port));
    http.join().unwrap();
}
