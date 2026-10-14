//! Child-process plumbing shared by the git backends.

use std::process::Command;

/// Windows `CREATE_NO_WINDOW`: the child gets no console window.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Keeps a console child (git, hostname) from opening its own console window.
///
/// The desktop binary is a GUI-subsystem program on Windows, so it has no console for children
/// to share: without this flag every `git` call would flash a terminal window. No-op elsewhere.
pub fn hide_console(cmd: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}
