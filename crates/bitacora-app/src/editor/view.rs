//! `OutlineEditor`: editing state of one page (BIT-US-0030..0039).
//!
//! The editor owns a core page snapshot, the render rows derived from it, the block selection
//! and, while a block is in edit mode, its text buffer. Every change goes through the core
//! command queue as a `Cmd`; the buffer is flushed to core as an `EditText` after 500 ms, on
//! blur, on Esc and before any other command. The page view embeds the editor: it draws the
//! rows (asking [`OutlineEditor::row_edit`] for the per-row state) and routes the key context
//! and actions to the editor entity.

use std::ops::Range;
use std::rc::Rc;
use std::time::Duration;

use bitacora_core::editor::{
    BlockId, Cmd, CommitError, EditProjection, EditorSettings, HiddenKeys, HistoryError,
    PRIVATE_MIME, PasteKind, Refusal, classify_paste, clipboard, parse_private,
};
use bitacora_core::graph::PageKey;
use bitacora_core::queue::{CommandQueue, QueueError, Request, Source};
use bitacora_markdown::properties::PropertyConfig;

use super::actions::{self, context};
use super::autopair;
use super::buffer::BlockBuffer;
use super::completion::{self, Item, Trigger};
use super::html;
use super::layout::BlockLayout;
use super::outline::{Outline, Prebuilt};
use super::row::RowEdit;
use super::style::{Palette, TextMetrics, source_runs, style_runs};
use super::text_ops::{range_from_utf16, range_to_utf16};
use crate::data::GraphHandle;
use crate::render::model::{BlockModel, Row, visible_rows};
use crate::ui::calendar::CalendarState;
use crate::ui::text_edit::{ClipboardItem, EntityInputHandler, UTF16Selection};
use crate::ui::theme::ActiveBitacoraTheme as _;
use crate::ui::{
    App, Bounds, Context, Entity, EventEmitter, FocusHandle, Focusable, Level, Pixels, Render,
    Subscription, Window, div, notify, px,
};

mod dnd;
mod slash;

pub use slash::Clock;

/// Idle time after which the edit buffer is committed.
pub const FLUSH_DELAY: Duration = Duration::from_millis(500);
/// Blocks longer than this are selected instead of edited (a huge buffer would stall typing).
pub const MAX_EDIT_LEN: usize = 10_000;
/// Indentation per depth level in the rendered rows.
pub const INDENT: f32 = 24.;

/// Where the caret goes when a block enters edit mode.
#[derive(Debug, Clone, PartialEq)]
pub enum Caret {
    /// Offset 0.
    Start,
    /// End of the visible text.
    End,
    /// A byte offset of the visible text.
    Visible(usize),
    /// A selection in offsets of the full block text (hidden lines included).
    Full(Range<usize>),
    /// First visual row, closest to this x (page coordinates).
    FirstRowAtX(Pixels),
    /// Last visual row, closest to this x.
    LastRowAtX(Pixels),
}

/// What the editor tells the page view.
#[derive(Debug, Clone, PartialEq)]
pub enum EditorEvent {
    /// Up/Down went past the first or last block of the page: a host that shows several
    /// pages (the journals feed) continues in the neighbouring one at the same x.
    Leave {
        /// Moving down (to the next page) or up.
        down: bool,
        /// Goal x of the caret, in page coordinates.
        goal: Pixels,
    },
    /// Rows were rebuilt (structure changed): copy them all.
    Structure,
    /// One row changed (its model or its height).
    Row(usize),
    /// The row got the caret: scroll it into view.
    Entered(usize),
    /// A block drag is near the top (negative) or bottom edge: scroll the page by this many
    /// pixels.
    Scroll(f32),
    /// The user asked to delete the file behind an asset link of a block (the host confirms).
    DeleteAsset {
        /// The link target as written in the block (`../assets/x.png`).
        link: String,
        /// Index uuid of the block the link is in, so that it is not counted as a user.
        block: Option<String>,
    },
}

/// The block selection: an anchor, a moving head, and the blocks between them in reading order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Selection {
    /// Where the selection started.
    pub anchor: Option<BlockId>,
    /// The end that moves with Shift+Up/Down.
    pub head: Option<BlockId>,
}

/// The open completion popup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionState {
    /// What the caret is inside of.
    pub trigger: Trigger,
    /// Candidates, best first.
    pub items: Vec<Item>,
    /// Highlighted candidate.
    pub selected: usize,
}

/// The block in edit mode.
#[derive(Debug)]
struct EditState {
    id: BlockId,
    proj: EditProjection,
    buf: BlockBuffer,
    /// The block text core holds (what the buffer is diffed against).
    full: String,
    goal_x: Option<Pixels>,
    /// External change of exactly this block while it was being edited.
    conflict: Option<(String, String)>,
}

/// The editor of one page.
pub struct OutlineEditor {
    queue: CommandQueue,
    config: std::sync::Arc<bitacora_config::EffectiveConfig>,
    gate: Option<std::sync::Arc<crate::editing::EditingGate>>,
    handle: Option<GraphHandle>,
    props: PropertyConfig,
    hidden: HiddenKeys,
    key: Option<PageKey>,
    outline: Option<Outline>,
    /// The outline the current `rows` were built from (row reuse after local commands).
    rows_outline: Option<Outline>,
    zoom: Option<BlockId>,
    rows: Vec<Row>,
    ids: Vec<BlockId>,
    visible: Vec<usize>,
    sel: Selection,
    edit: Option<EditState>,
    focus_handle: FocusHandle,
    // Geometry recorded while painting the edited block.
    last_layout: Option<Rc<BlockLayout>>,
    last_bounds: Option<Bounds<Pixels>>,
    metrics: Option<TextMetrics>,
    content_width0: Pixels,
    is_selecting: bool,
    /// Block under the press that started a drag; moving over other rows selects blocks.
    drag_anchor: Option<BlockId>,
    caret_visible: bool,
    blink_epoch: usize,
    blink_enabled: bool,
    flush_epoch: usize,
    completion: Option<CompletionState>,
    /// Where a dragged block would land (row block and zone), while dragging over this page.
    drop_hint: Option<(BlockId, super::dnd::DropZone)>,
    /// Auto-scroll step while dragging near an edge of the viewport (0 = none).
    scroll_dir: f32,
    scroll_task: Option<crate::ui::Task<()>>,
    /// The open calendar (`/date picker`, `/scheduled`, `/deadline`; BIT-US-0105).
    picker: Option<slash::DatePick>,
    /// Local date and time (replaced in tests).
    clock: Clock,
    /// The trigger the user dismissed with Esc (it stays closed until the text leaves it).
    dismissed: Option<usize>,
    _subscriptions: Vec<Subscription>,
}

impl std::fmt::Debug for OutlineEditor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OutlineEditor")
            .field("key", &self.key)
            .field("rows", &self.rows.len())
            .field("editing", &self.edit.as_ref().map(|e| e.id))
            .finish_non_exhaustive()
    }
}

impl EventEmitter<EditorEvent> for OutlineEditor {}

impl Focusable for OutlineEditor {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for OutlineEditor {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl crate::ui::IntoElement {
        // The editor draws nothing itself: the page view renders its rows.
        div()
    }
}

impl OutlineEditor {
    /// An editor over the pages of `handle`'s graph, committing through `queue`.
    pub fn new(
        queue: CommandQueue,
        hidden: HiddenKeys,
        blink: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        let weak = cx.weak_entity();
        dnd::EditorRegistry::register(cx, weak);
        let subscriptions = vec![
            cx.on_blur(&focus_handle, window, |this, _, cx| this.on_blur(cx)),
            cx.observe_window_activation(window, |this, window, cx| {
                if !window.is_window_active() {
                    this.flush(cx);
                }
            }),
        ];
        Self {
            props: PropertyConfig::default(),
            queue,
            config: std::sync::Arc::default(),
            gate: None,
            handle: None,
            hidden,
            key: None,
            outline: None,
            rows_outline: None,
            zoom: None,
            rows: Vec::new(),
            ids: Vec::new(),
            visible: Vec::new(),
            sel: Selection::default(),
            edit: None,
            focus_handle,
            last_layout: None,
            last_bounds: None,
            metrics: None,
            content_width0: px(640.),
            is_selecting: false,
            drag_anchor: None,
            caret_visible: true,
            blink_epoch: 0,
            blink_enabled: blink,
            flush_epoch: 0,
            completion: None,
            picker: None,
            drop_hint: None,
            scroll_dir: 0.,
            scroll_task: None,
            clock: slash::system_clock(),
            dismissed: None,
            _subscriptions: subscriptions,
        }
    }

    /// Publishes the edited block to the MCP write gate (`BLOCK_BUSY`).
    pub fn set_gate(&mut self, gate: std::sync::Arc<crate::editing::EditingGate>) {
        self.gate = Some(gate);
    }

    /// Tells core and the MCP gate which block has the caret (`None` = none).
    fn publish_editing(&self, id: Option<BlockId>, cx: &mut Context<Self>) {
        self.queue.set_editing_block(id);
        let Some(gate) = self.gate.clone() else {
            return;
        };
        let Some(id) = id else {
            gate.clear();
            return;
        };
        let Some(snap) = self.key.as_ref().and_then(|k| self.queue.snapshot(k)) else {
            gate.clear();
            return;
        };
        let Some(block) = snap.blocks.iter().find(|b| b.id == id) else {
            gate.clear();
            return;
        };
        if let Some(u) = block.uuid {
            gate.set(&snap.title, &u.to_string());
            return;
        }
        // No `id::` yet: agents address the block by its index uuid; look it up off the UI thread.
        gate.clear();
        let Some(handle) = self.handle.clone() else {
            return;
        };
        let (title, text) = (snap.title.clone(), block.text.clone());
        let task = cx.background_executor().spawn(async move {
            resolve_index_uuid(&handle, &title, &text, None).map(|u| (title, u))
        });
        cx.spawn(async move |this, cx| {
            if let Some((title, uuid)) = task.await {
                let _ = this.update(cx, |this, _| {
                    if this.editing() == Some(id) {
                        gate.set(&title, &uuid);
                    }
                });
            }
        })
        .detach();
    }

    /// The effective configuration (page paths of new pages).
    pub fn set_config(&mut self, config: std::sync::Arc<bitacora_config::EffectiveConfig>) {
        self.config = config;
    }

    /// The focus handle of the outline.
    pub fn focus_handle_ref(&self) -> &FocusHandle {
        &self.focus_handle
    }

    /// The graph whose index resolves block references and counts.
    pub fn set_handle(&mut self, handle: GraphHandle) {
        self.props = handle.settings.props.clone();
        self.handle = Some(handle);
    }

    /// Tells core the editing preferences of the graph.
    pub fn apply_settings(&self, settings: EditorSettings) {
        let _ = self.queue.set_settings(Source::Ui, settings);
    }

    // ---- accessors -----------------------------------------------------------------------

    /// The render rows (parallel to [`Self::block_ids`]).
    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    /// Core ids of the rows.
    pub fn block_ids(&self) -> &[BlockId] {
        &self.ids
    }

    /// The page being edited.
    pub fn page_key(&self) -> Option<&PageKey> {
        self.key.as_ref()
    }

