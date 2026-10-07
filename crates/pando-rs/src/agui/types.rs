//! Wire types of the AG-UI protocol as Pando speaks it.
//!
//! Everything here is deliberately tolerant: unknown fields are ignored, missing fields default,
//! and an event whose `type` we do not model (or whose payload does not fit) becomes
//! [`Event::Unknown`] with the raw JSON instead of failing the run. Shapes were verified against
//! Pando `internal/agui/{events,input,threads}.go`.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Message roles defined by AG-UI.
pub mod role {
    /// Developer message.
    pub const DEVELOPER: &str = "developer";
    /// System message.
    pub const SYSTEM: &str = "system";
    /// Assistant message.
    pub const ASSISTANT: &str = "assistant";
    /// User message.
    pub const USER: &str = "user";
    /// Tool result message.
    pub const TOOL: &str = "tool";
}

/// `RUN_FINISHED` outcome of a run that completed on its own.
pub const OUTCOME_SUCCESS: &str = "success";
/// `RUN_FINISHED` outcome of a run that stopped waiting for the client (a frontend tool result or
/// a human decision). Resume it by sending a `tool` message on the same thread.
pub const OUTCOME_INTERRUPT: &str = "interrupt";

/// One part of a multimodal message.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ContentPart {
    /// `text`, `image`, `audio`, `video` or `document`.
    #[serde(rename = "type")]
    pub kind: String,
    /// Text of a `text` part.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub text: String,
    /// URL of a non-text part.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub url: String,
    /// Inline data of a non-text part.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub data: String,
    /// MIME type of a non-text part.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub mime_type: String,
}

/// Message content: a plain string or a list of parts (AG-UI allows both on user messages).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    /// Plain text.
    Text(String),
    /// Multimodal parts.
    Parts(Vec<ContentPart>),
}

impl Default for MessageContent {
    fn default() -> Self {
        Self::Text(String::new())
    }
}

impl MessageContent {
    /// Flattens the content to text, joining the text parts of a multimodal message.
    pub fn text(&self) -> String {
        match self {
            Self::Text(t) => t.clone(),
            Self::Parts(parts) => parts
                .iter()
                .filter(|p| p.kind == "text" && !p.text.is_empty())
                .map(|p| p.text.as_str())
                .collect::<Vec<_>>()
                .join("\n"),
        }
    }

    fn is_empty(&self) -> bool {
        match self {
            Self::Text(t) => t.is_empty(),
            Self::Parts(p) => p.is_empty(),
        }
    }
}

impl From<&str> for MessageContent {
    fn from(value: &str) -> Self {
        Self::Text(value.to_owned())
    }
}

impl From<String> for MessageContent {
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}

/// OpenAI-shaped function payload of a tool call.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ToolCallFunction {
    /// Tool name.
    pub name: String,
    /// Arguments as a JSON string (possibly still incomplete while streaming).
    pub arguments: String,
}

/// An assistant-issued tool call.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ToolCall {
    /// Call id.
    pub id: String,
    /// Always `function` in practice.
    #[serde(rename = "type", skip_serializing_if = "String::is_empty")]
    pub kind: String,
    /// Name and arguments.
    pub function: ToolCallFunction,
}

/// One entry of the AG-UI conversation.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Message {
    /// Message id.
    pub id: String,
    /// One of the [`role`] constants (kept as a string: unknown roles are preserved).
    pub role: String,
    /// Text or multimodal content.
    #[serde(skip_serializing_if = "MessageContent::is_empty")]
    pub content: MessageContent,
    /// Optional author name (tool name on `tool` messages).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// Tool calls issued by an assistant message.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
    /// The call a `tool` message answers.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub tool_call_id: String,
    /// Client-side failure of a tool call (read by Pando as a denial / cancellation).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub error: String,
    /// Activity type of an `activity` message.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub activity_type: String,
}

impl Message {
    /// A user message with a fresh id.
    pub fn user(content: impl Into<MessageContent>) -> Self {
        Self {
            id: new_id("msg"),
            role: role::USER.to_owned(),
            content: content.into(),
            ..Self::default()
        }
    }

    /// A `tool` message answering `tool_call_id` (resumes an interrupted run).
    pub fn tool_result(
        tool_call_id: impl Into<String>,
        content: impl Into<MessageContent>,
    ) -> Self {
        Self {
            id: new_id("msg"),
            role: role::TOOL.to_owned(),
            content: content.into(),
            tool_call_id: tool_call_id.into(),
            ..Self::default()
        }
    }

