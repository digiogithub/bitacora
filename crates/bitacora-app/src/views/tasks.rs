//! The Tasks view (BIT-US-0126, design `mockups/Tareas.dc.html`): every open task of the graph
//! grouped Overdue / This week / Later / No date, with filter pills (marker class, priority,
//! page) that show counts, in a column capped at the `tasks_max` metric (860px).
//!
//! Queries come from `bitacora-index` ([`bitacora_index::IndexReader::task_groups`]). A click on
//! a row's text edits its block in place (BIT-US-0168, Shift+click opens it in the sidebar), the
//! checkbox completes it and the marker cycles it; all go through the core command queue as
//! undoable `Cmd`s (single writer).

use std::time::Duration;

use bitacora_core::editor::Cmd;
use bitacora_core::queue::Source;
use bitacora_index::{TaskFilter, TaskGroup, TaskGroups, TaskItem};
use bitacora_markdown::edit::state::{clear_planning, move_planning_date};
use bitacora_markdown::tasks::head::Marker;
use rust_i18n::t;

use crate::data::{self, GraphHandle};
use crate::editor;
use crate::editor::row::TextHook;
use crate::nav::OpenIn;
use crate::render::inline::NavTarget;
use crate::render::model::Row;
use crate::session::SessionLink;
use crate::ui::theme::{ActiveBitacoraTheme as _, TypeStyleExt as _};
use crate::ui::{
    ActiveTheme as _, AnyElement, Context, EventEmitter, FluentBuilder as _,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, SharedString,
    StatefulInteractiveElement as _, Styled as _, Task, Window, div, h_flex, v_flex,
};
use crate::views::calendar::Month;
use crate::views::kit::{Card, Overline, Pill, Surface, TaskMarker};
use crate::views::page_view::PageEvent;
use crate::views::planning::{self, OpenPicker, PlanningActions, PlanningChipView};
use crate::views::remote_edit::{BlockRef, RemoteEditors};

/// Debounce between an index change and the reload.
const REFRESH_DEBOUNCE: Duration = Duration::from_millis(300);

/// One open task, ready to draw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskRow {
    /// Block UUID in the index.
    pub uuid: String,
    /// Page title.
    pub page: String,
    /// Position of the block in the file (pre-order, pre-block included).
    pub ord: i64,
    /// First line without marker and priority.
    pub title: String,
    /// The marker word.
    pub marker: Marker,
    /// Priority `A`/`B`/`C`.
    pub priority: Option<String>,
    /// Earliest `SCHEDULED`/`DEADLINE` (`yyyyMMdd`).
    pub due: Option<i64>,
    /// The due date is a `DEADLINE`.
    pub deadline: bool,
    /// Section it belongs to.
    pub group: TaskGroup,
}

impl TaskRow {
    fn from_item(item: &TaskItem, group: TaskGroup) -> Option<Self> {
        let b = &item.block;
        let deadline = match (b.scheduled, b.deadline) {
            (Some(s), Some(d)) => d < s,
            (None, Some(_)) => true,
            _ => false,
        };
        Some(Self {
            uuid: b.uuid.clone(),
            page: item.page_name.clone(),
            ord: b.ord,
            title: b.title.clone(),
            marker: Marker::from_word(b.marker.as_deref()?)?,
            priority: b.priority.clone(),
            due: item.due(),
            deadline,
            group,
        })
    }
}

/// Which marker class a pill selects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MarkerClass {
    /// Every open task.
    #[default]
    All,
    /// Only overdue ones.
    Overdue,
    /// `DOING`, `NOW`, `STARTED`, `IN-PROGRESS`.
    InProgress,
    /// `TODO`.
    Todo,
    /// `LATER`, `WAITING`, `WAIT`.
    Later,
}

impl MarkerClass {
    const ALL_CLASSES: [MarkerClass; 5] = [
        Self::All,
        Self::Overdue,
        Self::InProgress,
        Self::Todo,
        Self::Later,
    ];

    fn matches(self, row: &TaskRow) -> bool {
        match self {
            Self::All => true,
            Self::Overdue => row.group == TaskGroup::Overdue,
            Self::InProgress => matches!(
                row.marker,
                Marker::Doing | Marker::Now | Marker::Started | Marker::InProgress
            ),
            Self::Todo => row.marker == Marker::Todo,
            Self::Later => matches!(row.marker, Marker::Later | Marker::Waiting | Marker::Wait),
        }
    }

    fn label(self) -> String {
        match self {
            Self::All => t!("tasks.filter_all"),
            Self::Overdue => t!("tasks.filter_overdue"),
            Self::InProgress => t!("tasks.filter_doing"),
            Self::Todo => t!("tasks.filter_todo"),
            Self::Later => t!("tasks.filter_later"),
        }
        .to_string()
    }
}

/// What the pills currently select.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Selection {
    /// Marker class.
    pub marker: MarkerClass,
    /// Priority letter.
    pub priority: Option<String>,
    /// Page title.
    pub page: Option<String>,
}

impl Selection {
    fn keeps(&self, row: &TaskRow) -> bool {
        self.marker.matches(row)
            && self
                .priority
                .as_ref()
                .is_none_or(|p| row.priority.as_ref() == Some(p))
            && self.page.as_ref().is_none_or(|p| &row.page == p)
    }

    /// This selection with every dimension that no longer matches anything in `model` dropped.
    ///
    /// Rescheduling or completing the last task a pill selects (the last overdue one, the last
    /// task of a page) would otherwise leave a filter that keeps zero rows while the model still
    /// holds tasks: the list looks emptied until the app is restarted (BIT-US-0173).
    #[must_use]
    pub fn reconciled(&self, model: &TaskModel) -> Selection {
        let mut next = self.clone();
        if let Some(p) = &next.page
            && !model.rows.iter().any(|r| &r.page == p)
        {
            next.page = None;
        }
        if let Some(p) = &next.priority
            && !model.rows.iter().any(|r| r.priority.as_ref() == Some(p))
        {
            next.priority = None;
        }
        if next.marker != MarkerClass::All && model.count_marker(next.marker, &next) == 0 {
            next.marker = MarkerClass::All;
        }
        next
    }
}

