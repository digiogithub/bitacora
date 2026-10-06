//! `cargo xtask`: repository automation.
//!
//! Commands:
//! - `fixtures update|verify`: maintain and check `fixtures/graphs/MANIFEST.sha256`.
//! - `check-deps`: enforce the workspace dependency direction, the GPUI pin and
//!   the "no tokio in `bitacora-core`" rule (ADR-001, ADR-012, ADR-016).

mod deps;
mod fixtures;

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
        Some("fixtures") => match fixtures::run(args.next().as_deref()) {
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
            eprintln!("usage: cargo xtask <check-deps | fixtures update | fixtures verify>");
            ExitCode::FAILURE
        }
    }
}