    /// A `tool` message reporting that the client could not run the call (a denial for
    /// permission prompts, a cancellation for questions).
    pub fn tool_error(tool_call_id: impl Into<String>, error: impl Into<String>) -> Self {
        Self {
            id: new_id("msg"),
            role: role::TOOL.to_owned(),
            tool_call_id: tool_call_id.into(),
            error: error.into(),
            ..Self::default()
        }
    }
}

/// A frontend-declared tool: the agent may call it, the client executes it and answers with a
/// `tool` message on the next run.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Tool {
    /// Tool name.
    pub name: String,
    /// What the tool does, for the model.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// JSON Schema of the arguments.
    #[serde(skip_serializing_if = "Value::is_null")]
    pub parameters: Value,
}

/// Ambient information the client attaches to a run.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ContextEntry {
    /// What the value is.
    pub description: String,
    /// The value.
    pub value: String,
}

/// Request body of a run (`RunAgentInput`). The client owns the visible transcript and resends it
/// in full on every turn.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct RunInput {
    /// Thread (conversation) id; reuse it across turns.
    pub thread_id: String,
    /// Id of this run.
    pub run_id: String,
    /// Parent run, for sub-runs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_run_id: Option<String>,
    /// Client-owned state, echoed back as `state.client`.
    #[serde(skip_serializing_if = "Value::is_null")]
    pub state: Value,
    /// The transcript.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub messages: Vec<Message>,
    /// Frontend tools; the agent calling one interrupts the run.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<Tool>,
    /// Ambient context.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub context: Vec<ContextEntry>,
    /// Forwarded verbatim (currently ignored by Pando).
    #[serde(skip_serializing_if = "Value::is_null")]
    pub forwarded_props: Value,
}

impl RunInput {
    /// An input for a new thread and run with generated ids.
    pub fn new() -> Self {
        Self {
            thread_id: new_id("thread"),
            run_id: new_id("run"),
            ..Self::default()
        }
    }

    /// An input continuing `thread_id` with a fresh run id.
    pub fn for_thread(thread_id: impl Into<String>) -> Self {
        Self {
            thread_id: thread_id.into(),
            run_id: new_id("run"),
            ..Self::default()
        }
    }

    /// Appends a user message.
    pub fn with_prompt(mut self, prompt: impl Into<MessageContent>) -> Self {
        self.messages.push(Message::user(prompt));
        self
    }

    /// Replaces the transcript.
    pub fn with_messages(mut self, messages: Vec<Message>) -> Self {
        self.messages = messages;
        self
    }

    /// Declares frontend tools.
    pub fn with_tools(mut self, tools: Vec<Tool>) -> Self {
        self.tools = tools;
        self
    }

    /// Attaches ambient context.
    pub fn with_context(mut self, context: Vec<ContextEntry>) -> Self {
        self.context = context;
        self
    }

    /// Sets the client-owned state.
    pub fn with_state(mut self, state: Value) -> Self {
        self.state = state;
        self
    }
}

/// One RFC-6902 operation carried by `STATE_DELTA`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PatchOp {
    /// `add`, `remove`, `replace`, `move`, `copy` or `test`.
    pub op: String,
    /// JSON pointer of the target.
    pub path: String,
    /// New value (`add`, `replace`, `test`).
    #[serde(skip_serializing_if = "Value::is_null")]
    pub value: Value,
    /// Source pointer (`move`, `copy`).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub from: String,
}

