//! AI agents on top of Pando's AG-UI API (BIT-SP-0011, plan D5): the GPUI-free backend.
//!
//! - [`chat`]: [`ChatSession`], the run loop bridge (events over a channel, [`ChatModel`] for the
//!   view), with frontend tools, approval cards and cancel.
//! - [`tools`]: the frontend tool set (`propose_edit`, `open_page`, `get_selection`) and the
//!   [`FrontendHost`] trait the app implements.
//! - [`edits`]: `propose_edit` proposals, validated against the current page and applied only
//!   through the core command queue ([`QueueEditApplier`]).
//! - [`approvals`]: [`PendingApprovals`], the fail-closed approval book.
//! - [`guard`]: [`ContentGuard`], consent and exclusions for everything sent to an agent.
//! - [`lookup`]: [`BlockLookup`], the index reads review and recommendations validate against.
//! - [`runs`]: one-shot runs and tolerant JSON extraction.
//! - [`review`] / [`cache`]: the journal review, its machine-local cache and optional schedule.
//! - [`compose`]: the editor compose box and ghost-text continuations.
//! - [`recommend`]: recommendations and the debounced auto mode.
//!
//! Profiles come from the managed `.pando.toml` ([`crate::managed::config::PROFILES`]); this
//! module only names them. Design: `docs/design/ai-agents.md`.

pub mod approvals;
pub mod cache;
pub mod chat;
pub mod compose;
pub mod edits;
pub mod guard;
pub mod lookup;
pub mod recommend;
pub mod review;
pub mod runs;
pub mod tools;

pub use approvals::{
    ApprovalKind, Clock, DEFAULT_APPROVAL_TIMEOUT, Decision, DenyReason, PendingApprovals,
    Resolution, SystemClock,
};
pub use cache::{ReviewCache, WallClock};
pub use chat::{
    AgentState, ApprovalCard, CHAT_PROFILE, CardKind, CardState, CardView, ChatConfig, ChatDeps,
    ChatEvent, ChatHandle, ChatMessage, ChatModel, ChatSession, RunEnd, SubAgent, ToolCallView,
    WRITER_PROFILE,
};
pub use compose::{ComposeDeps, ComposeMode, ComposeRequest, PageLookup, PageMeta, run_compose};
pub use edits::{
    AppliedEdit, AuditSink, EditApplier, EditError, EditOp, IndexedBlock, PageResolver, Place,
    Preview, PreviewOp, Proposal, QueueEditApplier, validate,
};
pub use guard::{AttachedBlock, ContentGuard, GuardSource, under_private_block};
pub use lookup::{BlockInfo, BlockLookup};
pub use recommend::{
    AutoRecommender, RECOMMENDER_PROFILE, RecommendDeps, RecommendOutcome, RecommendRequest,
    Suggestions, run_recommend,
};
pub use review::{
    JOURNAL_REVIEWER_PROFILE, Review, ReviewDeps, ReviewOutcome, ReviewRange, ReviewReport,
    run_review,
};
pub use tools::{FrontendHost, NoHost, tool_set};

/// Failures of agent runs and their post-processing.
#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    /// Pando refused or failed the run.
    #[error("pando: {0}")]
    Pando(#[from] pando::Error),
    /// The run took longer than its budget (it was cancelled).
    #[error("the agent run timed out")]
    Timeout,
    /// The agent answered, but not with the structured output we asked for.
    #[error("invalid agent output: {0}")]
    InvalidOutput(String),
    /// The index could not be read.
    #[error("index: {0}")]
    Index(String),
    /// Nothing was run (no consent, bad input).
    #[error("{0}")]
    Unavailable(String),
    /// The review cache could not be read or written.
    #[error("cache: {0}")]
    Cache(String),
}
