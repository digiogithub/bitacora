//! The chat session: an AG-UI run loop bridged to a channel the app consumes (BIT-T-0452).
//!
//! [`ChatSession::spawn`] starts one task on the service's tokio runtime that owns a
//! [`pando::agui::Thread`]. The app talks to it through the cloneable [`ChatHandle`] (send a
//! message, answer a card, cancel, close) and reads [`ChatEvent`]s from a plain
//! `std::sync::mpsc::Receiver`, which fits GPUI's poll-from-a-task style. [`ChatModel`] folds the
//! events into the message list a view renders (text, tool calls, approval cards), so the GPUI
//! entity stays thin.
//!
//! Interrupts end a run with `RUN_FINISHED{outcome:"interrupt"}`; the session then answers them
//! one at a time and resumes the thread:
//!
//! - frontend tools `open_page` / `get_selection` run on the [`FrontendHost`] immediately;
//! - `propose_edit` becomes an approval card; only an explicit approval reaches the
//!   [`EditApplier`] (a `bitacora-core` queue transaction, audited, undoable);
//! - Pando permission prompts and `AskUserQuestion` become cards too.
//!
//! Every card fails closed ([`super::approvals`]): a timeout, [`ChatHandle::cancel`],
//! [`ChatHandle::close`] (panel close, thread or graph switch, quit) or a dropped handle resolves
//! it as denied, and the run is cancelled on the server so the parked prompt is released.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use pando::agui::hitl::{PermissionRequest, QuestionRequest};
use pando::agui::{AguiClient, Event, Interrupt, Message, MessageContent, Thread, ThreadRun};
use serde_json::{Value, json};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use crate::activity::{ActivityEntry, ActivityKind};
use crate::events::EventSink;

use super::approvals::{
    ApprovalKind, Clock, DEFAULT_APPROVAL_TIMEOUT, Decision, DenyReason, PendingApprovals,
    Resolution, SystemClock,
};
use super::edits::{EditApplier, EditError, Preview, Proposal};
use super::guard::{AttachedBlock, ContentGuard, GuardSource};
use super::memory::ToolMemory;
use super::runs::is_read_only_tool;
use super::tools::{
    FrontendHost, GET_SELECTION_TOOL, NoHost, OPEN_PAGE_TOOL, PROPOSE_EDIT_TOOL, run_get_selection,
    tool_set,
};

/// Profile the chat panel talks to (see the managed `.pando.toml`).
pub const CHAT_PROFILE: &str = "bitacora-chat";
/// Profile that may call `propose_edit`.
pub const WRITER_PROFILE: &str = "bitacora-writer";
/// Prefix of the per-model chat profiles the managed config generates (BIT-US-0180).
pub const CHAT_MODEL_PROFILE_PREFIX: &str = "bitacora-chat--";

/// Name of the chat profile that pins `model`: [`CHAT_MODEL_PROFILE_PREFIX`] plus the model id
/// reduced to the characters a TOML bare key allows (`a-z`, `0-9`, `_`, `-`; everything else
/// becomes `-`, letters are lowercased). The managed config generator and the model selector
/// both use it, so they agree on the route.
#[must_use]
pub fn chat_profile_for_model(model: &str) -> String {
    let slug: String = model
        .trim()
        .chars()
        .map(|c| {
            let c = c.to_ascii_lowercase();
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();
    format!("{CHAT_MODEL_PROFILE_PREFIX}{slug}")
}

/// One entry of the agent panel's model selector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelChoice {
    /// Profile (route) a run posts to.
    pub profile: String,
    /// Model id the profile pins; for the default entry the model the server reports for it.
    pub model_id: String,
    /// Text shown for the entry.
    pub label: String,
}

/// What the selector offers, derived from the server's discovery document (`GET .../info`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModelChoices {
    /// The default chat profile with the model the server reports for it, when it is listed.
    pub default: Option<ModelChoice>,
    /// The per-model chat profiles the server exposes, in server order.
    pub extra: Vec<ModelChoice>,
}

impl ModelChoices {
    /// Reads the choices from `info`: the [`CHAT_PROFILE`] agent and every agent named
    /// `bitacora-chat--*`. Works the same for a managed and an external server.
    #[must_use]
    pub fn from_info(info: &pando::agui::Info) -> Self {
        let choice = |a: &pando::agui::AgentDescriptor| {
            let (id, name) = a
                .model
                .as_ref()
                .map(|m| (m.id.clone(), m.name.clone()))
                .unwrap_or_default();
            let label = if name.is_empty() { id.clone() } else { name };
            ModelChoice {
                profile: a.name.clone(),
                model_id: id,
                label,
            }
        };
        Self {
            default: info
                .agents
                .iter()
                .find(|a| a.name == CHAT_PROFILE)
                .map(choice),
            extra: info
                .agents
                .iter()
                .filter(|a| a.name.starts_with(CHAT_MODEL_PROFILE_PREFIX))
                .map(choice)
                .collect(),
        }
    }

    /// Whether there is anything to choose between.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.extra.is_empty()
    }

    /// The profile a run posts to for the remembered `chosen` model id: the profile pinning it
    /// when the server exposes one, else the default chat profile.
    #[must_use]
    pub fn profile_for(&self, chosen: Option<&str>) -> String {
        chosen
            .and_then(|id| self.extra.iter().find(|c| c.model_id == id))
            .map_or_else(|| CHAT_PROFILE.to_owned(), |c| c.profile.clone())
    }
}

/// Whether `name` is one of the frontend tools this client declares to Pando.
fn is_client_tool(name: &str) -> bool {
    matches!(
        name,
        PROPOSE_EDIT_TOOL | OPEN_PAGE_TOOL | GET_SELECTION_TOOL
    )
}

/// How long to keep reading a cancelled run's stream before giving up on it.
const CANCEL_DRAIN: Duration = Duration::from_secs(3);

/// What an approval card asks.
#[derive(Debug, Clone, PartialEq)]
pub enum CardKind {
    /// A Pando permission prompt.
    Permission(PermissionRequest),
    /// A `propose_edit` diff.
    Edit(Preview),
    /// An `AskUserQuestion`.
    Question(QuestionRequest),
}

/// A card waiting for the user.
#[derive(Debug, Clone, PartialEq)]
pub struct ApprovalCard {
    /// Tool call id; pass it to [`ChatHandle::decide`].
    pub id: String,
    /// What is asked.
    pub kind: CardKind,
    /// Seconds until the card is denied on its own.
    pub timeout_secs: u64,
    /// The tool name a "Remember my decision" choice would apply to; `None` when the card cannot
    /// offer it (questions, prompts that demand explicit approval, unknown tools).
    pub remember_tool: Option<String>,
}

