//! `bitacora-pando`: everything Bitacora does with [Pando](https://github.com/digiogithub/pando),
//! on top of the generic `pando-rs` SDK (ADR-027/028/029).
//!
//! - [`credentials`]: tokens in the OS keychain with environment override.
//! - [`service`]: [`PandoService`], the lifecycle owner started and stopped by
//!   `bitacora-runtime`: endpoint resolution (settings, loopback policy, consent), a dedicated
//!   tokio runtime, a health probe and an event channel ([`PandoEvent`]).
//! - [`supervisor`]: the [`Supervisor`] seam for managed mode; [`managed`] implements it
//!   ([`ManagedSupervisor`]: `pando serve` per graph in a cache instance dir, BIT-US-0141).
//!
//! The crate is the only place where the integration's async code lives; `bitacora-core` stays
//! synchronous and never depends on it.

pub mod credentials;
pub mod events;
pub mod managed;
pub mod service;
pub mod supervisor;

pub use credentials::{
    CredentialError, MemoryBackend, PandoCredentials, SecretBackend, TokenKind, TokenSource,
    os_keychain,
};
pub use events::{EventSink, PandoEvent, PandoStatus, RunEvent, SyncProgress};
pub use managed::config::external_config_snippet;
pub use managed::{ManagedOptions, ManagedStatus, ManagedSupervisor};
pub use service::{DEFAULT_PROBE_INTERVAL, Endpoints, PandoOptions, PandoService};
pub use supervisor::{ManagedEndpoint, McpAccess, Supervisor};

/// Crate name, used by smoke tests.
pub const CRATE_NAME: &str = "bitacora-pando";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_name_matches_package() {
        assert_eq!(CRATE_NAME, env!("CARGO_PKG_NAME"));
    }
}
