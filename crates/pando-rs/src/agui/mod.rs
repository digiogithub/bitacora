//! AG-UI agent API (`/api/v1/agui`): SSE runs, threads and human-in-the-loop interrupts.
//!
//! - [`AguiClient`] (from [`crate::PandoClient::agui`]): `info`, `run`/`run_text`, `attach`,
//!   `cancel_run`, `list_threads`, `thread_messages`, `delete_thread`. Auth is
//!   `Authorization: Bearer` (the adapter does not read `X-Pando-Token`).
//! - [`Thread`]: a transcript + shared-state reducer that resends the transcript every turn and
//!   tracks interrupts.
//! - [`hitl`]: answer payloads for permission prompts and `AskUserQuestion`.
//! - [`Event`] and friends: our own tolerant types (see the crate README for why not `ag-ui-core`).
//!
//! ```no_run
//! use pando::agui::{Interrupt, Thread, hitl};
//!
//! async fn chat(client: &pando::PandoClient) -> pando::Result<()> {
//!     let mut thread = Thread::new(client.agui());
//!     let mut run = thread.send("Refactor the parser").await?;
//!     while let Some(event) = run.next().await {
//!         let _event = event?; // already reduced into the thread
//!     }
//!     for interrupt in thread.interrupts() {
//!         if let Interrupt::Permission { tool_call_id, .. } = interrupt {
//!             thread.resume(&tool_call_id, hitl::deny()).await?.drain().await?;
//!         }
//!     }
//!     Ok(())
//! }
//! ```

mod client;
pub mod hitl;
mod patch;
mod sse;
mod thread;
mod types;

pub use client::{AguiClient, AguiOptions, DEFAULT_AGENT, DEFAULT_PATH, RunStream};
pub use hitl::Interrupt;
pub use patch::apply as apply_patch;
pub use sse::SseParser;
pub use thread::{PendingToolCall, RunOutcome, Thread, ThreadRun};
pub use types::{
    AgentDescriptor, Capabilities, ContentPart, ContextEntry, Event, Health, Info, Message,
    MessageContent, ModelDescriptor, OUTCOME_INTERRUPT, OUTCOME_SUCCESS, PatchOp, RunInput,
    ThreadSummary, ThreadsPage, Tool, ToolCall, ToolCallFunction, new_id, role,
};