/// Why a card was answered without being shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoAnswer {
    /// A permission prompt for an allow-listed read-only tool.
    ReadTool,
    /// The user chose "remember my decision" for this tool earlier.
    Remembered,
}

/// How a run ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunEnd {
    /// The agent answered.
    Finished,
    /// The run failed (a [`ChatEvent::Error`] precedes this).
    Failed,
    /// The user (or the app) cancelled it.
    Cancelled,
}

/// Everything the app receives from a chat session.
#[derive(Debug, Clone, PartialEq)]
pub enum ChatEvent {
    /// The server transcript of a resumed thread.
    History(Vec<Message>),
    /// A run (or a resumed segment) started.
    RunStarted {
        /// Run id.
        run_id: String,
    },
    /// An assistant message opened.
    MessageStart {
        /// Message id.
        message_id: String,
    },
    /// A chunk of assistant text.
    TextDelta {
        /// Message id.
        message_id: String,
        /// Text.
        delta: String,
    },
    /// An assistant message closed.
    MessageEnd {
        /// Message id.
        message_id: String,
    },
    /// A chunk of reasoning text.
    ReasoningDelta {
        /// Message id.
        message_id: String,
        /// Text.
        delta: String,
    },
    /// A tool call opened.
    ToolCallStart {
        /// Call id.
        call_id: String,
        /// Tool name.
        name: String,
    },
    /// A chunk of a tool call's arguments.
    ToolCallArgs {
        /// Call id.
        call_id: String,
        /// Arguments delta.
        delta: String,
    },
    /// A tool call's arguments are complete.
    ToolCallEnd {
        /// Call id.
        call_id: String,
    },
    /// A tool call's result.
    ToolCallResult {
        /// Call id.
        call_id: String,
        /// Result text.
        content: String,
    },
    /// The shared-state document after a `STATE_SNAPSHOT` or `STATE_DELTA` (model, token usage,
    /// sub-agents, ...). See [`AgentState::from_value`].
    State(Value),
    /// An `ACTIVITY_SNAPSHOT` (progress of something the agent does).
    Activity {
        /// Message id the activity belongs to.
        message_id: String,
        /// Activity type.
        kind: String,
        /// Content.
        content: Value,
    },
    /// A `pando.*` custom signal.
    Custom {
        /// Name.
        name: String,
        /// Value.
        value: Value,
    },
    /// A card needs an answer.
    ApprovalRequested(ApprovalCard),
    /// A card was resolved (answered, timed out, cancelled, ...).
    ApprovalResolved {
        /// Call id.
        id: String,
        /// Approved (always `false` for anything but an explicit approval).
        approved: bool,
        /// Why it was denied, when it was.
        reason: Option<DenyReason>,
    },
    /// A prompt was answered without a card: an allow-listed read tool, or a remembered decision.
    AutoAnswered {
        /// Call id.
        call_id: String,
        /// Tool name.
        tool: String,
        /// The answer given.
        approved: bool,
        /// Why no card was shown.
        why: AutoAnswer,
    },
    /// A `propose_edit` call was refused before any card (invalid, stale, page not loaded).
    EditRejected {
        /// Call id.
        call_id: String,
        /// Machine code (see [`EditError::code`]).
        code: String,
        /// Message.
        message: String,
    },
    /// An approved proposal was applied.
    EditApplied {
        /// Call id.
        call_id: String,
        /// Audit entry id, when the audit log recorded it.
        audit_id: Option<String>,
        /// Page title.
        page: String,
        /// Touched block uuids.
        affected: Vec<String>,
    },
    /// An approved proposal could not be applied (nothing was written).
    EditFailed {
        /// Call id.
        call_id: String,
        /// Machine code.
        code: String,
        /// Message.
        message: String,
    },
    /// Something went wrong; `fatal` ends the session.
    Error {
        /// Safe to show.
        message: String,
        /// The session ended.
        fatal: bool,
    },
    /// The turn ended.
    RunFinished(RunEnd),
    /// The session ended (after a close, or when the handle was dropped).
    Closed(DenyReason),
}

/// Configuration of a session.
#[derive(Debug, Clone)]
pub struct ChatConfig {
    /// Pando agent profile (route name).
    pub profile: String,
    /// Declare `propose_edit`.
    pub propose_edit: bool,
    /// Time a card waits for an answer.
    pub approval_timeout: Duration,
    /// Reattach to this existing thread and load its history.
    pub resume_thread: Option<String>,
}

impl Default for ChatConfig {
    fn default() -> Self {
        Self {
            profile: CHAT_PROFILE.to_owned(),
            propose_edit: false,
            approval_timeout: DEFAULT_APPROVAL_TIMEOUT,
            resume_thread: None,
        }
    }
}

impl ChatConfig {
    /// The writer profile with `propose_edit` declared.
    #[must_use]
    pub fn writer() -> Self {
        Self {
            profile: WRITER_PROFILE.to_owned(),
            propose_edit: true,
            ..Self::default()
        }
    }
}

/// What a session needs from the app.
#[derive(Clone)]
pub struct ChatDeps {
    /// The AG-UI client (from `PandoClient::agui`).
    pub agui: AguiClient,
    /// Session configuration.
    pub config: ChatConfig,
    /// The UI seam for `open_page` / `get_selection`.
    pub host: Arc<dyn FrontendHost>,
    /// Applies approved proposals; `None` rejects every `propose_edit`.
    pub applier: Option<Arc<dyn EditApplier>>,
    /// Consent and exclusions for attached context and selections.
    pub guard: ContentGuard,
    /// When set, the guard of every send and selection (so revoking consent or adding an
    /// exclusion applies to a chat that is already open); `guard` is the fallback.
    pub live_guard: Option<GuardSource>,
    /// Time source for approval deadlines.
    pub clock: Arc<dyn Clock>,
    /// Where runs, approvals and applied edits are logged (BIT-SP-0009.R7); `None` logs nothing.
    pub activity: Option<EventSink>,
    /// Remembered "allow/deny always" decisions per tool; `None` never remembers (every write
    /// prompt asks).
    pub tool_memory: Option<Arc<dyn ToolMemory>>,
}

impl std::fmt::Debug for ChatDeps {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChatDeps").finish_non_exhaustive()
    }
}

