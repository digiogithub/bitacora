//! The Tasks view (BIT-US-0126, design `mockups/Tareas.dc.html`): every open task of the graph
//! grouped Overdue / This week / Later / No date, with filter pills (marker class, priority,
//! page) that show counts, in a column capped at the `tasks_max` metric (860px).
//!
//! Queries come from `bitacora-index` ([`bitacora_index::IndexReader::task_groups`]). A row opens
//! its block, the checkbox completes it and the marker cycles it; both go through the core
//! command queue as undoable `Cmd`s (single writer).

use std::time::Duration;

use bitacora_core::editor::Cmd;
use bitacora_core::queue::Source;
use bitacora_index::{TaskFilter, TaskGroup, TaskGroups, TaskItem};
use bitacora_markdown::tasks::head::Marker;
use rust_i18n::t;

use crate::data::{self, GraphHandle};
use crate::editor;
use crate::nav::OpenIn;
use crate::render::inline::NavTarget;
use crate::session::SessionLink;
use crate::ui::theme::{ActiveBitacoraTheme as _, TypeStyleExt as _};
use crate::ui::{
    Context, EventEmitter, FluentBuilder as _, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, StatefulInteractiveElement as _, Styled as _, Task, Window, div,
    h_flex, v_flex,
};
use crate::views::kit::{Card, Overline, Pill, Surface, TaskMarker};
use crate::views::page_view::PageEvent;

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
            rows.extend(
                groups
                    .get(group)
                    .iter()
                    .filter_map(|i| TaskRow::from_item(i, group)),
            );
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
    };
    let label = match action {
        RowAction::Complete => "Complete task",
        RowAction::Cycle => "Cycle task marker",
    };
    link.queue
        .run(Source::Ui, label, cmd)
        .map(|_| ())
        .map_err(|e| e.to_string())
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
        }
    }

    /// Connects the view to the live session so rows can be completed.
    pub fn set_session_link(&mut self, link: Option<SessionLink>, cx: &mut Context<Self>) {
        self.link = link;
        cx.notify();
    }

    /// Shows the tasks of a graph.
    pub fn show(&mut self, handle: GraphHandle, cx: &mut Context<Self>) {
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
                    view.model = model;
                    cx.notify();
                }
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

    fn open(&mut self, uuid: &str, open: OpenIn, cx: &mut Context<Self>) {
        cx.emit(PageEvent::open(NavTarget::Block(uuid.to_owned()), open));
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

    fn row_card(&self, task: &TaskRow, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.bitacora().clone();
        let c = &theme.colors;
        let uuid = task.uuid.clone();
        let this = cx.entity();
        let editable = self.link.is_some();
        let (done_view, done_uuid) = (this.clone(), uuid.clone());
        let (cycle_view, cycle_uuid) = (this.clone(), uuid.clone());
        let (open_view, open_uuid) = (this.clone(), uuid.clone());
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
                .id(("tasks-marker", task.ord as usize))
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
        Card::new().surface(Surface::Panel).child(
            h_flex()
                .gap(theme.metrics.space[7])
                .items_start()
                .child(
                    div()
                        .id(("tasks-check", task.ord as usize))
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
                        .child(
                            div()
                                .id(("tasks-open", task.ord as usize))
                                .cursor_pointer()
                                .type_style(&theme.type_scale.panel_body)
                                .text_color(c.text)
                                .on_click(move |ev, _, cx| {
                                    let open = OpenIn::from_shift(ev.modifiers().shift);
                                    open_view.update(cx, |v, cx| v.open(&open_uuid, open, cx));
                                })
                                .child(title),
                        )
                        .child(
                            h_flex()
                                .flex_wrap()
                                .gap_x(theme.metrics.space[8])
                                .type_style(&theme.type_scale.caption)
                                .text_color(c.muted)
                                .child(
                                    div()
                                        .id(("tasks-page-link", task.ord as usize))
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
                                .child(div().when(overdue, |d| d.text_color(c.warn)).child(sched)),
                        ),
                ),
        )
    }
}

impl Render for TasksView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.bitacora().clone();
        let c = &theme.colors;
        let total = self.model.rows.len();
        let overdue = self.model.overdue();
        let groups = self.model.grouped(&self.selection);
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
            for row in rows {
                section = section.child(self.row_card(row, cx));
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
}
