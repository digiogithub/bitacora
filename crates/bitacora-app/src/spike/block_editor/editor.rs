//! `SpikeEditor`: a virtualized page of blocks with exactly one live editor (the focused
//! block). Implements the cross-block keyboard model, the platform input handler (IME)
//! and the unfocused rendering with click-to-caret mapping.

use std::ops::Range;
use std::rc::Rc;
use std::time::{Duration, Instant};

use super::buffer::BlockBuffer;
use super::doc::SpikeDoc;
use super::element::BlockTextElement;
use super::frame_marker::FrameEndMarker;
use super::inline::{InlineCache, InlineRender, Kind, source_runs};
use super::layout::BlockLayout;
use super::text_ops::{range_from_utf16, range_to_utf16};
use crate::ui::text_edit::{
    ClipboardItem, CursorStyle, EntityInputHandler, Font, FontWeight, ListAlignment, ListOffset,
    ListState, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, StrikethroughStyle,
    StyledText, TextLayout, TextRun, UTF16Selection, UnderlineStyle, list,
};
use crate::ui::{
    ActiveTheme as _, AnyElement, App, AppContext as _, Bounds, Context, Entity,
    FluentBuilder as _, FocusHandle, Focusable, Hsla, InteractiveElement as _, IntoElement as _,
    KeyBinding, ParentElement as _, Pixels, Render, Styled as _, Window, actions, div, h_flex, px,
    v_flex,
};

/// Indentation per depth level.
pub const INDENT: f32 = 22.;
/// Width of the fold arrow + bullet gutter.
pub const GUTTER: f32 = 36.;
/// Horizontal padding of a row (left + right).
pub const ROW_PAD: f32 = 16.;
/// Font size of block text.
pub const FONT_SIZE: f32 = 15.;
/// Line height of block text.
pub const LINE_HEIGHT: f32 = 22.;
/// Key context of the page.
pub const KEY_CONTEXT: &str = "SpikeBlockEditor";

actions!(
    spike_editor,
    [
        NewBlock,
        InsertNewline,
        Backspace,
        Delete,
        Left,
        Right,
        Up,
        Down,
        SelectLeft,
        SelectRight,
        SelectUp,
        SelectDown,
        WordLeft,
        WordRight,
        SelectWordLeft,
        SelectWordRight,
        Home,
        End,
        SelectHome,
        SelectEnd,
        SelectAll,
        Indent,
        Outdent,
        Copy,
        Cut,
        Paste,
        Undo,
        Redo,
        Collapse,
        Expand,
        ShowCharacterPalette,
    ]
);

#[cfg(target_os = "macos")]
pub const MOD: &str = "cmd";
#[cfg(not(target_os = "macos"))]
pub const MOD: &str = "ctrl";
#[cfg(target_os = "macos")]
const WORD: &str = "alt";
#[cfg(not(target_os = "macos"))]
const WORD: &str = "ctrl";

/// Installs the editor key bindings.
pub fn bind_keys(cx: &mut App) {
    let ctx = Some(KEY_CONTEXT);
    let mut bindings = vec![
        KeyBinding::new("enter", NewBlock, ctx),
        KeyBinding::new("shift-enter", InsertNewline, ctx),
        KeyBinding::new("backspace", Backspace, ctx),
        KeyBinding::new("delete", Delete, ctx),
        KeyBinding::new("left", Left, ctx),
        KeyBinding::new("right", Right, ctx),
        KeyBinding::new("up", Up, ctx),
        KeyBinding::new("down", Down, ctx),
        KeyBinding::new("shift-left", SelectLeft, ctx),
        KeyBinding::new("shift-right", SelectRight, ctx),
        KeyBinding::new("shift-up", SelectUp, ctx),
        KeyBinding::new("shift-down", SelectDown, ctx),
        KeyBinding::new("home", Home, ctx),
        KeyBinding::new("end", End, ctx),
        KeyBinding::new("shift-home", SelectHome, ctx),
        KeyBinding::new("shift-end", SelectEnd, ctx),
        KeyBinding::new("tab", Indent, ctx),
        KeyBinding::new("shift-tab", Outdent, ctx),
    ];
    for (keys, action) in [
        (
            format!("{WORD}-left"),
            Box::new(WordLeft) as Box<dyn crate::ui::Action>,
        ),
        (format!("{WORD}-right"), Box::new(WordRight)),
        (format!("{WORD}-shift-left"), Box::new(SelectWordLeft)),
        (format!("{WORD}-shift-right"), Box::new(SelectWordRight)),
        (format!("{MOD}-a"), Box::new(SelectAll)),
        (format!("{MOD}-c"), Box::new(Copy)),
        (format!("{MOD}-x"), Box::new(Cut)),
        (format!("{MOD}-v"), Box::new(Paste)),
        (format!("{MOD}-z"), Box::new(Undo)),
        (format!("{MOD}-shift-z"), Box::new(Redo)),
        (format!("{MOD}-up"), Box::new(Collapse)),
        (format!("{MOD}-down"), Box::new(Expand)),
    ] {
        match crate::ui::key_binding(&keys, action, Some(KEY_CONTEXT)) {
            Ok(binding) => bindings.push(binding),
            Err(err) => tracing::warn!("spike key binding `{keys}` rejected: {err}"),
        }
    }
    #[cfg(target_os = "macos")]
    bindings.push(KeyBinding::new("ctrl-cmd-space", ShowCharacterPalette, ctx));
    cx.bind_keys(bindings);
}

