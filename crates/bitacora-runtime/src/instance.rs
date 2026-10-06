//! Per-user single-instance lock and local IPC (BIT-US-0086).
//!
//! One process per user owns the graph and the MCP port: the desktop app or a headless
//! `bitacora-cli serve`. The owner holds an exclusive OS lock on `instance.lock` (released by the
//! kernel when the process dies, so a crash never leaves a stale lock) and publishes a sidecar
//! `instance.json` with its pid, kind and a loopback IPC endpoint. A second desktop launch
//! forwards its arguments to that endpoint and exits; a conflicting headless/desktop pair
//! refuses to start with a clear message.
//!
//! The IPC channel is a TCP listener on `127.0.0.1` with an ephemeral port and a random bearer
//! token stored in the sidecar (user-private data directory); this works the same on every OS
//! without named-pipe or socket-path special cases. The sidecar doubles as the "previous
//! session ended abnormally" marker (BIT-US-0111): a clean shutdown removes it, so finding one
//! while the lock is free means the last owner crashed.

use std::fs::{File, OpenOptions};
use std::io::{BufRead as _, BufReader, Read as _, Write as _};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Lock file name inside the instance directory.
pub const LOCK_FILE: &str = "instance.lock";
/// Sidecar file name inside the instance directory.
pub const INFO_FILE: &str = "instance.json";
/// How long a client waits for the owner's answer.
const IPC_TIMEOUT: Duration = Duration::from_secs(3);
/// How long a client waits for a just-started owner to publish its sidecar.
const SIDECAR_WAIT: Duration = Duration::from_secs(2);

/// What kind of process owns the instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InstanceKind {
    /// The desktop app.
    App,
    /// `bitacora-cli serve`.
    Headless,
}

/// What the owner publishes in `instance.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstanceInfo {
    /// Process id of the owner.
    pub pid: u32,
    /// App or headless server.
    pub kind: InstanceKind,
    /// Owner's version.
    pub version: String,
    /// Loopback IPC port (0 = the owner does not accept forwarded launches).
    pub ipc_port: u16,
    /// Bearer token for the IPC channel.
    pub token: String,
}

/// A launch request forwarded to the running instance.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Launch {
    /// Graph folder to open.
    pub graph: Option<PathBuf>,
    /// Page to show.
    pub page: Option<String>,
}

/// Errors of the instance lock and IPC.
#[derive(Debug, thiserror::Error)]
pub enum InstanceError {
    /// Filesystem or socket failure.
    #[error("instance i/o: {0}")]
    Io(#[from] std::io::Error),
    /// No random source for the IPC token.
    #[error("random source unavailable: {0}")]
    Random(String),
    /// The running instance cannot take forwarded launches (headless) or refused them.
    #[error("the running instance did not accept the request: {0}")]
    Refused(String),
}

/// Outcome of [`Primary::acquire`].
#[derive(Debug)]
pub enum Acquire {
    /// This process is now the instance owner.
    Primary(Box<Primary>),
    /// Another process owns the instance.
    Running(InstanceInfo),
}

/// Default instance directory: the per-user data directory.
pub fn default_dir() -> Option<PathBuf> {
    directories::ProjectDirs::from("es", "Digio", "Bitacora").map(|d| d.data_dir().to_path_buf())
}

/// The held instance lock plus the IPC listener. Dropping it releases the lock and removes the
/// sidecar (a clean exit).
#[derive(Debug)]
pub struct Primary {
    _lock: File,
    dir: PathBuf,
    info: InstanceInfo,
    listener: Option<TcpListener>,
    /// The previous owner's sidecar when it was left behind (it crashed or was killed).
    pub previous_abnormal: Option<InstanceInfo>,
}

impl Primary {
    /// Takes the per-user lock in `dir`, or reports the current owner.
    ///
    /// `accept_launches` opens the IPC listener (the app); headless servers publish port 0.
    pub fn acquire(
        dir: &Path,
        kind: InstanceKind,
        accept_launches: bool,
    ) -> Result<Acquire, InstanceError> {
        std::fs::create_dir_all(dir)?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(dir.join(LOCK_FILE))?;
        match lock.try_lock() {
            Ok(()) => {}
            Err(std::fs::TryLockError::WouldBlock) => {
                return match read_info_waiting(dir) {
                    Some(info) => Ok(Acquire::Running(info)),
                    None => Err(InstanceError::Refused(
                        "another Bitacora process holds the lock but did not publish its endpoint"
                            .into(),
                    )),
                };
            }
            Err(std::fs::TryLockError::Error(e)) => return Err(e.into()),
        }
        let previous_abnormal = read_info(dir);
        let (listener, ipc_port) = if accept_launches {
            let l = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
            let port = l.local_addr()?.port();
            (Some(l), port)
        } else {
            (None, 0)
        };
        let mut token = [0u8; 24];
        getrandom::fill(&mut token).map_err(|e| InstanceError::Random(e.to_string()))?;
        let info = InstanceInfo {
            pid: std::process::id(),
            kind,
            version: env!("CARGO_PKG_VERSION").to_owned(),
            ipc_port,
            token: token.iter().map(|b| format!("{b:02x}")).collect(),
        };
        write_info(dir, &info)?;
        Ok(Acquire::Primary(Box::new(Self {
            _lock: lock,
            dir: dir.to_path_buf(),
            info,
            listener,
            previous_abnormal,
        })))
    }

    /// This instance's published info.
    pub fn info(&self) -> &InstanceInfo {
        &self.info
    }