    /// The core page snapshot in use.
    pub fn outline(&self) -> Option<&Outline> {
        self.outline.as_ref()
    }

    /// The block in edit mode.
    pub fn editing(&self) -> Option<BlockId> {
        self.edit.as_ref().map(|e| e.id)
    }

    /// Text shown in the editor for the block in edit mode.
    pub fn buffer_text(&self) -> Option<&str> {
        self.edit.as_ref().map(|e| e.buf.text())
    }

    /// Caret of the edited block (offset in the visible text).
    pub fn cursor_offset(&self) -> usize {
        self.edit.as_ref().map_or(0, |e| e.buf.cursor())
    }

    /// Selection inside the edited block.
    pub fn selection_range(&self) -> Range<usize> {
        self.edit.as_ref().map_or(0..0, |e| e.buf.selection())
    }

    /// IME composition range of the edited block.
    pub fn marked_range(&self) -> Option<Range<usize>> {
        self.edit.as_ref().and_then(|e| e.buf.marked())
    }

    /// The selected blocks in reading order.
    pub fn selected_blocks(&self) -> Vec<BlockId> {
        let (Some(a), Some(h)) = (self.sel.anchor, self.sel.head) else {
            return Vec::new();
        };
        let (Some(ra), Some(rh)) = (self.row_of(a), self.row_of(h)) else {
            return Vec::new();
        };
        let (lo, hi) = (ra.min(rh), ra.max(rh));
        self.visible
            .iter()
            .copied()
            .filter(|r| (lo..=hi).contains(r))
            .map(|r| self.ids[r])
            .collect()
    }

    /// Whether blocks are selected.
    pub fn has_selection(&self) -> bool {
        self.edit.is_none() && self.sel.anchor.is_some()
    }

    /// The zoom root, when the view is zoomed into a block.
    pub fn zoom_root(&self) -> Option<BlockId> {
        self.zoom
    }

    /// Breadcrumb of the zoom: `(label, block to zoom to)`; `None` is the page itself.
    pub fn crumbs(&self) -> Vec<(String, Option<BlockId>)> {
        let (Some(root), Some(outline)) = (self.zoom, &self.outline) else {
            return Vec::new();
        };
        let mut out = vec![(outline.snapshot().title.clone(), None)];
        for id in outline.ancestors(root).into_iter().chain([root]) {
            let text = outline.block(id).map_or("", |b| b.text.as_str());
            let first = text.lines().next().unwrap_or("");
            let label: String = first.chars().take(40).collect();
            out.push((label, Some(id)));
        }
        out
    }

    /// Whether a completion popup is open (key context).
    pub fn completion_open(&self) -> bool {
        self.completion.is_some() || self.picker.is_some()
    }

    /// The calendar state while a date is being picked.
    pub fn picker_state(&self) -> Option<&Entity<CalendarState>> {
        self.picker.as_ref().map(|p| &p.state)
    }

    /// The day highlighted in the open calendar.
    pub fn picker_day(&self) -> Option<bitacora_core::date::Date> {
        self.picker.as_ref().map(|p| p.day)
    }

    /// The open completion popup.
    pub fn completion(&self) -> Option<&CompletionState> {
        self.completion.as_ref()
    }

    /// Caret position inside the edited block's text area (for anchoring the popup).
    pub fn caret_x(&self) -> Pixels {
        match (&self.last_layout, &self.edit) {
            (Some(layout), Some(e)) => layout.position_for_index(e.buf.cursor()).x,
            _ => px(0.),
        }
    }

    /// Whether the caret is in the visible half of its blink cycle.
    pub fn caret_visible(&self) -> bool {
        self.caret_visible
    }

    fn row_of(&self, id: BlockId) -> Option<usize> {
        self.ids.iter().position(|i| *i == id)
    }

    /// The key context of the focused outline.
    pub fn key_context_name(&self) -> &'static str {
        if self.edit.is_some() {
            if self.picker.is_some() {
                "Outliner BlockEditor Autocomplete DatePicker"
            } else if self.completion.is_some() {
                "Outliner BlockEditor Autocomplete"
            } else {
                "Outliner BlockEditor"
            }
        } else if self.sel.anchor.is_some() {
            "Outliner BlockSelection"
        } else {
            context::OUTLINER
        }
    }

    // ---- page ----------------------------------------------------------------------------

    /// Shows the page `key` from core. Showing the page that is already open refreshes it
    /// and keeps the edit buffer.
    pub fn set_page(&mut self, key: PageKey, cx: &mut Context<Self>) {
        if self.key.as_ref() != Some(&key) {
            self.exit_edit_silent();
            self.sel = Selection::default();
            self.zoom = None;
            self.key = Some(key);
        }
        self.refresh(cx);
    }

    /// [`OutlineEditor::set_page`] with the rows already built (off the UI thread). Falls back
    /// to a normal refresh when the page changed since the rows were built. Returns the rows
    /// the host view should show.
    pub fn set_page_prebuilt(
        &mut self,
        key: PageKey,
        pre: Prebuilt,
        cx: &mut Context<Self>,
    ) -> Option<Vec<Row>> {
        let _span = crate::perf::span("editor.set_page_prebuilt");
        if self.key.as_ref() != Some(&key) {
            self.exit_edit_silent();
            self.sel = Selection::default();
            self.zoom = None;
            self.key = Some(key.clone());
        }
        let fresh = self
            .queue
            .snapshot(&key)
            .is_some_and(|s| Arc_ptr_eq(pre.outline.snapshot(), &s));
        if !fresh || self.zoom.is_some() || self.handle.is_none() || self.edit.is_some() {
            tracing::debug!(target: "bitacora::perf", fresh, "prebuilt rows discarded");
            // While typing, untouched blocks keep their rows (the rebuild would otherwise parse
            // the whole page on every reload).
            self.refresh_with(self.edit.is_some(), cx);
            return None;
        }
        let Prebuilt {
            outline,
            rows,
            ids,
            host_rows,
        } = pre;
        self.rows_outline = Some(outline.clone());
        self.outline = Some(outline);
        self.rows = rows;
        self.ids = ids;
        self.visible = visible_rows(&self.rows);
        cx.emit(EditorEvent::Structure);
        cx.notify();
        Some(host_rows)
    }

    /// Forgets the page.
    pub fn clear(&mut self) {
        self.exit_edit_silent();
        self.key = None;
        self.outline = None;
        self.rows_outline = None;
        self.rows.clear();
        self.ids.clear();
        self.visible.clear();
        self.sel = Selection::default();
    }

