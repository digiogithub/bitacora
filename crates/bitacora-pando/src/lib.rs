//! `bitacora-pando`: everything Bitacora does with [Pando](https://github.com/digiogithub/pando),
//! on top of the generic `pando-rs` SDK (ADR-027/028/029).
//!
//! - [`credentials`]: tokens in the OS keychain with environment override.
//! - [`service`]: [`PandoService`], the lifecycle owner started and stopped by
//!   `bitacora-runtime`: endpoint resolution (settings, loopback policy, consent), a dedicated
//!   tokio runtime, a health probe and an event channel ([`PandoEvent`]).
//! - [`semantic`]: block documents, the durable sync ledger and the worker that keeps Pando's KB in
//!   step with the index (ADR-030).
//! - [`supervisor`]: the [`Supervisor`] seam for managed mode (`pando serve` per graph, BIT-US-0141).
//!
//! The crate is the only place where the integration's async code lives; `bitacora-core` stays
//! synchronous and never depends on it.

pub mod credentials;
pub mod events;
pub mod semantic;
pub mod service;
pub mod supervisor;

pub use credentials::{
    CredentialError, MemoryBackend, PandoCredentials, SecretBackend, TokenKind, TokenSource,
    os_keychain,
};
pub use events::{EventSink, PandoEvent, PandoStatus, RunEvent, SyncProgress};
pub use service::{DEFAULT_PROBE_INTERVAL, Endpoints, PandoOptions, PandoService, ServiceProbe};
pub use supervisor::{ManagedEndpoint, Supervisor};

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