/// Where the caret goes when a block takes focus.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Caret {
    /// Offset 0.
    Start,
    /// End of the text.
    End,
    /// A byte offset.
    Offset(usize),
    /// First visual row, closest to this page-relative x (vertical navigation).
    FirstRowAtX(Pixels),
    /// Last visual row, closest to this page-relative x (vertical navigation).
    LastRowAtX(Pixels),
}

/// Font and size used to shape block text; captured from the element's text style.
#[derive(Debug, Clone)]
pub struct TextMetrics {
    /// Base font.
    pub font: Font,
    /// Font size.
    pub font_size: Pixels,
    /// Row height.
    pub line_height: Pixels,
}

impl TextMetrics {
    /// Reads the inherited text style of the element being laid out.
    pub fn from_window(window: &Window) -> Self {
        let style = window.text_style();
        Self {
            font: style.font(),
            font_size: style.font_size.to_pixels(window.rem_size()),
            line_height: window.line_height(),
        }
    }
}

/// Theme colors used by the editor.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    /// Text.
    pub fg: Hsla,
    /// Dimmed markup.
    pub dim: Hsla,
    /// Page references.
    pub link: Hsla,
    /// Open task keyword.
    pub todo: Hsla,
    /// Finished task keyword.
    pub done: Hsla,
    /// Selection background.
    pub selection: Hsla,
    /// Caret.
    pub caret: Hsla,
    /// Lines (guides, bullets).
    pub border: Hsla,
}

impl Palette {
    /// Reads the active theme.
    pub fn from_theme(cx: &App) -> Self {
        let t = cx.theme();
        Self {
            fg: t.foreground,
            dim: t.muted_foreground,
            link: t.link,
            todo: t.primary,
            done: t.success,
            selection: t.selection,
            caret: t.caret,
            border: t.border,
        }
    }
}

/// Converts kind runs into GPUI text runs.
pub fn style_runs(
    kinds: impl IntoIterator<Item = (usize, Kind, bool)>,
    font: &Font,
    p: &Palette,
) -> Vec<TextRun> {
    kinds
        .into_iter()
        .filter(|(len, _, _)| *len > 0)
        .map(|(len, kind, underline)| {
            let mut font = font.clone();
            let color = match kind {
                Kind::Plain | Kind::Code => p.fg,
                Kind::Bracket | Kind::Marker => p.dim,
                Kind::Ref => p.link,
                Kind::Bold => {
                    font.weight = FontWeight::BOLD;
                    p.fg
                }
                Kind::Todo => {
                    font.weight = FontWeight::BOLD;
                    p.todo
                }
                Kind::Done => {
                    font.weight = FontWeight::BOLD;
                    p.done
                }
            };
            TextRun {
                len,
                font,
                color,
                background_color: (kind == Kind::Code).then(|| p.dim.opacity(0.15)),
                underline: underline.then_some(UnderlineStyle {
                    color: Some(p.fg),
                    thickness: px(1.),
                    wavy: false,
                }),
                strikethrough: (kind == Kind::Done).then_some(StrikethroughStyle {
                    thickness: px(1.),
                    color: Some(p.dim),
                }),
            }
        })
        .collect()
}

fn indent_px(depth: u8) -> Pixels {
    px(INDENT) * f32::from(depth)
}

/// The spike page view.
#[derive(Debug)]
pub struct SpikeEditor {
    doc: SpikeDoc,
    rows: Vec<usize>,
    list_state: ListState,
    focus_handle: FocusHandle,
    focused: Option<usize>,
    buf: BlockBuffer,
    goal_x: Option<Pixels>,
    last_layout: Option<Rc<BlockLayout>>,
    last_bounds: Option<Bounds<Pixels>>,
    metrics: Option<TextMetrics>,
    content_width0: Pixels,
    is_selecting: bool,
    caret_visible: bool,
    blink_epoch: usize,
    blink_enabled: bool,
    inline_cache: Rc<InlineCache>,
    use_cache: bool,
    rows_rendered: u64,
    row_build: Duration,
    last_paint_at: Option<Instant>,
    frame_start: Option<Instant>,
    frame_cpu: Option<Duration>,
}

