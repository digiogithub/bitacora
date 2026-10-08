//! `bitacora`: the GPUI Kit desktop binary.

use clap::Parser as _;

fn main() -> anyhow::Result<std::process::ExitCode> {
    // Multi-call: git/ssh run this very executable as GIT_ASKPASS (BIT-US-0176). Answer the
    // prompt and exit before any logging, GPUI or single-instance setup.
    if let Some(code) = bitacora_sync::askpass::run_if_askpass_mode() {
        return Ok(code);
    }
    bitacora_app::update::init_velopack();
    bitacora_app::app::run(bitacora_app::cli::Args::parse())?;
    Ok(std::process::ExitCode::SUCCESS)
}
