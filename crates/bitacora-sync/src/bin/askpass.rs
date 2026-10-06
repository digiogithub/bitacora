//! `bitacora-askpass`: the askpass helper for system git and ssh (BIT-T-0290).

fn main() -> std::process::ExitCode {
    bitacora_sync::askpass::helper_main()
}
