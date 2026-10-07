//! [`Thread`]: a stateful transcript, shared-state document and interrupt tracker over
//! [`AguiClient`], mirroring the reducer of Pando's TypeScript `PandoThread`.

use std::collections::HashMap;

use serde_json::Value;

use super::client::{AguiClient, RunStream};
use super::hitl::Interrupt;
use super::patch;
use super::types::{
    ContextEntry, Event, Message, MessageContent, OUTCOME_INTERRUPT, RunInput, Tool, ToolCall,
    ToolCallFunction, new_id, role,
};
use crate::error::{Error, Result};

/// A tool call the agent is blocked on.
#[derive(Debug, Clone, PartialEq)]
pub struct PendingToolCall {
    /// Call id; pass it to [`Thread::resume`].
    pub id: String,
    /// Tool name.
    pub name: String,
    /// Arguments as received.
    pub args_text: String,
    /// `args_text` parsed, when it is valid JSON.
    pub args: Option<Value>,
}

/// How a drained run ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunOutcome {
    /// `RUN_FINISHED` with `success` (or no) outcome.
    Finished,
    /// `RUN_FINISHED{outcome:"interrupt"}`: answer [`Thread::interrupts`] and resume.
    Interrupted,
}

/// One conversation with an agent. The thread owns the transcript and resends it on every turn.
#[derive(Debug)]
pub struct Thread {
    client: AguiClient,
    thread_id: String,
    agent: Option<String>,
    tools: Vec<Tool>,
    context: Vec<ContextEntry>,
    /// The transcript, exactly what the next run sends.
    pub messages: Vec<Message>,
    /// Reasoning text keyed by the assistant message id it belongs to.
    pub reasoning: HashMap<String, String>,
    /// Every `CUSTOM` event seen, as `(name, value)` (`pando.*` signals).
    pub custom_events: Vec<(String, Value)>,
    /// The shared-state document (`STATE_SNAPSHOT` + `STATE_DELTA`); `None` before the first run.
    pub state: Option<Value>,
    /// A `STATE_DELTA` could not be applied; `state` may be stale until the next snapshot.
    pub state_desynced: bool,
    /// The last `MESSAGES_SNAPSHOT` and whether the server truncated it. It is not merged into
    /// `messages` (ids differ between client and server); call [`Thread::load_history`] to adopt
    /// the server transcript instead.
    pub last_snapshot: Option<(Vec<Message>, bool)>,
    interrupted: bool,
    pending: Vec<String>,
}

impl Thread {
    /// A new thread with a generated id.
    pub fn new(client: AguiClient) -> Self {
        Self::with_id(client, new_id("thread"))
    }

    /// A thread bound to an existing id (for example one from [`AguiClient::list_threads`]).
    pub fn with_id(client: AguiClient, thread_id: impl Into<String>) -> Self {
        Self {
            client,
            thread_id: thread_id.into(),
            agent: None,
            tools: Vec::new(),
            context: Vec::new(),
            messages: Vec::new(),
            reasoning: HashMap::new(),
            custom_events: Vec::new(),
            state: None,
            state_desynced: false,
            last_snapshot: None,
            interrupted: false,
            pending: Vec::new(),
        }
    }

    /// Runs on `agent` instead of the client's default.
    pub fn with_agent(mut self, agent: impl Into<String>) -> Self {
        self.agent = Some(agent.into());
        self
    }

    /// Declares frontend tools on every run.
    pub fn with_tools(mut self, tools: Vec<Tool>) -> Self {
        self.tools = tools;
        self
    }

    /// Ambient context sent with the following runs (replaces the previous one; pass an empty
    /// list to send none). It is not part of the transcript.
    pub fn set_context(&mut self, context: Vec<ContextEntry>) {
        self.context = context;
    }

    /// The thread id.
    pub fn thread_id(&self) -> &str {
        &self.thread_id
    }

    /// The last run ended with `interrupt`.
    pub fn is_interrupted(&self) -> bool {
        self.interrupted
    }