impl ChatDeps {
    /// Dependencies with no UI host, no applier and the real clock.
    #[must_use]
    pub fn new(agui: AguiClient, guard: ContentGuard) -> Self {
        Self {
            agui,
            config: ChatConfig::default(),
            host: Arc::new(NoHost),
            applier: None,
            guard,
            live_guard: None,
            clock: Arc::new(SystemClock::default()),
            activity: None,
            tool_memory: None,
        }
    }
}

enum Command {
    Send {
        text: String,
        context: Vec<AttachedBlock>,
    },
    Decide {
        id: String,
        decision: Decision,
        remember: bool,
    },
    Abort {
        reason: DenyReason,
        end: bool,
    },
}

/// The app's end of a session. Cheap to clone; dropping every clone closes the session as if the
/// panel had been closed.
#[derive(Clone)]
pub struct ChatHandle {
    tx: UnboundedSender<Command>,
    thread_id: String,
}

impl std::fmt::Debug for ChatHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChatHandle")
            .field("thread_id", &self.thread_id)
            .finish_non_exhaustive()
    }
}

impl ChatHandle {
    /// The thread id (stable across turns; use it to resume later).
    #[must_use]
    pub fn thread_id(&self) -> &str {
        &self.thread_id
    }

    /// Sends a user message. `context` is the blocks the user attached; it goes through the
    /// [`ContentGuard`] and is the only graph content sent besides what the agent reads over MCP.
    /// `false` when the session ended.
    pub fn send(&self, text: impl Into<String>, context: Vec<AttachedBlock>) -> bool {
        self.tx
            .send(Command::Send {
                text: text.into(),
                context,
            })
            .is_ok()
    }

    /// Answers a card.
    pub fn decide(&self, id: impl Into<String>, decision: Decision) -> bool {
        self.tx
            .send(Command::Decide {
                id: id.into(),
                decision,
                remember: false,
            })
            .is_ok()
    }

    /// Answers a permission or edit card with Approve or Deny and remembers the answer for that
    /// tool ("allow always" / "deny always"). Ignored for cards that offer no
    /// [`ApprovalCard::remember_tool`] and for any other decision; the answer itself still
    /// applies.
    pub fn decide_and_remember(&self, id: impl Into<String>, decision: Decision) -> bool {
        self.tx
            .send(Command::Decide {
                id: id.into(),
                decision,
                remember: true,
            })
            .is_ok()
    }

    /// Approves a permission or edit card.
    pub fn approve(&self, id: impl Into<String>) -> bool {
        self.decide(id, Decision::Approve)
    }

    /// Denies a card.
    pub fn deny(&self, id: impl Into<String>) -> bool {
        self.decide(id, Decision::Deny)
    }

    /// Stops the running turn (`POST /runs/{thread}/cancel`); pending cards are denied. The
    /// session stays usable.
    pub fn cancel(&self) -> bool {
        self.tx
            .send(Command::Abort {
                reason: DenyReason::Cancelled,
                end: false,
            })
            .is_ok()
    }

    /// Ends the session: pending cards are denied for `reason` (panel close, thread switch, graph
    /// switch or quit) and the run is cancelled.
    pub fn close(&self, reason: DenyReason) -> bool {
        self.tx.send(Command::Abort { reason, end: true }).is_ok()
    }
}

/// Entry point.
#[derive(Debug)]
pub struct ChatSession;

impl ChatSession {
    /// Starts a session on `runtime` and returns the handle and the event receiver.
    #[must_use]
    pub fn spawn(
        runtime: &tokio::runtime::Handle,
        deps: ChatDeps,
    ) -> (ChatHandle, Receiver<ChatEvent>) {
        let (cmd_tx, cmd_rx) = unbounded_channel();
        let (ev_tx, ev_rx) = mpsc::channel();
        let mut thread = match &deps.config.resume_thread {
            Some(id) => Thread::with_id(deps.agui.clone(), id.clone()),
            None => Thread::new(deps.agui.clone()),
        }
        .with_agent(deps.config.profile.clone())
        .with_tools(tool_set(deps.config.propose_edit));
        let thread_id = thread.thread_id().to_owned();
        let resume = deps.config.resume_thread.is_some();
        runtime.spawn(async move {
            let mut ctx = Ctx::new(deps, cmd_rx, ev_tx, thread_id_of(&thread));
            if resume {
                match thread.load_history().await {
                    Ok(true) => ctx.emit(ChatEvent::History(thread.messages.clone())),
                    Ok(false) => {}
                    Err(e) => ctx.emit(ChatEvent::Error {
                        message: format!("could not load the thread: {e}"),
                        fatal: false,
                    }),
                }
            }
            ctx.main_loop(&mut thread).await;
        });
        (
            ChatHandle {
                tx: cmd_tx,
                thread_id,
            },
            ev_rx,
        )
    }
}

fn thread_id_of(t: &Thread) -> String {
    t.thread_id().to_owned()
}

enum Prep {
    Edit(Proposal),
    Rejected(EditError),
    Permission,
    Question,
    Immediate,
}

enum End {
    Finished,
    Interrupted,
    Error(String),
    Aborted,
}

struct Ctx {
    deps: ChatDeps,
    cmd_rx: UnboundedReceiver<Command>,
    events: Sender<ChatEvent>,
    thread_id: String,
    book: PendingApprovals,
    prepared: HashMap<String, Prep>,
    /// Tool name a remembered decision of this card would apply to, by call id.
    remember_tools: HashMap<String, String>,
    announced: HashSet<String>,
    /// Set by a cancel/close/dropped handle; ends the current turn.
    abort: Option<(DenyReason, bool)>,
    /// The command channel closed (the handle was dropped); never poll it again.
    cmd_closed: bool,
    /// The server-side cancel of the current turn was already sent.
    cancel_sent: bool,
    /// The shared-state document, patched by `STATE_DELTA`.
    state: Value,
}

impl Ctx {
    fn new(
        deps: ChatDeps,
        cmd_rx: UnboundedReceiver<Command>,
        events: Sender<ChatEvent>,
        thread_id: String,
    ) -> Self {
        let book = PendingApprovals::new(deps.config.approval_timeout);
        Self {
            deps,
            cmd_rx,
            events,
            thread_id,
            book,
            prepared: HashMap::new(),
            remember_tools: HashMap::new(),
            announced: HashSet::new(),
            abort: None,
            cmd_closed: false,
            cancel_sent: false,
            state: Value::Null,
        }
    }