    /// Reads the latest snapshot from core and rebuilds the rows.
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        self.refresh_with(false, cx);
    }

    fn refresh_with(&mut self, reuse: bool, cx: &mut Context<Self>) {
        let Some(snap) = self.key.as_ref().and_then(|k| self.queue.snapshot(k)) else {
            return;
        };
        if self
            .outline
            .as_ref()
            .is_some_and(|o| Arc_ptr_eq(o.snapshot(), &snap))
        {
            return;
        }
        self.outline = Some(Outline::new(snap));
        self.rebuild_rows_with(reuse, cx);
    }

    fn reload_outline(&mut self) {
        let _span = crate::perf::span("editor.reload_outline");
        if let Some(snap) = self.key.as_ref().and_then(|k| self.queue.snapshot(k)) {
            self.outline = Some(Outline::new(snap));
        }
    }

    /// Rebuilds every row from the current outline (index changes, zoom, first load).
    fn rebuild_rows(&mut self, cx: &mut Context<Self>) {
        self.rebuild_rows_with(false, cx);
    }

    /// Rebuilds every row from scratch (tests compare it with the reusing path).
    #[cfg(test)]
    pub(crate) fn rebuild_all_rows_for_test(&mut self, cx: &mut Context<Self>) {
        self.rebuild_rows(cx);
    }

    /// Rebuilds the rows after a local command: untouched blocks keep their parsed model.
    fn rebuild_rows_after_command(&mut self, cx: &mut Context<Self>) {
        self.rebuild_rows_with(true, cx);
    }

    fn rebuild_rows_with(&mut self, reuse: bool, cx: &mut Context<Self>) {
        let _span = crate::perf::span("editor.rebuild_rows");
        let (Some(outline), Some(handle)) = (&self.outline, &self.handle) else {
            return;
        };
        if self.zoom.is_some_and(|z| outline.index_of(z).is_none()) {
            self.zoom = None;
        }
        let prev = self
            .rows_outline
            .as_ref()
            .filter(|_| reuse)
            .map(|o| (o, std::mem::take(&mut self.rows), self.ids.as_slice()));
        let (rows, ids) = outline.rows_reusing(self.zoom, handle, &self.props, prev);
        self.rows_outline = Some(outline.clone());
        self.rows = rows;
        self.ids = ids;
        self.visible = visible_rows(&self.rows);
        // Drop selection and edit state of blocks that are gone.
        if self.sel.anchor.is_some_and(|a| self.row_of(a).is_none())
            || self.sel.head.is_some_and(|h| self.row_of(h).is_none())
        {
            self.sel = Selection::default();
        }
        if let Some(e) = self.edit.as_ref()
            && self.row_of(e.id).is_none()
        {
            // The block was removed by an external change: keep what was typed on the
            // clipboard rather than losing it.
            let typed = e.proj.to_text(e.buf.text());
            if typed != e.full {
                tracing::warn!("the edited block vanished; its unsaved text went to the clipboard");
                cx.write_to_clipboard(ClipboardItem::new_string(typed));
            }
            self.exit_edit_silent();
        }
        cx.emit(EditorEvent::Structure);
        cx.notify();
    }

    fn rebuild_row_model(&mut self, id: BlockId) -> Option<usize> {
        let r = self.row_of(id)?;
        let text = self.outline.as_ref()?.block(id)?.text.clone();
        let resolver = crate::data::IndexResolver(&self.handle.as_ref()?.reader);
        self.rows[r].block = BlockModel::from_content(&text, &self.props, &resolver);
        Some(r)
    }

    // ---- notices -------------------------------------------------------------------------

    fn notice(&self, window: &mut Window, cx: &mut Context<Self>, text: impl Into<String>) {
        notify(window, cx, Level::Info, text.into());
    }

    // ---- edit lifecycle ------------------------------------------------------------------

    fn restart_blink(&mut self, cx: &mut Context<Self>) {
        self.caret_visible = true;
        self.blink_epoch += 1;
        // A steady caret under "reduce motion" (BIT-T-0338).
        if !self.blink_enabled || cx.reduce_motion() {
            return;
        }
        let epoch = self.blink_epoch;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(530))
                    .await;
                let keep = this
                    .update(cx, |this, cx| {
                        if this.blink_epoch != epoch {
                            return false;
                        }
                        this.caret_visible = !this.caret_visible;
                        cx.notify();
                        true
                    })
                    .unwrap_or(false);
                if !keep {
                    break;
                }
            }
        })
        .detach();
    }

    fn schedule_flush(&mut self, cx: &mut Context<Self>) {
        self.flush_epoch += 1;
        let epoch = self.flush_epoch;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(FLUSH_DELAY).await;
            let _ = this.update(cx, |this, cx| {
                if this.flush_epoch == epoch {
                    if this.marked() {
                        // No commit while an IME composition is active: look again later.
                        this.schedule_flush(cx);
                    } else {
                        this.flush_in_background(cx);
                    }
                }
            });
        })
        .detach();
    }

    /// The debounced flush: submits the `EditText` without waiting for the writer, so typing
    /// never blocks on the queue thread. The queue keeps submission order, so later commands
    /// (which do wait) always see this edit applied.
    fn flush_in_background(&mut self, cx: &mut Context<Self>) {
        self.flush_epoch += 1;
        let Some(e) = self.edit.as_mut() else { return };
        let new_full = e.proj.to_text(e.buf.text());
        if new_full == e.full {
            return;
        }
        let (range, inserted) = text_diff(&e.full, &new_full);
        let id = e.id;
        let old = std::mem::replace(&mut e.full, new_full.clone());
        let reply = self.queue.submit(
            Source::Ui,
            Request::Run {
                label: "Typing",
                cmd: Cmd::EditText {
                    id,
                    range,
                    inserted,
                },
            },
        );
        cx.spawn(async move |this, cx| {
            let result = reply.await.unwrap_or(Err(QueueError::Closed));
            let _ = this.update(cx, |this, cx| match result {
                Ok(_) => {
                    this.reload_outline();
                    cx.notify();
                }
                Err(err) => {
                    tracing::warn!("cannot commit the edit buffer: {err}");
                    if let Some(e) = this.edit.as_mut()
                        && e.id == id
                        && e.full == new_full
                    {
                        e.full = old;
                    }
                }
            });
        })
        .detach();
    }

    /// Commits the edit buffer to core as one `EditText` (nothing when it is unchanged).
    /// Returns whether a transaction was committed.
    pub fn flush(&mut self, _cx: &mut Context<Self>) -> bool {
        self.flush_epoch += 1;
        let Some(e) = self.edit.as_mut() else {
            return false;
        };
        let new_full = e.proj.to_text(e.buf.text());
        if new_full == e.full {
            return false;
        }
        let (range, inserted) = text_diff(&e.full, &new_full);
        let id = e.id;
        let old = std::mem::replace(&mut e.full, new_full);
        let _span = crate::perf::span("editor.flush_blocking");
        let result = self.queue.run(
            Source::Ui,
            "Typing",
            Cmd::EditText {
                id,
                range,
                inserted,
            },
        );
        match result {
            Ok(_) => {
                self.reload_outline();
                true
            }
            Err(err) => {
                tracing::warn!("cannot commit the edit buffer: {err}");
                if let Some(e) = self.edit.as_mut() {
                    e.full = old;
                }
                false
            }
        }
    }

    fn on_blur(&mut self, cx: &mut Context<Self>) {
        // Clicking the calendar moves the focus into it; the block stays in edit mode.
        if self.edit.is_some() && self.picker.is_none() {
            self.exit_edit(cx);
        }
    }

    /// Leaves edit mode after flushing; the row model is refreshed from the new text.
    pub fn exit_edit(&mut self, cx: &mut Context<Self>) {
        self.flush(cx);
        let Some(e) = self.edit.take() else { return };
        self.publish_editing(None, cx);
        self.completion = None;
        self.picker = None;
        self.dismissed = None;
        self.reload_outline();
        if let Some(r) = self.rebuild_row_model(e.id) {
            cx.emit(EditorEvent::Row(r));
        }
        cx.notify();
    }

    fn exit_edit_silent(&mut self) {
        if self.edit.take().is_some() {
            self.queue.set_editing_block(None);
            if let Some(gate) = &self.gate {
                gate.clear();
            }
        }
    }

    /// Puts the block `id` in edit mode with the caret at `caret`.
    pub fn enter(
        &mut self,
        id: BlockId,
        caret: Caret,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.edit.as_ref().map(|e| e.id) != Some(id) {
            self.exit_edit(cx);
        } else {
            self.flush(cx);
        }
        self.reload_outline();
        let Some(r) = self.row_of(id) else { return };
        let Some(text) = self
            .outline
            .as_ref()
            .and_then(|o| o.block(id))
            .map(|b| b.text.clone())
        else {
            return;
        };
        if text.len() > MAX_EDIT_LEN {
            self.select_only(id, window, cx);
            self.notice(window, cx, rust_i18n::t!("editor.too_long"));
            return;
        }
        let proj = EditProjection::from_text(&text, &self.hidden);
        let visible = proj.visible().to_owned();
        let (selection, goal) = match caret {
            Caret::Start => (0..0, None),
            Caret::End => (visible.len()..visible.len(), None),
            Caret::Visible(o) => (o.min(visible.len())..o.min(visible.len()), None),
            Caret::Full(range) => (
                proj.full_to_visible(range.start)..proj.full_to_visible(range.end),
                None,
            ),
            Caret::FirstRowAtX(x) => {
                let o = self.offset_on_edge(&visible, r, x, false, window, cx);
                (o..o, Some(x))
            }
            Caret::LastRowAtX(x) => {
                let o = self.offset_on_edge(&visible, r, x, true, window, cx);
                (o..o, Some(x))
            }
        };
        let mut buf = BlockBuffer::with_cursor(visible, selection.end);
        if selection.start != selection.end {
            buf.set_selection(selection, false);
        }
        self.edit = Some(EditState {
            id,
            proj,
            buf,
            full: text,
            goal_x: goal,
            conflict: None,
        });
        self.sel = Selection::default();
        self.completion = None;
        self.picker = None;
        self.dismissed = None;
        self.last_layout = None;
        self.publish_editing(Some(id), cx);
        self.focus_handle.focus(window, cx);
        self.restart_blink(cx);
        cx.emit(EditorEvent::Row(r));
        cx.emit(EditorEvent::Entered(r));
        cx.notify();
    }

    fn offset_on_edge(
        &self,
        visible: &str,
        row: usize,
        x: Pixels,
        last: bool,
        window: &Window,
        cx: &App,
    ) -> usize {
        let depth = self.rows[row].depth;
        let layout = self.shape(visible, None, self.content_width_at(depth), window, cx);
        let target = if last { layout.row_count() - 1 } else { 0 };
        layout.index_on_row(target, x - indent_px(depth))
    }

    // ---- selection -----------------------------------------------------------------------

    /// Selects exactly `id` (leaves edit mode).
    pub fn select_only(&mut self, id: BlockId, window: &mut Window, cx: &mut Context<Self>) {
        self.exit_edit(cx);
        self.sel = Selection {
            anchor: Some(id),
            head: Some(id),
        };
        self.focus_handle.focus(window, cx);
        cx.notify();
        if let Some(r) = self.row_of(id) {
            cx.emit(EditorEvent::Entered(r));
        }
    }

    /// Clears the block selection.
    pub fn clear_selection(&mut self, cx: &mut Context<Self>) {
        if self.sel.anchor.take().is_some() {
            self.sel.head = None;
            cx.notify();
        }
    }

    fn visible_pos(&self, id: BlockId) -> Option<usize> {
        let r = self.row_of(id)?;
        self.visible.iter().position(|v| *v == r)
    }

    fn neighbor(&self, id: BlockId, down: bool) -> Option<BlockId> {
        let pos = self.visible_pos(id)?;
        let to = if down { pos + 1 } else { pos.checked_sub(1)? };
        self.visible.get(to).map(|r| self.ids[*r])
    }

    // ---- shaping -------------------------------------------------------------------------

    fn content_width_at(&self, depth: usize) -> Pixels {
        (self.content_width0 - indent_px(depth)).max(px(40.))
    }

    fn shape(
        &self,
        text: &str,
        marked: Option<Range<usize>>,
        width: Pixels,
        window: &Window,
        cx: &App,
    ) -> Rc<BlockLayout> {
        let metrics = self
            .metrics
            .clone()
            .unwrap_or_else(|| TextMetrics::from_window(window));
        self.shape_with(
            text,
            marked,
            width,
            &metrics,
            &Palette::from_theme(cx),
            window,
        )
    }

    /// Shapes `text` for a given content width.
    pub fn shape_with(
        &self,
        text: &str,
        marked: Option<Range<usize>>,
        width: Pixels,
        metrics: &TextMetrics,
        palette: &Palette,
        window: &Window,
    ) -> Rc<BlockLayout> {
        let runs = style_runs(source_runs(text, marked), &metrics.font, palette);
        let lines = window
            .text_system()
            .shape_text(
                text.to_owned().into(),
                metrics.font_size,
                &runs,
                Some(width),
                None,
            )
            .map(|l| l.into_iter().collect::<Vec<_>>())
            .unwrap_or_default();
        Rc::new(BlockLayout::new(lines, metrics.line_height))
    }

    /// Shapes the edited block at the width it was last painted.
    pub fn shape_edited(&self, window: &Window, cx: &App) -> Option<Rc<BlockLayout>> {
        let e = self.edit.as_ref()?;
        let r = self.row_of(e.id)?;
        Some(self.shape(
            e.buf.text(),
            e.buf.marked(),
            self.content_width_at(self.rows[r].depth),
            window,
            cx,
        ))
    }

    /// Remembers what the element painted, for hit testing and IME bounds.
    pub fn record_paint(
        &mut self,
        layout: Rc<BlockLayout>,
        bounds: Bounds<Pixels>,
        metrics: TextMetrics,
    ) {
        if let Some(r) = self.edit.as_ref().and_then(|e| self.row_of(e.id)) {
            self.content_width0 = bounds.size.width + indent_px(self.rows[r].depth);
        }
        self.last_layout = Some(layout);
        self.last_bounds = Some(bounds);
        self.metrics = Some(metrics);
    }

    // ---- commands ------------------------------------------------------------------------

    /// Runs `cmd` through core (the edit buffer is flushed first). Refusals are reported as a
    /// notice and return `None`; on success the snapshot and rows are refreshed.
    fn run(
        &mut self,
        label: &'static str,
        cmd: Cmd,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<bitacora_core::editor::Transaction> {
        self.flush(cx);
        let _span = crate::perf::span("editor.run_blocking");
        match self.queue.run(Source::Ui, label, cmd) {
            Ok(tx) => {
                self.reload_outline();
                self.rebuild_rows_after_command(cx);
                Some(tx)
            }
            Err(QueueError::Commit(CommitError::Refused(Refusal::NoChange))) => None,
            Err(QueueError::Commit(CommitError::Refused(r))) => {
                window.play_system_bell();
                self.notice(window, cx, crate::i18n::refusal(&r));
                None
            }
            Err(err) => {
                tracing::warn!("command failed: {err}");
                self.notice(
                    window,
                    cx,
                    rust_i18n::t!("editor.cannot_do", error = err.to_string()),
                );
                None
            }
        }
    }

    /// The blocks a structural command acts on: the edited block or the selection (top-level
    /// blocks only).
    fn targets(&self) -> Vec<BlockId> {
        if let Some(e) = &self.edit {
            return vec![e.id];
        }
        let sel = self.selected_blocks();
        match &self.outline {
            Some(o) => o.top_level(&sel),
            None => sel,
        }
    }

    /// Full-text selection of the edited block.
    fn full_selection(&self) -> Option<Range<usize>> {
        let e = self.edit.as_ref()?;
        let s = e.buf.selection();
        Some(
            e.proj.visible_to_full(e.buf.text(), s.start)
                ..e.proj.visible_to_full(e.buf.text(), s.end),
        )
    }

    /// Runs a structural command on the targets; the editor keeps the caret (or the selection)
    /// on the same blocks afterwards.
    fn structural(
        &mut self,
        label: &'static str,
        make: impl FnOnce(Vec<BlockId>) -> Cmd,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let targets = self.targets();
        if targets.is_empty() {
            return;
        }
        self.flush(cx);
        let editing = self.edit.as_ref().map(|e| e.id);
        let cursor = self.full_selection();
        let selection = self.sel.clone();
        let Some(tx) = self.run(label, make(targets.clone()), window, cx) else {
            return;
        };
        self.after_command(editing, cursor, selection, tx.cursor_after, window, cx);
    }

    fn after_command(
        &mut self,
        editing: Option<BlockId>,
        cursor: Option<Range<usize>>,
        selection: Selection,
        cursor_after: Option<bitacora_core::editor::CursorState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(id) = editing {
            self.edit = None;
            self.publish_editing(None, cx);
            let (block, sel) = match cursor_after {
                Some(c) if self.row_of(c.block).is_some() => (c.block, c.selection),
                _ => (id, cursor.unwrap_or(0..0)),
            };
            self.enter(block, Caret::Full(sel), window, cx);
        } else if selection.anchor.is_some() {
            self.sel = selection;
            if self.sel.anchor.is_some_and(|a| self.row_of(a).is_none()) {
                self.sel = Selection::default();
            }
            cx.notify();
        }
    }

    fn page(&self) -> Option<PageKey> {
        self.key.clone()
    }

    // ---- zoom ----------------------------------------------------------------------------

    /// Re-roots the view at `root` (`None` = the whole page). No file changes.
    pub fn zoom_to(&mut self, root: Option<BlockId>, cx: &mut Context<Self>) {
        self.exit_edit(cx);
        self.sel = Selection::default();
        self.zoom = root;
        self.reload_outline();
        self.rebuild_rows(cx);
    }

    fn zoom_target(&self) -> Option<BlockId> {
        self.edit
            .as_ref()
            .map(|e| e.id)
            .or_else(|| self.selected_blocks().first().copied())
    }

    // ---- click callbacks -----------------------------------------------------------------

    /// Click on rendered text of row `r` at block-text offset `offset` (`usize::MAX` = end).
    pub fn click_row(
        &mut self,
        r: usize,
        offset: usize,
        shift: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(id) = self.ids.get(r).copied() else {
            return;
        };
        if shift && let Some(anchor) = self.edit.as_ref().map(|e| e.id).or(self.sel.anchor) {
            self.exit_edit(cx);
            self.sel = Selection {
                anchor: Some(anchor),
                head: Some(id),
            };
            self.focus_handle.focus(window, cx);
            cx.notify();
            return;
        }
        let caret = if offset == usize::MAX {
            Caret::End
        } else {
            let len = self
                .outline
                .as_ref()
                .and_then(|o| o.block(id))
                .map_or(0, |b| b.text.len());
            let _ = len;
            Caret::Full(offset..offset)
        };
        self.enter(id, caret, window, cx);
        self.drag_anchor = Some(id);
    }

    /// The pointer moved over row `r` with the left button held: once it leaves the block the
    /// press started in, the drag selects whole blocks from that one to this one.
    pub fn drag_over(&mut self, r: usize, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(anchor), Some(id)) = (self.drag_anchor, self.ids.get(r).copied()) else {
            return;
        };
        if id == anchor && !self.has_selection() {
            return;
        }
        if self.edit.is_some() {
            self.exit_edit(cx);
            self.is_selecting = false;
        }
        let next = Selection {
            anchor: Some(anchor),
            head: Some(id),
        };
        if self.sel != next {
            self.sel = next;
            self.focus_handle.focus(window, cx);
            cx.notify();
        }
    }

    /// The button was released: the drag (if any) is over.
    pub fn drag_end(&mut self) {
        self.drag_anchor = None;
        self.is_selecting = false;
    }

    /// Fold arrow click.
    pub fn toggle_row(&mut self, r: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.ids.get(r).copied() else {
            return;
        };
        let collapsed = self.rows[r].is_collapsed();
        let editing = self.edit.as_ref().map(|e| e.id);
        let cursor = self.full_selection();
        let sel = self.sel.clone();
        if let Some(tx) = self.run(
            "Collapse",
            Cmd::CollapseBlocks {
                ids: vec![id],
                collapsed: !collapsed,
            },
            window,
            cx,
        ) {
            self.after_command(editing, cursor, sel, tx.cursor_after, window, cx);
        }
    }

    /// Checkbox click.
    pub fn toggle_done(&mut self, r: usize, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self.ids.get(r).copied() {
            self.run("Toggle task", Cmd::ToggleDone { ids: vec![id] }, window, cx);
        }
    }

    /// Bullet click: zoom into the block.
    pub fn bullet_click(&mut self, r: usize, cx: &mut Context<Self>) {
        if let Some(id) = self.ids.get(r).copied() {
            self.zoom_to(Some(id), cx);
        }
    }

    /// The per-row state the page view hands to the row renderer.
    pub fn row_edit(editor: &Entity<Self>, r: usize, cx: &App) -> Option<RowEdit> {
        let this = editor.read(cx);
        let id = *this.ids.get(r)?;
        let editing = this.edit.as_ref().is_some_and(|e| e.id == id);
        let selected = this.has_selection() && this.selected_blocks().contains(&id);
        let ed = editor.clone();
        let on_text = {
            let ed = ed.clone();
            Rc::new(
                move |offset: usize, shift: bool, window: &mut Window, cx: &mut App| {
                    ed.update(cx, |this, cx| this.click_row(r, offset, shift, window, cx));
                },
            )
        };
        let on_drag = {
            let ed = ed.clone();
            Rc::new(move |window: &mut Window, cx: &mut App| {
                ed.update(cx, |this, cx| this.drag_over(r, window, cx));
            })
        };
        let on_drop = {
            let ed = ed.clone();
            Rc::new(
                move |paths: &[std::path::PathBuf], window: &mut Window, cx: &mut App| {
                    ed.update(cx, |this, cx| this.drop_files(r, paths, window, cx));
                },
            )
        };
        let on_delete_asset = this.row_has_asset(r).then(|| {
            let ed = ed.clone();
            Rc::new(move |window: &mut Window, cx: &mut App| {
                ed.update(cx, |this, cx| this.delete_asset_of(id, window, cx));
            }) as super::row::Hook
        });
        let on_checkbox = {
            let ed = ed.clone();
            Rc::new(move |window: &mut Window, cx: &mut App| {
                ed.update(cx, |this, cx| this.toggle_done(r, window, cx));
            })
        };
        let on_bullet = {
            let ed = ed.clone();
            Rc::new(move |_: &mut Window, cx: &mut App| {
                ed.update(cx, |this, cx| this.bullet_click(r, cx));
            })
        };
        let on_toggle = {
            let ed = ed.clone();
            Rc::new(move |window: &mut Window, cx: &mut App| {
                ed.update(cx, |this, cx| this.toggle_row(r, window, cx));
            })
        };
        let popup = this.popup_data();
        let design = cx.bitacora().clone();
        let element = editing.then(|| {
            let ed = ed.clone();
            let popup = popup.clone();
            Rc::new(move |theme: &crate::ui::theme::Theme| {
                super::element::edit_content(ed.clone(), theme, &design, popup.clone())
            }) as Rc<dyn Fn(&crate::ui::theme::Theme) -> _>
        });
        let conflict = if editing {
            this.edit
                .as_ref()
                .and_then(|e| e.conflict.as_ref())
                .map(|_| {
                    let ed = ed.clone();
                    Rc::new(move |_: &crate::ui::theme::Theme| {
                        super::element::conflict_bar(ed.clone())
                    }) as Rc<dyn Fn(&crate::ui::theme::Theme) -> _>
                })
        } else {
            None
        };
        Some(RowEdit {
            selected,
            editing: element,
            conflict,
            on_text,
            on_drag,
            on_drop,
            on_delete_asset,
            on_checkbox,
            on_bullet,
            on_toggle,
            drag: Self::row_drag(editor, r, id, cx),
        })
    }

    // ---- external changes ----------------------------------------------------------------

    /// The block being edited changed on disk (`QueueEvent::EditingBlockChanged`): the editor
    /// offers "keep mine" or "take disk" (BIT-T-0344). Other blocks are not affected.
    pub fn on_editing_conflict(
        &mut self,
        block: BlockId,
        mine: String,
        disk: String,
        cx: &mut Context<Self>,
    ) {
        let Some(e) = self.edit.as_mut() else { return };
        if e.id != block {
            return;
        }
        // Core already holds the disk text: rebase so a later flush replaces it with ours.
        e.full = disk.clone();
        e.conflict = Some((mine, disk));
        self.reload_outline();
        if let Some(r) = self.row_of(block) {
            cx.emit(EditorEvent::Row(r));
        }
        cx.notify();
    }

    /// Resolves the conflict of the edited block: keep the buffer, or replace it by the disk
    /// text.
    pub fn resolve_conflict(
        &mut self,
        keep_mine: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(e) = self.edit.as_mut() else { return };
        let Some((_, disk)) = e.conflict.take() else {
            return;
        };
        let id = e.id;
        if keep_mine {
            self.schedule_flush(cx);
            cx.notify();
        } else {
            self.edit = None;
            self.publish_editing(None, cx);
            self.reload_outline();
            self.enter(id, Caret::Full(disk.len()..disk.len()), window, cx);
        }
        if let Some(r) = self.row_of(id) {
            cx.emit(EditorEvent::Row(r));
        }
    }

    // ---- text input helpers --------------------------------------------------------------

    fn edit_buffer(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut BlockBuffer)) {
        let Some(e) = self.edit.as_mut() else { return };
        f(&mut e.buf);
        e.goal_x = None;
        let id = e.id;
        self.picker = None;
        self.schedule_flush(cx);
        self.restart_blink(cx);
        self.update_completion();
        if let Some(r) = self.row_of(id) {
            cx.emit(EditorEvent::Row(r));
        }
        cx.notify();
    }

    fn motion(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut BlockBuffer)) {
        let Some(e) = self.edit.as_mut() else { return };
        f(&mut e.buf);
        e.goal_x = None;
        self.restart_blink(cx);
        let before = self.completion.is_some();
        self.update_completion();
        if (before || self.completion.is_some())
            && let Some(r) = self.edit.as_ref().and_then(|e| self.row_of(e.id))
        {
            cx.emit(EditorEvent::Row(r));
        }
        cx.notify();
    }

    /// Recomputes the completion popup from the caret position.
    fn update_completion(&mut self) {
        let Some(e) = &self.edit else {
            self.completion = None;
            return;
        };
        if e.buf.marked().is_some() || !e.buf.selection().is_empty() {
            return;
        }
        let Some(trigger) = completion::detect(e.buf.text(), e.buf.cursor()) else {
            self.completion = None;
            self.dismissed = None;
            return;
        };
        if self.dismissed == Some(trigger.start()) {
            self.completion = None;
            return;
        }
        let Some(handle) = &self.handle else { return };
        let page = self
            .outline
            .as_ref()
            .map(|o| o.snapshot().title.clone())
            .unwrap_or_default();
        let items = completion::candidates(handle, &trigger, &page, &e.full);
        let selected = match &self.completion {
            Some(c) if c.trigger.start() == trigger.start() => c.selected,
            _ => 0,
        };
        self.completion = (!items.is_empty()).then(|| CompletionState {
            selected: selected.min(items.len() - 1),
            trigger,
            items,
        });
    }

    fn marked(&self) -> bool {
        self.edit.as_ref().is_some_and(|e| e.buf.marked().is_some())
    }
}

