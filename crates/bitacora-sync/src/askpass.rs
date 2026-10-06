//! In-app askpass bridge (BIT-US-0046, BIT-T-0290).
//!
//! The system-git backend runs git with `GIT_ASKPASS` / `SSH_ASKPASS` pointing at a tiny helper
//! (`bitacora-askpass`, or any binary calling [`helper_main`]). The helper forwards git's prompt
//! to the running app through a loopback TCP channel guarded by a random per-session token and
//! prints the answer on stdout. The app side is an [`AskpassServer`] whose [`AskpassHandler`]
//! usually is a [`ProviderAskpass`] over the credential provider chain (keyring, then UI modal).
//!
//! Wire protocol (one connection per prompt, single lines, UTF-8):
//! request `"<token>\t<prompt>\n"`, reply `"OK\t<answer>\n"` or `"NO\n"` (cancelled / denied).
//! Tabs and newlines inside the prompt/answer are not representable and are replaced by spaces.
//! The channel binds to `127.0.0.1` only. Secrets are never logged.

use std::collections::HashMap;
use std::io::{self, BufRead, BufReader, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::credentials::{CredentialKind, CredentialProvider, CredentialRequest};

/// Environment variable with the server address (`127.0.0.1:port`).
pub const ENV_ADDR: &str = "BITACORA_ASKPASS_ADDR";
/// Environment variable with the session token.
pub const ENV_TOKEN: &str = "BITACORA_ASKPASS_TOKEN";

/// Answers askpass prompts. Returns `None` to cancel.
pub trait AskpassHandler: Send + Sync {
    /// Answers git's/ssh's prompt text (for example `Password for 'https://u@host': `).
    fn answer(&self, prompt: &str) -> Option<String>;
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| {
            if matches!(c, '\n' | '\r' | '\t') {
                ' '
            } else {
                c
            }
        })
        .collect()
}

fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Loopback server answering askpass helper requests; stops when dropped.
pub struct AskpassServer {
    addr: SocketAddr,
    token: String,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for AskpassServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AskpassServer")
            .field("addr", &self.addr)
            .finish_non_exhaustive()
    }
}

impl AskpassServer {
    /// Binds `127.0.0.1:0` and starts serving prompts with `handler`.
    pub fn start(handler: Arc<dyn AskpassHandler>) -> io::Result<Self> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
        let addr = listener.local_addr()?;
        let token = format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        );
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let (stop, token, handler) = (stop.clone(), token.clone(), handler);
            thread::Builder::new()
                .name("bitacora-askpass".into())
                .spawn(move || serve(&listener, &stop, &token, &*handler))?
        };
        Ok(Self {
            addr,
            token,
            stop,
            thread: Some(thread),
        })
    }

    /// Address the helper must connect to.
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Environment variables for git so its askpass helper finds this server.
    pub fn env(&self) -> Vec<(String, String)> {
        vec![
            (ENV_ADDR.to_string(), self.addr.to_string()),
            (ENV_TOKEN.to_string(), self.token.clone()),
        ]
    }
}

impl Drop for AskpassServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        // Wake the blocking accept.
        let _ = TcpStream::connect_timeout(&self.addr, Duration::from_millis(500));
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn serve(listener: &TcpListener, stop: &AtomicBool, token: &str, handler: &dyn AskpassHandler) {
    for stream in listener.incoming() {
        if stop.load(Ordering::SeqCst) {
            break;
        }
        let Ok(stream) = stream else { continue };
        // Prompts block on the user; one connection at a time is enough (git asks serially).
        let _ = handle(stream, token, handler);
    }
}

fn handle(stream: TcpStream, token: &str, handler: &dyn AskpassHandler) -> io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    let mut line = String::new();
    BufReader::new(&stream).take_line(&mut line)?;
    let line = line.trim_end_matches(['\n', '\r']);
    let reply = match line.split_once('\t') {
        Some((t, prompt)) if ct_eq(t.as_bytes(), token.as_bytes()) => handler
            .answer(prompt)
            .map(|a| format!("OK\t{}\n", sanitize(&a))),
        _ => None,
    };
    let mut stream = stream;
    stream.write_all(reply.as_deref().unwrap_or("NO\n").as_bytes())
}