    async fn cancel_remote(&mut self) {
        if !self.cancel_sent {
            self.cancel_sent = true;
            let _ = self.deps.agui.cancel_run(&self.thread_id).await;
        }
    }

    fn log_activity(&self, ev: &ChatEvent) {
        let Some(sink) = &self.deps.activity else {
            return;
        };
        let one =
            |kind, word: &str, id: &str| ActivityEntry::new(kind, word, 1, vec![id.to_owned()]);
        let entry = match ev {
            ChatEvent::RunStarted { run_id } => one(ActivityKind::Run, "started", run_id),
            ChatEvent::RunFinished(end) => one(
                ActivityKind::Run,
                match end {
                    RunEnd::Finished => "finished",
                    RunEnd::Failed => "failed",
                    RunEnd::Cancelled => "cancelled",
                },
                &self.thread_id,
            ),
            ChatEvent::AutoAnswered {
                call_id, approved, ..
            } => one(
                ActivityKind::Approval,
                if *approved {
                    "auto-approved"
                } else {
                    "auto-denied"
                },
                call_id,
            ),
            ChatEvent::ApprovalResolved { id, approved, .. } => one(
                ActivityKind::Approval,
                if *approved { "approved" } else { "denied" },
                id,
            ),
            ChatEvent::EditApplied { affected, .. } => ActivityEntry::new(
                ActivityKind::Edit,
                "applied",
                u64::try_from(affected.len()).unwrap_or(u64::MAX),
                affected.clone(),
            ),
            _ => return,
        };
        sink.record(entry);
    }

    fn emit(&mut self, ev: ChatEvent) {
        self.log_activity(&ev);
        if self.events.send(ev).is_err() {
            // Nobody is listening any more: same as closing the panel.
            self.abort.get_or_insert((DenyReason::PanelClosed, true));
        }
    }

    fn announce_resolved(&mut self, ids: &[String]) {
        for id in ids {
            let (approved, reason) = match self.book.resolution(id) {
                Some(Resolution::Approved) => (true, None),
                Some(Resolution::Denied(r)) => (false, Some(*r)),
                Some(Resolution::Answered(_)) => (true, None),
                None => continue,
            };
            self.emit(ChatEvent::ApprovalResolved {
                id: id.clone(),
                approved,
                reason,
            });
        }
    }

    async fn main_loop(&mut self, thread: &mut Thread) {
        loop {
            let cmd = self.cmd_rx.recv().await;
            match cmd {
                None => {
                    self.close(DenyReason::PanelClosed).await;
                    return;
                }
                Some(Command::Abort { reason, end: true }) => {
                    self.close(reason).await;
                    return;
                }
                Some(Command::Send { text, context }) => {
                    thread.set_context(self.guard().context_entries(&context));
                    self.abort = None;
                    self.cancel_sent = false;
                    self.drive(thread, text).await;
                    if let Some((reason, true)) = self.abort {
                        self.close(reason).await;
                        return;
                    }
                }
                // Nothing is pending between turns.
                Some(Command::Decide { .. } | Command::Abort { .. }) => {}
            }
        }
    }

    /// The guard as it is now.
    fn guard(&self) -> ContentGuard {
        match &self.deps.live_guard {
            Some(source) => source(),
            None => self.deps.guard.clone(),
        }
    }

    async fn close(&mut self, reason: DenyReason) {
        let ids = self.book.deny_all(reason);
        self.announce_resolved(&ids);
        self.cancel_remote().await;
        self.emit(ChatEvent::Closed(reason));
    }

    /// One user turn, including every interrupt resume it needs.
    async fn drive(&mut self, thread: &mut Thread, text: String) {
        let mut next: Option<(Option<String>, MessageContent)> = Some((None, text.into()));
        while let Some((call, content)) = next.take() {
            let end = match call {
                None => match thread.send(content).await {
                    Ok(run) => self.pump(run).await,
                    Err(e) => End::Error(e.to_string()),
                },
                Some(id) => match thread.resume(&id, content).await {
                    Ok(run) => self.pump(run).await,
                    Err(e) => End::Error(format!("could not deliver the answer: {e}")),
                },
            };
            match end {
                End::Finished => {
                    self.emit(ChatEvent::RunFinished(RunEnd::Finished));
                    return;
                }
                End::Error(message) => {
                    let ids = self.book.deny_all(DenyReason::ServerExpired);
                    self.announce_resolved(&ids);
                    self.emit(ChatEvent::Error {
                        message,
                        fatal: false,
                    });
                    self.emit(ChatEvent::RunFinished(RunEnd::Failed));
                    return;
                }
                End::Aborted => {
                    self.emit(ChatEvent::RunFinished(RunEnd::Cancelled));
                    return;
                }
                End::Interrupted => match self.answer_next(thread).await {
                    Some(step) => next = Some(step),
                    None => {
                        self.emit(ChatEvent::RunFinished(RunEnd::Cancelled));
                        return;
                    }
                },
            }
        }
    }

    /// Reads a run to its end, forwarding events and serving commands.
    async fn pump(&mut self, mut run: ThreadRun<'_>) -> End {
        loop {
            let aborted = self.abort.is_some();
            let drain = async {
                if aborted {
                    tokio::time::timeout(CANCEL_DRAIN, run.next())
                        .await
                        .ok()
                        .flatten()
                } else {
                    run.next().await
                }
            };
            tokio::select! {
                item = drain => {
                    let Some(item) = item else {
                        return if self.abort.is_some() { End::Aborted } else {
                            End::Error("the stream ended before the run finished".into())
                        };
                    };
                    match item {
                        Ok(Event::RunFinished { outcome, .. }) => {
                            if self.abort.is_some() {
                                return End::Aborted;
                            }
                            return if outcome.as_deref() == Some(pando::agui::OUTCOME_INTERRUPT) {
                                End::Interrupted
                            } else {
                                End::Finished
                            };
                        }
                        Ok(Event::RunError { message, code }) => {
                            if self.abort.is_some() || code.as_deref() == Some("cancelled") {
                                return End::Aborted;
                            }
                            return End::Error(message);
                        }
                        Ok(ev) => self.forward(ev),
                        Err(e) => {
                            if self.abort.is_some() {
                                return End::Aborted;
                            }
                            return End::Error(e.to_string());
                        }
                    }
                }
                cmd = self.cmd_rx.recv(), if !self.cmd_closed => {
                    self.handle(cmd).await;
                }
            }
        }
    }