/// User-facing text of a history failure (core's `Display` is English-only).
fn history_error(err: &HistoryError) -> String {
    match err {
        HistoryError::NothingToUndo => rust_i18n::t!("editor.nothing_undo").to_string(),
        HistoryError::NothingToRedo => rust_i18n::t!("editor.nothing_redo").to_string(),
        HistoryError::Truncated(_) => rust_i18n::t!("editor.history_truncated").to_string(),
    }
}

/// The index uuid of the block of page `title` whose text equals `text` (and, with `needle`,
/// mentions it).
fn resolve_index_uuid(
    handle: &GraphHandle,
    title: &str,
    text: &str,
    needle: Option<&str>,
) -> Option<String> {
    let page = handle.reader.page_by_name(title).ok().flatten()?;
    let rows = match needle {
        Some(n) => handle.reader.blocks_mentioning(n, 100).ok()?,
        None => handle.reader.outline(page.id, 0, 5000, false).ok()?,
    };
    rows.into_iter()
        .find(|b| b.page_id == page.id && !b.is_pre_block && b.content.trim() == text.trim())
        .map(|b| b.uuid)
}

#[allow(non_snake_case)]
fn Arc_ptr_eq<T>(a: &std::sync::Arc<T>, b: &std::sync::Arc<T>) -> bool {
    std::sync::Arc::ptr_eq(a, b)
}