impl SpikeEditor {
    /// A page over `doc`. `blink` enables the caret blink timer.
    pub fn new(doc: SpikeDoc, blink: bool, cx: &mut Context<Self>) -> Self {
        let rows = doc.visible_rows();
        let list_state = ListState::new(rows.len(), ListAlignment::Top, px(400.));
        Self {
            doc,
            rows,
            list_state,
            focus_handle: cx.focus_handle(),
            focused: None,
            buf: BlockBuffer::new(""),
            goal_x: None,
            last_layout: None,
            last_bounds: None,
            metrics: None,
            content_width0: px(640.),
            is_selecting: false,
            caret_visible: true,
            blink_epoch: 0,
            blink_enabled: blink,
            inline_cache: Rc::new(InlineCache::new(4096)),
            use_cache: true,
            rows_rendered: 0,
            row_build: Duration::ZERO,
            last_paint_at: None,
            frame_start: None,
            frame_cpu: None,
        }
    }

    // ----- Accessors (also used by tests and the benchmark) -----

    /// The page content.
    pub fn doc(&self) -> &SpikeDoc {
        &self.doc
    }

    /// The list state (scrolling, measurements).
    pub fn list_state(&self) -> &ListState {
        &self.list_state
    }

    /// Visible rows as block indices.
    pub fn rows(&self) -> &[usize] {
        &self.rows
    }

    /// The focused block.
    pub fn focused(&self) -> Option<usize> {
        self.focused
    }

    /// Text of the focused block (empty when none).
    pub fn focused_text(&self) -> &str {
        if self.focused.is_some() {
            self.buf.text()
        } else {
            ""
        }
    }

    /// IME composition range (UTF-8).
    pub fn marked_range(&self) -> Option<Range<usize>> {
        self.buf.marked()
    }

    /// Selection (UTF-8).
    pub fn selection_range(&self) -> Range<usize> {
        self.buf.selection()
    }

    /// Caret (UTF-8).
    pub fn cursor_offset(&self) -> usize {
        self.buf.cursor()
    }

    /// Whether the caret is in the visible half of its blink cycle.
    pub fn caret_visible(&self) -> bool {
        self.caret_visible
    }

    /// Cache statistics `(hits, misses)`.
    pub fn cache_stats(&self) -> (u64, u64) {
        self.inline_cache.stats()
    }

    /// Enables/disables the inline render cache (benchmark A/B).
    pub fn set_cache_enabled(&mut self, enabled: bool) {
        self.use_cache = enabled;
    }

    /// Rows built since creation (benchmark).
    pub fn rows_rendered(&self) -> u64 {
        self.rows_rendered
    }

    /// Time spent building row elements since creation (benchmark).
    pub fn row_build_time(&self) -> Duration {
        self.row_build
    }

    /// When the focused block's element last finished painting (benchmark).
    pub fn last_paint_at(&self) -> Option<Instant> {
        self.last_paint_at
    }

    /// CPU time of the last frame: view render to the end of paint (benchmark).
    pub fn frame_cpu(&self) -> Option<Duration> {
        self.frame_cpu
    }

    /// Called by the frame-end marker when paint reached the end of the tree.
    pub fn mark_frame_end(&mut self) {
        if let Some(start) = self.frame_start.take() {
            self.frame_cpu = Some(start.elapsed());
        }
    }

    /// The page focus handle.
    pub fn focus_handle(&self) -> &FocusHandle {
        &self.focus_handle
    }

    /// Jumps the list so `row` is the first visible row. Unlike `scroll_to_reveal_item`
    /// this also works for far-away rows that were never measured (their estimated
    /// offsets are unreliable; found by the 2,681-block run of BIT-US-0060).
    pub fn scroll_to_row(&self, row: usize) {
        self.list_state.scroll_to(ListOffset {
            item_ix: row,
            offset_in_item: px(0.),
        });
    }

    /// Remembers what the element painted, for hit testing and IME bounds.
    pub fn record_paint(
        &mut self,
        layout: Rc<BlockLayout>,
        bounds: Bounds<Pixels>,
        metrics: TextMetrics,
    ) {
        if let Some(ix) = self.focused {
            self.content_width0 = bounds.size.width + indent_px(self.doc.blocks[ix].depth);
        }
        self.last_layout = Some(layout);
        self.last_bounds = Some(bounds);
        self.metrics = Some(metrics);
        self.last_paint_at = Some(Instant::now());
    }

    fn row_of(&self, block: usize) -> Option<usize> {
        self.rows.binary_search(&block).ok()
    }