    fn forward(&mut self, ev: Event) {
        let out = match ev {
            Event::RunStarted { run_id, .. } => ChatEvent::RunStarted { run_id },
            Event::TextMessageStart { message_id, .. } => ChatEvent::MessageStart { message_id },
            Event::TextMessageContent { message_id, delta } => {
                ChatEvent::TextDelta { message_id, delta }
            }
            Event::TextMessageEnd { message_id } => ChatEvent::MessageEnd { message_id },
            Event::ReasoningMessageContent { message_id, delta } => {
                ChatEvent::ReasoningDelta { message_id, delta }
            }
            Event::ToolCallStart {
                tool_call_id,
                tool_call_name,
                ..
            } => ChatEvent::ToolCallStart {
                call_id: tool_call_id,
                name: tool_call_name,
            },
            Event::ToolCallArgs {
                tool_call_id,
                delta,
            } => ChatEvent::ToolCallArgs {
                call_id: tool_call_id,
                delta,
            },
            Event::ToolCallEnd { tool_call_id } => ChatEvent::ToolCallEnd {
                call_id: tool_call_id,
            },
            Event::ToolCallResult {
                tool_call_id,
                content,
                ..
            } => ChatEvent::ToolCallResult {
                call_id: tool_call_id,
                content,
            },
            Event::StateSnapshot { snapshot } => {
                self.state = snapshot;
                ChatEvent::State(self.state.clone())
            }
            Event::StateDelta { delta } => {
                for op in &delta {
                    if let Err(e) = pando::agui::apply_patch(&mut self.state, op) {
                        tracing::debug!(error = %e, "ignoring a state patch that does not apply");
                    }
                }
                ChatEvent::State(self.state.clone())
            }
            Event::ActivitySnapshot {
                message_id,
                activity_type,
                content,
                ..
            } => ChatEvent::Activity {
                message_id,
                kind: activity_type,
                content,
            },
            Event::Custom { name, value } => ChatEvent::Custom { name, value },
            _ => return,
        };
        self.emit(out);
    }

    /// Serves one command received while a turn is running or a card is waiting.
    async fn handle(&mut self, cmd: Option<Command>) {
        match cmd {
            None => {
                self.cmd_closed = true;
                self.abort_turn(DenyReason::PanelClosed, true).await;
            }
            Some(Command::Abort { reason, end }) => self.abort_turn(reason, end).await,
            Some(Command::Decide {
                id,
                decision,
                remember,
            }) => {
                let allow = match decision {
                    Decision::Approve => Some(true),
                    Decision::Deny => Some(false),
                    Decision::Answer(_) => None,
                };
                if self.book.decide(&id, decision) {
                    if remember
                        && let (Some(allow), Some(tool), Some(mem)) = (
                            allow,
                            self.remember_tools.get(&id),
                            self.deps.tool_memory.as_ref(),
                        )
                    {
                        mem.remember(tool, allow);
                    }
                    self.announce_resolved(&[id]);
                }
            }
            Some(Command::Send { .. }) => self.emit(ChatEvent::Error {
                message: "a run is already in progress".into(),
                fatal: false,
            }),
        }
    }

    async fn abort_turn(&mut self, reason: DenyReason, end: bool) {
        let end = end || matches!(self.abort, Some((_, true)));
        self.abort = Some((reason, end));
        let ids = self.book.deny_all(reason);
        self.announce_resolved(&ids);
        self.cancel_remote().await;
    }

    /// Prepares and announces cards for every pending interrupt, then answers the first one.
    async fn answer_next(&mut self, thread: &Thread) -> Option<(Option<String>, MessageContent)> {
        // Pando also lists the calls of its own tools (MCP, KB) that are parked behind a
        // permission prompt as pending: they are not ours to answer. Only prompts and the
        // frontend tools this client declared are (a real Pando answered "unknown tool" to the
        // server-side call first and the resume that followed was refused with 409).
        let interrupts: Vec<Interrupt> = thread
            .interrupts()
            .into_iter()
            .filter(|i| match i {
                Interrupt::FrontendTool(call) => is_client_tool(&call.name),
                _ => true,
            })
            .collect();
        if interrupts.is_empty() {
            return None;
        }
        for i in &interrupts {
            self.prepare(i).await;
        }
        if self.abort.is_some() {
            return None;
        }
        let first = interrupts.first()?;
        let id = first.tool_call_id().to_owned();
        let content = self.answer(first).await?;
        self.prepared.remove(&id);
        self.remember_tools.remove(&id);
        self.announced.remove(&id);
        self.book.remove(&id);
        Some((Some(id), content.into()))
    }