fn indent_px(depth: usize) -> Pixels {
    px(INDENT) * depth as f32
}

/// The replaced range and inserted text that turn `old` into `new` (common prefix and suffix
/// removed, on char boundaries).
pub fn text_diff(old: &str, new: &str) -> (Range<usize>, String) {
    let mut start = old
        .bytes()
        .zip(new.bytes())
        .take_while(|(a, b)| a == b)
        .count();
    while !old.is_char_boundary(start) || !new.is_char_boundary(start) {
        start -= 1;
    }
    let max_tail = old.len().min(new.len()) - start;
    let mut tail = old
        .bytes()
        .rev()
        .zip(new.bytes().rev())
        .take(max_tail)
        .take_while(|(a, b)| a == b)
        .count();
    while !old.is_char_boundary(old.len() - tail) || !new.is_char_boundary(new.len() - tail) {
        tail -= 1;
    }
    (
        start..old.len() - tail,
        new[start..new.len() - tail].to_owned(),
    )
}

// ---- actions ---------------------------------------------------------------------------

impl OutlineEditor {
    fn on_left(&mut self, _: &actions::Left, window: &mut Window, cx: &mut Context<Self>) {
        let Some(e) = &self.edit else { return };
        if e.buf.selection().is_empty()
            && e.buf.cursor() == 0
            && let Some(prev) = self.neighbor(e.id, false)
        {
            self.enter(prev, Caret::End, window, cx);
            return;
        }
        self.motion(cx, BlockBuffer::move_left);
    }

    fn on_right(&mut self, _: &actions::Right, window: &mut Window, cx: &mut Context<Self>) {
        let Some(e) = &self.edit else { return };
        if e.buf.selection().is_empty()
            && e.buf.cursor() == e.buf.text().len()
            && let Some(next) = self.neighbor(e.id, true)
        {
            self.enter(next, Caret::Start, window, cx);
            return;
        }
        self.motion(cx, BlockBuffer::move_right);
    }

    fn on_select_left(&mut self, _: &actions::SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.motion(cx, |b| b.select_to(b.prev_grapheme_offset()));
    }

    fn on_select_right(
        &mut self,
        _: &actions::SelectRight,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.motion(cx, |b| b.select_to(b.next_grapheme_offset()));
    }

    fn on_word_left(&mut self, _: &actions::WordLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.motion(cx, |b| b.set_cursor(b.prev_word_offset()));
    }

    fn on_word_right(&mut self, _: &actions::WordRight, _: &mut Window, cx: &mut Context<Self>) {
        self.motion(cx, |b| b.set_cursor(b.next_word_offset()));
    }

    fn on_select_word_left(
        &mut self,
        _: &actions::SelectWordLeft,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.motion(cx, |b| b.select_to(b.prev_word_offset()));
    }

    fn on_select_word_right(
        &mut self,
        _: &actions::SelectWordRight,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.motion(cx, |b| b.select_to(b.next_word_offset()));
    }

    fn on_select_all_text(
        &mut self,
        _: &actions::SelectAllText,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.motion(cx, BlockBuffer::select_all);
    }

    fn row_edge(&self, window: &Window, cx: &App, end: bool) -> Option<usize> {
        let layout = self.shape_edited(window, cx)?;
        let e = self.edit.as_ref()?;
        let row = layout
            .rows()
            .get(layout.row_for_index(e.buf.cursor()))?
            .clone();
        Some(if !end {
            row.start
        } else if row.last_in_line {
            row.end
        } else {
            super::text_ops::clamp_to_boundary(e.buf.text(), row.end - 1)
        })
    }

    fn on_home(&mut self, _: &actions::Home, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(to) = self.row_edge(window, cx, false) {
            self.motion(cx, |b| b.set_cursor(to));
        }
    }

    fn on_end(&mut self, _: &actions::End, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(to) = self.row_edge(window, cx, true) {
            self.motion(cx, |b| b.set_cursor(to));
        }
    }

    fn on_select_home(
        &mut self,
        _: &actions::SelectHome,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(to) = self.row_edge(window, cx, false) {
            self.motion(cx, |b| b.select_to(to));
        }
    }

    fn on_select_end(
        &mut self,
        _: &actions::SelectEnd,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(to) = self.row_edge(window, cx, true) {
            self.motion(cx, |b| b.select_to(to));
        }
    }

    fn on_up(&mut self, _: &actions::Up, window: &mut Window, cx: &mut Context<Self>) {
        self.vertical(false, false, window, cx);
    }

    fn on_down(&mut self, _: &actions::Down, window: &mut Window, cx: &mut Context<Self>) {
        self.vertical(true, false, window, cx);
    }

    fn on_select_up(&mut self, _: &actions::SelectUp, window: &mut Window, cx: &mut Context<Self>) {
        self.vertical(false, true, window, cx);
    }

    fn on_select_down(
        &mut self,
        _: &actions::SelectDown,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.vertical(true, true, window, cx);
    }

    /// Up/Down: move between visual rows keeping the goal x; on the first or last row move to
    /// the neighbouring block.
    fn vertical(&mut self, down: bool, select: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(layout) = self.shape_edited(window, cx) else {
            return;
        };
        let Some(id) = self.editing() else { return };
        let Some(r) = self.ids.iter().position(|i| *i == id) else {
            return;
        };
        let depth = self.rows[r].depth;
        let other_block = self.neighbor(id, down);
        let Some(e) = self.edit.as_mut() else { return };
        let cursor = e.buf.cursor();
        let goal = e
            .goal_x
            .unwrap_or_else(|| layout.position_for_index(cursor).x + indent_px(depth));
        e.goal_x = Some(goal);
        let row = layout.row_for_index(cursor);
        let last = layout.row_count() - 1;
        if (!down && row > 0) || (down && row < last) {
            let target = if down { row + 1 } else { row - 1 };
            let to = layout.index_on_row(target, goal - indent_px(depth));
            if select {
                e.buf.select_to(to);
            } else {
                e.buf.set_cursor(to);
            }
        } else if select {
            let to = if down { e.buf.text().len() } else { 0 };
            e.buf.select_to(to);
        } else if let Some(other) = other_block {
            let caret = if down {
                Caret::FirstRowAtX(goal)
            } else {
                Caret::LastRowAtX(goal)
            };
            self.enter(other, caret, window, cx);
            return;
        } else {
            let to = if down { e.buf.text().len() } else { 0 };
            e.buf.set_cursor(to);
            cx.emit(EditorEvent::Leave { down, goal });
        }
        self.restart_blink(cx);
        cx.notify();
    }

    /// Puts the caret in the first (or last) visible block, near page x `goal`: the entry point
    /// from a neighbouring page.
    pub fn enter_edge(
        &mut self,
        last: bool,
        goal: Pixels,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let pick = if last {
            self.visible.last()
        } else {
            self.visible.first()
        };
        if let Some(id) = pick.and_then(|r| self.ids.get(*r)).copied() {
            let caret = if last {
                Caret::LastRowAtX(goal)
            } else {
                Caret::FirstRowAtX(goal)
            };
            self.enter(id, caret, window, cx);
        }
    }

    fn on_delete_backward(
        &mut self,
        _: &actions::DeleteBackward,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(e) = &self.edit else { return };
        let (id, sel, cursor, marked) = (
            e.id,
            e.buf.selection(),
            e.buf.cursor(),
            e.buf.marked().is_some(),
        );
        if !marked && sel.is_empty() && cursor == 0 {
            let editing = Some(id);
            let selection = Selection::default();
            if let Some(tx) = self.run(
                "Merge with previous",
                Cmd::MergeWithPrevious { id },
                window,
                cx,
            ) {
                self.after_command(editing, None, selection, tx.cursor_after, window, cx);
            }
            return;
        }
        if !marked
            && sel.is_empty()
            && let Some(edit) = autopair::on_backspace(self.buffer_text().unwrap_or(""), cursor)
        {
            self.apply_pair_edit(edit, cx);
            return;
        }
        self.edit_buffer(cx, |b| {
            b.delete_backward();
        });
    }

    fn on_delete_forward(
        &mut self,
        _: &actions::DeleteForward,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(e) = &self.edit else { return };
        let at_end = e.buf.selection().is_empty()
            && e.buf.cursor() == e.buf.text().len()
            && e.buf.marked().is_none();
        let id = e.id;
        if at_end {
            let selection = Selection::default();
            if let Some(tx) = self.run("Merge next", Cmd::MergeNext { id }, window, cx) {
                self.after_command(Some(id), None, selection, tx.cursor_after, window, cx);
            }
            return;
        }
        self.edit_buffer(cx, |b| {
            b.delete_forward();
        });
    }

    fn on_delete_word_backward(
        &mut self,
        _: &actions::DeleteWordBackward,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.edit_buffer(cx, |b| {
            if b.selection().is_empty() {
                let from = b.prev_word_offset();
                b.delete_range(from..b.cursor());
            } else {
                b.delete_range(b.selection());
            }
        });
    }

    fn on_delete_word_forward(
        &mut self,
        _: &actions::DeleteWordForward,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.edit_buffer(cx, |b| {
            if b.selection().is_empty() {
                let to = b.next_word_offset();
                b.delete_range(b.cursor()..to);
            } else {
                b.delete_range(b.selection());
            }
        });
    }