    fn neighbor(&self, block: usize, down: bool) -> Option<usize> {
        let row = self.row_of(block)?;
        let target = if down { row + 1 } else { row.checked_sub(1)? };
        self.rows.get(target).copied()
    }

    fn content_width_at(&self, depth: u8) -> Pixels {
        (self.content_width0 - indent_px(depth)).max(px(40.))
    }

    // ----- Shaping -----

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

    fn shape_block(
        &self,
        text: &str,
        marked: Option<Range<usize>>,
        depth: u8,
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
            self.content_width_at(depth),
            &metrics,
            &Palette::from_theme(cx),
            window,
        )
    }

    pub fn shape_focused(&self, window: &Window, cx: &App) -> Option<Rc<BlockLayout>> {
        let ix = self.focused?;
        Some(self.shape_block(
            self.buf.text(),
            self.buf.marked(),
            self.doc.blocks[ix].depth,
            window,
            cx,
        ))
    }

    // ----- Row bookkeeping -----

    fn invalidate_row(&self, block: usize) {
        if let Some(row) = self.row_of(block) {
            self.list_state.splice(row..row + 1, 1);
        }
    }

    /// Rebuilds `rows` after an edit that shifted block indices from `first_block` on:
    /// everything below the first changed row is re-measured lazily.
    fn rows_changed_from(&mut self, first_block: usize) {
        let old_len = self.rows.len();
        let first = self.row_of(first_block).unwrap_or(0);
        let new = self.doc.visible_rows();
        self.list_state
            .splice(first..old_len, new.len().saturating_sub(first));
        self.rows = new;
    }

    /// Rebuilds `rows` when block indices did not shift (fold, indent): only the changed
    /// range is spliced.
    fn rows_changed_stable(&mut self) {
        let new = self.doc.visible_rows();
        let old = &self.rows;
        let prefix = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
        let max_suffix = old.len().min(new.len()) - prefix;
        let suffix = old
            .iter()
            .rev()
            .zip(new.iter().rev())
            .take(max_suffix)
            .take_while(|(a, b)| a == b)
            .count();
        let (old_end, new_len) = (old.len() - suffix, new.len() - suffix - prefix);
        if prefix != old_end || new_len != 0 {
            self.list_state.splice(prefix..old_end, new_len);
        }
        self.rows = new;
    }

    /// Re-measures the rows of `block`'s subtree (their width or content changed).
    fn invalidate_subtree(&self, block: usize) {
        let end = self.doc.subtree_end(block);
        for row_block in block..end {
            self.invalidate_row(row_block);
        }
    }

    fn sync_text(&mut self, cx: &mut Context<Self>) {
        if let Some(ix) = self.focused {
            if self.doc.blocks[ix].text != self.buf.text() {
                self.doc.blocks[ix].text = self.buf.text().to_owned();
            }
            self.invalidate_row(ix);
        }
        self.restart_blink(cx);
        cx.notify();
    }

    fn restart_blink(&mut self, cx: &mut Context<Self>) {
        self.caret_visible = true;
        self.blink_epoch += 1;
        if !self.blink_enabled {
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

    // ----- Focus -----

    /// Focuses `block` and places the caret.
    pub fn focus_block(
        &mut self,
        block: usize,
        caret: Caret,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if block >= self.doc.len() {
            return;
        }
        if let Some(old) = self.focused {
            self.invalidate_row(old);
        }
        let text = self.doc.blocks[block].text.clone();
        let depth = self.doc.blocks[block].depth;
        let on_row = |this: &Self, row: usize, x: Pixels| {
            let layout = this.shape_block(&text, None, depth, window, cx);
            let row = if row == usize::MAX {
                layout.row_count() - 1
            } else {
                row
            };
            layout.index_on_row(row, x - indent_px(depth))
        };
        let (offset, goal) = match caret {
            Caret::Start => (0, None),
            Caret::End => (text.len(), None),
            Caret::Offset(o) => (o.min(text.len()), None),
            Caret::FirstRowAtX(x) => (on_row(self, 0, x), Some(x)),
            Caret::LastRowAtX(x) => (on_row(self, usize::MAX, x), Some(x)),
        };
        self.buf = BlockBuffer::with_cursor(text, offset);
        self.focused = Some(block);
        self.goal_x = goal;
        self.last_layout = None;
        self.invalidate_row(block);
        if let Some(row) = self.row_of(block) {
            self.list_state.scroll_to_reveal_item(row);
        }
        self.focus_handle.focus(window, cx);
        self.restart_blink(cx);
        cx.notify();
    }

    // ----- Edit plumbing -----

    fn edit(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut BlockBuffer)) {
        if self.focused.is_none() {
            return;
        }
        f(&mut self.buf);
        self.goal_x = None;
        self.sync_text(cx);
    }

    fn motion(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut BlockBuffer)) {
        if self.focused.is_none() {
            return;
        }
        f(&mut self.buf);
        self.goal_x = None;
        self.restart_blink(cx);
        cx.notify();
    }

    // ----- Actions -----

    fn on_backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ix) = self.focused else { return };
        if self.buf.selection().is_empty() && self.buf.cursor() == 0 {
            if let Some((prev, join)) = self.doc.merge_with_previous(ix) {
                self.rows_changed_from(prev);
                self.focus_block(prev, Caret::Offset(join), window, cx);
            } else {
                window.play_system_bell();
            }
            return;
        }
        self.edit(cx, |b| {
            b.delete_backward();
        });
    }

    fn on_delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ix) = self.focused else { return };
        let at_end = self.buf.selection().is_empty() && self.buf.cursor() == self.buf.text().len();
        if at_end {
            if let Some(next) = self.neighbor(ix, true) {
                let join = self.buf.text().len();
                self.doc.blocks[ix].text = self.buf.text().to_owned();
                if let Some((prev, _)) = self.doc.merge_with_previous(next) {
                    self.rows_changed_from(prev);
                    self.focus_block(prev, Caret::Offset(join), window, cx);
                }
            } else {
                window.play_system_bell();
            }
            return;
        }
        self.edit(cx, |b| {
            b.delete_forward();
        });
    }

    fn on_new_block(&mut self, _: &NewBlock, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ix) = self.focused else { return };
        if self.buf.marked().is_some() {
            // Enter commits an in-progress composition instead of splitting.
            self.buf.ime_unmark();
            cx.notify();
            return;
        }
        if self.buf.text().is_empty() && self.doc.blocks[ix].depth > 0 {
            self.do_outdent(ix, cx);
            return;
        }
        if !self.buf.selection().is_empty() {
            self.buf.replace_range(self.buf.selection(), "");
        }
        let at = self.buf.cursor();
        self.doc.blocks[ix].text = self.buf.text().to_owned();
        let new = self.doc.split(ix, at);
        self.rows_changed_from(ix);
        self.focus_block(new, Caret::Start, window, cx);
    }

    fn on_insert_newline(&mut self, _: &InsertNewline, _: &mut Window, cx: &mut Context<Self>) {
        self.edit(cx, |b| b.insert("\n"));
    }

    fn do_outdent(&mut self, ix: usize, cx: &mut Context<Self>) {
        if self.doc.outdent(ix) {
            self.rows_changed_stable();
            self.invalidate_subtree(ix);
            self.restart_blink(cx);
            cx.notify();
        }
    }

    fn on_indent(&mut self, _: &Indent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ix) = self.focused else { return };
        if self.buf.marked().is_some() {
            return;
        }
        if self.doc.indent(ix) {
            self.rows_changed_stable();
            self.invalidate_subtree(ix);
            self.restart_blink(cx);
            cx.notify();
        } else {
            window.play_system_bell();
        }
    }

    fn on_outdent(&mut self, _: &Outdent, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(ix) = self.focused {
            self.do_outdent(ix, cx);
        }
    }

    fn on_collapse(&mut self, _: &Collapse, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(ix) = self.focused {
            self.set_collapsed(ix, true, cx);
        }
    }

    fn on_expand(&mut self, _: &Expand, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(ix) = self.focused {
            self.set_collapsed(ix, false, cx);
        }
    }

    /// Collapses or expands `block`, splicing only the affected rows.
    pub fn set_collapsed(&mut self, block: usize, collapsed: bool, cx: &mut Context<Self>) {
        if self.doc.set_collapsed(block, collapsed) {
            self.rows_changed_stable();
            cx.notify();
        }
    }

    fn on_left(&mut self, _: &Left, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ix) = self.focused else { return };
        if self.buf.selection().is_empty()
            && self.buf.cursor() == 0
            && let Some(prev) = self.neighbor(ix, false)
        {
            self.focus_block(prev, Caret::End, window, cx);
            return;
        }
        self.motion(cx, BlockBuffer::move_left);
    }

    fn on_right(&mut self, _: &Right, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ix) = self.focused else { return };
        if self.buf.selection().is_empty()
            && self.buf.cursor() == self.buf.text().len()
            && let Some(next) = self.neighbor(ix, true)
        {
            self.focus_block(next, Caret::Start, window, cx);
            return;
        }
        self.motion(cx, BlockBuffer::move_right);
    }

    fn on_select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.motion(cx, |b| b.select_to(b.prev_grapheme_offset()));
    }

    fn on_select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.motion(cx, |b| b.select_to(b.next_grapheme_offset()));
    }

    fn on_word_left(&mut self, _: &WordLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.motion(cx, |b| b.set_cursor(b.prev_word_offset()));
    }

    fn on_word_right(&mut self, _: &WordRight, _: &mut Window, cx: &mut Context<Self>) {
        self.motion(cx, |b| b.set_cursor(b.next_word_offset()));
    }

    fn on_select_word_left(&mut self, _: &SelectWordLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.motion(cx, |b| b.select_to(b.prev_word_offset()));
    }

    fn on_select_word_right(
        &mut self,
        _: &SelectWordRight,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.motion(cx, |b| b.select_to(b.next_word_offset()));
    }

    fn on_select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.motion(cx, BlockBuffer::select_all);
    }

    fn row_edge(&self, window: &Window, cx: &App, end: bool) -> Option<usize> {
        let layout = self.shape_focused(window, cx)?;
        let cursor = self.buf.cursor();
        let row = &layout.rows().get(layout.row_for_index(cursor))?.clone();
        Some(if !end {
            row.start
        } else if row.last_in_line {
            row.end
        } else {
            super::text_ops::clamp_to_boundary(self.buf.text(), row.end - 1)
        })
    }

    fn on_home(&mut self, _: &Home, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(to) = self.row_edge(window, cx, false) {
            self.motion(cx, |b| b.set_cursor(to));
        }
    }

    fn on_end(&mut self, _: &End, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(to) = self.row_edge(window, cx, true) {
            self.motion(cx, |b| b.set_cursor(to));
        }
    }

    fn on_select_home(&mut self, _: &SelectHome, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(to) = self.row_edge(window, cx, false) {
            self.motion(cx, |b| b.select_to(to));
        }
    }

    fn on_select_end(&mut self, _: &SelectEnd, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(to) = self.row_edge(window, cx, true) {
            self.motion(cx, |b| b.select_to(to));
        }
    }

    fn on_up(&mut self, _: &Up, window: &mut Window, cx: &mut Context<Self>) {
        self.vertical(false, false, window, cx);
    }

    fn on_down(&mut self, _: &Down, window: &mut Window, cx: &mut Context<Self>) {
        self.vertical(true, false, window, cx);
    }

    fn on_select_up(&mut self, _: &SelectUp, window: &mut Window, cx: &mut Context<Self>) {
        self.vertical(false, true, window, cx);
    }

    fn on_select_down(&mut self, _: &SelectDown, window: &mut Window, cx: &mut Context<Self>) {
        self.vertical(true, true, window, cx);
    }

    /// Up/Down: move between visual rows keeping `goal_x`; on the first/last row move to
    /// the adjacent block.
    fn vertical(&mut self, down: bool, select: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ix) = self.focused else { return };
        let Some(layout) = self.shape_focused(window, cx) else {
            return;
        };
        let depth = self.doc.blocks[ix].depth;
        let cursor = self.buf.cursor();
        let goal = match self.goal_x {
            Some(goal) => goal,
            None => layout.position_for_index(cursor).x + indent_px(depth),
        };
        self.goal_x = Some(goal);
        let row = layout.row_for_index(cursor);
        let last = layout.row_count() - 1;
        if (!down && row > 0) || (down && row < last) {
            let target = if down { row + 1 } else { row - 1 };
            let to = layout.index_on_row(target, goal - indent_px(depth));
            if select {
                self.buf.select_to(to);
            } else {
                self.buf.set_cursor(to);
            }
        } else if select {
            let to = if down { self.buf.text().len() } else { 0 };
            self.buf.select_to(to);
        } else if let Some(other) = self.neighbor(ix, down) {
            let caret = if down {
                Caret::FirstRowAtX(goal)
            } else {
                Caret::LastRowAtX(goal)
            };
            self.focus_block(other, caret, window, cx);
            return;
        } else {
            let to = if down { self.buf.text().len() } else { 0 };
            self.buf.set_cursor(to);
        }
        self.restart_blink(cx);
        cx.notify();
    }

    fn on_copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if self.focused.is_some() && !self.buf.selection().is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.buf.selected_text().to_owned(),
            ));
        }
    }

    fn on_cut(&mut self, _: &Cut, _: &mut Window, cx: &mut Context<Self>) {
        if self.focused.is_some() && !self.buf.selection().is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.buf.selected_text().to_owned(),
            ));
            self.edit(cx, |b| b.replace_range(b.selection(), ""));
        }
    }

    fn on_paste(&mut self, _: &Paste, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            // The spike keeps pasted newlines inside the block; the real editor turns
            // blank-line separated paragraphs into blocks (block-editor.md 7.5).
            self.edit(cx, |b| {
                b.replace_range(b.marked().unwrap_or_else(|| b.selection()), &text)
            });
        }
    }

    fn on_undo(&mut self, _: &Undo, _: &mut Window, cx: &mut Context<Self>) {
        self.edit(cx, |b| {
            b.undo();
        });
    }

    fn on_redo(&mut self, _: &Redo, _: &mut Window, cx: &mut Context<Self>) {
        self.edit(cx, |b| {
            b.redo();
        });
    }

    fn on_character_palette(
        &mut self,
        _: &ShowCharacterPalette,
        window: &mut Window,
        _: &mut Context<Self>,
    ) {
        window.show_character_palette();
    }

    // ----- Mouse -----

    fn offset_at(&self, position: crate::ui::Point<Pixels>) -> Option<usize> {
        let (layout, bounds) = (self.last_layout.as_ref()?, self.last_bounds?);
        Some(layout.index_for_position(position - bounds.origin))
    }

    fn on_focused_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_handle.focus(window, cx);
        let Some(offset) = self.offset_at(event.position) else {
            return;
        };
        match event.click_count {
            1 if event.modifiers.shift => self.buf.select_to(offset),
            1 => self.buf.set_cursor(offset),
            2 => {
                let word = super::text_ops::word_range_at(self.buf.text(), offset);
                self.buf.set_selection(word, false);
            }
            _ => self.buf.select_all(),
        }
        self.is_selecting = true;
        self.goal_x = None;
        self.restart_blink(cx);
        cx.notify();
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.is_selecting
            && let Some(offset) = self.offset_at(event.position)
        {
            self.buf.select_to(offset);
            cx.notify();
        }
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.is_selecting = false;
    }

    /// Click on an unfocused block: focus it with the caret at the source offset that
    /// matches the clicked display position.
    pub fn click_unfocused(
        &mut self,
        block: usize,
        display_offset: usize,
        render: &InlineRender,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let source = render.display_to_source(display_offset);
        self.focus_block(block, Caret::Offset(source), window, cx);
    }

    // ----- Rendering -----

    fn render_row(
        &mut self,
        row: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let started = Instant::now();
        let element = self.build_row(row, window, cx);
        self.rows_rendered += 1;
        self.row_build += started.elapsed();
        element
    }

    fn build_row(&mut self, row: usize, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let Some(&block_ix) = self.rows.get(row) else {
            return div().into_any_element();
        };
        let palette = Palette::from_theme(cx);
        let (depth, collapsed, has_children) = {
            let b = &self.doc.blocks[block_ix];
            (b.depth, b.collapsed, self.doc.has_children(block_ix))
        };
        let focused = self.focused == Some(block_ix);
        let content: AnyElement = if focused {
            div()
                .w_full()
                .cursor(CursorStyle::IBeam)
                .on_mouse_down(MouseButton::Left, cx.listener(Self::on_focused_mouse_down))
                .child(BlockTextElement::new(cx.entity()))
                .into_any_element()
        } else {
            let text = self.doc.blocks[block_ix].text.clone();
            let render = if self.use_cache {
                self.inline_cache.get(&text)
            } else {
                Rc::new(InlineRender::new(&text))
            };
            let metrics = self
                .metrics
                .clone()
                .unwrap_or_else(|| TextMetrics::from_window(window));
            let runs = style_runs(
                render
                    .segments
                    .iter()
                    .map(|s| (s.display.len(), s.kind, false)),
                &metrics.font,
                &palette,
            );
            let styled = StyledText::new(render.display.clone()).with_runs(runs);
            let text_layout: TextLayout = styled.layout().clone();
            div()
                .w_full()
                .min_h(px(LINE_HEIGHT))
                .cursor(CursorStyle::IBeam)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                        let display = text_layout
                            .index_for_position(event.position)
                            .unwrap_or_else(|i| i);
                        this.click_unfocused(block_ix, display, &render, window, cx);
                        this.is_selecting = true;
                    }),
                )
                .child(styled)
                .into_any_element()
        };

        let guides = (0..depth).map(|_| {
            div()
                .w(px(INDENT))
                .flex()
                .flex_none()
                .justify_center()
                .child(div().w(px(1.)).h_full().bg(palette.border.opacity(0.6)))
        });
        let arrow = div()
            .w(px(14.))
            .flex_none()
            .pt(px(2.))
            .text_size(px(11.))
            .text_color(palette.dim)
            .when(has_children, |d| {
                d.cursor(CursorStyle::PointingHand)
                    .child(if collapsed { "\u{25b8}" } else { "\u{25be}" })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                            this.set_collapsed(block_ix, !collapsed, cx);
                        }),
                    )
            });
        let bullet = div()
            .w(px(22.))
            .flex_none()
            .flex()
            .justify_center()
            .pt(px(8.))
            .child(
                div()
                    .size(if collapsed { px(8.) } else { px(6.) })
                    .rounded_full()
                    .bg(palette.dim)
                    .when(collapsed, |d| d.border_2().border_color(palette.border)),
            );

        h_flex()
            .items_stretch()
            .w_full()
            .px(px(ROW_PAD / 2.))
            .text_size(px(FONT_SIZE))
            .line_height(px(LINE_HEIGHT))
            .children(guides)
            .child(arrow)
            .child(bullet)
            .child(div().flex_1().min_w_0().py(px(2.)).child(content))
            .into_any_element()
    }
}

