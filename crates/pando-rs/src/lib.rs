//! `pando-rs` (library name `pando`): a generic async Rust SDK for [Pando].
//!
//! It knows nothing about any particular application. Module layout:
//!
//! - [`config`]: [`PandoConfig`] (base URL, token, timeouts) and the redacted [`Token`].
//! - [`error`]: the shared [`Error`] type.
//! - [`client`]: [`PandoClient`], the authenticated HTTP transport shared by every API.
//! - [`kb`]: [`kb::KbClient`], the REST knowledge-base API.
//!
//! A later AG-UI client (agent run streaming) is meant to live in a sibling `agui`
//! module that reuses [`PandoClient`] and [`Error`]; it is intentionally not part of
//! this first cut.
//!
//! [Pando]: https://github.com/digiogithub/pando

pub mod client;
pub mod config;
pub mod error;
pub mod kb;

pub use client::{PandoClient, ServerInfo};
pub use config::{PandoConfig, Token};
pub use error::{Error, Result};
