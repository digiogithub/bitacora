//! `cargo xtask`: repository automation.
//!
//! Commands:
//! - `check-deps`: enforce the workspace dependency direction, the GPUI pin and
//!   the "no tokio in `bitacora-core`" rule (ADR-001, ADR-012, ADR-016).

mod deps;

use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("check-deps") => match deps::run() {
            Ok(true) => ExitCode::SUCCESS,
            Ok(false) => ExitCode::FAILURE,
            Err(err) => {
                eprintln!("xtask: {err:#}");
                ExitCode::FAILURE
            }
        },
        other => {
            if let Some(cmd) = other {
                eprintln!("xtask: unknown command `{cmd}`");
            }
            eprintln!("usage: cargo xtask check-deps");
            ExitCode::FAILURE
        }
    }
}