    fn on_new_block(&mut self, _: &actions::NewBlock, window: &mut Window, cx: &mut Context<Self>) {
        if self.marked() {
            // Enter commits an in-progress composition instead of splitting the block.
            self.edit_buffer(cx, BlockBuffer::ime_unmark);
            return;
        }
        let (Some(id), Some(cursor)) = (self.editing(), self.full_selection()) else {
            return;
        };
        self.flush(cx);
        let zoom_root = self.zoom;
        match self.queue.run(
            Source::Ui,
            "Split block",
            Cmd::Enter {
                id,
                cursor,
                zoom_root,
            },
        ) {
            Ok(tx) => {
                self.reload_outline();
                self.rebuild_rows_after_command(cx);
                self.after_command(
                    Some(id),
                    None,
                    Selection::default(),
                    tx.cursor_after,
                    window,
                    cx,
                );
            }
            Err(QueueError::Commit(CommitError::Refused(Refusal::MoveCaret(at)))) => {
                // Enter inside `[[page]]` jumps past the closing brackets.
                if let Some(e) = &self.edit {
                    let to = e.proj.full_to_visible(at);
                    self.motion(cx, |b| b.set_cursor(to));
                }
            }
            Err(QueueError::Commit(CommitError::Refused(r))) => {
                window.play_system_bell();
                self.notice(window, cx, crate::i18n::refusal(&r));
            }
            Err(err) => tracing::warn!("Enter failed: {err}"),
        }
    }

    fn on_insert_newline(
        &mut self,
        _: &actions::InsertNewline,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.edit_buffer(cx, |b| b.insert("\n"));
    }

    fn on_exit_edit(&mut self, _: &actions::ExitEdit, window: &mut Window, cx: &mut Context<Self>) {
        if self.completion.is_some() || self.picker.is_some() {
            self.close_completion(cx);
            return;
        }
        if let Some(id) = self.editing() {
            self.select_only(id, window, cx);
        }
    }

    fn on_indent(&mut self, _: &actions::Indent, window: &mut Window, cx: &mut Context<Self>) {
        if self.marked() {
            return;
        }
        self.structural("Indent", |ids| Cmd::Indent { ids }, window, cx);
    }

    fn on_outdent(&mut self, _: &actions::Outdent, window: &mut Window, cx: &mut Context<Self>) {
        if self.marked() {
            return;
        }
        self.structural("Outdent", |ids| Cmd::Outdent { ids }, window, cx);
    }

    fn on_move_up(
        &mut self,
        _: &actions::MoveBlockUp,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.structural(
            "Move up",
            |ids| Cmd::MoveUpDown { ids, up: true },
            window,
            cx,
        );
    }

    fn on_move_down(
        &mut self,
        _: &actions::MoveBlockDown,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.structural(
            "Move down",
            |ids| Cmd::MoveUpDown { ids, up: false },
            window,
            cx,
        );
    }

    fn collapse_action(&mut self, collapse: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.targets().is_empty() {
            if let Some(page) = self.page() {
                self.run(
                    "Collapse level",
                    Cmd::CollapseLevel { page, collapse },
                    window,
                    cx,
                );
            }
            return;
        }
        self.structural(
            "Collapse",
            move |ids| Cmd::CollapseBlocks {
                ids,
                collapsed: collapse,
            },
            window,
            cx,
        );
    }

    fn on_collapse(
        &mut self,
        _: &actions::CollapseBlock,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.collapse_action(true, window, cx);
    }

    fn on_expand(&mut self, _: &actions::ExpandBlock, window: &mut Window, cx: &mut Context<Self>) {
        self.collapse_action(false, window, cx);
    }

    fn on_toggle_all(
        &mut self,
        _: &actions::ToggleCollapseAll,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(page) = self.page() else { return };
        let any_open = self
            .rows
            .iter()
            .any(|r| r.has_children && !r.is_collapsed());
        let editing = self.editing();
        let cursor = self.full_selection();
        let sel = self.sel.clone();
        if let Some(tx) = self.run(
            "Toggle all",
            Cmd::SetAllCollapsed {
                page,
                collapsed: any_open,
            },
            window,
            cx,
        ) {
            self.after_command(editing, cursor, sel, tx.cursor_after, window, cx);
        }
    }

    fn on_cycle_marker(
        &mut self,
        _: &actions::CycleMarker,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.structural("Cycle task", |ids| Cmd::CycleMarker { ids }, window, cx);
    }

    fn on_zoom_in(&mut self, _: &actions::ZoomIn, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self.zoom_target() {
            self.zoom_to(Some(id), cx);
        }
    }

    fn on_zoom_out(&mut self, _: &actions::ZoomOut, _: &mut Window, cx: &mut Context<Self>) {
        let Some(root) = self.zoom else { return };
        let parent = self
            .outline
            .as_ref()
            .and_then(|o| o.block(root))
            .and_then(|b| b.parent);
        self.zoom_to(parent, cx);
    }

    fn history(&mut self, undo: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.flush(cx);
        let was_editing = self.edit.is_some();
        let selection = self.sel.clone();
        let result = if undo {
            self.queue.undo(Source::Ui)
        } else {
            self.queue.redo(Source::Ui)
        };
        match result {
            Ok(Ok(step)) => {
                self.edit = None;
                self.publish_editing(None, cx);
                self.reload_outline();
                self.rebuild_rows_after_command(cx);
                match step.cursor {
                    Some(c) if self.row_of(c.block).is_some() => {
                        self.enter(c.block, Caret::Full(c.selection), window, cx);
                    }
                    _ if was_editing => {}
                    _ => {
                        self.sel = selection;
                        cx.notify();
                    }
                }
            }
            Ok(Err(HistoryError::NothingToUndo)) => {
                self.notice(
                    window,
                    cx,
                    if undo {
                        rust_i18n::t!("editor.nothing_undo")
                    } else {
                        rust_i18n::t!("editor.nothing_redo")
                    },
                );
            }
            Ok(Err(err)) => self.notice(window, cx, history_error(&err)),
            Err(err) => tracing::warn!("history failed: {err}"),
        }
    }

    fn on_undo(&mut self, _: &actions::Undo, window: &mut Window, cx: &mut Context<Self>) {
        self.history(true, window, cx);
    }

    fn on_redo(&mut self, _: &actions::Redo, window: &mut Window, cx: &mut Context<Self>) {
        self.history(false, window, cx);
    }

    // ---- selection actions ---------------------------------------------------------------

    fn move_selection(&mut self, down: bool, extend: bool, cx: &mut Context<Self>) {
        let Some(head) = self.sel.head else { return };
        let Some(next) = self.neighbor(head, down) else {
            return;
        };
        self.sel.head = Some(next);
        if !extend {
            self.sel.anchor = Some(next);
        }
        if let Some(r) = self.row_of(next) {
            cx.emit(EditorEvent::Entered(r));
        }
        cx.notify();
    }

    fn on_selection_up(
        &mut self,
        _: &actions::SelectionUp,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_selection(false, false, cx);
    }

    fn on_selection_down(
        &mut self,
        _: &actions::SelectionDown,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_selection(true, false, cx);
    }

    fn on_extend_up(
        &mut self,
        _: &actions::ExtendSelectionUp,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_selection(false, true, cx);
    }

    fn on_extend_down(
        &mut self,
        _: &actions::ExtendSelectionDown,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_selection(true, true, cx);
    }