    async fn prepare(&mut self, i: &Interrupt) {
        let id = i.tool_call_id().to_owned();
        if !self.announced.insert(id.clone()) {
            return;
        }
        let now = self.deps.clock.now();
        let timeout_secs = self.deps.config.approval_timeout.as_secs();
        match i {
            Interrupt::Permission { request, .. } => {
                self.book.register(&id, ApprovalKind::Permission, now);
                self.prepared.insert(id.clone(), Prep::Permission);
                let tool = request.tool_name.clone();
                if is_read_only_tool(request) {
                    // Allow-listed read tools never ask (as in the headless runs): the tool call
                    // itself shows in the transcript.
                    self.auto_answer(&id, &tool, true, AutoAnswer::ReadTool);
                    return;
                }
                // Prompts that demand explicit approval cannot be remembered and never
                // auto-allow; a remembered deny still applies.
                let explicit = request.require_explicit_approval || request.never_auto_approve;
                match self.remembered(&tool) {
                    Some(false) => {
                        self.auto_answer(&id, &tool, false, AutoAnswer::Remembered);
                        return;
                    }
                    Some(true) if !explicit && self.guard().has_consent() => {
                        self.auto_answer(&id, &tool, true, AutoAnswer::Remembered);
                        return;
                    }
                    _ => {}
                }
                let remember_tool = (!explicit && !tool.trim().is_empty()).then_some(tool);
                if let Some(t) = &remember_tool {
                    self.remember_tools.insert(id.clone(), t.clone());
                }
                self.emit(ChatEvent::ApprovalRequested(ApprovalCard {
                    id,
                    kind: CardKind::Permission(request.clone()),
                    timeout_secs,
                    remember_tool,
                }));
            }
            Interrupt::Question { request, .. } => {
                self.book.register(&id, ApprovalKind::Question, now);
                self.prepared.insert(id.clone(), Prep::Question);
                self.emit(ChatEvent::ApprovalRequested(ApprovalCard {
                    id,
                    kind: CardKind::Question(request.clone()),
                    timeout_secs,
                    remember_tool: None,
                }));
            }
            Interrupt::FrontendTool(call) if call.name == PROPOSE_EDIT_TOOL => {
                match self.prepare_edit(call.args.as_ref()).await {
                    Ok((proposal, preview)) => {
                        self.book.register(&id, ApprovalKind::ProposeEdit, now);
                        self.prepared.insert(id.clone(), Prep::Edit(proposal));
                        // A remembered allow only skips the card: the applier still runs the
                        // content guard and commits through the audited, undoable core queue.
                        // Without consent nothing is auto-allowed.
                        match self.remembered(PROPOSE_EDIT_TOOL) {
                            Some(false) => {
                                self.auto_answer(
                                    &id,
                                    PROPOSE_EDIT_TOOL,
                                    false,
                                    AutoAnswer::Remembered,
                                );
                                return;
                            }
                            Some(true) if self.guard().has_consent() => {
                                self.auto_answer(
                                    &id,
                                    PROPOSE_EDIT_TOOL,
                                    true,
                                    AutoAnswer::Remembered,
                                );
                                return;
                            }
                            _ => {}
                        }
                        self.remember_tools
                            .insert(id.clone(), PROPOSE_EDIT_TOOL.to_owned());
                        self.emit(ChatEvent::ApprovalRequested(ApprovalCard {
                            id,
                            kind: CardKind::Edit(preview),
                            timeout_secs,
                            remember_tool: Some(PROPOSE_EDIT_TOOL.to_owned()),
                        }));
                    }
                    Err(e) => {
                        self.emit(ChatEvent::EditRejected {
                            call_id: id.clone(),
                            code: e.code().to_owned(),
                            message: e.to_string(),
                        });
                        self.prepared.insert(id, Prep::Rejected(e));
                    }
                }
            }
            Interrupt::FrontendTool(_) => {
                self.prepared.insert(id, Prep::Immediate);
            }
            _ => {
                self.prepared.insert(id, Prep::Immediate);
            }
        }
    }

    fn remembered(&self, tool: &str) -> Option<bool> {
        self.deps.tool_memory.as_ref()?.decision(tool)
    }

    /// Resolves the registered approval `id` without a card.
    fn auto_answer(&mut self, id: &str, tool: &str, approved: bool, why: AutoAnswer) {
        let decision = if approved {
            Decision::Approve
        } else {
            Decision::Deny
        };
        self.book.decide(id, decision);
        self.emit(ChatEvent::AutoAnswered {
            call_id: id.to_owned(),
            tool: tool.to_owned(),
            approved,
            why,
        });
    }

    async fn prepare_edit(&self, args: Option<&Value>) -> Result<(Proposal, Preview), EditError> {
        if !self.deps.config.propose_edit {
            return Err(EditError::Refused(
                "this agent profile cannot propose edits".into(),
            ));
        }
        let args = args.ok_or_else(|| EditError::Invalid("arguments are not valid JSON".into()))?;
        let proposal = Proposal::parse(args)?;
        let applier = self
            .deps
            .applier
            .clone()
            .ok_or_else(|| EditError::Refused("editing is not available".into()))?;
        let p = proposal.clone();
        let preview = tokio::task::spawn_blocking(move || applier.preview(&p))
            .await
            .map_err(|e| EditError::Refused(e.to_string()))??;
        Ok((proposal, preview))
    }

    /// The `tool` message content that answers `i`; `None` when the turn was aborted.
    async fn answer(&mut self, i: &Interrupt) -> Option<String> {
        let id = i.tool_call_id().to_owned();
        match i {
            Interrupt::Permission { .. } => {
                let r = self.wait(&id).await?;
                Some(r.hitl_payload())
            }
            Interrupt::Question { .. } => {
                let r = self.wait(&id).await?;
                Some(r.question_payload())
            }
            Interrupt::FrontendTool(call) => match call.name.as_str() {
                OPEN_PAGE_TOOL => {
                    let name = call
                        .args
                        .as_ref()
                        .and_then(|a| a.get("name"))
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned();
                    Some(tool_json(
                        self.deps
                            .host
                            .open_page(&name)
                            .map(|v| json!({"opened": v})),
                    ))
                }
                GET_SELECTION_TOOL => Some(tool_json(run_get_selection(
                    self.deps.host.as_ref(),
                    &self.guard(),
                ))),
                PROPOSE_EDIT_TOOL => self.answer_edit(&id).await,
                other => Some(tool_json(Err(format!("unknown tool `{other}`")))),
            },
            _ => Some(tool_json(Err("unsupported interrupt".into()))),
        }
    }

    async fn answer_edit(&mut self, id: &str) -> Option<String> {
        let proposal = match self.prepared.remove(id) {
            Some(Prep::Rejected(e)) => {
                return Some(
                    json!({"approved": false, "applied": false,
                           "error": {"code": e.code(), "message": e.to_string()}})
                    .to_string(),
                );
            }
            Some(Prep::Edit(p)) => p,
            _ => {
                return Some(tool_json(Err("the proposal is not available".into())));
            }
        };
        let resolution = self.wait(id).await?;
        match resolution {
            Resolution::Approved => {}
            Resolution::Denied(reason) => {
                return Some(
                    json!({"approved": false, "applied": false, "reason": reason.as_str()})
                        .to_string(),
                );
            }
            Resolution::Answered(_) => {
                return Some(json!({"approved": false, "applied": false}).to_string());
            }
        }
        let Some(applier) = self.deps.applier.clone() else {
            return Some(
                json!({"approved": true, "applied": false,
                               "error": {"code": "refused", "message": "editing is not available"}})
                .to_string(),
            );
        };
        let p = proposal.clone();
        let applied = tokio::task::spawn_blocking(move || applier.apply(&p)).await;
        let result = match applied {
            Ok(Ok(a)) => {
                self.emit(ChatEvent::EditApplied {
                    call_id: id.to_owned(),
                    audit_id: a.audit_id.clone(),
                    page: a.page.clone(),
                    affected: a.affected.clone(),
                });
                json!({"approved": true, "applied": true, "affected": a.affected})
            }
            Ok(Err(e)) => {
                self.emit(ChatEvent::EditFailed {
                    call_id: id.to_owned(),
                    code: e.code().to_owned(),
                    message: e.to_string(),
                });
                json!({"approved": true, "applied": false,
                       "error": {"code": e.code(), "message": e.to_string()}})
            }
            Err(e) => json!({"approved": true, "applied": false,
                             "error": {"code": "refused", "message": e.to_string()}}),
        };
        Some(result.to_string())
    }

