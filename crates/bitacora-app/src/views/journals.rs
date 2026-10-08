//! Journals feed: today first (even without a file), then older journals as the user scrolls
//! (BIT-US-0076).
//!
//! One list item per day. A day's blocks are read from the index the first time its item is
//! drawn, so a long history costs nothing until it is scrolled into view. Viewing never
//! creates a file: today's page stays virtual until it is edited.

use crate::views::dims;
use std::rc::Rc;
use std::time::Duration;

use bitacora_core::date::Date;
use bitacora_index::IndexEvent;
use rust_i18n::t;

use crate::data::{self, GraphHandle, JournalDay};
use crate::editor::{self, EditorEvent, OutlineEditor};
use crate::nav::OpenIn;
use crate::nav::Scroll;
use crate::render::inline::NavTarget;
use crate::render::model::{Row, toggle_row, visible_rows};
use crate::session::SessionLink;
use crate::ui::text_edit::{ListAlignment, ListOffset, ListState, list};
use crate::ui::theme::ActiveBitacoraTheme as _;
use crate::ui::theme::TypeStyleExt as _;
use crate::ui::{
    ActiveTheme as _, AnyElement, App, AppContext as _, Context, EventEmitter, FluentBuilder as _,
    InteractiveElement as _, IntoElement, ParentElement as _, Render,
    StatefulInteractiveElement as _, Styled as _, Task, Window, div, h_flex, px, v_flex,
};
use crate::views::ai_assist::review_card::ReviewCard;
use crate::views::block_view::{Nav, RowActions, render_block_row};
use crate::views::kit::{Chip, ChipTone};
use crate::views::page_view::PageEvent;
use crate::views::today_panel::TodayPanel;

/// Days added per scroll step.
pub const DAYS_PER_STEP: usize = 7;
/// Blocks read for one day in the feed (the page itself shows the rest).
const ROWS_PER_DAY: usize = 100;
/// How often the date is checked for a midnight rollover.
pub const CLOCK_INTERVAL: Duration = Duration::from_secs(5);
/// Pause after the last index event before the feed is refreshed.
const REFRESH_DEBOUNCE: Duration = Duration::from_millis(250);

/// Where an entry's blocks are.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DayState {
    /// Not scrolled into view yet.
    Unloaded,
    /// Being read.
    Loading,
    /// Read (a virtual day is loaded with no rows).
    Loaded,
}

/// One day of the feed.
#[derive(Debug, Clone, PartialEq)]
pub struct DayEntry {
    /// Which day.
    pub day: JournalDay,
    /// Today's entry.
    pub is_today: bool,
    /// Whether the blocks were read.
    pub state: DayState,
    /// Blocks of the day (first [`ROWS_PER_DAY`]).
    pub rows: Vec<Row>,
    /// The day has more blocks than shown.
    pub truncated: bool,
}

/// Source of "today" (replaced in tests).
pub type Clock = Rc<dyn Fn() -> Option<Date>>;

/// The journals feed view.
pub struct JournalsView {
    handle: Option<GraphHandle>,
    entries: Vec<DayEntry>,
    today: Option<Date>,
    clock: Clock,
    before: Option<u32>,
    has_more: bool,
    loading_days: bool,
    list_state: ListState,
    generation: u64,
    load_task: Option<Task<()>>,
    more_task: Option<Task<()>>,
    clock_task: Option<Task<()>>,
    refresh_task: Option<Task<()>>,
    day_tasks: Vec<Task<()>>,
    /// Live session: the days become editable (BIT-US-0030).
    link: Option<SessionLink>,
    /// One editor per day (keyed by `yyyyMMdd`), created when the day is drawn.
    editors: std::collections::HashMap<u32, crate::ui::Entity<OutlineEditor>>,
    editor_subs: Vec<crate::ui::Subscription>,
    /// The AI review card shown above the feed (BIT-US-0151); it renders nothing while the
    /// feature is off.
    review: Option<crate::ui::Entity<ReviewCard>>,
    /// Tasks for today, tomorrow and in progress, under today's blocks (BIT-US-0178).
    today_panel: Option<crate::ui::Entity<TodayPanel>>,
    _panel_sub: Option<crate::ui::Subscription>,
}

impl std::fmt::Debug for JournalsView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JournalsView")
            .field("entries", &self.entries.len())
            .field("today", &self.today)
            .finish_non_exhaustive()
    }
}

impl EventEmitter<PageEvent> for JournalsView {}

impl JournalsView {
    /// An empty feed using the system clock.
    pub fn new(_cx: &mut Context<Self>) -> Self {
        Self::with_clock(Rc::new(data::today_local))
    }