/// An AG-UI event. Unmodelled or malformed events are kept as [`Event::Unknown`].
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "type")]
#[non_exhaustive]
pub enum Event {
    /// A run (or a resumed segment of one) started.
    #[serde(rename = "RUN_STARTED", rename_all = "camelCase")]
    RunStarted {
        /// Thread id.
        #[serde(default)]
        thread_id: String,
        /// Run id.
        #[serde(default)]
        run_id: String,
        /// Parent run, if any.
        #[serde(default)]
        parent_run_id: Option<String>,
    },
    /// A run finished; see [`OUTCOME_INTERRUPT`].
    #[serde(rename = "RUN_FINISHED", rename_all = "camelCase")]
    RunFinished {
        /// Thread id.
        #[serde(default)]
        thread_id: String,
        /// Run id.
        #[serde(default)]
        run_id: String,
        /// `success`, `interrupt`, or absent.
        #[serde(default)]
        outcome: Option<String>,
        /// Optional result payload.
        #[serde(default)]
        result: Option<Value>,
    },
    /// A run failed (`code` is `session_busy`, `cancelled`, or absent).
    #[serde(rename = "RUN_ERROR")]
    RunError {
        /// Message.
        #[serde(default)]
        message: String,
        /// Code.
        #[serde(default)]
        code: Option<String>,
    },
    /// A step started.
    #[serde(rename = "STEP_STARTED", rename_all = "camelCase")]
    StepStarted {
        /// Step name.
        #[serde(default)]
        step_name: String,
    },
    /// A step finished.
    #[serde(rename = "STEP_FINISHED", rename_all = "camelCase")]
    StepFinished {
        /// Step name.
        #[serde(default)]
        step_name: String,
    },
    /// An assistant text message opened.
    #[serde(rename = "TEXT_MESSAGE_START", rename_all = "camelCase")]
    TextMessageStart {
        /// Message id.
        #[serde(default)]
        message_id: String,
        /// Role (assistant).
        #[serde(default)]
        role: String,
    },
    /// A chunk of assistant text.
    #[serde(rename = "TEXT_MESSAGE_CONTENT", rename_all = "camelCase")]
    TextMessageContent {
        /// Message id.
        #[serde(default)]
        message_id: String,
        /// Text delta.
        #[serde(default)]
        delta: String,
    },
    /// An assistant text message closed.
    #[serde(rename = "TEXT_MESSAGE_END", rename_all = "camelCase")]
    TextMessageEnd {
        /// Message id.
        #[serde(default)]
        message_id: String,
    },
    /// A tool call opened.
    #[serde(rename = "TOOL_CALL_START", rename_all = "camelCase")]
    ToolCallStart {
        /// Call id.
        #[serde(default)]
        tool_call_id: String,
        /// Tool name.
        #[serde(default)]
        tool_call_name: String,
        /// Assistant message the call belongs to.
        #[serde(default)]
        parent_message_id: Option<String>,
    },
    /// A chunk of a tool call's JSON arguments.
    #[serde(rename = "TOOL_CALL_ARGS", rename_all = "camelCase")]
    ToolCallArgs {
        /// Call id.
        #[serde(default)]
        tool_call_id: String,
        /// Arguments delta.
        #[serde(default)]
        delta: String,
    },
    /// A tool call's arguments are complete.
    #[serde(rename = "TOOL_CALL_END", rename_all = "camelCase")]
    ToolCallEnd {
        /// Call id.
        #[serde(default)]
        tool_call_id: String,
    },
    /// A tool call's result (not JSON: Pando renders TOON/TOML/text).
    #[serde(rename = "TOOL_CALL_RESULT", rename_all = "camelCase")]
    ToolCallResult {
        /// Id of the result message.
        #[serde(default)]
        message_id: String,
        /// Call id.
        #[serde(default)]
        tool_call_id: String,
        /// Result text.
        #[serde(default)]
        content: String,
        /// Role, normally `tool`.
        #[serde(default)]
        role: Option<String>,
    },
    /// Full shared-state document.
    #[serde(rename = "STATE_SNAPSHOT")]
    StateSnapshot {
        /// The document.
        #[serde(default)]
        snapshot: Value,
    },
    /// RFC-6902 patch of the shared-state document.
    #[serde(rename = "STATE_DELTA")]
    StateDelta {
        /// Operations.
        #[serde(default)]
        delta: Vec<PatchOp>,
    },
    /// The thread's transcript (resync of a pre-existing thread).
    #[serde(rename = "MESSAGES_SNAPSHOT")]
    MessagesSnapshot {
        /// Messages, newest last.
        #[serde(default)]
        messages: Vec<Message>,
        /// The adapter cut the transcript to its cap: index 0 is not the conversation start.
        #[serde(default)]
        truncated: bool,
    },
    /// Activity snapshot.
    #[serde(rename = "ACTIVITY_SNAPSHOT", rename_all = "camelCase")]
    ActivitySnapshot {
        /// Message id.
        #[serde(default)]
        message_id: String,
        /// Activity type.
        #[serde(default)]
        activity_type: String,
        /// Content.
        #[serde(default)]
        content: Value,
        /// Replace the previous snapshot.
        #[serde(default)]
        replace: bool,
    },
    /// A reasoning block opened.
    #[serde(rename = "REASONING_START", rename_all = "camelCase")]
    ReasoningStart {
        /// Message id.
        #[serde(default)]
        message_id: String,
    },
    /// A reasoning message opened.
    #[serde(rename = "REASONING_MESSAGE_START", rename_all = "camelCase")]
    ReasoningMessageStart {
        /// Message id.
        #[serde(default)]
        message_id: String,
    },
    /// A chunk of reasoning text.
    #[serde(rename = "REASONING_MESSAGE_CONTENT", rename_all = "camelCase")]
    ReasoningMessageContent {
        /// Message id.
        #[serde(default)]
        message_id: String,
        /// Text delta.
        #[serde(default)]
        delta: String,
    },
    /// A reasoning message closed.
    #[serde(rename = "REASONING_MESSAGE_END", rename_all = "camelCase")]
    ReasoningMessageEnd {
        /// Message id.
        #[serde(default)]
        message_id: String,
    },
    /// A reasoning block closed.
    #[serde(rename = "REASONING_END", rename_all = "camelCase")]
    ReasoningEnd {
        /// Message id.
        #[serde(default)]
        message_id: String,
    },
    /// Application signal; Pando uses the `pando.*` namespace.
    #[serde(rename = "CUSTOM")]
    Custom {
        /// Event name.
        #[serde(default)]
        name: String,
        /// Payload.
        #[serde(default)]
        value: Value,
    },
    /// Passthrough of a foreign event.
    #[serde(rename = "RAW")]
    Raw {
        /// The foreign event.
        #[serde(default)]
        event: Value,
        /// Its source.
        #[serde(default)]
        source: Option<String>,
    },
    /// An event type this SDK does not model, or one whose payload did not fit; never fatal.
    #[serde(skip)]
    Unknown {
        /// The `type` discriminator (empty when absent).
        event_type: String,
        /// The raw JSON object.
        data: Value,
    },
}

