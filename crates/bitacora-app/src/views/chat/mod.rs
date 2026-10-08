//! The assistant chat of the right panel's Agent tab (BIT-US-0147..0150).
//!
//! [`ChatView`] is the GPUI side of `bitacora_runtime::ai::ChatSession`: it starts a session on
//! the Pando runtime (`Session::start_chat_with`), moves its events from a std channel into the
//! UI through a forwarding thread and an async channel, folds them into a `ChatModel` and renders
//! the transcript (streamed answers, tool-call cards, approval cards, the agent state header).
//! Nothing here blocks the UI thread: sending, approving and cancelling are channel sends, and the
//! network calls of the thread list run on the tokio bridge.
//!
//! Writes never happen here. A `propose_edit` becomes an approval card; only an explicit approval
//! reaches the backend applier, which commits through the core command queue (single writer,
//! undoable). The view closes the session with the right `DenyReason` on panel close, thread
//! switch, graph switch and quit ([`ChatView::close_session`]); every pending card is then denied.
//!
//! Layout: `mod.rs` holds state and behaviour, `render.rs` the elements, `host.rs` the seams to
//! the backend, `markdown.rs`, `diff.rs` and `tools.rs` pure helpers with unit tests.

mod diff;
mod host;
mod markdown;
mod render;
mod tools;

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use bitacora_runtime::RuntimeError;
use bitacora_runtime::ai::{
    ChatConfig, ChatEvent, ChatHandle, ChatModel, ContentGuard, Decision, DenyReason, ModelChoices,
    PageResolver, QuestionAnswer, QuestionAnswerEntry, ThreadSummary,
};
use rust_i18n::t;

pub use host::{
    AppHost, AppResolver, ChatContext, ContextItem, ContextKind, ContextProvider, HostRequest,
    PageBlocks, attached_blocks,
};

use crate::data::GraphHandle;
use crate::render::inline::NavTarget;
use crate::session::{SessionHandle, SessionLink};
use crate::ui::input::{InputEvent, TextareaState};
use crate::ui::{
    AppContext as _, Context, Entity, EventEmitter, Focusable as _, ScrollHandle, Subscription,
    Task, Window,
};

/// Threads listed at once.
const THREADS_PAGE: u32 = 50;
/// Threads whose first message is fetched to title the list.
const THREAD_TITLES: usize = 12;

/// What the chat asks of the workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatViewEvent {
    /// Open a page or block (a link in an answer, or the agent's `open_page`).
    Navigate(NavTarget),
    /// The user picked a model in the selector (`None` is the default); the workspace persists
    /// it in the Pando settings.
    ModelChosen(Option<String>),
}

/// Where the session stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Phase {
    /// No session; the next message starts one.
    Idle,
    /// A session is being started.
    Starting,
    /// A session is running.
    Live,
    /// Starting failed (Pando off, not connected, no consent, ...); the next message retries.
    Unavailable(String),
}

/// A message waiting for the session to start.
#[derive(Debug, Clone)]
struct Outgoing {
    text: String,
    items: Vec<ContextItem>,
}

/// What happened to an approved edit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditNote {
    /// Applied: page and number of touched blocks.
    Applied { page: String, blocks: usize },
    /// Not applied: why.
    Failed(String),
}

/// One row of the thread list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadRow {
    /// Thread id.
    pub id: String,
    /// First user message, when it could be read.
    pub title: Option<String>,
    /// Last update, `YYYY-MM-DD HH:MM`.
    pub updated: String,
}

/// The thread list state.
#[derive(Debug, Default)]
pub struct Threads {
    /// The list replaces the transcript.
    pub open: bool,
    /// A fetch is running.
    pub loading: bool,
    /// Rows, newest first.
    pub rows: Vec<ThreadRow>,
    /// Why the last fetch failed.
    pub error: Option<String>,
}