    fn on_select_all_blocks(
        &mut self,
        _: &actions::SelectAllBlocks,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (Some(first), Some(last)) = (self.visible.first(), self.visible.last()) else {
            return;
        };
        let (a, h) = (self.ids[*first], self.ids[*last]);
        self.exit_edit(cx);
        self.sel = Selection {
            anchor: Some(a),
            head: Some(h),
        };
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    fn on_select_parent(
        &mut self,
        _: &actions::SelectParent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(id) = self.sel.anchor.or_else(|| self.editing()) else {
            return;
        };
        let parent = self
            .outline
            .as_ref()
            .and_then(|o| o.block(id))
            .and_then(|b| b.parent)
            .filter(|p| self.row_of(*p).is_some());
        if let Some(p) = parent {
            self.select_only(p, window, cx);
        }
    }

    fn on_edit_selected(
        &mut self,
        _: &actions::EditSelected,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let sel = self.selected_blocks();
        if let [only] = sel[..] {
            self.enter(only, Caret::End, window, cx);
        }
    }

    fn on_clear_selection(
        &mut self,
        _: &actions::ClearSelection,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.clear_selection(cx);
    }

    fn on_delete_selected(
        &mut self,
        _: &actions::DeleteSelected,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ids = self.targets();
        if ids.is_empty() {
            return;
        }
        let next = ids.last().and_then(|l| self.neighbor_after_subtree(*l));
        if self
            .run("Delete blocks", Cmd::DeleteBlocks { ids }, window, cx)
            .is_some()
        {
            self.sel = Selection::default();
            if let Some(n) = next.filter(|n| self.row_of(*n).is_some()) {
                self.sel = Selection {
                    anchor: Some(n),
                    head: Some(n),
                };
            }
            cx.notify();
        }
    }

    fn neighbor_after_subtree(&self, id: BlockId) -> Option<BlockId> {
        let o = self.outline.as_ref()?;
        let ix = o.index_of(id)?;
        o.blocks().get(o.subtree_end(ix)).map(|b| b.id)
    }

    // ---- clipboard -----------------------------------------------------------------------

    fn write_blocks(&self, ids: &[BlockId], cut: bool, cx: &mut Context<Self>) -> bool {
        let Some(o) = &self.outline else { return false };
        let trees = o.trees(ids);
        if trees.is_empty() {
            return false;
        }
        let payload = clipboard::payload_of(&trees, cut);
        cx.write_to_clipboard(ClipboardItem::new_string_with_metadata(
            payload.text,
            payload.private,
        ));
        true
    }

    fn on_copy(&mut self, _: &actions::Copy, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(e) = &self.edit {
            if !e.buf.selection().is_empty() {
                cx.write_to_clipboard(ClipboardItem::new_string(e.buf.selected_text().to_owned()));
            } else {
                // No text selected: copy a `((reference))` to the block (its `id::` is written
                // now, in one undo step with nothing else).
                self.copy_block_ref(false, window, cx);
            }
            return;
        }
        let ids = self.targets();
        self.write_blocks(&ids, false, cx);
    }

    fn on_copy_embed(
        &mut self,
        _: &actions::CopyEmbed,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.edit.is_some() {
            self.copy_block_ref(true, window, cx);
        }
    }

    /// Copies `((uuid))` (or `{{embed ((uuid))}}`) of the edited block, giving it an `id::`.
    fn copy_block_ref(&mut self, embed: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.editing() else { return };
        let cursor = self.full_selection();
        let selection = Selection::default();
        let Some(tx) = self.run(
            "Copy block reference",
            Cmd::EnsureUuid { id, uuid: None },
            window,
            cx,
        ) else {
            // Already has an id::: nothing changed, read it from the snapshot below.
            self.reload_outline();
            self.write_ref(id, embed, cx);
            return;
        };
        self.after_command(Some(id), cursor, selection, tx.cursor_after, window, cx);
        self.write_ref(id, embed, cx);
    }

    fn write_ref(&mut self, id: BlockId, embed: bool, cx: &mut Context<Self>) {
        let uuid = self
            .outline
            .as_ref()
            .and_then(|o| o.block(id))
            .and_then(|b| b.uuid);
        if let Some(uuid) = uuid {
            let text = if embed {
                format!("{{{{embed (({uuid}))}}}}")
            } else {
                format!("(({uuid}))")
            };
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }

    fn on_cut(&mut self, _: &actions::Cut, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(e) = &self.edit {
            if !e.buf.selection().is_empty() {
                cx.write_to_clipboard(ClipboardItem::new_string(e.buf.selected_text().to_owned()));
                self.edit_buffer(cx, |b| b.replace_range(b.selection(), ""));
            }
            return;
        }
        let ids = self.targets();
        if self.write_blocks(&ids, true, cx) {
            self.on_delete_selected(&actions::DeleteSelected, window, cx);
        }
    }

    fn on_paste(&mut self, _: &actions::Paste, window: &mut Window, cx: &mut Context<Self>) {
        self.paste(false, window, cx);
    }

    fn on_paste_raw(&mut self, _: &actions::PasteRaw, window: &mut Window, cx: &mut Context<Self>) {
        self.paste(true, window, cx);
    }

    fn paste(&mut self, raw: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(item) = cx.read_from_clipboard() else {
            return;
        };
        // Copied files, or an image without text, become attachments (BIT-US-0096).
        let incoming = super::assets::from_clipboard(&item);
        let has_files = incoming
            .iter()
            .any(|i| matches!(i, super::assets::Incoming::Path(_)));
        let image_only = !incoming.is_empty() && item.text().is_none_or(|t| t.trim().is_empty());
        if has_files || image_only {
            self.attach_files(None, incoming, window, cx);
            return;
        }
        let Some(text) = item.text() else { return };
        let private = (!raw)
            .then(|| item.metadata().and_then(|m| parse_private(m)))
            .flatten();
        let _ = PRIVATE_MIME;
        let editing = self.editing();
        let target = editing.or_else(|| self.selected_blocks().last().copied());
        let Some(target) = target else { return };
        if let Some((cut, blocks)) = private {
            let sibling = editing.is_none().then_some(true);
            let selection = Selection::default();
            if let Some(tx) = self.run(
                "Paste blocks",
                Cmd::InsertBlocks {
                    target,
                    sibling,
                    blocks,
                    keep_uuids: cut,
                },
                window,
                cx,
            ) {
                self.after_command(editing, None, selection, tx.cursor_after, window, cx);
            }
            return;
        }
        let text = if !raw && html::looks_like_html(&text) {
            html::html_to_markdown(&text)
        } else {
            text
        };
        match editing {
            Some(_) => {
                let Some(cursor) = self.full_selection() else {
                    return;
                };
                // Keep the buffer as the user sees it when the paste is plain inline text.
                if matches!(classify_paste(&text, raw), PasteKind::Inline(_)) {
                    let inline = text.replace("\r\n", "\n").replace('\r', "\n");
                    self.edit_buffer(cx, |b| {
                        b.replace_range(b.marked().unwrap_or_else(|| b.selection()), &inline)
                    });
                    return;
                }
                let selection = Selection::default();
                if let Some(tx) = self.run(
                    "Paste",
                    Cmd::PasteText {
                        target,
                        cursor,
                        text,
                        raw,
                    },
                    window,
                    cx,
                ) {
                    self.after_command(editing, None, selection, tx.cursor_after, window, cx);
                }
            }
            None => {
                if let PasteKind::Blocks(blocks) = classify_paste(&text, raw) {
                    self.run(
                        "Paste blocks",
                        Cmd::InsertBlocks {
                            target,
                            sibling: Some(true),
                            blocks,
                            keep_uuids: true,
                        },
                        window,
                        cx,
                    );
                }
            }
        }
    }

    fn on_character_palette(
        &mut self,
        _: &actions::ShowCharacterPalette,
        window: &mut Window,
        _: &mut Context<Self>,
    ) {
        window.show_character_palette();
    }

    // ---- autocomplete popup ----------------------------------------------------------

    fn on_accept_completion(
        &mut self,
        _: &actions::AcceptCompletion,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.picker.is_some() {
            self.accept_picker(window, cx);
            return;
        }
        let selected = self.completion.as_ref().map_or(0, |c| c.selected);
        self.accept_completion(selected, window, cx);
    }

    /// Inserts candidate `ix` of the popup in place of the trigger text.
    pub fn accept_completion(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(c) = self.completion.take() else {
            return;
        };
        let Some(item) = c.items.get(ix).cloned() else {
            return;
        };
        let Some(e) = &self.edit else { return };
        let cursor = e.buf.cursor();
        let range = c.trigger.replace_range(cursor);
        match item {
            Item::Command(cmd) => self.run_command(cmd, range, window, cx),
            Item::Template { name } => self.insert_template(&name, range, window, cx),
            Item::Page { title, .. } => {
                let text = completion::page_text(&c.trigger, &title);
                let to = range.start + text.len();
                self.dismissed = None;
                self.edit_buffer(cx, |b| {
                    b.replace_range(range, &text);
                    b.set_cursor(to);
                });
                // The caret is past the inserted text: no popup for it.
                self.dismissed = Some(c.trigger.start());
                self.completion = None;
            }
            Item::Block { uuid, .. } => {
                let (Some(id), Some(referenced)) = (self.editing(), self.resolve_block(&uuid))
                else {
                    self.notice(window, cx, rust_i18n::t!("editor.not_linkable"));
                    return;
                };
                let Some(e) = &self.edit else { return };
                let full = e.proj.visible_to_full(e.buf.text(), range.start)
                    ..e.proj.visible_to_full(e.buf.text(), range.end);
                if let Some(tx) = self.run(
                    "Insert block reference",
                    Cmd::InsertBlockRef {
                        target: id,
                        range: full,
                        referenced,
                    },
                    window,
                    cx,
                ) {
                    self.after_command(
                        Some(id),
                        None,
                        Selection::default(),
                        tx.cursor_after,
                        window,
                        cx,
                    );
                }
            }
        }
        cx.notify();
    }

    /// The core id of the block with index uuid `uuid` (its page is loaded on demand).
    fn resolve_block(&self, uuid: &str) -> Option<BlockId> {
        let handle = self.handle.as_ref()?;
        let row = handle.reader.block(uuid).ok().flatten()?;
        let page = handle.reader.page_by_id(row.page_id).ok().flatten()?;
        let key = super::ensure_loaded(&self.queue, handle, &self.config, &page.original_name)?;
        let snap = self.queue.snapshot(&key)?;
        let wanted = uuid.to_ascii_lowercase();
        snap.blocks
            .iter()
            .find(|b| b.uuid.is_some_and(|u| u.to_string() == wanted))
            .or_else(|| {
                snap.blocks
                    .iter()
                    .find(|b| b.text.trim() == row.content.trim())
            })
            .map(|b| b.id)
    }

    fn on_completion_next(
        &mut self,
        _: &actions::CompletionNext,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.picker.is_some() {
            self.move_picker(7, cx);
            return;
        }
        if let Some(c) = &mut self.completion {
            c.selected = (c.selected + 1) % c.items.len();
            cx.notify();
        }
    }

    fn on_completion_previous(
        &mut self,
        _: &actions::CompletionPrevious,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.picker.is_some() {
            self.move_picker(-7, cx);
            return;
        }
        if let Some(c) = &mut self.completion {
            c.selected = (c.selected + c.items.len() - 1) % c.items.len();
            cx.notify();
        }
    }

    fn on_dismiss_completion(
        &mut self,
        _: &actions::DismissCompletion,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_completion(cx);
    }

    fn close_completion(&mut self, cx: &mut Context<Self>) {
        if self.picker.take().is_some() {
            if let Some(r) = self.edit.as_ref().and_then(|e| self.row_of(e.id)) {
                cx.emit(EditorEvent::Row(r));
            }
            cx.notify();
        }
        if let Some(c) = self.completion.take() {
            self.dismissed = Some(c.trigger.start());
            if let Some(r) = self.edit.as_ref().and_then(|e| self.row_of(e.id)) {
                cx.emit(EditorEvent::Row(r));
            }
            cx.notify();
        }
    }

    fn apply_pair_edit(&mut self, edit: autopair::Edit, cx: &mut Context<Self>) {
        self.edit_buffer(cx, |b| {
            if !edit.insert.is_empty() || !edit.range.is_empty() {
                b.replace_range(edit.range.clone(), &edit.insert);
            }
            b.set_selection(edit.selection.clone(), false);
        });
    }

    // ---- attachments (BIT-US-0096) -----------------------------------------------------------

    /// Saves `incoming` files under `assets/` and links them in `target` (default: the edited
    /// block, else the last selected one) at the caret. The files are read on a background
    /// thread; the command is one undoable transaction.
    pub fn attach_files(
        &mut self,
        target: Option<BlockId>,
        incoming: Vec<super::assets::Incoming>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let target = target
            .or_else(|| self.editing())
            .or_else(|| self.selected_blocks().last().copied());
        let Some(target) = target else {
            self.notice(window, cx, rust_i18n::t!("editor.select_to_attach"));
            return;
        };
        if incoming.is_empty() {
            return;
        }
        let task = cx
            .background_executor()
            .spawn(async move { super::assets::load(incoming) });
        cx.spawn_in(window, async move |this, cx| {
            let (files, skipped) = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.finish_attach(target, files, &skipped, window, cx);
            });
        })
        .detach();
    }

    fn finish_attach(
        &mut self,
        target: BlockId,
        files: Vec<super::assets::Loaded>,
        skipped: &[String],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !skipped.is_empty() {
            self.notice(
                window,
                cx,
                rust_i18n::t!("editor.attach_failed", files = skipped.join(", ")),
            );
        }
        if files.is_empty() {
            return;
        }
        self.flush(cx);
        let editing = self.editing();
        if editing.is_some_and(|e| e != target) {
            self.exit_edit(cx);
        }
        let cursor = match (self.editing(), self.full_selection()) {
            (Some(_), Some(c)) => c,
            _ => {
                let Some(text) = self
                    .outline
                    .as_ref()
                    .and_then(|o| o.block(target))
                    .map(|b| b.text.clone())
                else {
                    return;
                };
                let proj = EditProjection::from_text(&text, &self.hidden);
                let end = proj.visible_to_full(proj.visible(), proj.visible().len());
                end..end
            }
        };
        let page_file = self
            .key
            .as_ref()
            .and_then(|k| self.queue.snapshot(k))
            .and_then(|s| s.path.clone());
        let assets = super::assets::plan(files, page_file.as_ref(), super::assets::now_ms());
        let selection = self.sel.clone();
        let was_editing = self.editing() == Some(target);
        let Some(tx) = self.run(
            "Attach files",
            Cmd::ImportAssets {
                target,
                cursor: cursor.clone(),
                assets,
            },
            window,
            cx,
        ) else {
            return;
        };
        if was_editing {
            self.after_command(
                Some(target),
                Some(cursor),
                selection,
                tx.cursor_after,
                window,
                cx,
            );
        } else {
            let at = tx.cursor_after.map_or(0..0, |c| c.selection);
            self.enter(target, Caret::Full(at), window, cx);
        }
    }

