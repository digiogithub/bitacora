//! `pando-rs` (library name `pando`): a generic async Rust SDK for [Pando].
//!
//! It knows nothing about any particular application. Module layout:
//!
//! - [`config`]: [`PandoConfig`] (base URL, token, timeouts) and the redacted [`Token`].
//! - [`error`]: the shared [`Error`] type.
//! - [`client`]: [`PandoClient`], the authenticated HTTP transport shared by every API.
//! - [`kb`]: [`kb::KbClient`], the REST knowledge-base API.
//!
//! - [`agui`]: [`agui::AguiClient`], the AG-UI agent API (SSE runs, threads, interrupts) with
//!   its own tolerant event types.
//!
//! [Pando]: https://github.com/digiogithub/pando

pub mod agui;
pub mod client;
pub mod config;
pub mod error;
pub mod kb;

pub use client::{PandoClient, ServerInfo};
pub use config::{PandoConfig, Token};
pub use error::{Error, Result};