/// The chat view.
pub struct ChatView {
    session: Option<SessionHandle>,
    link: Option<SessionLink>,
    graph: Option<GraphHandle>,
    guard: Arc<Mutex<Option<ContentGuard>>>,
    provider: Option<ContextProvider>,
    model: ChatModel,
    chat: Option<ChatHandle>,
    thread_id: Option<String>,
    can_edit: bool,
    phase: Phase,
    queued: Option<Outgoing>,
    restore_text: Option<String>,
    composer: Entity<TextareaState>,
    contexts: Vec<ContextItem>,
    sent_context: HashMap<usize, Vec<String>>,
    edit_notes: HashMap<String, EditNote>,
    picks: HashMap<String, Vec<Option<(String, String)>>>,
    notice: Option<String>,
    expanded: HashSet<String>,
    /// Cards whose "Remember my decision" box is ticked.
    remembering: HashSet<String>,
    threads: Threads,
    /// Models the server offers as chat profiles (BIT-US-0180).
    choices: ModelChoices,
    /// Model id last chosen (`None`: the profile's default model).
    chosen_model: Option<String>,
    model_menu: bool,
    choices_task: Option<Task<()>>,
    scroll: ScrollHandle,
    follow: bool,
    layouts: render::LayoutCache,
    generation: u64,
    start_task: Option<Task<()>>,
    events_task: Option<Task<()>>,
    host_task: Option<Task<()>>,
    threads_task: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl std::fmt::Debug for ChatView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChatView")
            .field("phase", &self.phase)
            .field("messages", &self.model.messages.len())
            .finish_non_exhaustive()
    }
}

impl EventEmitter<ChatViewEvent> for ChatView {}

