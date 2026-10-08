//! The read-only "what is on today" panel under today's journal (BIT-US-0178): tasks scheduled
//! or due today, tasks for tomorrow and tasks in progress. Shown in the journals feed (today's
//! entry) and in today's page view; each section is hidden when empty.

use std::collections::HashSet;
use std::time::Duration;

use bitacora_index::{AgendaItem, TaskFilter, TaskItem};
use bitacora_markdown::tasks::head::Marker;
use rust_i18n::t;

use crate::data::{self, GraphHandle};
use crate::nav::OpenIn;
use crate::render::inline::NavTarget;
use crate::ui::theme::ActiveBitacoraTheme as _;
use crate::ui::{
    ActiveTheme as _, Context, EventEmitter, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, StatefulInteractiveElement as _, Styled as _, Task, Window, div,
    h_flex, v_flex,
};
use crate::views::kit::{Overline, TaskMarker};
use crate::views::page_view::PageEvent;

/// Pause after the last index event before the panel is reloaded.
const REFRESH_DEBOUNCE: Duration = Duration::from_millis(250);

/// One task line of the panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelItem {
    /// Index uuid of the block (navigation target).
    pub uuid: String,
    /// The task marker.
    pub marker: Marker,
    /// Task text without the marker.
    pub title: String,
    /// Title of the page that holds the task.
    pub page: String,
}

impl PanelItem {
    fn from_task(task: &TaskItem) -> Option<Self> {
        Some(Self {
            uuid: task.block.uuid.clone(),
            marker: Marker::from_word(task.block.marker.as_deref()?)?,
            title: task.block.title.clone(),
            page: task.page_name.clone(),
        })
    }
}

/// The three sections; empty ones are not drawn.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Sections {
    /// Open tasks scheduled or due today.
    pub today: Vec<PanelItem>,
    /// Open tasks scheduled or due tomorrow.
    pub tomorrow: Vec<PanelItem>,
    /// Open tasks marked `DOING` or `NOW`, any date.
    pub doing: Vec<PanelItem>,
}

impl Sections {
    /// Nothing to show.
    pub fn is_empty(&self) -> bool {
        self.today.is_empty() && self.tomorrow.is_empty() && self.doing.is_empty()
    }
}

/// Builds the sections from the agenda of `[today, tomorrow]` and the in-progress tasks
/// (`yyyyMMdd` days). A task listed under "Doing" is not repeated in a date section.
pub fn build_sections(
    agenda: &[AgendaItem],
    doing: &[TaskItem],
    today: i64,
    tomorrow: i64,
) -> Sections {
    let mut out = Sections::default();
    let mut seen: HashSet<&str> = HashSet::new();
    for task in doing {
        if is_in_progress(task) && seen.insert(task.block.uuid.as_str()) {
            out.doing.extend(PanelItem::from_task(task));
        }
    }
    for item in agenda {
        let bucket = if item.day == today {
            &mut out.today
        } else if item.day == tomorrow {
            &mut out.tomorrow
        } else {
            continue;
        };
        if seen.insert(item.task.block.uuid.as_str()) {
            bucket.extend(PanelItem::from_task(&item.task));
        }
    }
    out
}

fn is_in_progress(task: &TaskItem) -> bool {
    matches!(task.block.marker.as_deref(), Some("DOING" | "NOW"))
}

/// Reads the panel from the index (background thread). `today` is `yyyyMMdd`.
pub fn load_sections(handle: &GraphHandle, today: i64) -> Result<Sections, String> {
    let tomorrow = handle
        .reader
        .add_days(today, 1)
        .map_err(|e| e.to_string())?;
    let agenda = handle.reader.agenda(today, 1).map_err(|e| e.to_string())?;
    let filter = TaskFilter {
        markers: vec!["DOING".to_owned(), "NOW".to_owned()],
        ..TaskFilter::default()
    };
    let doing = handle.reader.tasks(&filter).map_err(|e| e.to_string())?;
    Ok(build_sections(&agenda, &doing, today, tomorrow))
}

/// The panel view.
#[derive(Debug)]
pub struct TodayPanel {
    handle: Option<GraphHandle>,
    sections: Sections,
    load_task: Option<Task<()>>,
    refresh_task: Option<Task<()>>,
}

impl EventEmitter<PageEvent> for TodayPanel {}

impl TodayPanel {
    /// An empty panel.
    pub fn new() -> Self {
        Self {
            handle: None,
            sections: Sections::default(),
            load_task: None,
            refresh_task: None,
        }
    }