/// Bounded line read (a hostile local client cannot make us buffer unbounded data).
trait TakeLine {
    fn take_line(&mut self, out: &mut String) -> io::Result<()>;
}

impl<R: BufRead> TakeLine for R {
    fn take_line(&mut self, out: &mut String) -> io::Result<()> {
        use std::io::Read;
        let mut limited = self.by_ref().take(16 * 1024);
        limited.read_line(out).map(|_| ())
    }
}

/// Client side: forwards `prompt` to the server and returns the answer (`None` = cancelled).
pub fn request_answer(addr: &str, token: &str, prompt: &str) -> io::Result<Option<String>> {
    let sock: SocketAddr = addr
        .parse()
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "bad askpass address"))?;
    if !sock.ip().is_loopback() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "askpass address must be loopback",
        ));
    }
    let mut stream = TcpStream::connect_timeout(&sock, Duration::from_secs(5))?;
    // The user may take minutes to type a password.
    stream.set_read_timeout(Some(Duration::from_secs(600)))?;
    stream.write_all(format!("{token}\t{}\n", sanitize(prompt)).as_bytes())?;
    let mut reply = String::new();
    BufReader::new(&stream).read_line(&mut reply)?;
    Ok(reply
        .trim_end_matches(['\n', '\r'])
        .strip_prefix("OK\t")
        .map(str::to_string))
}

/// Entry point for the askpass helper executable (`bitacora-askpass <prompt>`; also usable as a
/// `bitacora-cli askpass` subcommand). Prints the answer on stdout; exit code 1 on cancel/error.
pub fn helper_main() -> ExitCode {
    let prompt = std::env::args().nth(1).unwrap_or_default();
    let (Ok(addr), Ok(token)) = (std::env::var(ENV_ADDR), std::env::var(ENV_TOKEN)) else {
        eprintln!("bitacora-askpass: not started by Bitacora ({ENV_ADDR} unset)");
        return ExitCode::from(1);
    };
    match request_answer(&addr, &token, &prompt) {
        Ok(Some(answer)) => {
            println!("{answer}");
            ExitCode::SUCCESS
        }
        Ok(None) => ExitCode::from(1),
        Err(e) => {
            eprintln!("bitacora-askpass: {e}");
            ExitCode::from(1)
        }
    }
}

/// Parsed git/ssh prompt.
#[derive(Debug, PartialEq, Eq)]
enum Parsed {
    Username {
        url: String,
    },
    Password {
        url: String,
        username: Option<String>,
    },
    Passphrase {
        key: String,
    },
}

fn quoted(prompt: &str) -> Option<&str> {
    let start = prompt.find('\'')? + 1;
    let end = prompt.rfind('\'')?;
    (end >= start).then(|| &prompt[start..end])
}

/// Splits `scheme://user@host/path` into (url without userinfo, user).
fn split_userinfo(url: &str) -> (String, Option<String>) {
    if let Some((scheme, rest)) = url.split_once("://") {
        let host_end = rest.find('/').unwrap_or(rest.len());
        if let Some(at) = rest[..host_end].rfind('@') {
            let user = rest[..at].split(':').next().unwrap_or("").to_string();
            return (format!("{scheme}://{}", &rest[at + 1..]), Some(user));
        }
    }
    (url.to_string(), None)
}

fn parse_prompt(prompt: &str) -> Option<Parsed> {
    let lower = prompt.to_ascii_lowercase();
    if lower.starts_with("username for") {
        Some(Parsed::Username {
            url: split_userinfo(quoted(prompt)?).0,
        })
    } else if lower.starts_with("password for") {
        let (url, username) = split_userinfo(quoted(prompt)?);
        Some(Parsed::Password { url, username })
    } else if lower.contains("passphrase") {
        Some(Parsed::Passphrase {
            key: quoted(prompt).unwrap_or("ssh-key").to_string(),
        })
    } else {
        None
    }
}

/// [`AskpassHandler`] backed by the credential provider chain. Because git asks for the username
/// and the password in two separate calls, the credential resolved for the username call is held
/// briefly and reused for the password call so the user is prompted once.
pub struct ProviderAskpass {
    provider: Arc<dyn CredentialProvider>,
    pending: Mutex<HashMap<String, crate::credentials::Credential>>,
}