    /// Waits for the resolution of `id`: an answer, its deadline, or an abort (`None`).
    async fn wait(&mut self, id: &str) -> Option<Resolution> {
        loop {
            // An abort wins over whatever was resolved: nothing is delivered after a close.
            if self.abort.is_some() {
                return None;
            }
            if let Some(r) = self.book.resolution(id) {
                return Some(r.clone());
            }
            let now = self.deps.clock.now();
            let expired = self.book.expire(now);
            if !expired.is_empty() {
                self.announce_resolved(&expired);
                continue;
            }
            let wait = self
                .book
                .next_deadline()
                .map_or(Duration::from_secs(1), |d| d.saturating_sub(now))
                .max(Duration::from_millis(1));
            tokio::select! {
                cmd = self.cmd_rx.recv(), if !self.cmd_closed => self.handle(cmd).await,
                () = tokio::time::sleep(wait) => {}
            }
        }
    }
}

fn tool_json(r: Result<Value, String>) -> String {
    match r {
        Ok(v) => json!({"ok": true, "result": v}).to_string(),
        Err(e) => json!({"ok": false, "error": e}).to_string(),
    }
}

// ---------------------------------------------------------------------------------------------
// The message model a view renders.

/// A tool call as the view shows it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ToolCallView {
    /// Call id.
    pub id: String,
    /// Tool name.
    pub name: String,
    /// Arguments received so far.
    pub args: String,
    /// Result, once there is one.
    pub result: Option<String>,
    /// When the call opened (live calls only).
    pub started: Option<std::time::Instant>,
    /// How long the call took, once its result arrived (live calls only).
    pub duration: Option<Duration>,
}

/// The shared-state document of a thread, as the agent header shows it (BIT-T-0456). Every field
/// is optional: the document is a projection Pando owns and may grow.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AgentState {
    /// Model name (or id).
    pub model: Option<String>,
    /// Provider of the model.
    pub provider: Option<String>,
    /// Prompt tokens in the context.
    pub prompt_tokens: u64,
    /// Completion tokens.
    pub completion_tokens: u64,
    /// Context window of the model (0 when unknown).
    pub context_window: u64,
    /// The counts are estimates.
    pub estimated: bool,
    /// Sub-agents the thread delegated to.
    pub sub_agents: Vec<SubAgent>,
}

/// A delegated task listed in the state document.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SubAgent {
    /// Task id.
    pub id: String,
    /// `running`, `completed`, `failed`, ...
    pub status: String,
    /// Fixed role, when it has one.
    pub role: String,
}

impl SubAgent {
    /// The task has not ended yet.
    #[must_use]
    pub fn is_running(&self) -> bool {
        matches!(
            self.status.to_lowercase().as_str(),
            "running" | "pending" | "queued" | "started" | "in_progress"
        )
    }
}

impl AgentState {
    /// Reads the document leniently: unknown or missing fields are skipped.
    #[must_use]
    pub fn from_value(v: &Value) -> Self {
        let text = |p: &str| {
            v.pointer(p)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
        };
        let num = |p: &str| v.pointer(p).and_then(Value::as_u64).unwrap_or(0);
        let context_window = match num("/tokenUsage/contextWindow") {
            0 => num("/model/contextWindow"),
            n => n,
        };
        Self {
            model: text("/model/name").or_else(|| text("/model/id")),
            provider: text("/model/provider"),
            prompt_tokens: num("/tokenUsage/promptTokens"),
            completion_tokens: num("/tokenUsage/completionTokens"),
            context_window,
            estimated: v
                .pointer("/tokenUsage/estimated")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            sub_agents: v
                .pointer("/subAgents")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .map(|s| SubAgent {
                            id: s["id"].as_str().unwrap_or_default().to_owned(),
                            status: s["status"].as_str().unwrap_or_default().to_owned(),
                            role: s["role"].as_str().unwrap_or_default().to_owned(),
                        })
                        .collect()
                })
                .unwrap_or_default(),
        }
    }

    /// Tokens in use (prompt plus completion).
    #[must_use]
    pub fn used_tokens(&self) -> u64 {
        self.prompt_tokens + self.completion_tokens
    }

    /// Sub-agents still working.
    #[must_use]
    pub fn running_sub_agents(&self) -> usize {
        self.sub_agents.iter().filter(|s| s.is_running()).count()
    }
}

/// State of an approval card in the transcript.
#[derive(Debug, Clone, PartialEq)]
pub enum CardState {
    /// Waiting for the user.
    Pending,
    /// Approved or answered.
    Approved,
    /// Denied; why.
    Denied(Option<DenyReason>),
}

/// An approval card with its state.
#[derive(Debug, Clone, PartialEq)]
pub struct CardView {
    /// The request.
    pub card: ApprovalCard,
    /// Where it stands.
    pub state: CardState,
}

/// One entry of the chat transcript.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ChatMessage {
    /// Message id.
    pub id: String,
    /// `user` or `assistant`.
    pub role: String,
    /// Text so far.
    pub text: String,
    /// Reasoning text so far.
    pub reasoning: String,
    /// Tool calls of this message.
    pub tool_calls: Vec<ToolCallView>,
    /// The turn that produced it was cancelled.
    pub cancelled: bool,
}

/// The transcript a chat view renders, built from [`ChatEvent`]s. Applying an event touches only
/// the message it belongs to, so a view can re-render just that one.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ChatModel {
    /// Messages in order.
    pub messages: Vec<ChatMessage>,
    /// Approval cards in order.
    pub cards: Vec<CardView>,
    /// A turn is running.
    pub running: bool,
    /// Last error shown to the user.
    pub error: Option<String>,
    /// The session ended.
    pub closed: bool,
    /// Model, token usage and sub-agents from the shared state; `None` before the first one.
    pub state: Option<AgentState>,
    /// Latest `ACTIVITY_SNAPSHOT` as a one-line progress text (cleared when the turn ends).
    pub activity: Option<String>,
}

impl ChatModel {
    /// Records the user's message (the session does not echo it).
    pub fn push_user(&mut self, text: &str) {
        self.messages.push(ChatMessage {
            id: format!("user-{}", self.messages.len()),
            role: "user".into(),
            text: text.to_owned(),
            ..ChatMessage::default()
        });
        self.running = true;
        self.error = None;
    }

