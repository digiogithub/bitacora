//! `cargo xtask`: repository automation.
//!
//! Commands:
//! - `fixtures update|verify`: maintain and check `fixtures/graphs/MANIFEST.sha256`.
//! - `bundle`, `release-check`, `release-notes`, `sha256sums`, `bump`: packaging and release
//!   helpers (see `docs/design/release-process.md`).
//! - `check-deps`: enforce the workspace dependency direction, the GPUI pin and
//!   the "no tokio in `bitacora-core`" rule (ADR-001, ADR-012, ADR-016).

mod bundle;
mod deps;
mod fixtures;
mod release;

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
        Some(cmd @ ("bundle" | "release-check" | "release-notes" | "sha256sums" | "bump")) => {
            let rest: Vec<String> = args.collect();
            let first = rest.first().map(String::as_str);
            let result = match (cmd, first) {
                ("bundle", _) => bundle::run(&rest),
                ("release-check", Some(tag)) => release::check(tag),
                ("release-notes", _) => release::notes(&rest),
                ("sha256sums", Some(dir)) => release::sha256sums(std::path::Path::new(dir)),
                ("bump", Some(version)) => release::bump(version),
                _ => Err(anyhow::anyhow!("`{cmd}` needs an argument")),
            };
            match result {
                Ok(true) => ExitCode::SUCCESS,
                Ok(false) => ExitCode::FAILURE,
                Err(err) => {
                    eprintln!("xtask: {err:#}");
                    ExitCode::FAILURE
                }
            }
        }
        other => {
            if let Some(cmd) = other {
                eprintln!("xtask: unknown command `{cmd}`");
            }
            eprintln!(
                "usage: cargo xtask <check-deps | fixtures update|verify | bundle | release-check TAG | \
                 release-notes VERSION | sha256sums DIR | bump VERSION>"
            );
            ExitCode::FAILURE
        }
    }
}
