//! AI help inside the editor (BIT-US-0153, BIT-SP-0011.R6): the "Compose with AI" box and the
//! inline ghost text.
//!
//! Nothing here talks to Pando directly. The editor asks an [`AiBackend`] (a GPUI global the
//! workspace installs while a graph is open) to run a [`ComposeRequest`] and reads
//! [`AiEvent`]s from the [`AiRun`] it gets back. Dropping the run cancels it. The backend
//! refuses what the consent and exclusions gate hides, so a page that is excluded or private
//! never reaches an agent; the editor only adds the user-facing switches
//! ([`crate::settings::AiAssistSettings`], off by default) and the debounce.
//!
//! The pure decisions (when a continuation may be requested, how it is glued to the text) live
//! here so they are tested without a window; the entity state and key handling are in
//! `view/ai.rs`, the drawing in `ai_view.rs`.

use std::any::Any;
use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use async_channel::Receiver;
use bitacora_core::editor::BlockId;
pub use bitacora_runtime::{ComposeMode, ComposeRequest};

use crate::session::SessionHandle;
use crate::ui::input::InputState;
use crate::ui::{App, Entity, Global, Subscription, Task};

/// Quiet time after the last keystroke before a continuation is requested.
pub const GHOST_DELAY: Duration = Duration::from_millis(1200);
/// Shortest block text (in characters) a continuation is requested for.
pub const GHOST_MIN_CHARS: usize = 12;
/// How long ghost requests pause after one failed (Pando down, no consent, ...).
pub const GHOST_BACKOFF: Duration = Duration::from_secs(60);

/// What a run reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiEvent {
    /// The answer so far (the whole text, not a delta).
    Preview(String),
    /// The final answer.
    Done(String),
    /// The run failed or was refused; the text is for the user.
    Failed(String),
}

/// A run in progress. Dropping it cancels the run.
pub struct AiRun {
    /// Events of the run; closed after `Done` or `Failed`.
    pub events: Receiver<AiEvent>,
    _cancel: Box<dyn Any>,
}

impl AiRun {
    /// A run reading `events`; `cancel` is dropped with the run (a task, a guard).
    pub fn new(events: Receiver<AiEvent>, cancel: impl Any) -> Self {
        Self {
            events,
            _cancel: Box::new(cancel),
        }
    }
}

impl std::fmt::Debug for AiRun {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AiRun").finish_non_exhaustive()
    }
}

/// Runs compose and continue requests.
pub trait AiBackend {
    /// Starts `request`. Must not block: the work runs elsewhere.
    fn start(&self, cx: &App, request: ComposeRequest) -> AiRun;
}

/// The installed backend (none while no graph is open).
#[derive(Default)]
struct AiAssist {
    backend: Option<Rc<dyn AiBackend>>,
}

impl Global for AiAssist {}

/// Installs (or, with `None`, removes) the backend editors use.
pub fn install(cx: &mut App, backend: Option<Rc<dyn AiBackend>>) {
    cx.set_global(AiAssist { backend });
}

/// The installed backend.
#[must_use]
pub fn backend(cx: &App) -> Option<Rc<dyn AiBackend>> {
    cx.try_global::<AiAssist>().and_then(|a| a.backend.clone())
}

/// The compose box is switched on and a backend is installed.
#[must_use]
pub fn compose_available(cx: &App) -> bool {
    backend(cx).is_some() && crate::theme::try_settings(cx).is_some_and(|s| s.ai_assist.compose)
}

/// Inline continuations are switched on and a backend is installed.
#[must_use]
pub fn ghost_available(cx: &App) -> bool {
    backend(cx).is_some() && crate::theme::try_settings(cx).is_some_and(|s| s.ai_assist.ghost_text)
}

/// The backend of a running graph session: runs on the app's tokio runtime with the session's
/// AG-UI client, consent guard and index.
#[derive(Debug, Clone)]
pub struct SessionAiBackend {
    handle: SessionHandle,
}

impl SessionAiBackend {
    /// A backend asking `handle`'s session for its agent dependencies on every run (consent
    /// and exclusions are read live).
    #[must_use]
    pub fn new(handle: SessionHandle) -> Self {
        Self { handle }
    }
}

impl AiBackend for SessionAiBackend {
    fn start(&self, cx: &App, request: ComposeRequest) -> AiRun {
        let (tx, rx) = async_channel::unbounded();
        let handle = self.handle.clone();
        let task = crate::tokio_bridge::spawn(cx, async move {
            let deps = match handle.run(|s| s.compose_deps()).recv().await {
                Ok(Ok(deps)) => deps,
                Ok(Err(err)) => {
                    let _ = tx.send(AiEvent::Failed(err.to_string())).await;
                    return;
                }
                Err(_) => {
                    let _ = tx
                        .send(AiEvent::Failed("the graph session is closed".into()))
                        .await;
                    return;
                }
            };
            let preview = tx.clone();
            let result = bitacora_runtime::run_agent_compose(&deps, &request, move |p| {
                let _ = preview.try_send(AiEvent::Preview(p.to_owned()));
            })
            .await;
            let event = match result {
                Ok(text) => AiEvent::Done(text),
                Err(err) => AiEvent::Failed(err.to_string()),
            };
            let _ = tx.send(event).await;
        });
        AiRun::new(rx, task)
    }
}

/// A run and the GPUI task reading its events. Dropping the slot cancels both.
pub(crate) struct RunSlot {
    pub(crate) _run: AiRun,
    pub(crate) _reader: Task<()>,
}