    /// An empty feed using `clock` as "now".
    pub fn with_clock(clock: Clock) -> Self {
        Self {
            handle: None,
            entries: Vec::new(),
            today: None,
            clock,
            before: None,
            has_more: false,
            loading_days: false,
            list_state: ListState::new(0, ListAlignment::Top, dims::PX_400),
            generation: 0,
            load_task: None,
            more_task: None,
            clock_task: None,
            refresh_task: None,
            day_tasks: Vec::new(),
            link: None,
            editors: std::collections::HashMap::new(),
            editor_subs: Vec::new(),
            today_panel: None,
            _panel_sub: None,
            review: None,
        }
    }

    /// Mounts the review card above the feed.
    pub fn set_review_card(
        &mut self,
        card: Option<crate::ui::Entity<ReviewCard>>,
        cx: &mut Context<Self>,
    ) {
        self.review = card;
        cx.notify();
    }

    /// Connects the feed to the live session: days drawn from now on are editable.
    pub fn set_session_link(&mut self, link: Option<SessionLink>, cx: &mut Context<Self>) {
        self.editors.clear();
        self.editor_subs.clear();
        self.link = link;
        if self.handle.is_some() {
            self.list_state.reset(self.entries.len());
        }
        cx.notify();
    }

    /// The scroll state of the feed (benchmarks).
    pub fn list_state(&self) -> &ListState {
        &self.list_state
    }

    /// The editor of day `day` (`yyyyMMdd`), if it was created.
    pub fn editor_for(&self, day: u32) -> Option<&crate::ui::Entity<OutlineEditor>> {
        self.editors.get(&day)
    }

    /// The block being edited changed on disk: every day editor looks at it.
    pub fn on_editing_conflict(
        &mut self,
        conflict: &bitacora_core::editor::EditingConflict,
        cx: &mut Context<Self>,
    ) {
        for ed in self.editors.values() {
            let (block, mine, disk) =
                (conflict.block, conflict.mine.clone(), conflict.disk.clone());
            ed.update(cx, |e, cx| e.on_editing_conflict(block, mine, disk, cx));
        }
    }

