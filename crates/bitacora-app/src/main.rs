//! `bitacora`: the GPUI Kit desktop binary (views, block editor, keymaps, themes).
//!
//! The only crate that depends on `gpui-kit`; GPUI is reached via `gpui_kit::gpui`.

use bitacora_config as _;
use bitacora_core as _;
use bitacora_index as _;
use bitacora_mcp as _;
use bitacora_sync as _;
use bitacora_watch as _;
use gpui_kit::gpui as _;

fn main() -> anyhow::Result<()> {
    println!(
        "bitacora {} (UI not implemented yet)",
        env!("CARGO_PKG_VERSION")
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn package_name() {
        assert_eq!(env!("CARGO_PKG_NAME"), "bitacora-app");
    }
}