impl Focusable for SpikeEditor {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for SpikeEditor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl crate::ui::IntoElement {
        self.frame_start = Some(Instant::now());
        let (hits, misses) = self.cache_stats();
        let header = format!(
            "Block editor spike - {} blocks, {} visible rows, focused: {}, inline cache {}/{}",
            self.doc.len(),
            self.rows.len(),
            self.focused
                .map_or_else(|| "-".to_owned(), |f| f.to_string()),
            hits,
            hits + misses,
        );
        v_flex()
            .size_full()
            .key_context(KEY_CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::on_new_block))
            .on_action(cx.listener(Self::on_insert_newline))
            .on_action(cx.listener(Self::on_backspace))
            .on_action(cx.listener(Self::on_delete))
            .on_action(cx.listener(Self::on_left))
            .on_action(cx.listener(Self::on_right))
            .on_action(cx.listener(Self::on_up))
            .on_action(cx.listener(Self::on_down))
            .on_action(cx.listener(Self::on_select_left))
            .on_action(cx.listener(Self::on_select_right))
            .on_action(cx.listener(Self::on_select_up))
            .on_action(cx.listener(Self::on_select_down))
            .on_action(cx.listener(Self::on_word_left))
            .on_action(cx.listener(Self::on_word_right))
            .on_action(cx.listener(Self::on_select_word_left))
            .on_action(cx.listener(Self::on_select_word_right))
            .on_action(cx.listener(Self::on_home))
            .on_action(cx.listener(Self::on_end))
            .on_action(cx.listener(Self::on_select_home))
            .on_action(cx.listener(Self::on_select_end))
            .on_action(cx.listener(Self::on_select_all))
            .on_action(cx.listener(Self::on_indent))
            .on_action(cx.listener(Self::on_outdent))
            .on_action(cx.listener(Self::on_copy))
            .on_action(cx.listener(Self::on_cut))
            .on_action(cx.listener(Self::on_paste))
            .on_action(cx.listener(Self::on_undo))
            .on_action(cx.listener(Self::on_redo))
            .on_action(cx.listener(Self::on_collapse))
            .on_action(cx.listener(Self::on_expand))
            .on_action(cx.listener(Self::on_character_palette))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .child(
                div()
                    .px(px(12.))
                    .py(px(6.))
                    .text_size(px(12.))
                    .child(header),
            )
            .child(
                div().flex_1().min_h_0().child(
                    list(
                        self.list_state.clone(),
                        cx.processor(|this, row: usize, window, cx| {
                            this.render_row(row, window, cx)
                        }),
                    )
                    .size_full(),
                ),
            )
            .child(FrameEndMarker::new(cx.entity()))
    }
}

