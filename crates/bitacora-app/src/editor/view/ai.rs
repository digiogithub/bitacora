//! AI help in the block editor (BIT-US-0153): the inline ghost text and the compose box.
//!
//! Both only ever show or hold text. Nothing reaches the page until the user accepts it:
//!
//! * Ghost text is a suggestion drawn after the caret at the end of the block. Tab inserts it as
//!   if it had been typed (a normal `EditText` through the command queue, undoable); typing,
//!   moving the caret, Esc, IME composition or leaving the block discards it. Requests are
//!   debounced and cancelled by the next keystroke, and never made during IME composition.
//! * The compose box (Ctrl/Cmd+J) takes an instruction, streams a draft, and offers Insert
//!   below / Replace block / Retry / Discard. Insert and Replace are single transactions.

use std::time::Instant;

use bitacora_core::editor::{Cmd, PasteKind, classify_paste};
use bitacora_core::queue::Source;
use bitacora_markdown::edit::properties::get_property;

use super::{Context, EditorEvent, OutlineEditor, Selection, Window};
use crate::editor::actions;
use crate::editor::ai::{
    self, AiEvent, AiView, Compose, ComposeMode, ComposePhase, ComposeRequest, ComposeView, Ghost,
    RunSlot,
};
use crate::ui::AppContext as _;
use crate::ui::input::{InputEvent, InputState};

/// A block that carries `private:: true` is never sent to an agent, whatever the page says.
fn is_private(full_text: &str) -> bool {
    get_property(full_text, "private").is_some_and(|v| v.trim().eq_ignore_ascii_case("true"))
}

impl OutlineEditor {
    /// The edited block is private, or sits under a private block (which hides its subtree).
    fn edit_is_private(&self, e: &super::EditState) -> bool {
        is_private(&e.full)
            || self.outline.as_ref().is_some_and(|o| {
                bitacora_runtime::ai::under_private_block(&o.snapshot().blocks, e.id)
            })
    }

    /// The ghost text to draw after the caret, when it still belongs to the buffer as it is.
    #[must_use]
    pub fn ghost(&self) -> Option<&str> {
        let g = self.ai.ghost.as_ref()?;
        let e = self.edit.as_ref()?;
        (e.id == g.block
            && e.buf.text() == g.base
            && e.buf.cursor() == g.cursor
            && e.buf.selection().is_empty()
            && e.buf.marked().is_none()
            && !self.completion_open()
            && self.ai.compose.is_none())
        .then_some(g.text.as_str())
    }

    /// Whether the compose box is open.
    #[must_use]
    pub fn compose_open(&self) -> bool {
        self.ai.compose.is_some()
    }

    /// Phase of the compose box, when open.
    #[must_use]
    pub fn compose_phase(&self) -> Option<&ComposePhase> {
        self.ai.compose.as_ref().map(|c| &c.phase)
    }

    /// The draft of the compose box (the preview while it runs).
    #[must_use]
    pub fn compose_draft(&self) -> Option<&str> {
        self.ai.compose.as_ref().map(|c| c.preview.as_str())
    }

    /// The instruction field of the open compose box.
    #[cfg(test)]
    pub(crate) fn compose_input(&self) -> Option<crate::ui::Entity<InputState>> {
        self.ai.compose.as_ref().map(|c| c.input.clone())
    }

    /// What the edited row draws for AI help.
    pub(super) fn ai_view(&self, cx: &crate::ui::App) -> Option<AiView> {
        let ghost_hint = self.ghost().is_some();
        let compose = self.ai.compose.as_ref().map(|c| ComposeView {
            input: c.input.clone(),
            include_block: c.include_block,
            phase: c.phase.clone(),
            preview: c.preview.clone(),
            focus_pending: c.focus_pending.clone(),
            page: self
                .outline
                .as_ref()
                .map(|o| o.snapshot().title.clone())
                .unwrap_or_default(),
        });
        let _ = cx;
        (ghost_hint || compose.is_some()).then_some(AiView {
            ghost_hint,
            compose,
        })
    }

    fn row_event(&self, cx: &mut Context<Self>) {
        if let Some(r) = self.edit.as_ref().and_then(|e| self.row_of(e.id)) {
            cx.emit(EditorEvent::Row(r));
        }
    }

