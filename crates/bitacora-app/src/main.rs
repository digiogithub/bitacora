//! `bitacora`: the GPUI Kit desktop binary.

// A GUI-subsystem program on Windows: launching it never opens a console window (whose close
// button would also end the app). Linux and macOS never attach a terminal to a GUI launch.
// Started from a terminal, `console::attach_parent` still routes `--help`, logs and bench
// output there. `bitacora-cli` stays a console program.
#![cfg_attr(windows, windows_subsystem = "windows")]

use clap::Parser as _;

fn main() -> anyhow::Result<std::process::ExitCode> {
    // Multi-call: git/ssh run this very executable as GIT_ASKPASS (BIT-US-0176). Answer the
    // prompt and exit before any logging, GPUI or single-instance setup.
    if let Some(code) = bitacora_sync::askpass::run_if_askpass_mode() {
        return Ok(code);
    }
    #[cfg(windows)]
    console::attach_parent();
    bitacora_app::update::init_velopack();
    bitacora_app::app::run(bitacora_app::cli::Args::parse())?;
    Ok(std::process::ExitCode::SUCCESS)
}

/// Console output for a GUI-subsystem binary started from a terminal (Windows only).
#[cfg(windows)]
mod console {
    // The only `unsafe` code of the binary: three kernel32 calls with plain integer/handle
    // arguments and no pointers into Rust memory.
    #![allow(unsafe_code)]

    use std::ffi::c_void;
    use std::os::windows::io::IntoRawHandle as _;

    /// `ATTACH_PARENT_PROCESS`: `(DWORD)-1`.
    const ATTACH_PARENT_PROCESS: u32 = u32::MAX;
    /// `STD_OUTPUT_HANDLE`: `(DWORD)-11`.
    const STD_OUTPUT_HANDLE: u32 = 0xFFFF_FFF5;
    /// `STD_ERROR_HANDLE`: `(DWORD)-12`.
    const STD_ERROR_HANDLE: u32 = 0xFFFF_FFF4;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn AttachConsole(process_id: u32) -> i32;
        fn GetStdHandle(std_handle: u32) -> *mut c_void;
        fn SetStdHandle(std_handle: u32, handle: *mut c_void) -> i32;
    }

    /// Whether the standard handle `id` is unset (no console, no redirection).
    fn missing(id: u32) -> bool {
        // SAFETY: GetStdHandle takes a constant id and only returns a handle value.
        let handle = unsafe { GetStdHandle(id) };
        handle.is_null() || handle as isize == -1
    }

    /// Attaches to the console of the launching terminal, if any, and points unset stdout /
    /// stderr at it. Redirected handles (`> out.json`, git's askpass pipes) are kept; a launch
    /// from Explorer, the Start menu or a shortcut has no parent console and stays silent.
    pub fn attach_parent() {
        let (out, err) = (missing(STD_OUTPUT_HANDLE), missing(STD_ERROR_HANDLE));
        if !out && !err {
            return;
        }
        // SAFETY: AttachConsole takes a process id constant; failure only means no console.
        if unsafe { AttachConsole(ATTACH_PARENT_PROCESS) } == 0 {
            return;
        }
        for (id, unset) in [(STD_OUTPUT_HANDLE, out), (STD_ERROR_HANDLE, err)] {
            if !unset || !missing(id) {
                continue;
            }
            if let Ok(conout) = std::fs::OpenOptions::new().write(true).open("CONOUT$") {
                // SAFETY: the handle is a valid, owned console handle that is intentionally
                // leaked: it stays the process's standard handle until exit.
                unsafe {
                    SetStdHandle(id, conout.into_raw_handle());
                }
            }
        }
    }
}
