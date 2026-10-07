//! Managed Pando mode (ADR-029, BIT-US-0141): Bitacora starts, configures and supervises its own
//! `pando serve` per graph, the way git-in-track's managed mode does.
//!
//! - [`instance`]: the per-graph instance directory in the machine-local cache (layout, atomic
//!   writes, `state.json`, redacted rotated log).
//! - [`config`]: the generated `.pando.toml` (AG-UI listener, Bitacora profiles and personas,
//!   `[MCPServers.bitacora]`) merged by Pando over the user's global configuration.
//! - [`supervise`]: [`ManagedSupervisor`], the real [`Supervisor`](crate::Supervisor).
//! - `sys`: process groups, parent-death signal, orphan handling and the lifeline watchdog.

pub mod config;
pub mod instance;
mod supervise;
mod sys;

pub use instance::{InstanceDir, LogSink, ManagedState, ManagedStatus, instance_key};
pub use supervise::{
    DEFAULT_MIN_VERSION, HttpProber, ManagedOptions, ManagedSupervisor, ProbeOutcome, Prober,
    Timing, default_ca_path, default_cache_root, parse_version, resolve_binary,
};
pub use sys::{HAS_PDEATHSIG, SUPPORTED};
#[cfg(unix)]
pub use sys::{run_watchdog, run_watchdog_fd3};