impl Event {
    /// Decodes one SSE `data:` payload. Valid JSON always yields an event (possibly
    /// [`Event::Unknown`]); `None` means the payload was not JSON at all.
    pub fn parse(payload: &str) -> Option<Self> {
        let value: Value = serde_json::from_str(payload).ok()?;
        Some(Self::from_value(value))
    }

    /// Decodes an already-parsed JSON value, falling back to [`Event::Unknown`].
    pub fn from_value(value: Value) -> Self {
        match serde_json::from_value::<Self>(value.clone()) {
            Ok(event) => event,
            Err(_) => Self::Unknown {
                event_type: value
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                data: value,
            },
        }
    }

    /// The protocol discriminator, for logging.
    pub fn type_name(&self) -> &str {
        match self {
            Self::RunStarted { .. } => "RUN_STARTED",
            Self::RunFinished { .. } => "RUN_FINISHED",
            Self::RunError { .. } => "RUN_ERROR",
            Self::StepStarted { .. } => "STEP_STARTED",
            Self::StepFinished { .. } => "STEP_FINISHED",
            Self::TextMessageStart { .. } => "TEXT_MESSAGE_START",
            Self::TextMessageContent { .. } => "TEXT_MESSAGE_CONTENT",
            Self::TextMessageEnd { .. } => "TEXT_MESSAGE_END",
            Self::ToolCallStart { .. } => "TOOL_CALL_START",
            Self::ToolCallArgs { .. } => "TOOL_CALL_ARGS",
            Self::ToolCallEnd { .. } => "TOOL_CALL_END",
            Self::ToolCallResult { .. } => "TOOL_CALL_RESULT",
            Self::StateSnapshot { .. } => "STATE_SNAPSHOT",
            Self::StateDelta { .. } => "STATE_DELTA",
            Self::MessagesSnapshot { .. } => "MESSAGES_SNAPSHOT",
            Self::ActivitySnapshot { .. } => "ACTIVITY_SNAPSHOT",
            Self::ReasoningStart { .. } => "REASONING_START",
            Self::ReasoningMessageStart { .. } => "REASONING_MESSAGE_START",
            Self::ReasoningMessageContent { .. } => "REASONING_MESSAGE_CONTENT",
            Self::ReasoningMessageEnd { .. } => "REASONING_MESSAGE_END",
            Self::ReasoningEnd { .. } => "REASONING_END",
            Self::Custom { .. } => "CUSTOM",
            Self::Raw { .. } => "RAW",
            Self::Unknown { event_type, .. } => event_type,
        }
    }
}