impl ChatView {
    /// An idle chat; connect it with `set_session`, `set_link` and `set_graph`.
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let composer = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(t!("chat.placeholder").to_string())
                .auto_grow(2, 8)
                .submit_on_enter(true)
        });
        let subscription = cx.subscribe_in(
            &composer,
            window,
            |this, _, event: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { shift, .. } = event
                    && !*shift
                {
                    this.submit(window, cx);
                }
            },
        );
        Self {
            session: None,
            link: None,
            graph: None,
            guard: Arc::new(Mutex::new(None)),
            provider: None,
            model: ChatModel::default(),
            chat: None,
            thread_id: None,
            can_edit: false,
            phase: Phase::Idle,
            queued: None,
            restore_text: None,
            composer,
            contexts: Vec::new(),
            sent_context: HashMap::new(),
            edit_notes: HashMap::new(),
            picks: HashMap::new(),
            notice: None,
            expanded: HashSet::new(),
            remembering: HashSet::new(),
            threads: Threads::default(),
            choices: ModelChoices::default(),
            chosen_model: None,
            model_menu: false,
            choices_task: None,
            scroll: ScrollHandle::new(),
            follow: true,
            layouts: render::LayoutCache::default(),
            generation: 0,
            start_task: None,
            events_task: None,
            host_task: None,
            threads_task: None,
            _subscriptions: vec![subscription],
        }
    }

    // ---- wiring ---------------------------------------------------------------------------------

    /// The handle that runs closures on the session thread (`None` closes the chat first).
    pub fn set_session(&mut self, session: Option<SessionHandle>, cx: &mut Context<Self>) {
        if session.is_none() {
            self.reset(DenyReason::GraphSwitch, cx);
        }
        self.session = session;
    }

    /// Drops the session, link and graph (the graph closed or the app is quitting): pending cards
    /// are denied for `reason` and the conversation is forgotten.
    pub fn disconnect(&mut self, reason: DenyReason, cx: &mut Context<Self>) {
        self.reset(reason, cx);
        self.session = None;
        self.link = None;
        self.graph = None;
    }

    /// The writer queue and effective config of the live session.
    pub fn set_link(&mut self, link: Option<SessionLink>) {
        self.link = link;
    }

    /// The reader of the open graph (resolves `((block))` links, finds pages).
    pub fn set_graph(&mut self, graph: Option<GraphHandle>, cx: &mut Context<Self>) {
        if graph.is_none() {
            self.reset(DenyReason::GraphSwitch, cx);
        }
        self.graph = graph;
        self.layouts.clear();
    }

    /// How the chat asks the workspace for the page and selection to attach.
    pub fn set_context_provider(&mut self, provider: ContextProvider) {
        self.provider = Some(provider);
    }

    /// The phase of the session.
    #[must_use]
    pub fn phase(&self) -> &Phase {
        &self.phase
    }

    /// The transcript model.
    #[must_use]
    pub fn model(&self) -> &ChatModel {
        &self.model
    }

    /// The items attached to the next message.
    #[must_use]
    pub fn contexts(&self) -> &[ContextItem] {
        &self.contexts
    }

    /// Whether the thread list is showing.
    #[must_use]
    pub fn threads_open(&self) -> bool {
        self.threads.open
    }

    /// Whether the writer profile (approval-gated `propose_edit`) is on.
    #[must_use]
    pub fn can_edit(&self) -> bool {
        self.can_edit
    }

    /// Id of the current thread, once there is one.
    #[must_use]
    pub fn thread_id(&self) -> Option<&str> {
        self.thread_id.as_deref()
    }

    /// Puts the caret in the composer.
    pub fn focus_composer(&self, window: &mut Window, cx: &mut Context<Self>) {
        let handle = self.composer.read(cx).focus_handle(cx);
        window.focus(&handle, cx);
    }

    // ---- closing --------------------------------------------------------------------------------

    /// Ends the session for `reason` (panel close, thread switch, graph switch, quit). Every
    /// pending card is denied (the backend does it on its side; the cards here are marked at
    /// once because the event task stops), the run is cancelled and nothing is applied after it.
    /// The thread id is kept, so the next message resumes the same conversation.
    pub fn close_session(&mut self, reason: DenyReason, cx: &mut Context<Self>) {
        if let Some(chat) = self.chat.take() {
            chat.close(reason);
        }
        self.events_task = None;
        self.host_task = None;
        self.start_task = None;
        self.generation += 1;
        if matches!(self.phase, Phase::Live | Phase::Starting) {
            self.phase = Phase::Idle;
        }
        for card in &mut self.model.cards {
            if card.state == bitacora_runtime::ai::CardState::Pending {
                card.state = bitacora_runtime::ai::CardState::Denied(Some(reason));
            }
        }
        self.model.running = false;
        cx.notify();
    }

    /// The right panel was closed.
    pub fn panel_closed(&mut self, cx: &mut Context<Self>) {
        self.close_session(DenyReason::PanelClosed, cx);
    }

    /// The app is quitting.
    pub fn quit(&mut self, cx: &mut Context<Self>) {
        self.close_session(DenyReason::Quit, cx);
    }

    /// The graph is closing or switching: ends the session and forgets the conversation.
    pub fn reset(&mut self, reason: DenyReason, cx: &mut Context<Self>) {
        self.close_session(reason, cx);
        self.model = ChatModel::default();
        self.thread_id = None;
        self.queued = None;
        self.contexts.clear();
        self.sent_context.clear();
        self.edit_notes.clear();
        self.expanded.clear();
        self.threads = Threads::default();
        self.layouts.clear();
        if let Ok(mut guard) = self.guard.lock() {
            *guard = None;
        }
        self.phase = Phase::Idle;
    }

    // ---- sending --------------------------------------------------------------------------------

    /// Sends the composer text (with the attached context) as a new turn.
    pub fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.composer.read(cx).value().trim().to_owned();
        if text.is_empty() || self.model.running || self.phase == Phase::Starting {
            return;
        }
        self.composer
            .update(cx, |c, cx| c.set_value("", window, cx));
        let items = std::mem::take(&mut self.contexts);
        self.threads.open = false;
        self.notice = None;
        self.follow = true;
        self.send_message(text, items, cx);
    }

    /// Sends `text` with `items` attached, starting a session first when needed.
    pub fn send_message(&mut self, text: String, items: Vec<ContextItem>, cx: &mut Context<Self>) {
        if self.chat.is_some() {
            self.deliver(text, items, cx);
        } else {
            self.queued = Some(Outgoing { text, items });
            self.start(cx);
        }
        cx.notify();
    }

    fn deliver(&mut self, text: String, items: Vec<ContextItem>, cx: &mut Context<Self>) {
        let Some(chat) = self.chat.clone() else {
            return;
        };
        self.model.push_user(&text);
        let labels: Vec<String> = items.iter().map(ContextItem::label).collect();
        if !labels.is_empty() {
            self.sent_context
                .insert(self.model.messages.len().saturating_sub(1), labels);
        }
        // The guard is read again for every message so consent changes apply at once; the
        // backend filters the attached blocks through it a second time (fail closed).
        let fresh = self.session.as_ref().map(|s| s.run(|s| s.agent_guard()));
        let shared = Arc::clone(&self.guard);
        cx.spawn(async move |_, _| {
            let fresh = match fresh {
                Some(rx) => rx.recv().await.ok(),
                None => None,
            };
            if let (Some(g), Ok(mut slot)) = (&fresh, shared.lock()) {
                *slot = Some(g.clone());
            }
            let guard = fresh
                .or_else(|| shared.lock().ok().and_then(|g| g.clone()))
                .unwrap_or_else(deny_all_guard);
            chat.send(text, attached_blocks(&items, &guard));
        })
        .detach();
    }

    /// Stops the running turn; pending cards are denied and the session stays usable.
    pub fn stop(&mut self, cx: &mut Context<Self>) {
        if let Some(chat) = &self.chat {
            chat.cancel();
        }
        cx.notify();
    }

    // ---- starting -------------------------------------------------------------------------------

    fn start(&mut self, cx: &mut Context<Self>) {
        let (Some(session), Some(graph), Some(link)) =
            (self.session.clone(), self.graph.clone(), self.link.clone())
        else {
            self.fail_start(t!("chat.no_graph").to_string(), cx);
            return;
        };
        self.phase = Phase::Starting;
        self.generation += 1;
        let generation = self.generation;
        let (host_tx, host_rx) = async_channel::unbounded();
        let resolver = Arc::new(AppResolver::new(
            link.queue.clone(),
            graph,
            Arc::clone(&link.config),
        ));
        let host = Arc::new(AppHost::new(
            host_tx,
            Arc::clone(&resolver),
            Arc::clone(&self.guard),
        ));
        let mut config = if self.can_edit {
            ChatConfig::writer()
        } else {
            ChatConfig::default()
        };
        if !self.can_edit {
            config.profile = self.choices.profile_for(self.chosen_model.as_deref());
        }
        config.resume_thread = self.thread_id.clone();
        let resolver: Option<Arc<dyn PageResolver>> =
            if self.can_edit { Some(resolver) } else { None };
        let rx = session.run(move |s| {
            let guard = s.agent_guard();
            s.start_chat_with(config, host, resolver)
                .map(|started| (guard, started))
        });
        self.host_task = Some(cx.spawn(async move |this, cx| {
            while let Ok(request) = host_rx.recv().await {
                if this
                    .update(cx, |this, cx| this.on_host_request(request, cx))
                    .is_err()
                {
                    break;
                }
            }
        }));
        self.start_task = Some(cx.spawn(async move |this, cx| {
            let result = rx.recv().await;
            let _ = this.update(cx, |this, cx| this.started(generation, result, cx));
        }));
        cx.notify();
    }

    #[allow(clippy::type_complexity)]
    fn started(
        &mut self,
        generation: u64,
        result: Result<
            Result<
                (
                    ContentGuard,
                    (ChatHandle, std::sync::mpsc::Receiver<ChatEvent>),
                ),
                RuntimeError,
            >,
            async_channel::RecvError,
        >,
        cx: &mut Context<Self>,
    ) {
        let started = match result {
            Ok(Ok(started)) => started,
            Ok(Err(e)) => {
                if generation == self.generation {
                    self.fail_start(e.to_string(), cx);
                }
                return;
            }
            Err(_) => {
                if generation == self.generation {
                    self.fail_start(t!("chat.no_graph").to_string(), cx);
                }
                return;
            }
        };
        let (guard, (chat, events)) = started;
        if generation != self.generation {
            // Closed or switched while starting: the session must not outlive its reason.
            chat.close(DenyReason::ThreadSwitch);
            return;
        }
        if let Ok(mut slot) = self.guard.lock() {
            *slot = Some(guard);
        }
        self.thread_id = Some(chat.thread_id().to_owned());
        self.chat = Some(chat);
        self.phase = Phase::Live;
        self.pump_events(events, generation, cx);
        if let Some(out) = self.queued.take() {
            self.deliver(out.text, out.items, cx);
        }
        cx.notify();
    }

    fn fail_start(&mut self, reason: String, cx: &mut Context<Self>) {
        self.phase = Phase::Unavailable(reason);
        self.host_task = None;
        // Give the user's text back instead of losing it.
        if let Some(out) = self.queued.take() {
            self.contexts = out.items;
            // `set_value` needs a window: the next render puts the text back.
            self.restore_text = Some(out.text);
        }
        cx.notify();
    }

    /// Moves the session's events (a blocking std channel) into the UI without polling: a
    /// forwarding thread feeds an async channel, and one foreground task folds batches of events
    /// into the model so a burst of deltas costs one render.
    fn pump_events(
        &mut self,
        events: std::sync::mpsc::Receiver<ChatEvent>,
        generation: u64,
        cx: &mut Context<Self>,
    ) {
        let (tx, rx) = async_channel::unbounded();
        let spawned = std::thread::Builder::new()
            .name("chat-events".into())
            .spawn(move || {
                while let Ok(event) = events.recv() {
                    if tx.send_blocking(event).is_err() {
                        break;
                    }
                }
            });
        if spawned.is_err() {
            self.model.apply(&ChatEvent::Error {
                message: t!("chat.no_events").to_string(),
                fatal: true,
            });
            return;
        }
        self.events_task = Some(cx.spawn(async move |this, cx| {
            while let Ok(first) = rx.recv().await {
                let mut batch = vec![first];
                while let Ok(more) = rx.try_recv() {
                    batch.push(more);
                }
                if this
                    .update(cx, |this, cx| this.apply_events(generation, batch, cx))
                    .is_err()
                {
                    break;
                }
            }
        }));
    }

    fn apply_events(&mut self, generation: u64, batch: Vec<ChatEvent>, cx: &mut Context<Self>) {
        if generation != self.generation {
            return;
        }
        for event in batch {
            match &event {
                ChatEvent::History(_) => {
                    self.sent_context.clear();
                    self.layouts.clear();
                }
                ChatEvent::EditApplied {
                    call_id,
                    page,
                    affected,
                    ..
                } => {
                    self.edit_notes.insert(
                        call_id.clone(),
                        EditNote::Applied {
                            page: page.clone(),
                            blocks: affected.len(),
                        },
                    );
                }
                ChatEvent::EditFailed {
                    call_id, message, ..
                }
                | ChatEvent::EditRejected {
                    call_id, message, ..
                } => {
                    self.edit_notes
                        .insert(call_id.clone(), EditNote::Failed(message.clone()));
                }
                ChatEvent::Closed(_) | ChatEvent::Error { fatal: true, .. } => {
                    self.chat = None;
                    self.phase = Phase::Idle;
                }
                _ => {}
            }
            self.model.apply(&event);
        }
        cx.notify();
    }

    fn on_host_request(&mut self, request: HostRequest, cx: &mut Context<Self>) {
        match request {
            HostRequest::OpenPage(name) => cx.emit(ChatViewEvent::Navigate(NavTarget::Page(name))),
            HostRequest::Selection(reply) => {
                let selection = self
                    .provider
                    .as_ref()
                    .map(|p| p(cx))
                    .and_then(|c| c.selection)
                    .unwrap_or_else(|| PageBlocks {
                        page: String::new(),
                        file_path: String::new(),
                        preamble: None,
                        blocks: Vec::new(),
                    });
                let _ = reply.send(Ok(selection));
            }
        }
    }

    // ---- cards ----------------------------------------------------------------------------------

    /// Approves a permission or edit card.
    pub fn approve(&mut self, id: &str, cx: &mut Context<Self>) {
        let remember = self.remembering.remove(id);
        if let Some(chat) = &self.chat {
            if remember {
                chat.decide_and_remember(id, Decision::Approve);
            } else {
                chat.approve(id);
            }
        }
        cx.notify();
    }

    /// Denies a card.
    pub fn deny(&mut self, id: &str, cx: &mut Context<Self>) {
        let remember = self.remembering.remove(id);
        if let Some(chat) = &self.chat {
            if remember {
                chat.decide_and_remember(id, Decision::Deny);
            } else {
                chat.deny(id);
            }
        }
        cx.notify();
    }

    /// Ticks or clears "Remember my decision" on a card.
    pub fn toggle_remember(&mut self, id: &str, cx: &mut Context<Self>) {
        if !self.remembering.remove(id) {
            self.remembering.insert(id.to_owned());
        }
        cx.notify();
    }

    /// Whether "Remember my decision" is ticked on card `id`.
    #[must_use]
    pub fn is_remembering(&self, id: &str) -> bool {
        self.remembering.contains(id)
    }

    /// Records the option chosen for question `ix` of a question card; once every question has
    /// one, the card is answered.
    pub fn pick(
        &mut self,
        id: &str,
        ix: usize,
        header: &str,
        label: &str,
        total: usize,
        cx: &mut Context<Self>,
    ) {
        let picks = self
            .picks
            .entry(id.to_owned())
            .or_insert_with(|| vec![None; total]);
        if let Some(slot) = picks.get_mut(ix) {
            *slot = Some((header.to_owned(), label.to_owned()));
        }
        if picks.iter().all(Option::is_some) {
            let answers = picks
                .iter()
                .flatten()
                .map(|(header, label)| QuestionAnswerEntry {
                    question_id: header.clone(),
                    header: header.clone(),
                    selected: vec![label.clone()],
                    other_text: String::new(),
                })
                .collect();
            self.picks.remove(id);
            if let Some(chat) = &self.chat {
                chat.decide(
                    id,
                    Decision::Answer(QuestionAnswer {
                        cancelled: false,
                        answers,
                    }),
                );
            }
        }
        cx.notify();
    }

    /// Skips a question card: the agent proceeds on its own judgement.
    pub fn skip(&mut self, id: &str, cx: &mut Context<Self>) {
        self.picks.remove(id);
        if let Some(chat) = &self.chat {
            chat.decide(
                id,
                Decision::Answer(QuestionAnswer {
                    cancelled: true,
                    answers: Vec::new(),
                }),
            );
        }
        cx.notify();
    }

    /// Expands or folds a tool call or reasoning block.
    pub fn toggle_expanded(&mut self, key: &str, cx: &mut Context<Self>) {
        if !self.expanded.remove(key) {
            self.expanded.insert(key.to_owned());
        }
        cx.notify();
    }

    // ---- context chips (BIT-US-0150) --------------------------------------------------------------

    /// Attaches `item` to the next message (replacing an earlier attachment of the same thing).
    pub fn attach(&mut self, item: ContextItem, cx: &mut Context<Self>) {
        self.contexts.retain(|c| !c.same_as(&item));
        self.contexts.push(item);
        self.notice = None;
        cx.notify();
    }

    /// Removes the attached item at `ix`.
    pub fn detach(&mut self, ix: usize, cx: &mut Context<Self>) {
        if ix < self.contexts.len() {
            self.contexts.remove(ix);
            cx.notify();
        }
    }

    /// Attaches the page on screen (the "+ Page" chip).
    pub fn attach_page(&mut self, cx: &mut Context<Self>) {
        let context = self.provider.clone().map(|p| p(cx)).unwrap_or_default();
        match context.page {
            Some((data, journal)) => self.attach(
                ContextItem {
                    kind: if journal {
                        ContextKind::Journal
                    } else {
                        ContextKind::Page
                    },
                    data,
                },
                cx,
            ),
            None => {
                self.notice = Some(t!("chat.ctx_no_page").to_string());
                cx.notify();
            }
        }
    }

    /// Attaches the selected blocks (the "+ Selection" chip).
    pub fn attach_selection(&mut self, cx: &mut Context<Self>) {
        let context = self.provider.clone().map(|p| p(cx)).unwrap_or_default();
        match context.selection {
            Some(data) if !data.blocks.is_empty() => self.attach(
                ContextItem {
                    kind: ContextKind::Selection,
                    data,
                },
                cx,
            ),
            _ => {
                self.notice = Some(t!("chat.ctx_no_selection").to_string());
                cx.notify();
            }
        }
    }

    // ---- threads (BIT-T-0454) ---------------------------------------------------------------------

    /// Starts an empty conversation (the old thread stays on the server).
    pub fn new_thread(&mut self, cx: &mut Context<Self>) {
        self.refresh_model_choices(cx);
        self.close_session(DenyReason::ThreadSwitch, cx);
        self.model = ChatModel::default();
        self.thread_id = None;
        self.sent_context.clear();
        self.edit_notes.clear();
        self.expanded.clear();
        self.layouts.clear();
        self.threads.open = false;
        self.phase = Phase::Idle;
        cx.notify();
    }

    /// Reattaches to `id`: the server transcript is loaded as history.
    pub fn resume_thread(&mut self, id: &str, cx: &mut Context<Self>) {
        self.close_session(DenyReason::ThreadSwitch, cx);
        self.model = ChatModel::default();
        self.sent_context.clear();
        self.edit_notes.clear();
        self.expanded.clear();
        self.layouts.clear();
        self.threads.open = false;
        self.thread_id = Some(id.to_owned());
        self.follow = true;
        self.start(cx);
    }

    /// Shows or hides the thread list, refreshing it when it opens.
    pub fn toggle_threads(&mut self, cx: &mut Context<Self>) {
        self.threads.open = !self.threads.open;
        if self.threads.open {
            self.load_threads(cx);
        }
        cx.notify();
    }

    fn load_threads(&mut self, cx: &mut Context<Self>) {
        let Some(session) = self.session.clone() else {
            self.threads.error = Some(t!("chat.no_graph").to_string());
            return;
        };
        self.threads.loading = true;
        self.threads.error = None;
        let client = session.run(|s| s.agui_client());
        self.threads_task = Some(cx.spawn(async move |this, cx| {
            let outcome = match client.recv().await.ok().flatten() {
                None => Err(t!("chat.threads_unavailable").to_string()),
                Some(client) => {
                    let task = this.update(cx, |_, cx| {
                        crate::tokio_bridge::spawn(cx, fetch_threads(client))
                    });
                    match task {
                        Ok(task) => task
                            .await
                            .map_err(|e| e.to_string())
                            .and_then(|result| result),
                        Err(_) => return,
                    }
                }
            };
            let _ = this.update(cx, |this, cx| {
                this.threads.loading = false;
                match outcome {
                    Ok(rows) => this.threads.rows = rows,
                    Err(e) => this.threads.error = Some(e),
                }
                cx.notify();
            });
        }));
    }

    /// Deletes a thread on the server; the current one starts over.
    pub fn delete_thread(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(session) = self.session.clone() else {
            return;
        };
        let id = id.to_owned();
        let client = session.run(|s| s.agui_client());
        cx.spawn(async move |this, cx| {
            let Some(client) = client.recv().await.ok().flatten() else {
                return;
            };
            let target = id.clone();
            let task = this.update(cx, |_, cx| {
                crate::tokio_bridge::spawn(cx, async move { client.delete_thread(&target).await })
            });
            let ok = match task {
                Ok(task) => matches!(task.await, Ok(Ok(()))),
                Err(_) => return,
            };
            let _ = this.update(cx, |this, cx| {
                if ok {
                    this.threads.rows.retain(|r| r.id != id);
                    if this.thread_id.as_deref() == Some(id.as_str()) {
                        this.new_thread(cx);
                        this.threads.open = true;
                    }
                } else {
                    this.threads.error = Some(t!("chat.thread_delete_failed").to_string());
                }
                cx.notify();
            });
        })
        .detach();
    }

    // ---- model selector (BIT-US-0180) --------------------------------------------------------------

    /// The remembered model (from the Pando settings); `None` is the profile's default.
    pub fn set_chosen_model(&mut self, model: Option<String>, cx: &mut Context<Self>) {
        if self.chosen_model != model {
            self.chosen_model = model;
            cx.notify();
        }
    }

    /// The model the next run uses, as remembered.
    #[must_use]
    pub fn chosen_model(&self) -> Option<&str> {
        self.chosen_model.as_deref()
    }

    /// The profile the next run posts to.
    #[must_use]
    pub fn active_profile(&self) -> String {
        if self.can_edit {
            bitacora_runtime::ai::WRITER_PROFILE.to_owned()
        } else {
            self.choices.profile_for(self.chosen_model.as_deref())
        }
    }

    /// Reads the model choices from the server's `/info` (the default chat profile and every
    /// `bitacora-chat--*` profile), in managed and external mode alike.
    pub fn refresh_model_choices(&mut self, cx: &mut Context<Self>) {
        let Some(session) = self.session.clone() else {
            return;
        };
        let client = session.run(|s| s.agui_client());
        self.choices_task = Some(cx.spawn(async move |this, cx| {
            let Some(client) = client.recv().await.ok().flatten() else {
                return;
            };
            let task = this.update(cx, |_, cx| {
                crate::tokio_bridge::spawn(cx, async move {
                    client
                        .info()
                        .await
                        .ok()
                        .map(|i| ModelChoices::from_info(&i))
                })
            });
            let Ok(task) = task else { return };
            if let Ok(Some(choices)) = task.await {
                let _ = this.update(cx, |this, cx| {
                    this.choices = choices;
                    cx.notify();
                });
            }
        }));
    }

    /// Picks the model of the next runs (`None`: default). A live session restarts on the same
    /// thread so the next message goes to the other profile.
    pub fn choose_model(&mut self, model: Option<String>, cx: &mut Context<Self>) {
        self.model_menu = false;
        if self.model.running || self.chosen_model == model {
            cx.notify();
            return;
        }
        self.chosen_model = model.clone();
        cx.emit(ChatViewEvent::ModelChosen(model));
        if self.chat.is_some() {
            self.close_session(DenyReason::ThreadSwitch, cx);
            if self.thread_id.is_some() && !self.model.messages.is_empty() {
                self.start(cx);
            }
        }
        cx.notify();
    }

    /// Opens or closes the selector menu.
    pub fn toggle_model_menu(&mut self, cx: &mut Context<Self>) {
        self.model_menu = !self.model_menu;
        if self.model_menu {
            self.refresh_model_choices(cx);
        }
        cx.notify();
    }

    // ---- profile --------------------------------------------------------------------------------

    /// Switches between the read-only chat profile and the writer profile (`propose_edit` behind
    /// approval cards). A live session restarts on the same thread.
    pub fn toggle_edits(&mut self, cx: &mut Context<Self>) {
        if self.model.running {
            return;
        }
        self.can_edit = !self.can_edit;
        let live = self.chat.is_some();
        if live {
            self.close_session(DenyReason::ThreadSwitch, cx);
            if self.thread_id.is_some() && !self.model.messages.is_empty() {
                self.start(cx);
            }
        }
        cx.notify();
    }
}

