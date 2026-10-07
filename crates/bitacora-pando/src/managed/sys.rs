//! Process plumbing of the supervisor: process groups, parent-death signal, signals, orphan
//! detection and the lifeline watchdog for hosts without `PR_SET_PDEATHSIG`.
//!
//! The only `unsafe` code of the crate lives here (libc calls and `pre_exec`); every block states
//! why it is sound. Managed mode is not supported on Windows yet (no Job Object): the functions
//! there are inert and [`SUPPORTED`] is `false`.
#![allow(unsafe_code)]

use std::path::Path;

/// Whether managed mode can supervise processes on this platform.
pub const SUPPORTED: bool = cfg!(unix);

/// Whether this platform has a parent-death signal (Linux). Elsewhere the child runs under the
/// lifeline watchdog when one is configured.
pub const HAS_PDEATHSIG: bool = cfg!(target_os = "linux");

#[cfg(unix)]
pub use unix::*;

#[cfg(not(unix))]
pub use other::*;

#[cfg(unix)]
mod unix {
    use std::fs::File;
    use std::io::Read as _;
    use std::os::fd::{AsRawFd, FromRawFd as _, OwnedFd, RawFd};
    use std::os::unix::process::CommandExt as _;
    use std::process::{Child, Command, Stdio};
    use std::time::{Duration, Instant};

    use super::Path;

    /// Whether a process with this pid exists.
    #[must_use]
    pub fn pid_alive(pid: u32) -> bool {
        let Ok(pid) = i32::try_from(pid) else {
            return false;
        };
        // SAFETY: `kill` with signal 0 only checks existence and permission; no signal is sent.
        let r = unsafe { libc::kill(pid, 0) };
        r == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
    }

    /// Sends SIGTERM (or SIGKILL) to the process group led by `pid`.
    pub fn signal_group(pid: u32, kill: bool) {
        let Ok(pid) = i32::try_from(pid) else {
            return;
        };
        if pid <= 1 {
            return;
        }
        let sig = if kill { libc::SIGKILL } else { libc::SIGTERM };
        // SAFETY: plain `kill(2)` on a negative pid (a process group this module created with
        // `process_group(0)`); `pid > 1` was checked above so it never targets init or "all".
        unsafe {
            libc::kill(-pid, sig);
        }
    }

    /// Whether `pid` looks like a process we started for the instance in `dir`: on Linux its
    /// working directory is `dir`, elsewhere its command line names the binary.
    #[must_use]
    pub fn is_instance_process(pid: u32, dir: &Path, binary: &Path) -> bool {
        if let Ok(cwd) = std::fs::read_link(format!("/proc/{pid}/cwd")) {
            return cwd == dir;
        }
        let name = binary
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if name.is_empty() {
            return false;
        }
        Command::new("ps")
            .args(["-p", &pid.to_string(), "-o", "command="])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).contains(&name))
            .unwrap_or(false)
    }

    /// Ends an orphaned child: SIGTERM, then SIGKILL after `grace`.
    pub fn kill_orphan(pid: u32, grace: Duration) {
        signal_group(pid, false);
        let end = Instant::now() + grace;
        while Instant::now() < end {
            if !pid_alive(pid) {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        signal_group(pid, true);
    }

    /// A pipe whose read end the child inherits as descriptor 3; it reads EOF when the
    /// supervisor dies, however it dies.
    pub struct Lifeline {
        write: Option<OwnedFd>,
        read: Option<OwnedFd>,
    }

    impl Lifeline {
        /// Creates the pipe (close-on-exec on both ends; the child gets a dup of the read end).
        pub fn new() -> std::io::Result<Self> {
            let mut fds: [RawFd; 2] = [0; 2];
            // SAFETY: `pipe` fills the two-element array we pass.
            if unsafe { libc::pipe(fds.as_mut_ptr()) } != 0 {
                return Err(std::io::Error::last_os_error());
            }
            for fd in fds {
                // SAFETY: `fd` is a valid descriptor just returned by `pipe`.
                unsafe {
                    libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC);
                }
            }
            // SAFETY: both descriptors are valid, open and owned by nobody else.
            let (read, write) =
                unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) };
            Ok(Self {
                write: Some(write),
                read: Some(read),
            })
        }

        /// After the child started: the supervisor keeps only the write end (kept open until
        /// drop) and closes its copy of the read end.
        pub fn started(&mut self) {
            self.read = None;
        }
    }

    /// Prepares `cmd`: own process group, and on Linux SIGTERM when the supervisor thread ends;
    /// with a lifeline the read end becomes descriptor 3.
    pub fn prepare(cmd: &mut Command, lifeline: Option<&Lifeline>) {
        cmd.process_group(0);
        let lifeline_fd: Option<RawFd> =
            lifeline.and_then(|l| l.read.as_ref().map(AsRawFd::as_raw_fd));
        // SAFETY: the closure runs between fork and exec and only calls async-signal-safe
        // functions (`prctl`, `dup2`) without allocating.
        unsafe {
            cmd.pre_exec(move || {
                #[cfg(target_os = "linux")]
                {
                    libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM as libc::c_ulong);
                }
                if let Some(fd) = lifeline_fd
                    && fd != 3
                    && libc::dup2(fd, 3) < 0
                {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }

    /// Runs `program args...` as a child in its own group and ends that group when the lifeline
    /// closes (EOF on `lifeline`), then returns the child's exit code. This is the body of the
    /// watchdog that Bitacora binaries expose on hosts without a parent-death signal.
    pub fn run_watchdog(lifeline: File, program: &str, args: &[String]) -> std::io::Result<i32> {
        let mut child: Child = {
            let mut cmd = Command::new(program);
            cmd.args(args).stdin(Stdio::null()).process_group(0);
            cmd.spawn()?
        };
        let pid = child.id();
        std::thread::spawn(move || {
            let mut lifeline = lifeline;
            let mut buf = [0_u8; 64];
            // Anything but data (EOF, error) means the supervisor is gone.
            while matches!(lifeline.read(&mut buf), Ok(n) if n > 0) {}
            signal_group(pid, false);
            std::thread::sleep(Duration::from_secs(2));
            signal_group(pid, true);
        });
        let status = child.wait()?;
        Ok(status.code().unwrap_or(1))
    }

    /// [`run_watchdog`] reading the lifeline from descriptor 3, as the supervisor arranges.
    pub fn run_watchdog_fd3(program: &str, args: &[String]) -> std::io::Result<i32> {
        // SAFETY: descriptor 3 is the lifeline pipe the supervisor installed before exec; this
        // process owns it and nothing else wraps it.
        let file = unsafe { File::from_raw_fd(3) };
        run_watchdog(file, program, args)
    }

    /// Whether `Lifeline::write` is still open (kept alive by the supervisor).
    impl Drop for Lifeline {
        fn drop(&mut self) {
            self.write = None;
        }
    }
}

#[cfg(not(unix))]
mod other {
    use super::Path;
    use std::process::Command;
    use std::time::Duration;

    #[must_use]
    pub fn pid_alive(_pid: u32) -> bool {
        false
    }
    pub fn signal_group(_pid: u32, _kill: bool) {}
    #[must_use]
    pub fn is_instance_process(_pid: u32, _dir: &Path, _binary: &Path) -> bool {
        false
    }
    pub fn kill_orphan(_pid: u32, _grace: Duration) {}
    pub struct Lifeline;
    impl Lifeline {
        pub fn new() -> std::io::Result<Self> {
            Ok(Self)
        }
        pub fn started(&mut self) {}
    }
    pub fn prepare(_cmd: &mut Command, _lifeline: Option<&Lifeline>) {}
}