/// Model behind an agent (`GET {path}/info`).
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ModelDescriptor {
    /// Model id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Provider.
    pub provider: String,
    /// Context window in tokens.
    pub context_window: i64,
}

/// One agent of the discovery document.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct AgentDescriptor {
    /// Agent name (the route segment).
    pub name: String,
    /// Description.
    pub description: String,
    /// Absolute run URL as the server sees itself (unreliable behind proxies).
    pub url: String,
    /// Model, when known.
    pub model: Option<ModelDescriptor>,
}

/// Optional protocol halves a deployment implements.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Capabilities {
    /// Frontend tools.
    pub frontend_tools: bool,
    /// Permission and question prompts reach the client.
    pub human_in_the_loop: bool,
    /// Shared-state snapshots and deltas.
    pub shared_state: bool,
    /// `RUN_FINISHED{outcome:"interrupt"}`.
    pub interrupts: bool,
}

/// `GET {path}/info`.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Info {
    /// Protocol name.
    pub protocol: String,
    /// Adapter version.
    pub version: String,
    /// Route prefix.
    pub path: String,
    /// Agents.
    pub agents: Vec<AgentDescriptor>,
    /// Capabilities.
    pub capabilities: Capabilities,
}

/// `GET {path}/healthz`: liveness plus the concurrency gauge (no secrets).
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Health {
    /// `ok` when serving.
    pub status: String,
    /// Adapter version.
    pub version: String,
    /// Process uptime.
    pub uptime_seconds: f64,
    /// Runs currently admitted (parked runs count).
    pub active_runs: u32,
    /// Admission cap, `0` when unlimited.
    pub max_concurrent_runs: u32,
    /// The server is shutting down and rejects new runs.
    pub draining: bool,
}

/// One entry of `GET {path}/threads`.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ThreadSummary {
    /// Thread id.
    pub thread_id: String,
    /// Pando session bound to it.
    pub session_id: String,
    /// Agent that owns it.
    pub agent: String,
    /// Last update, as stored by the server.
    pub updated_at: String,
}

/// A page of `GET {path}/threads` (newest first).
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ThreadsPage {
    /// Threads of this page.
    pub threads: Vec<ThreadSummary>,
    /// Page size the server applied.
    pub limit: u32,
    /// Offset of this page.
    pub offset: u32,
    /// More pages follow.
    pub has_more: bool,
}

/// Process-unique id of the form `prefix-<hex time>-<hex counter>-<hex pid>`. Pass your own ids
/// when cross-process uniqueness matters.
pub fn new_id(prefix: &str) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}-{nanos:x}-{n:x}-{:x}", std::process::id())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_and_malformed_events_are_preserved() {
        let e = Event::parse(r#"{"type":"FUTURE_EVENT","x":1}"#).unwrap();
        assert!(matches!(&e, Event::Unknown { event_type, .. } if event_type == "FUTURE_EVENT"));
        // Known type with a payload of the wrong shape degrades instead of failing.
        let e = Event::parse(r#"{"type":"TEXT_MESSAGE_CONTENT","delta":5}"#).unwrap();
        assert_eq!(e.type_name(), "TEXT_MESSAGE_CONTENT");
        assert!(matches!(e, Event::Unknown { .. }));
        assert!(Event::parse("not json").is_none());
    }

    #[test]
    fn run_input_serializes_camel_case_and_omits_empties() {
        let input = RunInput::for_thread("t1").with_prompt("hi");
        let v = serde_json::to_value(&input).unwrap();
        assert_eq!(v["threadId"], "t1");
        assert!(v.get("tools").is_none() && v.get("state").is_none());
        assert_eq!(v["messages"][0]["role"], "user");
        assert_eq!(v["messages"][0]["content"], "hi");
    }

    #[test]
    fn message_content_accepts_both_shapes() {
        let m: Message = serde_json::from_str(
            r#"{"id":"1","role":"user","content":[{"type":"text","text":"a"},{"type":"image","url":"u"}]}"#,
        )
        .unwrap();
        assert_eq!(m.content.text(), "a");
        let m: Message =
            serde_json::from_str(r#"{"id":"1","role":"tool","toolCallId":"c"}"#).unwrap();
        assert_eq!(m.tool_call_id, "c");
    }
}