impl std::fmt::Debug for ProviderAskpass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderAskpass").finish_non_exhaustive()
    }
}

impl ProviderAskpass {
    /// Wraps `provider`.
    pub fn new(provider: Arc<dyn CredentialProvider>) -> Self {
        Self {
            provider,
            pending: Mutex::new(HashMap::new()),
        }
    }
}

impl AskpassHandler for ProviderAskpass {
    fn answer(&self, prompt: &str) -> Option<String> {
        let mut pending = self.pending.lock().unwrap_or_else(PoisonError::into_inner);
        match parse_prompt(prompt)? {
            Parsed::Username { url } => {
                let cred = self.provider.credential(&CredentialRequest {
                    url: url.clone(),
                    username: None,
                    kind: CredentialKind::UserPassword,
                })?;
                let user = cred.username.clone();
                pending.insert(url, cred);
                Some(user)
            }
            Parsed::Password { url, username } => {
                if let Some(cred) = pending.remove(&url) {
                    return Some(cred.secret.expose().to_string());
                }
                let cred = self.provider.credential(&CredentialRequest {
                    url,
                    username,
                    kind: CredentialKind::UserPassword,
                })?;
                Some(cred.secret.expose().to_string())
            }
            Parsed::Passphrase { key } => {
                let cred = self.provider.credential(&CredentialRequest {
                    url: key,
                    username: None,
                    kind: CredentialKind::SshPassphrase,
                })?;
                Some(cred.secret.expose().to_string())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::credentials::{
        ChainProvider, Credential, MemoryStore, PromptAnswer, PromptHandler, Secret,
    };

    #[test]
    fn prompt_parsing() {
        assert_eq!(
            parse_prompt("Username for 'https://github.com': "),
            Some(Parsed::Username {
                url: "https://github.com".into()
            })
        );
        assert_eq!(
            parse_prompt("Password for 'https://me@github.com': "),
            Some(Parsed::Password {
                url: "https://github.com".into(),
                username: Some("me".into())
            })
        );
        assert_eq!(
            parse_prompt("Enter passphrase for key '/home/u/.ssh/id_ed25519': "),
            Some(Parsed::Passphrase {
                key: "/home/u/.ssh/id_ed25519".into()
            })
        );
        assert_eq!(parse_prompt("Are you sure (yes/no)?"), None);
    }

    struct Once(std::sync::atomic::AtomicUsize);
    impl PromptHandler for Once {
        fn prompt(&self, r: &CredentialRequest) -> Option<PromptAnswer> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Some(PromptAnswer {
                credential: Credential {
                    username: r.username.clone().unwrap_or_else(|| "alice".into()),
                    secret: Secret::new("s3cret"),
                },
                remember: false,
            })
        }
    }

    #[test]
    fn end_to_end_username_then_password_prompts_once() {
        let prompts = Arc::new(Once(Default::default()));
        let chain = ChainProvider::new(Arc::new(MemoryStore::new()), Some(prompts.clone()));
        let server = AskpassServer::start(Arc::new(ProviderAskpass::new(Arc::new(chain)))).unwrap();
        let (addr, token) = (server.addr().to_string(), server.env()[1].1.clone());
        let user = request_answer(&addr, &token, "Username for 'https://h.example': ").unwrap();
        assert_eq!(user.as_deref(), Some("alice"));
        let pw = request_answer(&addr, &token, "Password for 'https://alice@h.example': ").unwrap();
        assert_eq!(pw.as_deref(), Some("s3cret"));
        assert_eq!(prompts.0.load(Ordering::SeqCst), 1);
        // Unknown prompt and wrong token are refused.
        assert_eq!(request_answer(&addr, &token, "Continue?").unwrap(), None);
        assert_eq!(
            request_answer(&addr, "wrong", "Username for 'https://h.example': ").unwrap(),
            None
        );
    }

    #[test]
    fn non_loopback_address_rejected() {
        assert!(request_answer("10.0.0.1:80", "t", "p").is_err());
    }
}