/// A guard that lets nothing through (used when the live one cannot be read).
fn deny_all_guard() -> ContentGuard {
    ContentGuard::from_consent(&bitacora_config::pando::GraphConsent::default())
}

/// `YYYY-MM-DD HH:MM` of a server timestamp.
fn short_time(raw: &str) -> String {
    raw.chars().take(16).collect::<String>().replace('T', " ")
}

/// The chat threads of the server, newest first, titled by their first user message.
async fn fetch_threads(client: bitacora_runtime::ai::AguiClient) -> Result<Vec<ThreadRow>, String> {
    let page = client
        .list_threads(THREADS_PAGE, 0)
        .await
        .map_err(|e| e.to_string())?;
    let ours: Vec<ThreadSummary> = page
        .threads
        .into_iter()
        .filter(|t| {
            t.agent == bitacora_runtime::ai::CHAT_PROFILE
                || t.agent == bitacora_runtime::ai::WRITER_PROFILE
        })
        .collect();
    let mut rows = Vec::with_capacity(ours.len());
    for (ix, t) in ours.into_iter().enumerate() {
        let title = if ix < THREAD_TITLES {
            client
                .thread_messages(&t.thread_id)
                .await
                .ok()
                .flatten()
                .and_then(|messages| {
                    messages
                        .into_iter()
                        .find(|m| m.role == "user")
                        .map(|m| match m.content {
                            bitacora_runtime::ai::MessageContent::Text(text) => text,
                            bitacora_runtime::ai::MessageContent::Parts(_) => String::new(),
                        })
                })
                .map(|text| text.split_whitespace().collect::<Vec<_>>().join(" "))
                .filter(|text| !text.is_empty())
                .map(|text| text.chars().take(60).collect())
        } else {
            None
        };
        rows.push(ThreadRow {
            id: t.thread_id,
            title,
            updated: short_time(&t.updated_at),
        });
    }
    Ok(rows)
}

#[cfg(test)]
mod tests;