    fn message(&mut self, id: &str) -> &mut ChatMessage {
        let i = match self.messages.iter().position(|m| m.id == id) {
            Some(i) => i,
            None => {
                self.messages.push(ChatMessage {
                    id: id.to_owned(),
                    role: "assistant".into(),
                    ..ChatMessage::default()
                });
                self.messages.len() - 1
            }
        };
        &mut self.messages[i]
    }

    fn call(&mut self, id: &str) -> Option<&mut ToolCallView> {
        self.messages
            .iter_mut()
            .flat_map(|m| m.tool_calls.iter_mut())
            .find(|c| c.id == id)
    }

    /// Folds one event into the model.
    pub fn apply(&mut self, ev: &ChatEvent) {
        match ev {
            ChatEvent::History(msgs) => {
                self.messages = msgs
                    .iter()
                    .filter(|m| m.role == "user" || m.role == "assistant")
                    .map(|m| ChatMessage {
                        id: m.id.clone(),
                        role: m.role.clone(),
                        text: match &m.content {
                            MessageContent::Text(t) => t.clone(),
                            MessageContent::Parts(_) => String::new(),
                        },
                        tool_calls: m
                            .tool_calls
                            .iter()
                            .map(|c| ToolCallView {
                                id: c.id.clone(),
                                name: c.function.name.clone(),
                                args: c.function.arguments.clone(),
                                ..ToolCallView::default()
                            })
                            .collect(),
                        ..ChatMessage::default()
                    })
                    .collect();
            }
            ChatEvent::RunStarted { .. } => self.running = true,
            ChatEvent::MessageStart { message_id } => {
                self.message(message_id);
            }
            ChatEvent::TextDelta { message_id, delta } => {
                self.message(message_id).text.push_str(delta);
            }
            ChatEvent::ReasoningDelta { message_id, delta } => {
                self.message(message_id).reasoning.push_str(delta);
            }
            ChatEvent::ToolCallStart { call_id, name } => {
                let target = match self.messages.last() {
                    Some(m) if m.role == "assistant" => m.id.clone(),
                    _ => format!("assistant-{}", self.messages.len()),
                };
                self.message(&target).tool_calls.push(ToolCallView {
                    id: call_id.clone(),
                    name: name.clone(),
                    started: Some(std::time::Instant::now()),
                    ..ToolCallView::default()
                });
            }
            ChatEvent::ToolCallArgs { call_id, delta } => {
                if let Some(c) = self.call(call_id) {
                    c.args.push_str(delta);
                }
            }
            ChatEvent::ToolCallResult { call_id, content } => {
                if let Some(c) = self.call(call_id) {
                    c.result = Some(content.clone());
                    c.duration = c.started.map(|t| t.elapsed());
                }
            }
            ChatEvent::ApprovalRequested(card) => self.cards.push(CardView {
                card: card.clone(),
                state: CardState::Pending,
            }),
            ChatEvent::ApprovalResolved {
                id,
                approved,
                reason,
            } => {
                if let Some(c) = self.cards.iter_mut().find(|c| c.card.id == *id) {
                    c.state = if *approved {
                        CardState::Approved
                    } else {
                        CardState::Denied(*reason)
                    };
                }
            }
            ChatEvent::EditRejected { message, .. } | ChatEvent::EditFailed { message, .. } => {
                self.error = Some(message.clone());
            }
            ChatEvent::Error { message, .. } => self.error = Some(message.clone()),
            ChatEvent::State(doc) => self.state = Some(AgentState::from_value(doc)),
            ChatEvent::Activity { kind, content, .. } => {
                self.activity = Some(activity_line(kind, content));
            }
            ChatEvent::RunFinished(end) => {
                self.running = false;
                self.activity = None;
                if *end == RunEnd::Cancelled
                    && let Some(m) = self
                        .messages
                        .iter_mut()
                        .rev()
                        .find(|m| m.role == "assistant")
                {
                    m.cancelled = true;
                }
            }
            ChatEvent::Closed(_) => {
                self.running = false;
                self.closed = true;
            }
            ChatEvent::MessageEnd { .. }
            | ChatEvent::ToolCallEnd { .. }
            | ChatEvent::Custom { .. }
            | ChatEvent::AutoAnswered { .. }
            | ChatEvent::EditApplied { .. } => {}
        }
    }
}

/// One line for an `ACTIVITY_SNAPSHOT`: its `title`/`message`/`text` when it has one, else its
/// type.
fn activity_line(kind: &str, content: &Value) -> String {
    ["title", "message", "text", "status"]
        .iter()
        .find_map(|k| content.get(*k).and_then(Value::as_str))
        .or_else(|| content.as_str())
        .map_or_else(|| kind.to_owned(), |t| t.trim().to_owned())
}

#[cfg(test)]
mod model_choice_tests {
    use super::*;
    use pando::agui::{AgentDescriptor, Info, ModelDescriptor};

    fn agent(name: &str, model: &str) -> AgentDescriptor {
        AgentDescriptor {
            name: name.into(),
            model: (!model.is_empty()).then(|| ModelDescriptor {
                id: model.into(),
                name: format!("{model} name"),
                ..ModelDescriptor::default()
            }),
            ..AgentDescriptor::default()
        }
    }

    #[test]
    fn profile_names_are_toml_bare_keys() {
        assert_eq!(
            chat_profile_for_model("Claude-Sonnet_4.5"),
            "bitacora-chat--claude-sonnet_4-5"
        );
        assert_eq!(
            chat_profile_for_model("openai/gpt 4"),
            "bitacora-chat--openai-gpt-4"
        );
    }

    #[test]
    fn selector_maps_the_choice_to_a_profile() {
        let info = Info {
            agents: vec![
                agent("coder", "x"),
                agent("bitacora-chat", "default-m"),
                agent("bitacora-chat--m1", "m1"),
                agent("bitacora-writer", "w"),
            ],
            ..Info::default()
        };
        let c = ModelChoices::from_info(&info);
        assert_eq!(c.default.as_ref().unwrap().model_id, "default-m");
        assert_eq!(c.extra.len(), 1);
        assert!(!c.is_empty());
        assert_eq!(c.profile_for(None), CHAT_PROFILE);
        assert_eq!(c.profile_for(Some("m1")), "bitacora-chat--m1");
        // A remembered model the server no longer exposes falls back to the default.
        assert_eq!(c.profile_for(Some("gone")), CHAT_PROFILE);
        assert!(ModelChoices::from_info(&Info::default()).is_empty());
    }
}