    /// Creates (once) the editor of entry `ix` when its page exists in core.
    fn ensure_editor(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(link), Some(handle)) = (self.link.clone(), self.handle.clone()) else {
            return;
        };
        let Some(entry) = self.entries.get(ix) else {
            return;
        };
        let day = entry.day.day;
        if self.editors.contains_key(&day) || entry.state != DayState::Loaded {
            return;
        }
        let Some(key) = editor::ensure_loaded(&link.queue, &handle, &link.config, &entry.day.title)
        else {
            return;
        };
        let hidden =
            bitacora_core::editor::HiddenKeys::with_extra(link.config.block_hidden_properties());
        let queue = link.queue.clone();
        let ed = cx.new(|cx| OutlineEditor::new(queue, hidden, true, window, cx));
        let config = link.config.clone();
        let gate = link.gate.clone();
        ed.update(cx, |e, cx| {
            e.set_config(config);
            e.set_gate(gate);
            e.set_handle(handle);
            e.set_page(key, cx);
        });
        self.editor_subs.push(
            cx.subscribe_in(&ed, window, move |view, _, event, window, cx| {
                view.on_day_editor_event(day, event, window, cx);
            }),
        );
        self.editor_subs
            .push(cx.observe(&ed, |_, _, cx| cx.notify()));
        self.editors.insert(day, ed);
    }

    fn on_day_editor_event(
        &mut self,
        day: u32,
        event: &EditorEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(ix) = self.entries.iter().position(|e| e.day.day == day) else {
            return;
        };
        match event {
            EditorEvent::DeleteAsset { link, block } => cx.emit(PageEvent::DeleteAsset {
                link: link.clone(),
                block: block.clone(),
            }),
            EditorEvent::Leave { down, goal } => {
                self.continue_in_neighbour(ix, *down, *goal, window, cx)
            }
            EditorEvent::Entered(_) => {}
            EditorEvent::Scroll(dy) => self.list_state.scroll_by(px(*dy)),
            EditorEvent::Row(_) | EditorEvent::Structure => {
                self.list_state.remeasure_items(ix..ix + 1);
                cx.notify();
            }
        }
    }

    /// Up/Down went past the first or last block of day `ix`: the caret continues in the next
    /// (older, when moving down) day that has an editor, at the same x.
    fn continue_in_neighbour(
        &mut self,
        ix: usize,
        down: bool,
        goal: crate::ui::Pixels,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut next = ix;
        loop {
            next = if down {
                next + 1
            } else {
                match next.checked_sub(1) {
                    Some(n) => n,
                    None => return,
                }
            };
            if next >= self.entries.len() {
                return;
            }
            self.ensure_editor(next, window, cx);
            let day = self.entries[next].day.day;
            if let Some(ed) = self.editors.get(&day).cloned() {
                self.list_state.scroll_to_reveal_item(next);
                let from = self.entries[ix].day.day;
                if let Some(src) = self.editors.get(&from).cloned() {
                    src.update(cx, |e, cx| e.exit_edit(cx));
                }
                ed.update(cx, |e, cx| e.enter_edge(!down, goal, window, cx));
                return;
            }
        }
    }

    /// The days of the feed, newest first.
    pub fn entries(&self) -> &[DayEntry] {
        &self.entries
    }

    /// Whether older journals can still be loaded.
    pub fn has_more(&self) -> bool {
        self.has_more
    }

    /// The date the feed considers today.
    pub fn today(&self) -> Option<Date> {
        self.today
    }

    /// The current scroll position, for the navigation history.
    pub fn scroll(&self) -> Scroll {
        let top = self.list_state.logical_scroll_top();
        Scroll {
            item_ix: top.item_ix,
            offset_px: f32::from(top.offset_in_item),
        }
    }

    /// Scrolls to a saved position.
    pub fn restore_scroll(&self, scroll: Scroll) {
        let last = self.entries.len().saturating_sub(1);
        self.list_state.scroll_to(ListOffset {
            item_ix: scroll.item_ix.min(last),
            offset_in_item: px(scroll.offset_px),
        });
    }

    /// Starts checking the date every [`CLOCK_INTERVAL`] to roll over at midnight.
    pub fn start_clock(&mut self, cx: &mut Context<Self>) {
        self.clock_task = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(CLOCK_INTERVAL).await;
                if this.update(cx, |view, cx| view.tick(cx)).is_err() {
                    break;
                }
            }
        }));
    }

    /// Inserts the new "today" when the date changed since the feed was built.
    pub fn tick(&mut self, cx: &mut Context<Self>) {
        let now = (self.clock)();
        if self.handle.is_some() && now.is_some() && now != self.today {
            self.refresh(cx);
        }
    }

    /// Shows the feed for a graph, from the top (or `restore`).
    pub fn show(&mut self, handle: GraphHandle, restore: Option<Scroll>, cx: &mut Context<Self>) {
        if self.today_panel.is_none() {
            let panel = cx.new(|_| TodayPanel::new());
            self._panel_sub = Some(cx.subscribe(&panel, |_, _, event: &PageEvent, cx| {
                cx.emit(event.clone());
            }));
            self.today_panel = Some(panel);
        }
        if let Some(panel) = &self.today_panel {
            let handle = handle.clone();
            panel.update(cx, |p, cx| p.show(handle, cx));
        }
        self.handle = Some(handle);
        self.rebuild(DAYS_PER_STEP, restore, cx);
    }

    /// Rebuilds the feed keeping the scroll position and as many days as were loaded.
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        if self.handle.is_none() {
            return;
        }
        let days = self
            .entries
            .iter()
            .filter(|e| !e.is_today)
            .count()
            .max(DAYS_PER_STEP);
        let scroll = Some(self.scroll());
        self.rebuild(days, scroll, cx);
    }

    /// Any index change may touch a journal: refresh (debounced).
    pub fn on_index_event(&mut self, _event: &IndexEvent, cx: &mut Context<Self>) {
        if self.handle.is_none() {
            return;
        }
        if let Some(panel) = &self.today_panel {
            panel.update(cx, |p, cx| p.on_index_changed(cx));
        }
        self.refresh_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(REFRESH_DEBOUNCE).await;
            let _ = this.update(cx, |view, cx| view.refresh(cx));
        }));
    }

    fn rebuild(&mut self, days: usize, restore: Option<Scroll>, cx: &mut Context<Self>) {
        let Some(handle) = self.handle.clone() else {
            return;
        };
        let Some(today) = (self.clock)() else {
            return;
        };
        self.generation += 1;
        let generation = self.generation;
        self.day_tasks.clear();
        self.more_task = None;
        self.loading_days = false;
        self.load_task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let entry = data::today_entry(&handle, today)?;
                    let (list, more) = data::journal_days(&handle, today, None, days)?;
                    Ok::<_, String>((entry, list, more))
                })
                .await;
            let _ = this.update(cx, |view, cx| match result {
                Ok((today_entry, list, more)) => {
                    view.apply_days(generation, today, today_entry, list, more, restore, cx);
                }
                Err(message) => tracing::warn!("cannot load the journals: {message}"),
            });
        }));
    }

    #[allow(clippy::too_many_arguments)]
    fn apply_days(
        &mut self,
        generation: u64,
        today: Date,
        today_entry: JournalDay,
        list: Vec<JournalDay>,
        more: bool,
        restore: Option<Scroll>,
        cx: &mut Context<Self>,
    ) {
        if generation != self.generation {
            return;
        }
        let today_day = today_entry.day;
        let mut entries = vec![DayEntry {
            day: today_entry,
            is_today: true,
            state: DayState::Unloaded,
            rows: Vec::new(),
            truncated: false,
        }];
        entries.extend(
            list.into_iter()
                .filter(|d| d.day != today_day)
                .map(|day| DayEntry {
                    day,
                    is_today: false,
                    state: DayState::Unloaded,
                    rows: Vec::new(),
                    truncated: false,
                }),
        );
        self.before = entries
            .iter()
            .filter(|e| !e.is_today)
            .map(|e| e.day.day)
            .min();
        self.today = Some(today);
        self.has_more = more;
        self.entries = entries;
        // Editable days follow core: pick up changes made by other programs or by sync.
        for ed in self.editors.values() {
            ed.update(cx, |e, cx| e.refresh(cx));
        }
        self.list_state.reset(self.entries.len());
        if let Some(scroll) = restore {
            self.restore_scroll(scroll);
        }
        cx.notify();
    }

    /// Loads the next [`DAYS_PER_STEP`] older journals.
    pub fn load_more_days(&mut self, cx: &mut Context<Self>) {
        let (Some(handle), Some(today)) = (self.handle.clone(), self.today) else {
            return;
        };
        if !self.has_more || self.loading_days {
            return;
        }
        self.loading_days = true;
        let generation = self.generation;
        let before = self.before;
        self.more_task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { data::journal_days(&handle, today, before, DAYS_PER_STEP) })
                .await;
            let _ = this.update(cx, |view, cx| {
                if generation != view.generation {
                    return;
                }
                view.loading_days = false;
                match result {
                    Ok((days, more)) => {
                        let old = view.entries.len();
                        view.before = days.iter().map(|d| d.day).min().or(view.before);
                        view.has_more = more;
                        let today_day = today.journal_day();
                        view.entries
                            .extend(days.into_iter().filter(|d| d.day != today_day).map(|day| {
                                DayEntry {
                                    day,
                                    is_today: false,
                                    state: DayState::Unloaded,
                                    rows: Vec::new(),
                                    truncated: false,
                                }
                            }));
                        let added = view.entries.len() - old;
                        view.list_state.splice(old..old, added);
                        cx.notify();
                    }
                    Err(message) => {
                        tracing::warn!("cannot load more journals: {message}");
                        view.has_more = false;
                    }
                }
            });
        }));
    }

    /// Reads the blocks of entry `ix` (first time it is drawn).
    fn load_day(&mut self, ix: usize, cx: &mut Context<Self>) {
        let Some(handle) = self.handle.clone() else {
            return;
        };
        let Some(entry) = self.entries.get_mut(ix) else {
            return;
        };
        if entry.state != DayState::Unloaded {
            return;
        }
        let Some(page_id) = entry.day.page_id else {
            // A virtual day: nothing to read, nothing written.
            entry.state = DayState::Loaded;
            return;
        };
        entry.state = DayState::Loading;
        let day = entry.day.day;
        let title = entry.day.title.clone();
        let link = self.link.clone();
        let generation = self.generation;
        let task = cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    // Hand the page to core here, off the UI thread, so that making the day
                    // editable (when it is drawn) does not read files or wait for the writer.
                    if let Some(link) = &link {
                        let _ = editor::ensure_loaded(&link.queue, &handle, &link.config, &title);
                    }
                    data::journal_rows(&handle, page_id, ROWS_PER_DAY)
                })
                .await;
            let _ = this.update(cx, |view, cx| {
                if generation != view.generation {
                    return;
                }
                let Some(pos) = view.entries.iter().position(|e| e.day.day == day) else {
                    return;
                };
                let entry = &mut view.entries[pos];
                entry.state = DayState::Loaded;
                match result {
                    Ok((rows, more)) => {
                        entry.rows = rows;
                        entry.truncated = more;
                    }
                    Err(message) => tracing::warn!("cannot read journal {day}: {message}"),
                }
                view.list_state.remeasure_items(pos..pos + 1);
                cx.notify();
            });
        });
        self.day_tasks.push(task);
    }

    /// Flips the view-only collapse state of a block of entry `entry`.
    pub fn toggle_block(&mut self, entry: usize, row: usize, cx: &mut Context<Self>) {
        if let Some(e) = self.entries.get_mut(entry)
            && toggle_row(&mut e.rows, row)
        {
            self.list_state.remeasure_items(entry..entry + 1);
            cx.notify();
        }
    }

    /// Opens the page of entry `ix` (a click on its title).
    pub fn open_entry(&mut self, ix: usize, cx: &mut Context<Self>) {
        self.open_entry_in(ix, OpenIn::Main, cx);
    }

    /// Opens the page of entry `ix` in `open` (Shift: sidebar, Ctrl/Cmd: new tab).
    pub fn open_entry_in(&mut self, ix: usize, open: OpenIn, cx: &mut Context<Self>) {
        if let Some(entry) = self.entries.get(ix) {
            cx.emit(PageEvent::open(
                NavTarget::Page(entry.day.title.clone()),
                open,
            ));
        }
    }

    fn render_entry(
        &mut self,
        ix: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if ix + 3 >= self.entries.len() && self.has_more {
            self.load_more_days(cx);
        }
        crate::perf::mark("journals_first_render");
        self.load_day(ix, cx);
        if self
            .entries
            .get(ix)
            .is_some_and(|e| e.state == DayState::Loaded)
        {
            crate::perf::mark("journals_first_day");
        }
        {
            let _span = crate::perf::span("journals.ensure_editor");
            self.ensure_editor(ix, window, cx);
        }
        let theme = cx.theme().clone();
        let bt = cx.bitacora().clone();
        let this = cx.entity();
        let nav: Nav = {
            let this = this.clone();
            Rc::new(move |target: NavTarget, open: OpenIn, cx: &mut App| {
                this.update(cx, |_, cx| cx.emit(PageEvent::open(target, open)));
            })
        };
        let root = self.handle.as_ref().map(|h| h.root.clone());
        let Some(entry) = self.entries.get(ix) else {
            return div().into_any_element();
        };
        let title_this = this.clone();
        let title = div()
            .id(("journal-title", ix))
            .type_style(&bt.type_scale.display)
            .text_color(bt.colors.text)
            .cursor_pointer()
            .child(entry.day.title.clone())
            .on_click(move |_, window, cx| {
                let open = OpenIn::from_modifiers(&window.modifiers());
                title_this.update(cx, |v, cx| v.open_entry_in(ix, open, cx));
            });
        let header = h_flex()
            .w_full()
            .items_center()
            .gap(bt.metrics.space[5])
            .pb(bt.metrics.space[4])
            .child(title)
            .when(entry.is_today, |d| {
                d.child(Chip::new(t!("journals.today_badge").to_string()).tone(ChipTone::Accent))
            });
        let mut col = v_flex()
            .w_full()
            .px(bt.metrics.reading_pad_x)
            .py(dims::PX_12)
            .gap_1()
            .child(header);
        // An editable day shows core's rows; the others the index rows.
        let day_editor = self.editors.get(&entry.day.day).cloned();
        let live_rows: Option<Vec<Row>> = day_editor.as_ref().map(|e| e.read(cx).rows().to_vec());
        let rows: &[Row] = live_rows.as_deref().unwrap_or(&entry.rows);
        let entry_title = entry.day.title.clone();
        let visible = visible_rows(rows);
        if entry.state == DayState::Loaded && visible.is_empty() {
            // A day core holds but that has no block yet: a click starts its first block.
            let placeholder = div()
                .id(("journal-empty", ix))
                .w_full()
                .text_color(theme.muted_foreground)
                .child(if entry.is_today {
                    t!("journals.today_empty").to_string()
                } else {
                    t!("page.empty").to_string()
                });
            col = col.child(match day_editor.clone() {
                Some(ed) => placeholder
                    .cursor_text()
                    .on_click(move |_, window, cx| {
                        ed.update(cx, |e, cx| e.focus_first_block(window, cx));
                    })
                    .into_any_element(),
                None => placeholder.into_any_element(),
            });
        }
        for r in visible {
            let toggle_this = this.clone();
            let edit = day_editor
                .as_ref()
                .and_then(|ed| OutlineEditor::row_edit(ed, r, cx));
            let widgets = self.handle.clone().and_then(|handle| {
                let host = crate::views::widgets::Host::for_page(
                    handle,
                    self.link.clone(),
                    &entry_title,
                    rows,
                    r,
                );
                crate::views::widgets::prepare(
                    &host,
                    &rows[r],
                    &format!("{ix}:{r}"),
                    nav.clone(),
                    crate::views::widgets::edit_hook(edit.as_ref()),
                    cx,
                )
            });
            let actions = RowActions {
                nav: nav.clone(),
                toggle: Some(Rc::new(move |_, cx| {
                    toggle_this.update(cx, |v, cx| v.toggle_block(ix, r, cx));
                })),
                referrers: None,
                focus: None,
                edit,
                widgets,
                activate: None,
            };
            col = col.child(render_block_row(
                ((ix + 1) << 20) | (r & 0xF_FFFF),
                &rows[r],
                root.as_deref(),
                &theme,
                &bt,
                &actions,
            ));
        }
        if entry.is_today
            && let Some(panel) = &self.today_panel
        {
            col = col.child(panel.clone());
        }
        if entry.truncated && day_editor.is_none() {
            let title_this = this.clone();
            col = col.child(
                div()
                    .id(("journal-more", ix))
                    .pl(dims::PX_24)
                    .text_sm()
                    .text_color(theme.info)
                    .cursor_pointer()
                    .child(t!("journals.more").to_string())
                    .on_click(move |_, _, cx| {
                        title_this.update(cx, |v, cx| v.open_entry(ix, cx));
                    }),
            );
        }
        let separator = (ix + 1 < self.entries.len()).then(|| {
            div()
                .w_full()
                .h(dims::PX_1)
                .mt(dims::PX_8)
                .bg(theme.border)
                .into_any_element()
        });
        div()
            .w_full()
            .flex()
            .justify_center()
            .child(
                v_flex()
                    .w_full()
                    .max_w(bt.metrics.reading_max + bt.metrics.reading_pad_x * 2.)
                    .child(match &day_editor {
                        Some(ed) => editor::element::wrap(col.into_any_element(), ed, cx),
                        None => col.into_any_element(),
                    })
                    .children(separator),
            )
            .into_any_element()
    }
}