// ----- Platform input handler (IME) -----

impl EntityInputHandler for SpikeEditor {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        self.focused?;
        let text = self.buf.text();
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
        self.focused?;
        Some(UTF16Selection {
            range: range_to_utf16(self.buf.text(), &self.buf.selection()),
            reversed: self.buf.reversed(),
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.focused?;
        self.buf
            .marked()
            .map(|r| range_to_utf16(self.buf.text(), &r))
    }

    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.buf.ime_unmark();
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.edit(cx, |b| b.ime_replace(range_utf16, new_text));
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.edit(cx, |b| {
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
        let layout = self.shape_focused(window, cx)?;
        let range = range_from_utf16(self.buf.text(), &range_utf16);
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
        Some(super::text_ops::offset_to_utf16(self.buf.text(), offset))
    }

    fn set_selected_text_range(
        &mut self,
        range_utf16: Range<usize>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_from_utf16(self.buf.text(), &range_utf16);
        self.motion(cx, |b| b.set_selection(range, false));
    }

    fn text_length_utf16(&mut self, _: &mut Window, _: &mut Context<Self>) -> Option<usize> {
        self.focused?;
        Some(super::text_ops::offset_to_utf16(
            self.buf.text(),
            self.buf.text().len(),
        ))
    }
}

/// Creates the editor entity (convenience for windows and tests).
pub fn new_editor(doc: SpikeDoc, blink: bool, cx: &mut App) -> Entity<SpikeEditor> {
    cx.new(|cx| SpikeEditor::new(doc, blink, cx))
}