    /// Tool calls the agent is blocked on; empty unless [`Thread::is_interrupted`].
    pub fn pending_tool_calls(&self) -> Vec<PendingToolCall> {
        if !self.interrupted {
            return Vec::new();
        }
        self.pending
            .iter()
            .filter_map(|id| self.find_call(id))
            .map(|(call, _)| PendingToolCall {
                id: call.id.clone(),
                name: call.function.name.clone(),
                args_text: call.function.arguments.clone(),
                args: serde_json::from_str(&call.function.arguments).ok(),
            })
            .collect()
    }

    /// The pending calls classified as permission prompts, questions or frontend tools.
    pub fn interrupts(&self) -> Vec<Interrupt> {
        self.pending_tool_calls()
            .iter()
            .map(Interrupt::classify)
            .collect()
    }

    /// Replaces the transcript with the server's persisted one. Use it when attaching to an
    /// existing thread. Returns `false` (leaving the transcript alone) when the server does not
    /// know the thread.
    pub async fn load_history(&mut self) -> Result<bool> {
        match self.client.thread_messages(&self.thread_id).await? {
            Some(messages) => {
                self.messages = messages;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    /// Sends a user message and streams the run. The message joins the transcript only once the
    /// server accepted the run.
    pub async fn send(&mut self, prompt: impl Into<MessageContent>) -> Result<ThreadRun<'_>> {
        self.start(Message::user(prompt)).await
    }

    /// Answers an interrupted run's tool call and resumes it. `result` is the tool output, or the
    /// payload of a [`crate::agui::hitl`] helper for prompts.
    pub async fn resume(
        &mut self,
        tool_call_id: &str,
        result: impl Into<MessageContent>,
    ) -> Result<ThreadRun<'_>> {
        self.start(Message::tool_result(tool_call_id, result)).await
    }

    /// Resumes with an arbitrary `tool` message (for example [`Message::tool_error`]).
    pub async fn resume_with(&mut self, message: Message) -> Result<ThreadRun<'_>> {
        self.start(message).await
    }