impl Render for JournalsView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let body: AnyElement = if self.entries.is_empty() {
            h_flex()
                .size_full()
                .items_center()
                .justify_center()
                .child(t!("page.loading").to_string())
                .into_any_element()
        } else {
            list(
                self.list_state.clone(),
                cx.processor(|this, ix: usize, window, cx| this.render_entry(ix, window, cx)),
            )
            .size_full()
            .into_any_element()
        };
        v_flex()
            .size_full()
            .bg(theme.background)
            .text_color(theme.foreground)
            .children(self.review.clone())
            .child(div().flex_1().min_h_0().child(body))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TestGraph;
    use crate::ui::Entity;
    use crate::ui::testing::{TestAppContext, VisualTestContext, gpui_test};
    use crate::{settings::AppSettings, theme};
    use std::cell::Cell;

    fn setup(cx: &mut TestAppContext) {
        cx.update(|cx| {
            crate::ui::init(cx);
            theme::install(cx, AppSettings::default(), None);
        });
    }

    fn d(y: i32, m: u8, day: u8) -> Date {
        Date::new(y, m, day).expect("date")
    }

    fn open(
        cx: &mut TestAppContext,
        today: Rc<Cell<Date>>,
    ) -> (Entity<JournalsView>, &mut VisualTestContext) {
        cx.add_window_view(move |_, _| JournalsView::with_clock(Rc::new(move || Some(today.get()))))
    }

    fn settle(
        cx: &mut VisualTestContext,
        view: &Entity<JournalsView>,
        done: impl Fn(&JournalsView) -> bool,
    ) {
        cx.executor().allow_parking();
        for _ in 0..400 {
            cx.run_until_parked();
            if view.read_with(cx, |v, _| done(v)) {
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("journals did not settle");
    }

    fn journal_files(count: u8) -> Vec<(String, String)> {
        (1..=count)
            .map(|n| {
                (
                    format!("journals/2024_01_{n:02}.md"),
                    format!("- note of day {n}\n"),
                )
            })
            .collect()
    }

    fn graph_with(count: u8) -> TestGraph {
        let files = journal_files(count);
        let refs: Vec<(&str, &str)> = files
            .iter()
            .map(|(a, b)| (a.as_str(), b.as_str()))
            .collect();
        TestGraph::new(&refs)
    }

    #[gpui_test]
    fn today_is_virtual_when_there_is_no_file_and_nothing_is_created(cx: &mut TestAppContext) {
        setup(cx);
        let g = graph_with(10);
        let today = Rc::new(Cell::new(d(2024, 3, 10)));
        let (view, cx) = open(cx, today);
        view.update(cx, |v, cx| v.show(g.handle.clone(), None, cx));
        settle(cx, &view, |v| !v.entries().is_empty());
        let first = view.read_with(cx, |v, _| v.entries()[0].clone());
        assert!(first.is_today);
        assert_eq!(first.day.page_id, None, "no file: virtual page");
        assert_eq!(first.day.title, "Mar 10th, 2024");
        // The next entries are the newest real journals, newest first.
        let days: Vec<u32> =
            view.read_with(cx, |v, _| v.entries().iter().map(|e| e.day.day).collect());
        assert_eq!(&days[..3], &[20_240_310, 20_240_110, 20_240_109]);
        // Viewing created nothing on disk.
        assert!(!g.path().join("journals/2024_03_10.md").exists());
        assert_eq!(
            std::fs::read_dir(g.path().join("journals"))
                .expect("dir")
                .count(),
            10
        );
    }

    #[gpui_test]
    fn an_existing_file_for_today_is_not_duplicated(cx: &mut TestAppContext) {
        setup(cx);
        let g = graph_with(5);
        let today = Rc::new(Cell::new(d(2024, 1, 5)));
        let (view, cx) = open(cx, today);
        view.update(cx, |v, cx| v.show(g.handle.clone(), None, cx));
        settle(cx, &view, |v| !v.entries().is_empty());
        let days: Vec<u32> =
            view.read_with(cx, |v, _| v.entries().iter().map(|e| e.day.day).collect());
        assert_eq!(
            days,
            [20_240_105, 20_240_104, 20_240_103, 20_240_102, 20_240_101]
        );
        assert!(view.read_with(cx, |v, _| v.entries()[0].day.page_id.is_some()));
        // Future journals are not shown.
        let g2 = graph_with(5);
        let past = Rc::new(Cell::new(d(2024, 1, 3)));
        let (view2, cx) = open(cx, past);
        view2.update(cx, |v, cx| v.show(g2.handle.clone(), None, cx));
        settle(cx, &view2, |v| !v.entries().is_empty());
        let days: Vec<u32> =
            view2.read_with(cx, |v, _| v.entries().iter().map(|e| e.day.day).collect());
        assert_eq!(days, [20_240_103, 20_240_102, 20_240_101]);
    }

    #[gpui_test]
    fn scrolling_loads_seven_more_days_and_blocks_only_when_drawn(cx: &mut TestAppContext) {
        setup(cx);
        // 400 days: more than any viewport can draw, so the feed stays partial.
        let files: Vec<(String, String)> = (0..400_u32)
            .map(|i| {
                (
                    format!(
                        "journals/{}_{:02}_{:02}.md",
                        1900 + i / 336,
                        1 + (i % 336) / 28,
                        1 + i % 28
                    ),
                    format!("- note {i}\n"),
                )
            })
            .collect();
        let refs: Vec<(&str, &str)> = files
            .iter()
            .map(|(a, b)| (a.as_str(), b.as_str()))
            .collect();
        let g = TestGraph::new(&refs);
        let today = Rc::new(Cell::new(d(2024, 2, 1)));
        let (view, cx) = open(cx, today);
        view.update(cx, |v, cx| v.show(g.handle.clone(), None, cx));
        settle(cx, &view, |v| !v.entries().is_empty());
        for _ in 0..5 {
            cx.run_until_parked();
            std::thread::sleep(Duration::from_millis(10));
        }
        // Today plus whole steps of seven days, far from everything.
        let len = view.read_with(cx, |v, _| v.entries().len());
        assert!(len > DAYS_PER_STEP, "first step shown: {len}");
        assert_eq!((len - 1) % DAYS_PER_STEP, 0, "{len}");
        assert!(len < 400);
        assert!(view.read_with(cx, |v, _| v.has_more()));
        // The far end of the feed has not been drawn, so its blocks are not read.
        assert_eq!(
            view.read_with(cx, |v, _| v.entries().last().map(|e| e.state)),
            Some(DayState::Unloaded)
        );
        view.update(cx, |v, cx| v.load_more_days(cx));
        settle(cx, &view, |v| v.entries().len() > len);
        let more = view.read_with(cx, |v, _| v.entries().len()) - len;
        assert_eq!(more % DAYS_PER_STEP, 0);
        let days: Vec<u32> =
            view.read_with(cx, |v, _| v.entries().iter().map(|e| e.day.day).collect());
        let mut sorted = days[1..].to_vec();
        sorted.sort_by(|a, b| b.cmp(a));
        assert_eq!(days[1..], sorted[..], "newest first");
        sorted.dedup();
        assert_eq!(sorted.len(), days.len() - 1, "no duplicates");
    }

    #[gpui_test]
    fn drawn_days_read_their_blocks_and_titles_navigate(cx: &mut TestAppContext) {
        setup(cx);
        let g = graph_with(3);
        let today = Rc::new(Cell::new(d(2024, 1, 3)));
        let (view, cx) = open(cx, today);
        let events = Rc::new(std::cell::RefCell::new(Vec::new()));
        let sink = events.clone();
        let _sub = cx.update(|_, cx| {
            cx.subscribe(&view, move |_, event: &PageEvent, _| {
                sink.borrow_mut().push(event.clone());
            })
        });
        view.update(cx, |v, cx| v.show(g.handle.clone(), None, cx));
        settle(cx, &view, |v| {
            v.entries()
                .first()
                .is_some_and(|e| e.state == DayState::Loaded)
        });
        let rows = view.read_with(cx, |v, _| v.entries()[0].rows.len());
        assert_eq!(rows, 1);
        view.update(cx, |v, cx| {
            v.open_entry(1, cx);
            v.open_entry_in(1, OpenIn::Sidebar, cx);
            v.open_entry_in(1, OpenIn::NewTab, cx);
        });
        let page = || NavTarget::Page("Jan 2nd, 2024".into());
        assert_eq!(
            *events.borrow(),
            vec![
                PageEvent::Navigate(page()),
                PageEvent::OpenInSidebar(page()),
                PageEvent::OpenInNewTab(page()),
            ]
        );
    }

    #[gpui_test]
    fn midnight_rollover_inserts_the_new_today(cx: &mut TestAppContext) {
        setup(cx);
        let g = graph_with(3);
        let today = Rc::new(Cell::new(d(2024, 1, 3)));
        let (view, cx) = open(cx, today.clone());
        view.update(cx, |v, cx| v.show(g.handle.clone(), None, cx));
        settle(cx, &view, |v| !v.entries().is_empty());
        assert_eq!(
            view.read_with(cx, |v, _| v.entries()[0].day.day),
            20_240_103
        );
        today.set(d(2024, 1, 4));
        view.update(cx, |v, cx| v.tick(cx));
        settle(cx, &view, |v| {
            v.entries().first().is_some_and(|e| e.day.day == 20_240_104)
        });
        let first = view.read_with(cx, |v, _| v.entries()[0].clone());
        assert!(first.is_today && first.day.page_id.is_none());
        assert_eq!(
            view.read_with(cx, |v, _| v.entries()[1].day.day),
            20_240_103
        );
        assert_eq!(view.read_with(cx, |v, _| v.today()), Some(d(2024, 1, 4)));
        // No change of date, no rebuild.
        let generation_before = view.read_with(cx, |v, _| v.generation);
        view.update(cx, |v, cx| v.tick(cx));
        assert_eq!(view.read_with(cx, |v, _| v.generation), generation_before);
    }

    #[gpui_test]
    fn empty_today_in_the_feed_gets_an_editor_with_core_first_block(cx: &mut TestAppContext) {
        setup(cx);
        cx.update(|cx| crate::keymap::load_with_user(cx, None).expect("keymap"));
        let env =
            crate::views::widgets::tests::Env::new(&[("journals/2024_03_09.md", "- yesterday\n")]);
        let today = Rc::new(Cell::new(d(2024, 3, 10)));
        let (view, cx) = open(cx, today);
        view.update(cx, |v, cx| {
            v.set_session_link(Some(env.link.clone()), cx);
            v.show(env.handle.clone(), None, cx);
        });
        settle(cx, &view, |v| !v.entries().is_empty());
        cx.run_until_parked();
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
        let ed = view.read_with(cx, |v, _| v.editor_for(20_240_310).cloned());
        let ed = ed.expect("today has an editor");
        let rows = ed.read_with(cx, |e, _| e.rows().len());
        assert_eq!(rows, 1, "core's virtual first block");
        assert!(!env.graph.path().join("journals/2024_03_10.md").exists());
        // Type, Enter (new block) and Tab (indent) work in the feed like on the page.
        let id = ed
            .read_with(cx, |e, _| e.block_ids().first().copied())
            .expect("id");
        ed.update_in(cx, |e, window, cx| {
            e.enter(id, crate::editor::Caret::End, window, cx);
        });
        cx.run_until_parked();
        cx.simulate_input("first");
        cx.simulate_keystrokes("enter");
        cx.simulate_input("second");
        cx.simulate_keystrokes("tab");
        ed.update(cx, |e, cx| {
            e.flush(cx);
        });
        let key = bitacora_core::graph::PageKey::from_title("Mar 10th, 2024");
        let texts: Vec<(usize, String)> = env
            .link
            .queue
            .snapshot(&key)
            .map(|s| s.blocks.iter().map(|b| (b.depth, b.text.clone())).collect())
            .unwrap_or_default();
        assert_eq!(texts, [(1, "first".to_owned()), (2, "second".to_owned())]);
        assert_eq!(env.disk("journals/2024_03_10.md"), "- first\n\t- second\n");
    }

    #[gpui_test]
    fn an_empty_day_file_starts_its_first_block_from_the_feed(cx: &mut TestAppContext) {
        setup(cx);
        cx.update(|cx| crate::keymap::load_with_user(cx, None).expect("keymap"));
        let env = crate::views::widgets::tests::Env::new(&[
            ("journals/2024_03_09.md", "- yesterday\n"),
            ("journals/2024_03_10.md", ""),
        ]);
        let today = Rc::new(Cell::new(d(2024, 3, 10)));
        let (view, cx) = open(cx, today);
        view.update(cx, |v, cx| {
            v.set_session_link(Some(env.link.clone()), cx);
            v.show(env.handle.clone(), None, cx);
        });
        settle(cx, &view, |v| !v.entries().is_empty());
        cx.run_until_parked();
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
        let ed = view
            .read_with(cx, |v, _| v.editor_for(20_240_310).cloned())
            .expect("today has an editor");
        assert_eq!(ed.read_with(cx, |e, _| e.rows().len()), 0, "no block yet");
        ed.update_in(cx, |e, window, cx| e.focus_first_block(window, cx));
        cx.run_until_parked();
        assert_eq!(ed.read_with(cx, |e, _| e.rows().len()), 1);
        assert!(ed.read_with(cx, |e, _| e.editing().is_some()));
        cx.simulate_input("first");
        cx.simulate_keystrokes("enter");
        cx.simulate_input("second");
        cx.simulate_keystrokes("tab");
        ed.update(cx, |e, cx| {
            e.flush(cx);
        });
        assert_eq!(
            env.disk("journals/2024_03_10.md").trim_end(),
            "- first\n\t- second"
        );
    }
}