/// All open tasks of a graph, in display order (groups, then date or outline order).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TaskModel {
    /// Every open task.
    pub rows: Vec<TaskRow>,
}

impl TaskModel {
    /// Builds the model from the index buckets.
    pub fn from_groups(groups: &TaskGroups) -> Self {
        let mut rows = Vec::new();
        for group in TaskGroup::ALL {
            for item in groups.get(group) {
                match TaskRow::from_item(item, group) {
                    Some(row) => rows.push(row),
                    // One odd block never empties the list.
                    None => tracing::warn!(
                        "task {} on {} skipped: unknown marker {:?}",
                        item.block.uuid,
                        item.page_name,
                        item.block.marker
                    ),
                }
            }
        }
        Self { rows }
    }

    /// Rows kept by `selection`, grouped in display order (empty groups dropped).
    pub fn grouped(&self, selection: &Selection) -> Vec<(TaskGroup, Vec<&TaskRow>)> {
        TaskGroup::ALL
            .into_iter()
            .filter_map(|g| {
                let rows: Vec<&TaskRow> = self
                    .rows
                    .iter()
                    .filter(|r| r.group == g && selection.keeps(r))
                    .collect();
                (!rows.is_empty()).then_some((g, rows))
            })
            .collect()
    }

    /// Count of a marker pill, given the priority and page selections.
    pub fn count_marker(&self, class: MarkerClass, selection: &Selection) -> usize {
        let s = Selection {
            marker: class,
            ..selection.clone()
        };
        self.rows.iter().filter(|r| s.keeps(r)).count()
    }

    /// Count of a priority pill, given the marker and page selections.
    pub fn count_priority(&self, letter: &str, selection: &Selection) -> usize {
        let s = Selection {
            priority: Some(letter.to_owned()),
            ..selection.clone()
        };
        self.rows.iter().filter(|r| s.keeps(r)).count()
    }

    /// Number of overdue tasks.
    pub fn overdue(&self) -> usize {
        self.rows
            .iter()
            .filter(|r| r.group == TaskGroup::Overdue)
            .count()
    }
}

/// Reads the open tasks from the index (background thread). `today` is `yyyyMMdd`.
pub fn load_tasks(handle: &GraphHandle, today: i64) -> Result<TaskModel, String> {
    handle
        .reader
        .task_groups(today, &TaskFilter::default())
        .map(|g| TaskModel::from_groups(&g))
        .map_err(|e| e.to_string())
}

/// What a row action does to the block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowAction {
    /// Checkbox: done.
    Complete,
    /// Marker click: next marker of the workflow.
    Cycle,
    /// Date chip: move (`Some(yyyyMMdd)`) or remove (`None`) the `SCHEDULED:` / `DEADLINE:`
    /// date.
    Reschedule {
        /// Keyword as written in the file.
        keyword: &'static str,
        /// The new day.
        day: Option<u32>,
    },
}

/// Applies `action` to the block of `row` through the command queue (one undoable transaction).
///
/// # Errors
/// A user-presentable message when the page cannot be loaded, the block moved on disk since it
/// was indexed, or the queue refuses.
pub fn apply_action(
    link: &SessionLink,
    handle: &GraphHandle,
    row: &TaskRow,
    action: RowAction,
) -> Result<(), String> {
    let key = editor::ensure_loaded(&link.queue, handle, &link.config, &row.page)
        .ok_or_else(|| t!("tasks.err_page", page = row.page).to_string())?;
    let snapshot = link
        .queue
        .snapshot(&key)
        .ok_or_else(|| t!("tasks.err_page", page = row.page).to_string())?;
    // The index counts the page-properties pre-block as ord 0.
    let offset = i64::from(snapshot.preamble.is_some());
    let block = usize::try_from(row.ord - offset)
        .ok()
        .and_then(|ix| snapshot.blocks.get(ix))
        .filter(|b| b.text.contains(&row.title))
        .ok_or_else(|| t!("tasks.err_moved").to_string())?;
    let cmd = match action {
        RowAction::Complete => Cmd::ToggleDone {
            ids: vec![block.id],
        },
        RowAction::Cycle => Cmd::CycleMarker {
            ids: vec![block.id],
        },
        RowAction::Reschedule { keyword, day } => {
            // Only the planning line changes (BIT-US-0167).
            let text = match day {
                Some(key) => {
                    let (y, m, d) = planning::split_key(key);
                    move_planning_date(&block.text, keyword, y, m, d)
                }
                None => clear_planning(&block.text, keyword),
            };
            Cmd::SetText { id: block.id, text }
        }
    };
    let label = match action {
        RowAction::Complete => "Complete task",
        RowAction::Cycle => "Cycle task marker",
        RowAction::Reschedule { day: Some(_), .. } => "Reschedule task",
        RowAction::Reschedule { day: None, .. } => "Remove task date",
    };
    link.queue
        .run(Source::Ui, label, cmd)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Where the open button of `task` leads: its page, with the block scrolled into view,
/// highlighted and in edit mode, not the block zoomed in alone.
fn open_target(task: &TaskRow) -> NavTarget {
    NavTarget::PageAt {
        page: task.page.clone(),
        block: task.uuid.clone(),
    }
}

/// The month a date picker opens on: the month of `due` (`yyyyMMdd`), or the current month for a
/// task that has no date yet (BIT-US-0187).
fn picker_month(due: Option<i64>, this_year: i32, this_month: u8) -> Month {
    match due {
        Some(d) => Month {
            year: i32::try_from(d / 10_000).unwrap_or(this_year),
            month: u8::try_from(d / 100 % 100)
                .unwrap_or(this_month)
                .clamp(1, 12),
        },
        None => Month {
            year: this_year,
            month: this_month.clamp(1, 12),
        },
    }
}

/// `yyyyMMdd` as `yyyy-MM-dd`.
fn format_day(day: i64) -> String {
    format!(
        "{:04}-{:02}-{:02}",
        day / 10_000,
        day / 100 % 100,
        day % 100
    )
}

/// The Tasks view.
pub struct TasksView {
    handle: Option<GraphHandle>,
    link: Option<SessionLink>,
    model: TaskModel,
    selection: Selection,
    loaded: bool,
    notice: Option<String>,
    load_task: Option<Task<()>>,
    refresh_task: Option<Task<()>>,
    action_task: Option<Task<()>>,
    /// Editors of the pages the tasks live on: a click on a title edits the block in place.
    remote: RemoteEditors<TasksView>,
    /// The open date picker: block uuid and picker (BIT-US-0167).
    planning: Option<(String, OpenPicker)>,
}

impl std::fmt::Debug for TasksView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TasksView")
            .field("tasks", &self.model.rows.len())
            .finish_non_exhaustive()
    }
}