    /// Starts accepting forwarded launches on a background thread. Returns `None` if the
    /// listener is not open or was already taken.
    pub fn take_launches(&mut self) -> Option<mpsc::Receiver<Launch>> {
        let listener = self.listener.take()?;
        let token = self.info.token.clone();
        let (tx, rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("bitacora-instance-ipc".into())
            .spawn(move || {
                for stream in listener.incoming() {
                    let Ok(stream) = stream else { continue };
                    if let Some(launch) = serve_one(stream, &token)
                        && tx.send(launch).is_err()
                    {
                        break;
                    }
                }
            })
            .ok()?;
        Some(rx)
    }
}

impl Drop for Primary {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(self.dir.join(INFO_FILE));
    }
}

fn serve_one(stream: TcpStream, token: &str) -> Option<Launch> {
    #[derive(Deserialize)]
    struct Request {
        token: String,
        launch: Launch,
    }
    stream.set_read_timeout(Some(IPC_TIMEOUT)).ok()?;
    let mut line = String::new();
    BufReader::new(stream.try_clone().ok()?)
        .take(64 * 1024)
        .read_line(&mut line)
        .ok()?;
    let mut stream = stream;
    match serde_json::from_str::<Request>(&line) {
        Ok(req) if req.token == token => {
            let _ = stream.write_all(b"ok\n");
            Some(req.launch)
        }
        _ => {
            let _ = stream.write_all(b"denied\n");
            None
        }
    }
}

/// Forwards `launch` to the running instance described by `info`.
pub fn forward(info: &InstanceInfo, launch: &Launch) -> Result<(), InstanceError> {
    if info.kind != InstanceKind::App || info.ipc_port == 0 {
        return Err(InstanceError::Refused(format!(
            "a headless Bitacora server (pid {}) holds the instance",
            info.pid
        )));
    }
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, info.ipc_port));
    let mut stream = TcpStream::connect_timeout(&addr, IPC_TIMEOUT)?;
    stream.set_read_timeout(Some(IPC_TIMEOUT))?;
    stream.set_write_timeout(Some(IPC_TIMEOUT))?;
    let request = serde_json::json!({ "token": info.token, "launch": launch });
    writeln!(stream, "{request}")?;
    let mut answer = String::new();
    BufReader::new(stream).read_line(&mut answer)?;
    if answer.trim() == "ok" {
        Ok(())
    } else {
        Err(InstanceError::Refused(answer.trim().to_owned()))
    }
}

fn read_info(dir: &Path) -> Option<InstanceInfo> {
    serde_json::from_slice(&std::fs::read(dir.join(INFO_FILE)).ok()?).ok()
}

fn read_info_waiting(dir: &Path) -> Option<InstanceInfo> {
    let deadline = std::time::Instant::now() + SIDECAR_WAIT;
    loop {
        if let Some(info) = read_info(dir) {
            return Some(info);
        }
        if std::time::Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn write_info(dir: &Path, info: &InstanceInfo) -> std::io::Result<()> {
    let tmp = dir.join(format!("{INFO_FILE}.tmp-{}", std::process::id()));
    let mut file = File::create(&tmp)?;
    file.write_all(&serde_json::to_vec_pretty(info).map_err(std::io::Error::other)?)?;
    file.sync_all()?;
    std::fs::rename(&tmp, dir.join(INFO_FILE))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn primary(dir: &Path, kind: InstanceKind, accept: bool) -> Box<Primary> {
        match Primary::acquire(dir, kind, accept).expect("acquire") {
            Acquire::Primary(p) => p,
            Acquire::Running(i) => panic!("unexpected owner {i:?}"),
        }
    }

    #[test]
    fn second_acquire_reports_the_owner_and_release_frees_it() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let first = primary(tmp.path(), InstanceKind::App, true);
        match Primary::acquire(tmp.path(), InstanceKind::App, true).expect("second") {
            Acquire::Running(info) => assert_eq!(&info, first.info()),
            Acquire::Primary(_) => panic!("lock must be exclusive"),
        }
        drop(first);
        assert!(
            !tmp.path().join(INFO_FILE).exists(),
            "clean exit removes the sidecar"
        );
        let again = primary(tmp.path(), InstanceKind::App, false);
        assert!(again.previous_abnormal.is_none());
    }

    #[test]
    fn leftover_sidecar_without_lock_is_reported_as_abnormal_exit() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let stale = InstanceInfo {
            pid: 999_999,
            kind: InstanceKind::App,
            version: "0.0.1".into(),
            ipc_port: 1,
            token: "x".into(),
        };
        write_info(tmp.path(), &stale).expect("write");
        let p = primary(tmp.path(), InstanceKind::App, true);
        assert_eq!(p.previous_abnormal.as_ref(), Some(&stale));
        assert_ne!(p.info().pid, 999_999);
    }

    #[test]
    fn launches_are_forwarded_to_the_owner() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let mut owner = primary(tmp.path(), InstanceKind::App, true);
        let rx = owner.take_launches().expect("listener");
        let launch = Launch {
            graph: Some(PathBuf::from("/graphs/work")),
            page: Some("Inbox".into()),
        };
        forward(owner.info(), &launch).expect("forward");
        assert_eq!(rx.recv_timeout(Duration::from_secs(2)), Ok(launch));
    }

    #[test]
    fn wrong_token_is_denied() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let mut owner = primary(tmp.path(), InstanceKind::App, true);
        let rx = owner.take_launches().expect("listener");
        let mut forged = owner.info().clone();
        forged.token = "nope".into();
        assert!(matches!(
            forward(&forged, &Launch::default()),
            Err(InstanceError::Refused(_))
        ));
        assert!(rx.recv_timeout(Duration::from_millis(200)).is_err());
    }

    #[test]
    fn headless_owner_cannot_take_forwarded_launches() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let owner = primary(tmp.path(), InstanceKind::Headless, false);
        assert!(matches!(
            forward(owner.info(), &Launch::default()),
            Err(InstanceError::Refused(_))
        ));
    }
}