    /// The loaded sections.
    pub fn sections(&self) -> &Sections {
        &self.sections
    }

    /// Points the panel at a graph and loads it.
    pub fn show(&mut self, handle: GraphHandle, cx: &mut Context<Self>) {
        self.handle = Some(handle);
        self.reload(cx);
    }

    /// Reads the sections on a background thread.
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
                .spawn(async move { load_sections(&handle, today) })
                .await;
            let _ = this.update(cx, |view, cx| match result {
                Ok(sections) => {
                    if view.sections != sections {
                        view.sections = sections;
                        cx.notify();
                    }
                }
                Err(message) => tracing::warn!("cannot load the today panel: {message}"),
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
}

impl Default for TodayPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl Render for TodayPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let bt = cx.bitacora().clone();
        let this = cx.entity();
        let mut col = v_flex().w_full().gap(bt.metrics.space[4]);
        if self.sections.is_empty() {
            return col;
        }
        let parts = [
            (
                "today-panel-today",
                t!("journals.panel_today").to_string(),
                &self.sections.today,
            ),
            (
                "today-panel-tomorrow",
                t!("journals.panel_tomorrow").to_string(),
                &self.sections.tomorrow,
            ),
            (
                "today-panel-doing",
                t!("journals.panel_doing").to_string(),
                &self.sections.doing,
            ),
        ];
        for (tag, title, items) in parts {
            if items.is_empty() {
                continue;
            }
            let mut section = v_flex()
                .gap(bt.metrics.space[2])
                .child(Overline::new(title).count(items.len()));
            for (i, item) in items.iter().enumerate() {
                let uuid = item.uuid.clone();
                let view = this.clone();
                section = section.child(
                    h_flex()
                        .id((tag, i))
                        .gap(bt.metrics.space[3])
                        .items_baseline()
                        .cursor_pointer()
                        .on_click(move |_, window, cx| {
                            let open = OpenIn::from_modifiers(&window.modifiers());
                            view.update(cx, |_, cx| {
                                cx.emit(PageEvent::open(NavTarget::Block(uuid.clone()), open));
                            });
                        })
                        .child(TaskMarker::new(item.marker))
                        .child(div().flex_1().min_w_0().child(item.title.clone()))
                        .child(
                            div()
                                .text_sm()
                                .text_color(theme.muted_foreground)
                                .child(item.page.clone()),
                        ),
                );
            }
            col = col.child(section);
        }
        col.pt(bt.metrics.space[5])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TestGraph;

    fn graph() -> TestGraph {
        TestGraph::new(&[(
            "pages/Work.md",
            "- TODO due today\n  SCHEDULED: <2026-10-08 Thu>\n\
             - TODO deadline today\n  DEADLINE: <2026-10-08 Thu>\n\
             - TODO for tomorrow\n  SCHEDULED: <2026-10-09 Fri>\n\
             - DOING busy and scheduled\n  SCHEDULED: <2026-10-08 Thu>\n\
             - NOW undated\n\
             - DOING far away\n  DEADLINE: <2026-12-01 Tue>\n\
             - TODO next week\n  SCHEDULED: <2026-10-15 Thu>\n\
             - DONE finished\n  SCHEDULED: <2026-10-08 Thu>\n\
             - TODO plain\n",
        )])
    }

    #[test]
    fn sections_split_by_day_and_doing_wins() {
        let g = graph();
        let s = load_sections(&g.handle, 20_261_008).expect("sections");
        let titles = |v: &[PanelItem]| v.iter().map(|i| i.title.clone()).collect::<Vec<_>>();
        let mut today = titles(&s.today);
        today.sort();
        assert_eq!(today, ["deadline today", "due today"]);
        assert_eq!(titles(&s.tomorrow), ["for tomorrow"]);
        let mut doing = titles(&s.doing);
        doing.sort();
        assert_eq!(doing, ["busy and scheduled", "far away", "undated"]);
        assert!(s.doing.iter().all(|i| i.page == "Work"));
        assert!(s.doing.iter().any(|i| i.marker == Marker::Now));
    }

    #[test]
    fn empty_sections_are_empty() {
        assert!(build_sections(&[], &[], 20_261_008, 20_261_009).is_empty());
        let g = TestGraph::new(&[("pages/A.md", "- just text\n- TODO later\n")]);
        let s = load_sections(&g.handle, 20_261_008).expect("sections");
        assert!(s.is_empty());
    }
}