impl EventEmitter<PageEvent> for TasksView {}

impl TasksView {
    /// An empty view.
    pub fn new(_: &mut Context<Self>) -> Self {
        Self {
            handle: None,
            link: None,
            model: TaskModel::default(),
            selection: Selection::default(),
            loaded: false,
            notice: None,
            load_task: None,
            refresh_task: None,
            action_task: None,
            remote: RemoteEditors::new(|v| &mut v.remote, None),
            planning: None,
        }
    }

    /// The editor of source page `page`, once a click created it (tests).
    #[cfg(test)]
    pub(crate) fn editor(
        &self,
        page: &str,
    ) -> Option<crate::ui::Entity<crate::editor::OutlineEditor>> {
        self.remote.editor(page).cloned()
    }

    /// What a click on the title of task `uuid` does (tests).
    #[cfg(test)]
    pub(crate) fn click_task(&mut self, uuid: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(task) = self.model.rows.iter().find(|r| r.uuid == uuid).cloned() else {
            return;
        };
        let target = BlockRef {
            page: task.page,
            uuid: Some(task.uuid),
            ord: usize::try_from(task.ord).ok(),
            content: None,
            contains: Some(task.title),
        };
        self.remote.activate(&target, usize::MAX, false, window, cx);
    }

    /// The block being edited changed on disk: the editors of the task pages look at it.
    pub fn on_editing_conflict(
        &mut self,
        conflict: &bitacora_core::editor::EditingConflict,
        cx: &mut Context<Self>,
    ) {
        self.remote.on_editing_conflict(conflict, cx);
    }

    /// Connects the view to the live session so rows can be completed.
    pub fn set_session_link(&mut self, link: Option<SessionLink>, cx: &mut Context<Self>) {
        self.link = link.clone();
        self.remote.configure(link, self.handle.clone());
        cx.notify();
    }

    /// Shows the tasks of a graph.
    pub fn show(&mut self, handle: GraphHandle, cx: &mut Context<Self>) {
        self.remote
            .configure(self.link.clone(), Some(handle.clone()));
        self.handle = Some(handle);
        self.reload(cx);
    }