    fn page_title(&self) -> String {
        self.outline
            .as_ref()
            .map(|o| o.snapshot().title.clone())
            .unwrap_or_default()
    }

    // ---- ghost text ----------------------------------------------------------------------

    /// The buffer changed by typing, a paste or an IME update: the ghost and any request in
    /// flight are void, and a new request waits for a quiet moment.
    pub(super) fn ai_on_edit(&mut self, cx: &mut Context<Self>) {
        self.ai.ghost = None;
        self.ai.ghost_run = None;
        self.ai.ghost_timer = None;
        self.ai.ghost_epoch += 1;
        if std::mem::take(&mut self.ai.skip_next) || !ai::ghost_available(cx) {
            return;
        }
        let epoch = self.ai.ghost_epoch;
        self.ai.ghost_timer = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(ai::GHOST_DELAY).await;
            let _ = this.update(cx, |this, cx| {
                if this.ai.ghost_epoch == epoch {
                    this.start_ghost(cx);
                }
            });
        }));
    }

    /// The caret moved: a suggestion for the old position is void (one that still fits stays).
    pub(super) fn ai_on_motion(&mut self, cx: &mut Context<Self>) {
        if self.ghost().is_none() {
            self.ai_cancel(cx);
        }
    }

    /// Drops the ghost and every request or timer for it.
    pub(super) fn ai_cancel(&mut self, cx: &mut Context<Self>) {
        let had_ghost = self.ai.ghost.take().is_some();
        if had_ghost || self.ai.ghost_run.is_some() || self.ai.ghost_timer.is_some() {
            self.ai.ghost_run = None;
            self.ai.ghost_timer = None;
            self.ai.ghost_epoch += 1;
        }
        if had_ghost {
            self.row_event(cx);
        }
    }

    /// Leaves everything AI behind (the block left edit mode).
    pub(super) fn ai_reset(&mut self) {
        self.ai = ai::AiState::default();
    }

    fn start_ghost(&mut self, cx: &mut Context<Self>) {
        self.ai.ghost_timer = None;
        if !ai::ghost_available(cx)
            || self.ai.compose.is_some()
            || self.ai.backoff_until.is_some_and(|t| t > Instant::now())
        {
            return;
        }
        let Some(e) = &self.edit else { return };
        if e.conflict.is_some()
            || self.edit_is_private(e)
            || !ai::may_request_ghost(
                e.buf.text(),
                e.buf.cursor(),
                e.buf.selection().is_empty(),
                e.buf.marked().is_some(),
                self.completion_open(),
            )
        {
            return;
        }
        let Some(backend) = ai::backend(cx) else {
            return;
        };
        let (block, base, cursor) = (e.id, e.buf.text().to_owned(), e.buf.cursor());
        let request = ComposeRequest {
            mode: ComposeMode::Continue,
            page: self.page_title(),
            instruction: String::new(),
            block_text: base.clone(),
            include_block: true,
        };
        let run = backend.start(cx, request);
        let events = run.events.clone();
        let reader = cx.spawn(async move |this, cx| {
            while let Ok(event) = events.recv().await {
                match event {
                    AiEvent::Preview(_) => {}
                    AiEvent::Done(text) => {
                        let _ = this.update(cx, |this, cx| {
                            this.finish_ghost(block, &base, cursor, &text, cx);
                        });
                        break;
                    }
                    AiEvent::Failed(err) => {
                        tracing::debug!("ghost text unavailable: {err}");
                        let _ = this.update(cx, |this, _| {
                            this.ai.ghost_run = None;
                            this.ai.backoff_until = Some(Instant::now() + ai::GHOST_BACKOFF);
                        });
                        break;
                    }
                }
            }
        });
        self.ai.ghost_run = Some(RunSlot {
            _run: run,
            _reader: reader,
        });
    }

    fn finish_ghost(
        &mut self,
        block: bitacora_core::editor::BlockId,
        base: &str,
        cursor: usize,
        raw: &str,
        cx: &mut Context<Self>,
    ) {
        self.ai.ghost_run = None;
        let Some(e) = &self.edit else { return };
        if e.id != block
            || e.buf.text() != base
            || e.buf.cursor() != cursor
            || e.buf.marked().is_some()
            || !e.buf.selection().is_empty()
        {
            return;
        }
        let text = ai::glue(base, raw);
        if text.is_empty() {
            return;
        }
        self.ai.ghost = Some(Ghost {
            block,
            base: base.to_owned(),
            cursor,
            text,
        });
        self.row_event(cx);
        cx.notify();
    }

    /// Tab: inserts the suggestion as if it had been typed.
    pub(super) fn on_accept_ghost(
        &mut self,
        _: &actions::AcceptGhost,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(text) = self.ghost().map(str::to_owned) else {
            return;
        };
        self.ai.ghost = None;
        // What was typed so far is its own undo step; the suggestion is the next one.
        self.flush(cx);
        self.ai.skip_next = true;
        self.edit_buffer(cx, |b| b.ime_replace(None, &text));
        self.commit_as_step("Accept AI suggestion");
    }

    /// Commits the edit buffer at once as a `SetText` of its own undo step (typing edits
    /// coalesce with their neighbours, this must not).
    fn commit_as_step(&mut self, label: &'static str) {
        self.flush_epoch += 1;
        let Some(e) = self.edit.as_mut() else { return };
        let new_full = e.proj.to_text(e.buf.text());
        if new_full == e.full {
            return;
        }
        let id = e.id;
        let old = std::mem::replace(&mut e.full, new_full.clone());
        let result = self
            .queue
            .run(Source::Ui, label, Cmd::SetText { id, text: new_full });
        match result {
            Ok(_) => self.reload_outline(),
            Err(err) => {
                tracing::warn!("cannot commit the AI text: {err}");
                if let Some(e) = self.edit.as_mut() {
                    e.full = old;
                }
            }
        }
    }

    /// Esc: drops the suggestion and keeps editing.
    pub(super) fn on_dismiss_ghost(
        &mut self,
        _: &actions::DismissGhost,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ai_cancel(cx);
        cx.notify();
    }

    // ---- compose box ---------------------------------------------------------------------

    /// Ctrl/Cmd+J.
    pub(super) fn on_ai_compose(
        &mut self,
        _: &actions::AiCompose,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_compose(window, cx);
    }

    /// Opens the compose box under the edited block.
    pub fn open_compose(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(c) = &self.ai.compose {
            let input = c.input.clone();
            input.update(cx, |i, cx| i.focus(window, cx));
            return;
        }
        if !ai::compose_available(cx) {
            self.notice(window, cx, rust_i18n::t!("editor.ai.unavailable"));
            return;
        }
        if self.edit.as_ref().is_none_or(|e| self.edit_is_private(e)) {
            self.notice(window, cx, rust_i18n::t!("editor.ai.private"));
            return;
        }
        self.flush(cx);
        self.ai_cancel(cx);
        let Some(block) = self.editing() else { return };
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(rust_i18n::t!("editor.ai.placeholder").to_string())
        });
        let submit = cx.subscribe_in(&input, window, |this, _, event: &InputEvent, window, cx| {
            if matches!(event, InputEvent::PressEnter { .. }) {
                this.compose_submit(window, cx);
            }
        });
        self.ai.compose = Some(Compose {
            block,
            input: input.clone(),
            include_block: true,
            phase: ComposePhase::Idle,
            preview: String::new(),
            run: None,
            focus_pending: std::rc::Rc::new(std::cell::Cell::new(true)),
            _submit: submit,
        });
        self.row_event(cx);
        cx.notify();
    }

    /// Sends the instruction (Enter in the field, or Retry).
    pub fn compose_submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(backend) = ai::backend(cx) else {
            return;
        };
        let page = self.page_title();
        let Some(c) = self.ai.compose.as_mut() else {
            return;
        };
        if c.phase == ComposePhase::Running {
            return;
        }
        let instruction = c.input.read(cx).value().to_string();
        if !ai::instruction_ready(&instruction) {
            return;
        }
        let block_text = if c.include_block {
            self.edit
                .as_ref()
                .map(|e| e.buf.text().to_owned())
                .or_else(|| {
                    self.outline
                        .as_ref()
                        .and_then(|o| o.block(c.block))
                        .map(|b| b.text.clone())
                })
                .unwrap_or_default()
        } else {
            String::new()
        };
        let request = ComposeRequest {
            mode: ComposeMode::Compose,
            page,
            instruction,
            block_text,
            include_block: c.include_block,
        };
        let run = backend.start(cx, request);
        let events = run.events.clone();
        c.phase = ComposePhase::Running;
        c.preview.clear();
        let reader = cx.spawn(async move |this, cx| {
            while let Ok(event) = events.recv().await {
                let done = !matches!(event, AiEvent::Preview(_));
                let _ = this.update(cx, |this, cx| this.compose_event(event, cx));
                if done {
                    break;
                }
            }
        });
        if let Some(c) = self.ai.compose.as_mut() {
            c.run = Some(RunSlot {
                _run: run,
                _reader: reader,
            });
        }
        let _ = window;
        self.row_event(cx);
        cx.notify();
    }

    fn compose_event(&mut self, event: AiEvent, cx: &mut Context<Self>) {
        let Some(c) = self.ai.compose.as_mut() else {
            return;
        };
        match event {
            AiEvent::Preview(p) => c.preview = p,
            AiEvent::Done(text) => {
                c.preview = text;
                c.phase = ComposePhase::Ready;
                c.run = None;
            }
            AiEvent::Failed(err) => {
                c.phase = ComposePhase::Failed(err);
                c.run = None;
            }
        }
        self.row_event(cx);
        cx.notify();
    }

    /// Toggles the "this block" context chip.
    pub fn compose_toggle_block(&mut self, cx: &mut Context<Self>) {
        if let Some(c) = self.ai.compose.as_mut()
            && c.phase != ComposePhase::Running
        {
            c.include_block = !c.include_block;
            self.row_event(cx);
            cx.notify();
        }
    }

    /// Closes the box and drops the draft; the block is untouched.
    pub fn compose_discard(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.ai.compose.take().is_none() {
            return;
        }
        self.focus_handle.focus(window, cx);
        self.row_event(cx);
        cx.notify();
    }

    pub(super) fn on_dismiss_compose(
        &mut self,
        _: &actions::DismissCompose,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.compose_discard(window, cx);
    }

    /// The finished draft, when it can be inserted.
    fn compose_ready(&self) -> Option<(bitacora_core::editor::BlockId, String)> {
        let c = self.ai.compose.as_ref()?;
        (c.phase == ComposePhase::Ready && !c.preview.trim().is_empty())
            .then(|| (c.block, c.preview.clone()))
    }

    /// "Insert below": the draft becomes new block(s) after the block, one transaction.
    pub fn compose_insert_below(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((block, text)) = self.compose_ready() else {
            return;
        };
        self.ai.compose = None;
        self.focus_handle.focus(window, cx);
        let editing = self.editing();
        let cmd = match classify_paste(&text, false) {
            PasteKind::Blocks(blocks) => Cmd::InsertBlocks {
                target: block,
                sibling: Some(true),
                blocks,
                keep_uuids: false,
            },
            _ => Cmd::InsertSibling {
                after: block,
                text: text.trim().to_owned(),
            },
        };
        if let Some(tx) = self.run("AI compose", cmd, window, cx) {
            self.after_command(
                editing,
                None,
                Selection::default(),
                tx.cursor_after,
                window,
                cx,
            );
        }
        cx.notify();
    }

    /// "Replace block": the visible text of the block becomes the draft, one transaction.
    pub fn compose_replace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((block, text)) = self.compose_ready() else {
            return;
        };
        self.ai.compose = None;
        self.focus_handle.focus(window, cx);
        if self.editing() != Some(block) {
            return;
        }
        let draft = text.trim().to_owned();
        self.ai.skip_next = true;
        self.edit_buffer(cx, |b| {
            let len = b.text().len();
            b.replace_range(0..len, &draft);
            b.set_cursor(draft.len());
        });
        // One `SetText` of its own: the buffer was flushed when the box opened.
        self.commit_as_step("AI compose");
        cx.notify();
    }
}