/// An unaccepted continuation shown after the caret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Ghost {
    pub(crate) block: BlockId,
    /// The visible buffer text the suggestion continues.
    pub(crate) base: String,
    /// Caret (end of `base`) the suggestion belongs to.
    pub(crate) cursor: usize,
    pub(crate) text: String,
}

/// Where the compose box is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComposePhase {
    /// Waiting for an instruction.
    Idle,
    /// The agent is writing (the preview grows).
    Running,
    /// The draft is complete.
    Ready,
    /// The run failed or was refused.
    Failed(String),
}

/// The open compose box.
pub(crate) struct Compose {
    pub(crate) block: BlockId,
    pub(crate) input: Entity<InputState>,
    pub(crate) include_block: bool,
    pub(crate) phase: ComposePhase,
    pub(crate) preview: String,
    pub(crate) run: Option<RunSlot>,
    /// The instruction field must take focus once the box is on screen.
    pub(crate) focus_pending: Rc<Cell<bool>>,
    pub(crate) _submit: Subscription,
}

/// Everything the editor remembers about AI help.
#[derive(Default)]
pub(crate) struct AiState {
    pub(crate) ghost: Option<Ghost>,
    pub(crate) ghost_epoch: usize,
    pub(crate) ghost_run: Option<RunSlot>,
    pub(crate) ghost_timer: Option<Task<()>>,
    pub(crate) backoff_until: Option<std::time::Instant>,
    /// The next edit is an accepted suggestion: do not chain another request.
    pub(crate) skip_next: bool,
    pub(crate) compose: Option<Compose>,
}

/// What the row draws for AI help.
#[derive(Clone, Debug)]
pub struct AiView {
    /// A ghost is shown: draw the hint bar.
    pub ghost_hint: bool,
    /// The compose box, when open.
    pub compose: Option<ComposeView>,
}

/// A snapshot of the compose box for drawing.
#[derive(Clone, Debug)]
pub struct ComposeView {
    /// The instruction field.
    pub input: Entity<InputState>,
    /// The "this block" context chip is on.
    pub include_block: bool,
    /// Phase of the run.
    pub phase: ComposePhase,
    /// The draft so far.
    pub preview: String,
    /// Title of the page the block is on (context chip).
    pub page: String,
    /// Set while the field still has to take focus (the popover shell takes it first).
    pub focus_pending: Rc<Cell<bool>>,
}

/// Whether a continuation may be requested for a buffer in this state.
#[must_use]
pub fn may_request_ghost(
    text: &str,
    cursor: usize,
    selection_empty: bool,
    composing: bool,
    popup_open: bool,
) -> bool {
    !composing
        && !popup_open
        && selection_empty
        && cursor == text.len()
        && text.chars().filter(|c| !c.is_whitespace()).count() >= GHOST_MIN_CHARS
        && !is_property_line(text.lines().last().unwrap_or(""))
}

/// A `key:: value` property line (a continuation there would be a property value).
fn is_property_line(line: &str) -> bool {
    let line = line.trim();
    line.ends_with("::") || line.contains(":: ")
}

/// The suggestion as it will be inserted after `base`: one line, glued with a space when both
/// sides are words, empty when nothing usable is left.
#[must_use]
pub fn glue(base: &str, suggestion: &str) -> String {
    let flat: String = suggestion
        .split(['\n', '\r'])
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let flat = flat.trim_end();
    let flat = flat.trim_start();
    if flat.is_empty() {
        return String::new();
    }
    let base_open = base.chars().last().is_some_and(|c| !c.is_whitespace());
    let starts_punct = flat.chars().next().is_some_and(|c| {
        matches!(
            c,
            '.' | ',' | ';' | ':' | '!' | '?' | ')' | ']' | '}' | '\'' | '-'
        )
    });
    if base_open && !starts_punct {
        format!(" {flat}")
    } else {
        flat.to_owned()
    }
}

/// The instruction is usable (not blank).
#[must_use]
pub fn instruction_ready(text: &str) -> bool {
    !text.trim().is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ghost_needs_a_quiet_caret_at_the_end_of_enough_text() {
        let t = "The quick brown fox jumps";
        assert!(may_request_ghost(t, t.len(), true, false, false));
        assert!(
            !may_request_ghost(t, 3, true, false, false),
            "caret not at end"
        );
        assert!(
            !may_request_ghost(t, t.len(), false, false, false),
            "selection"
        );
        assert!(
            !may_request_ghost(t, t.len(), true, true, false),
            "IME composition"
        );
        assert!(
            !may_request_ghost(t, t.len(), true, false, true),
            "popup open"
        );
        assert!(!may_request_ghost("too short", 9, true, false, false));
        assert!(!may_request_ghost(
            "status:: some long property value",
            33,
            true,
            false,
            false
        ));
        assert!(!may_request_ghost("key:: ", 6, true, false, false));
    }

    #[test]
    fn glue_adds_a_space_between_words_only() {
        assert_eq!(glue("hello", "world"), " world");
        assert_eq!(glue("hello ", "world"), "world");
        assert_eq!(glue("hello", ", world"), ", world");
        assert_eq!(glue("hello", " world"), " world");
        assert_eq!(glue("hello", "a\nb\n"), " a b");
        assert_eq!(glue("hello", "  \n "), "");
    }

    #[test]
    fn instructions_must_not_be_blank() {
        assert!(!instruction_ready("  \n"));
        assert!(instruction_ready(" x "));
    }
}