    /// Forgets the graph.
    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.handle = None;
        self.loaded = false;
        self.model = TaskModel::default();
        cx.notify();
    }

    /// Whether the tasks were loaded at least once.
    pub fn is_loaded(&self) -> bool {
        self.loaded
    }

    /// The loaded tasks.
    pub fn model(&self) -> &TaskModel {
        &self.model
    }

    /// The pill selection.
    pub fn selection(&self) -> &Selection {
        &self.selection
    }

    /// Replaces the pill selection.
    pub fn set_selection(&mut self, selection: Selection, cx: &mut Context<Self>) {
        self.selection = selection;
        cx.notify();
    }

    /// Reads the tasks from the index on a background thread.
    pub fn reload(&mut self, cx: &mut Context<Self>) {
        let Some(handle) = self.handle.clone() else {
            return;
        };
        let Some(today) = data::today_key() else {
            return;
        };
        self.load_task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { load_tasks(&handle, today) })
                .await;
            let _ = this.update(cx, |view, cx| match result {
                Ok(model) => {
                    view.loaded = true;
                    view.selection = view.selection.reconciled(&model);
                    view.model = model;
                    cx.notify();
                }
                // Keep the rows already on screen: a failed read never blanks the list.
                Err(message) => tracing::warn!("cannot list the tasks: {message}"),
            });
        }));
    }

    /// Any index change may touch a task: reload (debounced).
    pub fn on_index_changed(&mut self, cx: &mut Context<Self>) {
        if self.handle.is_none() {
            return;
        }
        self.refresh_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(REFRESH_DEBOUNCE).await;
            let _ = this.update(cx, |view, cx| view.reload(cx));
        }));
    }

    /// Completes or cycles `uuid` through the command queue, then reloads.
    pub fn act(&mut self, uuid: &str, action: RowAction, cx: &mut Context<Self>) {
        let (Some(link), Some(handle)) = (self.link.clone(), self.handle.clone()) else {
            return;
        };
        let Some(row) = self.model.rows.iter().find(|r| r.uuid == uuid).cloned() else {
            return;
        };
        self.action_task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { apply_action(&link, &handle, &row, action) })
                .await;
            let _ = this.update(cx, |view, cx| {
                view.notice = result.err();
                view.reload(cx);
                cx.notify();
            });
        }));
    }

    /// The picker open on the row `uuid`, if any.
    #[must_use]
    pub fn planning_open(&self) -> Option<(&str, OpenPicker)> {
        self.planning.as_ref().map(|(u, o)| (u.as_str(), *o))
    }

    /// Opens the date picker of the row `uuid` on the month of its date.
    pub fn open_planning(&mut self, uuid: &str, keyword: &'static str, cx: &mut Context<Self>) {
        let due = self
            .model
            .rows
            .iter()
            .find(|r| r.uuid == uuid)
            .and_then(|r| r.due);
        let now = jiff::Zoned::now();
        let month = picker_month(
            due,
            i32::from(now.year()),
            u8::try_from(now.month()).unwrap_or(1),
        );
        self.planning = Some((uuid.to_owned(), OpenPicker { keyword, month }));
        cx.notify();
    }

    /// Closes the date picker.
    pub fn close_planning(&mut self, cx: &mut Context<Self>) {
        if self.planning.take().is_some() {
            cx.notify();
        }
    }

    /// The month arrows of the open picker.
    pub fn shift_planning(&mut self, delta: i32, cx: &mut Context<Self>) {
        if let Some((_, open)) = self.planning.as_mut() {
            open.month = crate::views::calendar::shift_month(open.month, delta);
            cx.notify();
        }
    }

    /// Moves (`Some(yyyyMMdd)`) or removes (`None`) the date of the open picker's row.
    pub fn reschedule(&mut self, day: Option<u32>, cx: &mut Context<Self>) {
        let Some((uuid, open)) = self.planning.take() else {
            return;
        };
        let keyword = open.keyword;
        self.act(&uuid, RowAction::Reschedule { keyword, day }, cx);
    }

    fn planning_actions(&self, uuid: &str, cx: &mut Context<Self>) -> PlanningActions {
        let view = cx.entity();
        let open = self
            .planning
            .as_ref()
            .filter(|(u, _)| u == uuid)
            .map(|(_, o)| *o);
        let today = jiff::Zoned::now();
        let today = bitacora_core::date::Date::new(
            i32::from(today.year()),
            u8::try_from(today.month()).unwrap_or(1),
            u8::try_from(today.day()).unwrap_or(1),
        );
        let uuid = uuid.to_owned();
        let on_open = {
            let (view, uuid) = (view.clone(), uuid);
            std::rc::Rc::new(
                move |kw: &'static str, _: &mut Window, cx: &mut crate::ui::App| {
                    view.update(cx, |v, cx| v.open_planning(&uuid, kw, cx));
                },
            )
        };
        let on_shift = {
            let view = view.clone();
            std::rc::Rc::new(move |delta: i32, _: &mut Window, cx: &mut crate::ui::App| {
                view.update(cx, |v, cx| v.shift_planning(delta, cx));
            })
        };
        let on_pick = {
            let view = view.clone();
            std::rc::Rc::new(
                move |_: &'static str, key: u32, _: &mut Window, cx: &mut crate::ui::App| {
                    view.update(cx, |v, cx| v.reschedule(Some(key), cx));
                },
            )
        };
        let on_clear = {
            let view = view.clone();
            std::rc::Rc::new(
                move |_: &'static str, _: &mut Window, cx: &mut crate::ui::App| {
                    view.update(cx, |v, cx| v.reschedule(None, cx));
                },
            )
        };
        let on_close = std::rc::Rc::new(move |_: &mut Window, cx: &mut crate::ui::App| {
            view.update(cx, |v, cx| v.close_planning(cx));
        });
        PlanningActions {
            open,
            today,
            on_open,
            on_shift,
            on_pick,
            on_clear,
            on_close,
        }
    }

    /// Opens the page of task `uuid` with the block revealed (BIT-US-0187).
    fn open(&mut self, uuid: &str, open: OpenIn, cx: &mut Context<Self>) {
        let Some(task) = self.model.rows.iter().find(|r| r.uuid == uuid) else {
            return;
        };
        cx.emit(PageEvent::open(open_target(task), open));
    }

    fn pills(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let sel = self.selection.clone();
        let model = &self.model;
        let this = cx.entity();
        let mut row = h_flex().flex_wrap().gap(cx.bitacora().metrics.space[3]);
        for class in MarkerClass::ALL_CLASSES {
            let view = this.clone();
            let next = Selection {
                marker: class,
                ..sel.clone()
            };
            row = row.child(
                Pill::new(("tasks-pill", class as usize), class.label())
                    .count(model.count_marker(class, &sel))
                    .active(sel.marker == class)
                    .on_click(move |_, _, cx| {
                        view.update(cx, |v, cx| v.set_selection(next.clone(), cx));
                    }),
            );
        }
        for (ix, letter) in ["A", "B", "C"].into_iter().enumerate() {
            let view = this.clone();
            let active = sel.priority.as_deref() == Some(letter);
            let next = Selection {
                priority: (!active).then(|| letter.to_owned()),
                ..sel.clone()
            };
            row = row.child(
                Pill::new(
                    ("tasks-priority", ix),
                    t!("tasks.priority", letter = letter).to_string(),
                )
                .count(model.count_priority(letter, &sel))
                .active(active)
                .on_click(move |_, _, cx| {
                    view.update(cx, |v, cx| v.set_selection(next.clone(), cx));
                }),
            );
        }
        if let Some(page) = sel.page.clone() {
            let view = this;
            let next = Selection {
                page: None,
                ..sel.clone()
            };
            row = row.child(
                Pill::new("tasks-page", format!("{page} \u{d7}"))
                    .active(true)
                    .on_click(move |_, _, cx| {
                        view.update(cx, |v, cx| v.set_selection(next.clone(), cx));
                    }),
            );
        }
        row
    }

    fn row_card(
        &mut self,
        task: &TaskRow,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.bitacora().clone();
        let ui_theme = cx.theme().clone();
        let host = cx.entity();
        let target = BlockRef {
            page: task.page.clone(),
            uuid: Some(task.uuid.clone()),
            ord: usize::try_from(task.ord).ok(),
            content: None,
            contains: Some(task.title.clone()),
        };
        let remote =
            self.remote
                .prepare_target(&host, &target, &Row::default(), None, None, window, cx);
        let editing = remote.as_ref().is_some_and(|rr| rr.editing);
        let edit_build = remote
            .as_ref()
            .and_then(|rr| rr.edit.as_ref())
            .and_then(|e| e.editing.clone());
        let click: Option<TextHook> =
            remote.and_then(|rr| rr.edit.map(|e| e.on_text).or(rr.activate));
        let c = &theme.colors;
        let uuid = task.uuid.clone();
        let this = cx.entity();
        let editable = self.link.is_some();
        let (done_view, done_uuid) = (this.clone(), uuid.clone());
        let (cycle_view, cycle_uuid) = (this.clone(), uuid.clone());
        let (open_view, open_uuid) = (this.clone(), uuid.clone());
        let (open_block_view, open_block_uuid) = (this.clone(), uuid.clone());
        let (page_view, page_name) = (this, task.page.clone());
        let overdue = task.group == TaskGroup::Overdue;
        let sched = match task.due {
            Some(d) => {
                let key = if task.deadline {
                    "tasks.deadline"
                } else {
                    "tasks.scheduled"
                };
                let mut s = t!(key, date = format_day(d)).to_string();
                if overdue {
                    s.push_str(" \u{b7} ");
                    s.push_str(&t!("tasks.overdue_tag"));
                }
                s
            }
            None => t!("tasks.no_date").to_string(),
        };
        let mut title = h_flex()
            .flex_wrap()
            .items_baseline()
            .gap_x(theme.metrics.space[3]);
        title = title.child(
            div()
                .id(SharedString::from(format!("tasks-marker-{}", task.uuid)))
                .cursor_pointer()
                .when(editable, |d| {
                    d.on_click(move |_, _, cx| {
                        cycle_view.update(cx, |v, cx| v.act(&cycle_uuid, RowAction::Cycle, cx));
                    })
                })
                .child(TaskMarker::new(task.marker)),
        );
        if let Some(p) = &task.priority {
            title = title.child(
                div()
                    .text_color(c.muted)
                    .type_style(&theme.type_scale.mono)
                    .child(format!("[#{p}]")),
            );
        }
        for word in task.title.split_whitespace() {
            let tag = word.starts_with('#');
            title = title.child(
                div()
                    .when(tag, |d| d.text_color(c.accent))
                    .child(word.to_owned()),
            );
        }
        let card = Card::new().surface(Surface::Panel).child(
            h_flex()
                .gap(theme.metrics.space[7])
                .items_start()
                .child(
                    div()
                        .id(SharedString::from(format!("tasks-check-{}", task.uuid)))
                        .debug_selector({
                            let uuid = task.uuid.clone();
                            move || format!("tasks-check-{uuid}")
                        })
                        .flex_shrink_0()
                        .mt(theme.metrics.space[1])
                        .size(theme.metrics.icon_sm)
                        .rounded(theme.metrics.radius_chip)
                        .border_1()
                        .border_color(c.line_2)
                        .when(editable, |d| {
                            d.cursor_pointer()
                                .hover(|s| s.border_color(c.accent))
                                .on_click(move |_, _, cx| {
                                    done_view.update(cx, |v, cx| {
                                        v.act(&done_uuid, RowAction::Complete, cx);
                                    });
                                })
                        }),
                )
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .gap(theme.metrics.space[3])
                        .child(match edit_build {
                            Some(build) => div()
                                .id(SharedString::from(format!("tasks-open-{}", task.uuid)))
                                .type_style(&theme.type_scale.panel_body)
                                .text_color(c.text)
                                .child(build(&ui_theme))
                                .into_any_element(),
                            None => {
                                let area = div()
                                    .id(SharedString::from(format!("tasks-open-{}", task.uuid)))
                                    .cursor_pointer()
                                    .type_style(&theme.type_scale.panel_body)
                                    .text_color(c.text);
                                match click {
                                    // Like Logseq: a click on the text edits the block; Shift
                                    // opens it in the sidebar, Ctrl/Cmd in a new tab.
                                    Some(hook) => {
                                        let open_view = open_view.clone();
                                        let open_uuid = open_uuid.clone();
                                        area.on_mouse_down(
                                            crate::ui::text_edit::MouseButton::Left,
                                            move |ev, window, cx| {
                                                cx.stop_propagation();
                                                let open = OpenIn::from_modifiers(&ev.modifiers);
                                                if open != OpenIn::Main {
                                                    open_view.update(cx, |v, cx| {
                                                        v.open(&open_uuid, open, cx);
                                                    });
                                                } else {
                                                    hook(usize::MAX, false, window, cx);
                                                }
                                            },
                                        )
                                        .child(title)
                                        .into_any_element()
                                    }
                                    None => area
                                        .on_click(move |ev, _, cx| {
                                            let open = OpenIn::from_modifiers(&ev.modifiers());
                                            open_view
                                                .update(cx, |v, cx| v.open(&open_uuid, open, cx));
                                        })
                                        .child(title)
                                        .into_any_element(),
                                }
                            }
                        })
                        .child(
                            h_flex()
                                .flex_wrap()
                                .gap_x(theme.metrics.space[8])
                                .type_style(&theme.type_scale.caption)
                                .text_color(c.muted)
                                .child(
                                    div()
                                        .id(SharedString::from(format!(
                                            "tasks-page-link-{}",
                                            task.uuid
                                        )))
                                        .cursor_pointer()
                                        .hover(|s| s.text_color(c.accent))
                                        .on_click(move |_, _, cx| {
                                            let name = page_name.clone();
                                            page_view.update(cx, |v, cx| {
                                                let next = Selection {
                                                    page: Some(name),
                                                    ..v.selection.clone()
                                                };
                                                v.set_selection(next, cx);
                                            });
                                        })
                                        .child(t!("tasks.in_page", page = task.page).to_string()),
                                )
                                .child(match task.due {
                                    Some(d) => div().when(overdue, |d| d.text_color(c.warn)).child(
                                        PlanningChipView::new(
                                            format!("task-{}", task.uuid),
                                            if task.deadline {
                                                "DEADLINE"
                                            } else {
                                                "SCHEDULED"
                                            },
                                            &format!("<{}>", format_day(d)),
                                            editable.then(|| self.planning_actions(&uuid, cx)),
                                        )
                                        .label(sched),
                                    ),
                                    // No date yet: the label opens the same picker and a pick
                                    // adds `SCHEDULED:` (BIT-US-0187).
                                    None if editable => div().child(
                                        PlanningChipView::new(
                                            format!("task-{}", task.uuid),
                                            "SCHEDULED",
                                            "",
                                            Some(self.planning_actions(&uuid, cx)),
                                        )
                                        .label(sched),
                                    ),
                                    None => div().child(sched),
                                })
                                .child(
                                    div()
                                        .id(SharedString::from(format!(
                                            "tasks-open-block-{}",
                                            task.uuid
                                        )))
                                        .cursor_pointer()
                                        .hover(|s| s.text_color(c.accent))
                                        .on_click(move |ev, _, cx| {
                                            let open = OpenIn::from_modifiers(&ev.modifiers());
                                            open_block_view.update(cx, |v, cx| {
                                                v.open(&open_block_uuid, open, cx)
                                            });
                                        })
                                        .child(t!("tasks.open_block").to_string()),
                                ),
                        ),
                ),
        );
        if editing {
            self.remote.wrap(&task.page, card.into_any_element(), cx)
        } else {
            card.into_any_element()
        }
    }
}