    /// Files were dropped on row `r`.
    pub fn drop_files(
        &mut self,
        r: usize,
        paths: &[std::path::PathBuf],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let target = self.ids.get(r).copied();
        self.drop_files_target(target, paths, window, cx);
    }

    /// Files were dropped outside any block: they go to the edited or selected block.
    pub fn drop_files_on_page(
        &mut self,
        paths: &[std::path::PathBuf],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.drop_files_target(None, paths, window, cx);
    }

    fn drop_files_target(
        &mut self,
        target: Option<BlockId>,
        paths: &[std::path::PathBuf],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let incoming = paths
            .iter()
            .cloned()
            .map(super::assets::Incoming::Path)
            .collect();
        self.attach_files(target, incoming, window, cx);
    }

    /// "Delete asset": asks the host to delete the file behind the (first) asset link of the
    /// edited or selected block. The block's text is not changed.
    fn on_delete_asset(
        &mut self,
        _: &actions::DeleteAsset,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(id) = self
            .editing()
            .or_else(|| self.selected_blocks().last().copied())
        else {
            return;
        };
        self.delete_asset_of(id, window, cx);
    }

    /// Requests deletion of the asset linked from block `id`.
    pub fn delete_asset_of(&mut self, id: BlockId, window: &mut Window, cx: &mut Context<Self>) {
        self.flush(cx);
        let Some(text) = self
            .outline
            .as_ref()
            .and_then(|o| o.block(id))
            .map(|b| b.text.clone())
        else {
            return;
        };
        let Some(path) = bitacora_core::recycle::asset_links(&text)
            .into_iter()
            .next()
        else {
            self.notice(window, cx, rust_i18n::t!("editor.no_attachment"));
            return;
        };
        let block = self.index_uuid(id, &text, path.as_str());
        cx.emit(EditorEvent::DeleteAsset {
            link: path.as_str().to_owned(),
            block,
        });
    }

    /// The index uuid of block `id`: its `id::`, or the index row of this page that has the same
    /// text and mentions `needle`.
    fn index_uuid(&self, id: BlockId, text: &str, needle: &str) -> Option<String> {
        let snap = self.key.as_ref().and_then(|k| self.queue.snapshot(k))?;
        if let Some(u) = snap.blocks.iter().find(|b| b.id == id).and_then(|b| b.uuid) {
            return Some(u.to_string());
        }
        resolve_index_uuid(self.handle.as_ref()?, &snap.title, text, Some(needle))
    }

    /// Whether row `r` links to an asset file (it shows the "delete asset" button).
    #[must_use]
    pub fn row_has_asset(&self, r: usize) -> bool {
        self.ids
            .get(r)
            .and_then(|id| self.outline.as_ref()?.block(*id))
            .is_some_and(|b| !bitacora_core::recycle::asset_links(&b.text).is_empty())
    }

    // ---- mouse (edited block) ------------------------------------------------------------

    fn offset_at(&self, position: crate::ui::Point<Pixels>) -> Option<usize> {
        let (layout, bounds) = (self.last_layout.as_ref()?, self.last_bounds?);
        Some(layout.index_for_position(position - bounds.origin))
    }

    /// Mouse down inside the edited block: caret, word or block selection.
    pub fn edit_mouse_down(
        &mut self,
        position: crate::ui::Point<Pixels>,
        click_count: usize,
        shift: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_handle.focus(window, cx);
        let Some(offset) = self.offset_at(position) else {
            return;
        };
        let Some(e) = self.edit.as_mut() else { return };
        match click_count {
            1 if shift => e.buf.select_to(offset),
            1 => e.buf.set_cursor(offset),
            2 => {
                let word = super::text_ops::word_range_at(e.buf.text(), offset);
                e.buf.set_selection(word, false);
            }
            _ => e.buf.select_all(),
        }
        e.goal_x = None;
        self.drag_anchor = Some(e.id);
        self.is_selecting = true;
        self.restart_blink(cx);
        cx.notify();
    }

    /// Mouse move: extends the text selection while the button is held.
    pub fn edit_mouse_move(&mut self, position: crate::ui::Point<Pixels>, cx: &mut Context<Self>) {
        if self.is_selecting
            && let Some(offset) = self.offset_at(position)
            && let Some(e) = self.edit.as_mut()
        {
            e.buf.select_to(offset);
            cx.notify();
        }
    }

    /// Mouse up: ends the text selection.
    pub fn edit_mouse_up(&mut self) {
        self.is_selecting = false;
        self.drag_anchor = None;
    }
}

/// Registers the editor actions on `element`; handlers forward to `editor`.
pub fn attach<E: crate::ui::InteractiveElement>(
    mut element: E,
    editor: &Entity<OutlineEditor>,
) -> E {
    macro_rules! bind {
        ($($action:ty => $method:ident),* $(,)?) => {$(
            {
                let ed = editor.clone();
                element = element.on_action(move |a: &$action, window, cx| {
                    ed.update(cx, |this, cx| this.$method(a, window, cx));
                });
            }
        )*};
    }
    bind!(
        actions::Left => on_left,
        actions::Right => on_right,
        actions::Up => on_up,
        actions::Down => on_down,
        actions::SelectLeft => on_select_left,
        actions::SelectRight => on_select_right,
        actions::SelectUp => on_select_up,
        actions::SelectDown => on_select_down,
        actions::WordLeft => on_word_left,
        actions::WordRight => on_word_right,
        actions::SelectWordLeft => on_select_word_left,
        actions::SelectWordRight => on_select_word_right,
        actions::Home => on_home,
        actions::End => on_end,
        actions::SelectHome => on_select_home,
        actions::SelectEnd => on_select_end,
        actions::SelectAllText => on_select_all_text,
        actions::DeleteBackward => on_delete_backward,
        actions::DeleteForward => on_delete_forward,
        actions::DeleteWordBackward => on_delete_word_backward,
        actions::DeleteWordForward => on_delete_word_forward,
        actions::NewBlock => on_new_block,
        actions::InsertNewline => on_insert_newline,
        actions::Copy => on_copy,
        actions::CopyEmbed => on_copy_embed,
        actions::Cut => on_cut,
        actions::Paste => on_paste,
        actions::PasteRaw => on_paste_raw,
        actions::ExitEdit => on_exit_edit,
        actions::ShowCharacterPalette => on_character_palette,
        actions::Indent => on_indent,
        actions::Outdent => on_outdent,
        actions::MoveBlockUp => on_move_up,
        actions::MoveBlockDown => on_move_down,
        actions::CollapseBlock => on_collapse,
        actions::ExpandBlock => on_expand,
        actions::CycleMarker => on_cycle_marker,
        actions::ZoomIn => on_zoom_in,
        actions::ZoomOut => on_zoom_out,
        actions::Undo => on_undo,
        actions::Redo => on_redo,
        actions::ToggleCollapseAll => on_toggle_all,
        actions::SelectionUp => on_selection_up,
        actions::SelectionDown => on_selection_down,
        actions::ExtendSelectionUp => on_extend_up,
        actions::ExtendSelectionDown => on_extend_down,
        actions::SelectAllBlocks => on_select_all_blocks,
        actions::SelectParent => on_select_parent,
        actions::EditSelected => on_edit_selected,
        actions::ClearSelection => on_clear_selection,
        actions::DeleteSelected => on_delete_selected,
        actions::DeleteAsset => on_delete_asset,
        actions::AcceptCompletion => on_accept_completion,
        actions::CompletionNext => on_completion_next,
        actions::CompletionPrevious => on_completion_previous,
        actions::DismissCompletion => on_dismiss_completion,
        actions::PickerPreviousDay => on_picker_previous,
        actions::PickerNextDay => on_picker_next,
    );
    element
}

// ---- platform input handler (IME) --------------------------------------------------------

impl EntityInputHandler for OutlineEditor {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let e = self.edit.as_ref()?;
        let text = e.buf.text();
        let range = range_from_utf16(text, &range_utf16);
        actual_range.replace(range_to_utf16(text, &range));
        Some(text[range].to_owned())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let e = self.edit.as_ref()?;
        Some(UTF16Selection {
            range: range_to_utf16(e.buf.text(), &e.buf.selection()),
            reversed: e.buf.reversed(),
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        let e = self.edit.as_ref()?;
        e.buf.marked().map(|r| range_to_utf16(e.buf.text(), &r))
    }

    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.edit_buffer(cx, BlockBuffer::ime_unmark);
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // A single typed character may autopair; composed text and explicit ranges never do.
        if range_utf16.is_none()
            && let Some(e) = &self.edit
            && e.buf.marked().is_none()
            && let mut chars = new_text.chars()
            && let (Some(ch), None) = (chars.next(), chars.next())
            && let Some(edit) = autopair::on_char(e.buf.text(), e.buf.selection(), ch)
        {
            self.apply_pair_edit(edit, cx);
            return;
        }
        self.edit_buffer(cx, |b| b.ime_replace(range_utf16, new_text));
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.edit_buffer(cx, |b| {
            b.ime_replace_and_mark(range_utf16, new_text, new_selected_range_utf16);
        });
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        element_bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let layout = self.shape_edited(window, cx)?;
        let e = self.edit.as_ref()?;
        let range = range_from_utf16(e.buf.text(), &range_utf16);
        let local = layout.bounds_for_range(range);
        Some(Bounds::new(
            element_bounds.origin + local.origin,
            local.size,
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: crate::ui::Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        let offset = self.offset_at(point)?;
        let e = self.edit.as_ref()?;
        Some(super::text_ops::offset_to_utf16(e.buf.text(), offset))
    }

    fn set_selected_text_range(
        &mut self,
        range_utf16: Range<usize>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(e) = &self.edit else { return };
        let range = range_from_utf16(e.buf.text(), &range_utf16);
        self.motion(cx, |b| b.set_selection(range, false));
    }

    fn text_length_utf16(&mut self, _: &mut Window, _: &mut Context<Self>) -> Option<usize> {
        let e = self.edit.as_ref()?;
        Some(super::text_ops::offset_to_utf16(
            e.buf.text(),
            e.buf.text().len(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_finds_the_changed_middle() {
        assert_eq!(text_diff("abcdef", "abXef"), (2..4, "X".to_owned()));
        assert_eq!(text_diff("abc", "abcd"), (3..3, "d".to_owned()));
        assert_eq!(text_diff("abc", "ab"), (2..3, String::new()));
        assert_eq!(text_diff("same", "same"), (4..4, String::new()));
        assert_eq!(text_diff("", "x"), (0..0, "x".to_owned()));
    }

    #[test]
    fn diff_respects_char_boundaries() {
        // "é" (C3 A9) vs "è" (C3 A8): the common first byte must not split the char.
        let (range, ins) = text_diff("a\u{e9}b", "a\u{e8}b");
        assert_eq!((range, ins.as_str()), (1..3, "\u{e8}"));
        let (range, ins) = text_diff("\u{6f22}", "\u{6f23}");
        assert_eq!((range, ins.as_str()), (0..3, "\u{6f23}"));
    }
}
