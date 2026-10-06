//! `bitacora`: the GPUI Kit desktop binary.

use clap::Parser as _;

fn main() -> anyhow::Result<()> {
    bitacora_app::app::run(bitacora_app::cli::Args::parse())
}