impl Render for TasksView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.bitacora().clone();
        let c = &theme.colors;
        let total = self.model.rows.len();
        let overdue = self.model.overdue();
        let groups: Vec<(TaskGroup, Vec<TaskRow>)> = self
            .model
            .grouped(&self.selection)
            .into_iter()
            .map(|(g, rows)| (g, rows.into_iter().cloned().collect()))
            .collect();
        let mut sections = Vec::new();
        for (group, rows) in groups {
            let title = match group {
                TaskGroup::Overdue => t!("tasks.group_overdue"),
                TaskGroup::ThisWeek => t!("tasks.group_week"),
                TaskGroup::Later => t!("tasks.group_later"),
                TaskGroup::NoDate => t!("tasks.group_none"),
            }
            .to_string();
            let mut section = v_flex().gap(theme.metrics.space[3]).child(
                div().py(theme.metrics.space[3]).child(
                    Overline::new(title)
                        .count(rows.len())
                        .warn(group == TaskGroup::Overdue),
                ),
            );
            for row in &rows {
                section = section.child(self.row_card(row, window, cx));
            }
            sections.push(section);
        }
        let empty = sections.is_empty() && self.loaded;
        div()
            .id("tasks-view")
            .size_full()
            .bg(c.bg)
            .overflow_y_scroll()
            .child(
                v_flex()
                    .max_w(theme.metrics.tasks_max)
                    .mx_auto()
                    .px(theme.metrics.reading_pad_x)
                    .pt(theme.metrics.reading_pad_top)
                    .pb(theme.metrics.reading_pad_bottom)
                    .gap(theme.metrics.space[10])
                    .child(
                        v_flex()
                            .gap(theme.metrics.space[2])
                            .child(
                                div()
                                    .type_style(&theme.type_scale.page_title)
                                    .text_color(c.text)
                                    .child(t!("tasks.title").to_string()),
                            )
                            .child(
                                h_flex()
                                    .gap(theme.metrics.space[3])
                                    .type_style(&theme.type_scale.ui)
                                    .text_color(c.text_2)
                                    .child(t!("tasks.summary", count = total).to_string())
                                    .when(overdue > 0, |d| {
                                        d.child(div().text_color(c.warn).child(format!(
                                            "\u{b7} {}",
                                            t!("tasks.overdue_count", count = overdue)
                                        )))
                                    }),
                            ),
                    )
                    .child(self.pills(cx))
                    .children(sections)
                    .when(empty, |d| {
                        d.child(
                            div()
                                .type_style(&theme.type_scale.ui)
                                .text_color(c.muted)
                                .child(t!("tasks.empty").to_string()),
                        )
                    }),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TestGraph;
    use crate::ui::testing::{TestAppContext, gpui_test};
    use crate::{settings::AppSettings, theme};
    use bitacora_runtime::{RuntimeConfig, Session};
    use std::sync::Arc;

    const TODAY: i64 = 20_261_007;

    const FILES: [(&str, &str); 2] = [
        (
            "pages/Work.md",
            "- TODO [#A] write report #Astara\n  SCHEDULED: <2026-10-06 Tue>\n\
             - DOING review\n  DEADLINE: <2026-10-08 Thu>\n\
             - DONE shipped\n  SCHEDULED: <2026-10-06 Tue>\n\
             - LATER someday\n\
             - TODO far away\n  SCHEDULED: <2026-12-01 Tue>\n\
             - TODO past\n  SCHEDULED: <2026-09-30 Wed>\n",
        ),
        ("pages/Home.md", "- TODO [#B] water plants\n"),
    ];

    fn model() -> TaskModel {
        let g = TestGraph::new(&FILES);
        load_tasks(&g.handle, TODAY).expect("tasks")
    }

    fn titles(groups: &[(TaskGroup, Vec<&TaskRow>)], group: TaskGroup) -> Vec<String> {
        groups
            .iter()
            .filter(|(g, _)| *g == group)
            .flat_map(|(_, rows)| rows.iter().map(|r| r.title.clone()))
            .collect()
    }

    #[test]
    fn groups_and_pill_counts_match_the_fixture() {
        let m = model();
        let sel = Selection::default();
        let groups = m.grouped(&sel);
        assert_eq!(
            titles(&groups, TaskGroup::Overdue),
            ["past", "write report #Astara"]
        );
        assert_eq!(titles(&groups, TaskGroup::ThisWeek), ["review"]);
        assert_eq!(titles(&groups, TaskGroup::Later), ["far away"]);
        assert_eq!(
            titles(&groups, TaskGroup::NoDate),
            ["water plants", "someday"]
        );
        assert_eq!(m.rows.len(), 6, "DONE is not listed");
        assert_eq!(m.overdue(), 2);
        let count = |c| m.count_marker(c, &sel);
        assert_eq!(count(MarkerClass::All), 6);
        assert_eq!(count(MarkerClass::Overdue), 2);
        assert_eq!(count(MarkerClass::InProgress), 1);
        assert_eq!(count(MarkerClass::Todo), 4);
        assert_eq!(count(MarkerClass::Later), 1);
        assert_eq!(m.count_priority("A", &sel), 1);
        assert_eq!(m.count_priority("B", &sel), 1);
        assert_eq!(m.count_priority("C", &sel), 0);
    }

    #[test]
    fn selection_narrows_rows_and_facet_counts() {
        let m = model();
        let sel = Selection {
            marker: MarkerClass::Todo,
            page: Some("Work".into()),
            ..Selection::default()
        };
        let rows: usize = m.grouped(&sel).iter().map(|(_, r)| r.len()).sum();
        assert_eq!(rows, 3);
        // Facet counts respect the other dimensions: Home's task is outside page Work.
        assert_eq!(m.count_priority("B", &sel), 0);
        assert_eq!(m.count_marker(MarkerClass::All, &sel), 5);
    }

    #[test]
    fn rescheduling_the_last_overdue_task_resets_the_stale_overdue_filter() {
        // Regression for BIT-US-0173: with the Overdue pill selected, moving the last overdue
        // task out of it left a filter keeping zero rows (the list looked emptied).
        let before = model();
        let sel = Selection {
            marker: MarkerClass::Overdue,
            page: Some("Work".into()),
            ..Selection::default()
        };
        assert_eq!(sel.reconciled(&before), sel, "still matches rows");
        let after = TaskModel {
            rows: before
                .rows
                .iter()
                .filter(|r| r.group != TaskGroup::Overdue)
                .cloned()
                .collect(),
        };
        assert!(after.grouped(&sel).is_empty(), "the stale filter hides all");
        let fixed = sel.reconciled(&after);
        assert_eq!(fixed.marker, MarkerClass::All);
        assert_eq!(fixed.page.as_deref(), Some("Work"));
        assert!(!after.grouped(&fixed).is_empty());
        let gone = Selection {
            page: Some("Nowhere".into()),
            priority: Some("C".into()),
            ..Selection::default()
        };
        assert_eq!(gone.reconciled(&after), Selection::default());
    }

    #[test]
    fn the_open_button_targets_the_page_with_the_block() {
        let m = model();
        let row = m
            .rows
            .iter()
            .find(|r| r.title == "water plants")
            .expect("row");
        assert_eq!(
            open_target(row),
            NavTarget::PageAt {
                page: "Home".into(),
                block: row.uuid.clone()
            }
        );
    }

    #[test]
    fn the_picker_opens_on_the_due_month_or_the_current_one() {
        let m = picker_month(Some(20_261_207), 2026, 10);
        assert_eq!((m.year, m.month), (2026, 12));
        let m = picker_month(None, 2026, 10);
        assert_eq!((m.year, m.month), (2026, 10));
    }

    #[test]
    fn picking_a_day_for_an_undated_task_adds_a_scheduled_line() {
        let (y, m, d) = planning::split_key(20_261_020);
        assert_eq!(
            move_planning_date("TODO write", planning::SCHEDULED, y, m, d),
            "TODO write\nSCHEDULED: <2026-10-20 Tue>"
        );
    }

    #[test]
    fn dates_are_formatted_as_iso_days() {
        assert_eq!(format_day(20_261_007), "2026-10-07");
    }

    #[gpui_test]
    fn the_view_loads_and_filters(cx: &mut TestAppContext) {
        cx.update(|cx| {
            crate::ui::init(cx);
            theme::install(cx, AppSettings::default(), None);
        });
        let g = TestGraph::new(&FILES);
        let (view, cx) = cx.add_window_view(|_, cx| TasksView::new(cx));
        view.update(cx, |v, cx| v.show(g.handle.clone(), cx));
        cx.executor().allow_parking();
        for _ in 0..400 {
            cx.run_until_parked();
            if view.read_with(cx, |v, _| v.is_loaded()) {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        let total = view.read_with(cx, |v, _| v.model().rows.len());
        assert!(total >= 4, "loaded {total} tasks");
        view.update(cx, |v, cx| {
            v.set_selection(
                Selection {
                    marker: MarkerClass::Later,
                    ..Selection::default()
                },
                cx,
            );
        });
        cx.run_until_parked();
        assert_eq!(
            view.read_with(cx, |v, _| v.selection().marker),
            MarkerClass::Later
        );
    }

    fn session(files: &[(&str, &str)]) -> (tempfile::TempDir, tempfile::TempDir, Session) {
        let graph = tempfile::tempdir().expect("graph");
        let data = tempfile::tempdir().expect("data");
        std::fs::create_dir_all(graph.path().join("logseq")).expect("logseq");
        std::fs::write(graph.path().join("logseq/config.edn"), "{}").expect("config");
        for (path, content) in files {
            let file = graph.path().join(path);
            std::fs::create_dir_all(file.parent().expect("parent")).expect("dirs");
            std::fs::write(file, content).expect("write");
        }
        let mut cfg = RuntimeConfig::new(graph.path());
        cfg.data_dir = Some(data.path().to_path_buf());
        cfg.global_config = Some(data.path().join("no-global.edn"));
        cfg.watch = None;
        cfg.debounce = None;
        let session = Session::open(cfg).expect("session");
        (graph, data, session)
    }

    #[test]
    fn rescheduling_rewrites_only_the_date_and_is_undoable() {
        let src = "- TODO first\n  SCHEDULED: <2026-10-06 Tue .+1d>\n  id:: 11111111-1111-1111-1111-111111111111\n- TODO second\n  DEADLINE: <2026-10-09 Fri>\n";
        let (graph, _data, session) = session(&[("pages/Work.md", src)]);
        let handle = GraphHandle {
            reader: session.read_api(),
            root: session.root().to_path_buf(),
            settings: Arc::new(crate::data::ViewSettings::from_config(session.config())),
        };
        let link = SessionLink {
            queue: session.queue().clone(),
            config: Arc::new(session.config().clone()),
            mcp_endpoint: None,
            gate: Arc::default(),
            lookup: session.ref_lookup(),
            hybrid: None,
        };
        let model = load_tasks(&handle, TODAY).expect("tasks");
        let first = model
            .rows
            .iter()
            .find(|r| r.title == "first")
            .expect("first");
        let second = model
            .rows
            .iter()
            .find(|r| r.title == "second")
            .expect("second");
        let read = || {
            let _ = link.queue.flush(Source::Ui).expect("flush");
            std::fs::read_to_string(graph.path().join("pages/Work.md")).expect("read")
        };
        let day = Some(20_261_015);
        apply_action(
            &link,
            &handle,
            first,
            RowAction::Reschedule {
                keyword: planning::SCHEDULED,
                day,
            },
        )
        .expect("reschedule");
        assert_eq!(
            read(),
            src.replace("2026-10-06 Tue", "2026-10-15 Thu"),
            "the repeater and the other lines are untouched"
        );
        apply_action(
            &link,
            &handle,
            second,
            RowAction::Reschedule {
                keyword: planning::DEADLINE,
                day: None,
            },
        )
        .expect("remove");
        assert!(!read().contains("DEADLINE"));
        link.queue.undo(Source::Ui).expect("queue").expect("undo");
        link.queue.undo(Source::Ui).expect("queue").expect("undo");
        assert_eq!(read(), src);
        let _ = session.shutdown(Duration::from_secs(10));
    }

    #[test]
    fn completing_a_task_goes_through_the_queue_and_is_undoable() {
        let (graph, _data, session) = session(&[
            (
                "pages/Work.md",
                "title:: Work\n\n- TODO first\n- TODO second\n",
            ),
            ("pages/Other.md", "- TODO third\n"),
        ]);
        let handle = GraphHandle {
            reader: session.read_api(),
            root: session.root().to_path_buf(),
            settings: Arc::new(crate::data::ViewSettings::from_config(session.config())),
        };
        let link = SessionLink {
            queue: session.queue().clone(),
            config: Arc::new(session.config().clone()),
            mcp_endpoint: None,
            gate: Arc::default(),
            lookup: session.ref_lookup(),
            hybrid: None,
        };
        let model = load_tasks(&handle, TODAY).expect("tasks");
        let second = model
            .rows
            .iter()
            .find(|r| r.title == "second")
            .expect("second")
            .clone();
        apply_action(&link, &handle, &second, RowAction::Complete).expect("complete");
        let _ = link.queue.flush(Source::Ui).expect("flush");
        let disk = std::fs::read_to_string(graph.path().join("pages/Work.md")).expect("read");
        assert!(disk.contains("- DONE second"), "{disk}");
        assert!(
            disk.contains("- TODO first"),
            "only the clicked block changed"
        );
        let cycled = model
            .rows
            .iter()
            .find(|r| r.title == "first")
            .expect("first");
        apply_action(&link, &handle, cycled, RowAction::Cycle).expect("cycle");
        let _ = link.queue.flush(Source::Ui).expect("flush");
        let disk = std::fs::read_to_string(graph.path().join("pages/Work.md")).expect("read");
        assert!(disk.contains("- DOING first"), "{disk}");
        // Undo reverts the last transaction (the cycle), then the completion.
        link.queue
            .undo(Source::Ui)
            .expect("queue")
            .expect("undo cycle");
        link.queue
            .undo(Source::Ui)
            .expect("queue")
            .expect("undo done");
        let _ = link.queue.flush(Source::Ui).expect("flush");
        let disk = std::fs::read_to_string(graph.path().join("pages/Work.md")).expect("read");
        assert!(
            disk.contains("- TODO second") && disk.contains("- TODO first"),
            "{disk}"
        );
        let _ = session.shutdown(Duration::from_secs(10));
    }

    /// A live session over `files` and the view-side handles to it.
    fn live(
        files: &[(&str, &str)],
    ) -> (
        tempfile::TempDir,
        tempfile::TempDir,
        Session,
        GraphHandle,
        SessionLink,
    ) {
        let (graph, data, session) = session(files);
        let handle = GraphHandle {
            reader: session.read_api(),
            root: session.root().to_path_buf(),
            settings: Arc::new(crate::data::ViewSettings::from_config(session.config())),
        };
        let link = SessionLink {
            queue: session.queue().clone(),
            config: Arc::new(session.config().clone()),
            mcp_endpoint: None,
            gate: Arc::default(),
            lookup: session.ref_lookup(),
            hybrid: None,
        };
        (graph, data, session, handle, link)
    }

    #[gpui_test]
    fn clicking_the_checkbox_completes_the_task(cx: &mut TestAppContext) {
        cx.update(|cx| {
            crate::ui::init(cx);
            theme::install(cx, AppSettings::default(), None);
        });
        // Both pages have blocks at the same `ord`: the rows must still be told apart.
        let (graph, _data, session, handle, link) = live(&[
            ("pages/Work.md", "- TODO first\n- TODO second\n"),
            ("pages/Home.md", "- TODO third\n- TODO fourth\n"),
        ]);
        let (view, cx) = cx.add_window_view(|_, cx| TasksView::new(cx));
        view.update(cx, |v, cx| {
            v.set_session_link(Some(link.clone()), cx);
            v.show(handle, cx);
        });
        cx.executor().allow_parking();
        for _ in 0..400 {
            cx.run_until_parked();
            if view.read_with(cx, |v, _| v.is_loaded()) {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
        for (page, title, done) in [
            ("Work", "second", "- DONE second"),
            ("Home", "third", "- DONE third"),
            ("Work", "first", "- DONE first"),
            ("Home", "fourth", "- DONE fourth"),
        ] {
            let uuid = view
                .read_with(cx, |v, _| {
                    v.model()
                        .rows
                        .iter()
                        .find(|r| r.title == title)
                        .map(|r| r.uuid.clone())
                })
                .expect("the task is listed");
            let selector: &'static str = Box::leak(format!("tasks-check-{uuid}").into_boxed_str());
            let bounds = cx.debug_bounds(selector).expect("the checkbox is painted");
            cx.simulate_click(bounds.center(), Default::default());
            let file = graph.path().join(format!("pages/{page}.md"));
            let mut disk = String::new();
            for _ in 0..200 {
                cx.run_until_parked();
                let _ = link.queue.flush(Source::Ui);
                disk = std::fs::read_to_string(&file).unwrap_or_default();
                if disk.contains(done) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            assert!(disk.contains(done), "{title}: {disk}");
        }
        let _ = session.shutdown(Duration::from_secs(10));
    }
}