    /// Reattaches to this thread's live run (events still reduce into the thread). `None` when no
    /// run is live.
    pub async fn attach(&mut self) -> Result<Option<ThreadRun<'_>>> {
        Ok(self
            .client
            .attach(&self.thread_id)
            .await?
            .map(|stream| ThreadRun {
                thread: self,
                stream,
            }))
    }

    /// Cancels this thread's run on the server.
    pub async fn cancel(&self) -> Result<()> {
        self.client.cancel_run(&self.thread_id).await
    }

    async fn start(&mut self, message: Message) -> Result<ThreadRun<'_>> {
        let mut transcript = self.messages.clone();
        transcript.push(message.clone());
        let input = RunInput::for_thread(self.thread_id.clone())
            .with_messages(transcript)
            .with_tools(self.tools.clone())
            .with_context(self.context.clone());
        let agent = self.agent.as_deref().unwrap_or_else(|| self.client.agent());
        let stream = self.client.run_agent(agent, &input).await?;

        // Accepted: commit. The server never echoes the result of a call the client just answered.
        if message.role == role::TOOL {
            self.pending.retain(|id| *id != message.tool_call_id);
        }
        self.messages.push(message);
        Ok(ThreadRun {
            thread: self,
            stream,
        })
    }

    fn find_call(&self, id: &str) -> Option<(&ToolCall, usize)> {
        self.messages
            .iter()
            .enumerate()
            .find_map(|(i, m)| m.tool_calls.iter().find(|c| c.id == id).map(|c| (c, i)))
    }

    fn assistant_message(&mut self, id: &str) -> &mut Message {
        let index = match self.messages.iter().position(|m| m.id == id) {
            Some(i) => i,
            None => {
                self.messages.push(Message {
                    id: id.to_owned(),
                    role: role::ASSISTANT.to_owned(),
                    ..Message::default()
                });
                self.messages.len() - 1
            }
        };
        &mut self.messages[index]
    }

    /// Folds one event into the transcript, state and interrupt tracking.
    pub fn reduce(&mut self, event: &Event) {
        match event {
            Event::RunStarted { .. } => self.interrupted = false,
            Event::TextMessageStart { message_id, .. } => {
                self.assistant_message(message_id);
            }
            Event::TextMessageContent { message_id, delta } => {
                let msg = self.assistant_message(message_id);
                let mut text = match &msg.content {
                    MessageContent::Text(t) => t.clone(),
                    MessageContent::Parts(_) => String::new(),
                };
                text.push_str(delta);
                msg.content = MessageContent::Text(text);
            }
            Event::ReasoningMessageContent { message_id, delta } => {
                self.reasoning
                    .entry(message_id.clone())
                    .or_default()
                    .push_str(delta);
            }
            Event::ToolCallStart {
                tool_call_id,
                tool_call_name,
                parent_message_id,
            } => {
                let parent = match parent_message_id {
                    Some(id) if !id.is_empty() => id.clone(),
                    _ => match self.messages.last() {
                        Some(m) if m.role == role::ASSISTANT => m.id.clone(),
                        _ => new_id("msg"),
                    },
                };
                self.assistant_message(&parent).tool_calls.push(ToolCall {
                    id: tool_call_id.clone(),
                    kind: "function".to_owned(),
                    function: ToolCallFunction {
                        name: tool_call_name.clone(),
                        arguments: String::new(),
                    },
                });
            }
            Event::ToolCallArgs {
                tool_call_id,
                delta,
            } => {
                if let Some(call) = self
                    .messages
                    .iter_mut()
                    .flat_map(|m| m.tool_calls.iter_mut())
                    .find(|c| c.id == *tool_call_id)
                {
                    call.function.arguments.push_str(delta);
                }
            }
            Event::ToolCallEnd { tool_call_id } => {
                if self.find_call(tool_call_id).is_some() && !self.pending.contains(tool_call_id) {
                    self.pending.push(tool_call_id.clone());
                }
            }
            Event::ToolCallResult {
                message_id,
                tool_call_id,
                content,
                role: msg_role,
            } => {
                self.messages.push(Message {
                    id: message_id.clone(),
                    role: msg_role.clone().unwrap_or_else(|| role::TOOL.to_owned()),
                    content: MessageContent::Text(content.clone()),
                    tool_call_id: tool_call_id.clone(),
                    ..Message::default()
                });
                self.pending.retain(|id| id != tool_call_id);
            }
            Event::StateSnapshot { snapshot } => {
                self.state = Some(snapshot.clone());
                self.state_desynced = false;
            }
            Event::StateDelta { delta } => {
                let doc = self.state.get_or_insert(Value::Null);
                for op in delta {
                    if patch::apply(doc, op).is_err() {
                        self.state_desynced = true;
                    }
                }
            }
            Event::MessagesSnapshot {
                messages,
                truncated,
            } => self.last_snapshot = Some((messages.clone(), *truncated)),
            Event::Custom { name, value } => {
                self.custom_events.push((name.clone(), value.clone()));
            }
            Event::RunFinished { outcome, .. } => {
                self.interrupted = outcome.as_deref() == Some(OUTCOME_INTERRUPT);
            }
            _ => {}
        }
    }
}

/// A run in progress on a [`Thread`]: every event it yields has already been reduced into the
/// thread. Borrowing the thread mutably keeps a second run from starting on it.
#[derive(Debug)]
pub struct ThreadRun<'a> {
    thread: &'a mut Thread,
    stream: RunStream,
}

impl ThreadRun<'_> {
    /// The next event (already reduced), `None` at end of stream.
    pub async fn next(&mut self) -> Option<Result<Event>> {
        let item = self.stream.next().await?;
        if let Ok(event) = &item {
            self.thread.reduce(event);
        }
        Some(item)
    }

    /// Consumes the stream to its end. `RUN_ERROR` becomes [`Error::Run`]; a stream that ends
    /// without `RUN_FINISHED` is a [`Error::Protocol`].
    pub async fn drain(mut self) -> Result<RunOutcome> {
        let mut outcome = None;
        while let Some(event) = self.next().await {
            match event? {
                Event::RunFinished { outcome: o, .. } => {
                    outcome = Some(if o.as_deref() == Some(OUTCOME_INTERRUPT) {
                        RunOutcome::Interrupted
                    } else {
                        RunOutcome::Finished
                    });
                }
                Event::RunError { message, code } => return Err(Error::Run { code, message }),
                _ => {}
            }
        }
        outcome.ok_or_else(|| Error::Protocol("stream ended before RUN_FINISHED".to_owned()))
    }
}
