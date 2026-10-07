//! The "All pages" table (BIT-US-0079, BIT-T-0197): name, backlinks, created, updated and a
//! journal flag; sortable by clicking a header, filterable by name, journals and built-ins hidden
//! by default. Built on GPUI Kit's `DataTable`.

use crate::views::dims;
use std::rc::Rc;
use std::time::Duration;

use rust_i18n::t;

use crate::actions::OpenSelectedPage;
use crate::data::{self, GraphHandle, PageItem};
use crate::nav::OpenIn;
use crate::render::inline::NavTarget;
use crate::ui::button::{Button, ButtonVariants as _};
use crate::ui::input::{Input, InputEvent, InputState};
use crate::ui::table::{Column, ColumnSort, DataTable, TableDelegate, TableEvent, TableState};
use crate::ui::{
    ActiveTheme as _, App, AppContext as _, Context, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, Selectable as _,
    Sizable as _, StatefulInteractiveElement as _, Styled as _, Subscription, Task, Window, div,
    h_flex, v_flex,
};
use crate::views::page_view::PageEvent;

/// Debounce between an index change and the table reload.
const REFRESH_DEBOUNCE: Duration = Duration::from_millis(300);

const COL_NAME: usize = 0;
const COL_BACKLINKS: usize = 1;
const COL_CREATED: usize = 2;
const COL_UPDATED: usize = 3;
const COL_JOURNAL: usize = 4;

type OpenPage = Rc<dyn Fn(String, OpenIn, &mut App)>;

/// Rows, columns and sort state behind the table.
pub struct PagesDelegate {
    items: Vec<PageItem>,
    visible: Vec<usize>,
    filter: String,
    sort: (usize, ColumnSort),
    columns: Vec<Column>,
    open: OpenPage,
}

impl std::fmt::Debug for PagesDelegate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PagesDelegate")
            .field("rows", &self.visible.len())
            .finish_non_exhaustive()
    }
}

impl PagesDelegate {
    fn new(open: OpenPage) -> Self {
        let columns = vec![
            Column::new("name", t!("all_pages.name").to_string())
                .width(dims::PX_260)
                .ascending(),
            Column::new("backlinks", t!("all_pages.backlinks").to_string())
                .width(dims::PX_90)
                .text_right()
                .sortable(),
            Column::new("created", t!("all_pages.created").to_string())
                .width(dims::PX_100)
                .sortable(),
            Column::new("updated", t!("all_pages.updated").to_string())
                .width(dims::PX_100)
                .sortable(),
            Column::new("journal", t!("all_pages.journal").to_string())
                .width(dims::PX_80)
                .sortable(),
        ];
        Self {
            items: Vec::new(),
            visible: Vec::new(),
            filter: String::new(),
            sort: (COL_NAME, ColumnSort::Ascending),
            columns,
            open,
        }
    }

    /// Replaces the data and re-applies the filter and the sort.
    pub fn set_items(&mut self, items: Vec<PageItem>) {
        self.items = items;
        self.apply();
    }

    /// Sets the (case-insensitive substring) name filter.
    pub fn set_filter(&mut self, filter: &str) {
        self.filter = filter.trim().to_lowercase();
        self.apply();
    }

    fn apply(&mut self) {
        let items = &self.items;
        let mut visible: Vec<usize> = (0..items.len())
            .filter(|&i| {
                self.filter.is_empty() || items[i].name.to_lowercase().contains(&self.filter)
            })
            .collect();
        let (col, order) = self.sort;
        visible.sort_by(|&a, &b| {
            let (a, b) = (&items[a], &items[b]);
            let by_name = || a.name.to_lowercase().cmp(&b.name.to_lowercase());
            let ord = match col {
                COL_BACKLINKS => a.backlinks.cmp(&b.backlinks).then_with(by_name),
                COL_CREATED => a.created.cmp(&b.created).then_with(by_name),
                COL_UPDATED => a.updated.cmp(&b.updated).then_with(by_name),
                COL_JOURNAL => a.is_journal.cmp(&b.is_journal).then_with(by_name),
                _ => by_name(),
            };
            if order == ColumnSort::Descending {
                ord.reverse()
            } else {
                ord
            }
        });
        self.visible = visible;
    }

    /// Names of the rows shown, in order.
    pub fn visible_names(&self) -> Vec<&str> {
        self.visible
            .iter()
            .map(|&i| self.items[i].name.as_str())
            .collect()
    }

    fn item(&self, row_ix: usize) -> Option<&PageItem> {
        self.visible.get(row_ix).and_then(|&i| self.items.get(i))
    }
}

impl TableDelegate for PagesDelegate {
    fn columns_count(&self, _: &App) -> usize {
        self.columns.len()
    }

    fn rows_count(&self, _: &App) -> usize {
        self.visible.len()
    }

    fn column(&self, col_ix: usize, _: &App) -> Column {
        self.columns[col_ix].clone()
    }

    fn perform_sort(
        &mut self,
        col_ix: usize,
        sort: ColumnSort,
        _: &mut Window,
        _: &mut Context<TableState<Self>>,
    ) {
        // Back to "default" means the name order the list started with.
        self.sort = match sort {
            ColumnSort::Default => (COL_NAME, ColumnSort::Ascending),
            other => (col_ix, other),
        };
        self.apply();
    }

    fn render_td(
        &mut self,
        row_ix: usize,
        col_ix: usize,
        _: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let Some(item) = self.item(row_ix) else {
            return div().into_any_element();
        };
        match col_ix {
            COL_NAME => {
                let open = self.open.clone();
                let name = item.name.clone();
                div()
                    .id(("page-name", row_ix))
                    .cursor_pointer()
                    .text_color(cx.theme().info)
                    .child(item.name.clone())
                    .on_click(move |_, window, cx| {
                        open(
                            name.clone(),
                            OpenIn::from_modifiers(&window.modifiers()),
                            cx,
                        );
                    })
                    .into_any_element()
            }
            COL_BACKLINKS => div().child(item.backlinks.to_string()).into_any_element(),
            COL_CREATED => div()
                .child(data::format_day(item.created))
                .into_any_element(),
            COL_UPDATED => div()
                .child(data::format_day(item.updated))
                .into_any_element(),
            _ => div()
                .child(if item.is_journal { "\u{2713}" } else { "" })
                .into_any_element(),
        }
    }
}

/// The all-pages view.
pub struct AllPagesView {
    table: Entity<TableState<PagesDelegate>>,
    filter: Entity<InputState>,
    handle: Option<GraphHandle>,
    show_journals: bool,
    loaded: bool,
    focus: FocusHandle,
    load_task: Option<Task<()>>,
    refresh_task: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl std::fmt::Debug for AllPagesView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AllPagesView")
            .field("show_journals", &self.show_journals)
            .finish_non_exhaustive()
    }
}

impl EventEmitter<PageEvent> for AllPagesView {}

impl AllPagesView {
    /// Creates the (empty) table.
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let this = cx.entity();
        let open: OpenPage = Rc::new(move |name, open_in, cx| {
            this.update(cx, |_, cx| {
                cx.emit(PageEvent::open(NavTarget::Page(name), open_in));
            });
        });
        let table = cx.new(|cx| {
            TableState::new(PagesDelegate::new(open), window, cx)
                .col_resizable(true)
                .col_movable(false)
        });
        let filter = cx
            .new(|cx| InputState::new(window, cx).placeholder(t!("all_pages.filter").to_string()));
        let subscriptions = vec![
            cx.subscribe(&filter, |this, input, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    let text = input.read(cx).value();
                    this.table.update(cx, |t, cx| {
                        t.delegate_mut().set_filter(&text);
                        t.refresh(cx);
                    });
                }
            }),
            cx.subscribe_in(&table, window, |this, _, event: &TableEvent, window, cx| {
                if let TableEvent::DoubleClickedRow(ix) = event {
                    this.open_row(*ix, OpenIn::from_modifiers(&window.modifiers()), cx);
                }
            }),
        ];
        Self {
            table,
            filter,
            handle: None,
            show_journals: false,
            loaded: false,
            focus: cx.focus_handle(),
            load_task: None,
            refresh_task: None,
            _subscriptions: subscriptions,
        }
    }

    /// Whether the list was loaded at least once.
    pub fn is_loaded(&self) -> bool {
        self.loaded
    }

    /// Names shown, in table order.
    pub fn visible_names(&self, cx: &App) -> Vec<String> {
        self.table
            .read(cx)
            .delegate()
            .visible_names()
            .into_iter()
            .map(str::to_owned)
            .collect()
    }

    /// Whether journal pages are listed.
    pub fn shows_journals(&self) -> bool {
        self.show_journals
    }

    /// The table state (tests).
    pub fn table(&self) -> &Entity<TableState<PagesDelegate>> {
        &self.table
    }

    /// Shows the list for a graph.
    pub fn show(&mut self, handle: GraphHandle, cx: &mut Context<Self>) {
        self.handle = Some(handle);
        self.reload(cx);
    }

    /// Forgets the graph.
    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.handle = None;
        self.loaded = false;
        self.table.update(cx, |t, cx| {
            t.delegate_mut().set_items(Vec::new());
            t.refresh(cx);
        });
    }

    /// Reads the pages from the index on a background thread.
    pub fn reload(&mut self, cx: &mut Context<Self>) {
        let Some(handle) = self.handle.clone() else {
            return;
        };
        let journals = self.show_journals;
        self.load_task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { data::all_pages(&handle, journals) })
                .await;
            let _ = this.update(cx, |view, cx| match result {
                Ok(items) => {
                    view.loaded = true;
                    view.table.update(cx, |t, cx| {
                        t.delegate_mut().set_items(items);
                        t.refresh(cx);
                    });
                    cx.notify();
                }
                Err(message) => tracing::warn!("cannot list the pages: {message}"),
            });
        }));
    }

    /// Any index change may add, remove or rename a page: reload (debounced).
    pub fn on_index_changed(&mut self, cx: &mut Context<Self>) {
        if self.handle.is_none() {
            return;
        }
        self.refresh_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(REFRESH_DEBOUNCE).await;
            let _ = this.update(cx, |view, cx| view.reload(cx));
        }));
    }

    /// Lists journals too (or hides them again).
    pub fn set_show_journals(&mut self, show: bool, cx: &mut Context<Self>) {
        if self.show_journals != show {
            self.show_journals = show;
            self.reload(cx);
            cx.notify();
        }
    }

    fn open_row(&mut self, row_ix: usize, open: OpenIn, cx: &mut Context<Self>) {
        let name = self
            .table
            .read(cx)
            .delegate()
            .item(row_ix)
            .map(|p| p.name.clone());
        if let Some(name) = name {
            cx.emit(PageEvent::open(NavTarget::Page(name), open));
        }
    }

    /// Opens the selected row (Enter).
    pub fn open_selected(
        &mut self,
        _: &OpenSelectedPage,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(row) = self.table.read(cx).selected_row() {
            self.open_row(row, OpenIn::from_modifiers(&window.modifiers()), cx);
        }
    }

    /// Moves the keyboard focus to the filter field.
    pub fn focus_filter(&self, window: &mut Window, cx: &mut App) {
        self.filter.read(cx).focus_handle(cx).focus(window, cx);
    }
}

impl Focusable for AllPagesView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for AllPagesView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let count = self.table.read(cx).delegate().visible.len();
        let this = cx.entity();
        let journals = self.show_journals;
        v_flex()
            .id("all-pages")
            .key_context("AllPages")
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::open_selected))
            .size_full()
            .p(dims::PX_16)
            .gap_2()
            .child(
                div()
                    .text_xl()
                    .font_weight(crate::ui::text_edit::FontWeight::SEMIBOLD)
                    .child(t!("all_pages.title").to_string()),
            )
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(div().w(dims::PX_280).child(Input::new(&self.filter)))
                    .child(
                        Button::new("all-pages-journals")
                            .ghost()
                            .small()
                            .selected(journals)
                            .label(t!("all_pages.show_journals").to_string())
                            .on_click(move |_, _, cx| {
                                this.update(cx, |v, cx| v.set_show_journals(!v.show_journals, cx));
                            }),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .child(t!("all_pages.count", count = count).to_string()),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .child(DataTable::new(&self.table).stripe(true)),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TestGraph;
    use crate::ui::testing::{TestAppContext, VisualTestContext, gpui_test};
    use crate::{settings::AppSettings, theme};

    fn setup(cx: &mut TestAppContext) {
        cx.update(|cx| {
            crate::ui::init(cx);
            theme::install(cx, AppSettings::default(), None);
        });
    }

    fn open(cx: &mut TestAppContext) -> (Entity<AllPagesView>, &mut VisualTestContext) {
        cx.add_window_view(AllPagesView::new)
    }

    fn settle(cx: &mut VisualTestContext, view: &Entity<AllPagesView>) {
        cx.executor().allow_parking();
        for _ in 0..400 {
            cx.run_until_parked();
            if view.read_with(cx, |v, _| v.is_loaded()) {
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("all pages did not load");
    }

    fn graph() -> TestGraph {
        TestGraph::new(&[
            ("pages/Alpha.md", "- see [[Beta]]\n"),
            ("pages/Beta.md", "- b\n"),
            ("pages/Gamma.md", "- [[Beta]] and [[Alpha]]\n"),
            ("journals/2024_01_01.md", "- day [[Beta]]\n"),
        ])
    }

    #[gpui_test]
    fn lists_pages_without_journals_and_sorts_by_backlinks(cx: &mut TestAppContext) {
        setup(cx);
        let g = graph();
        let (view, cx) = open(cx);
        view.update(cx, |v, cx| v.show(g.handle.clone(), cx));
        settle(cx, &view);
        assert_eq!(
            view.read_with(cx, |v, cx| v.visible_names(cx)),
            ["Alpha", "Beta", "Gamma"]
        );
        // Descending backlinks: Beta (3 incl. the journal) then Alpha, then Gamma.
        view.update(cx, |v, cx| {
            v.table().update(cx, |t, _| {
                let d = t.delegate_mut();
                d.sort = (COL_BACKLINKS, ColumnSort::Descending);
                d.apply();
            });
        });
        assert_eq!(
            view.read_with(cx, |v, cx| v.visible_names(cx)),
            ["Beta", "Alpha", "Gamma"]
        );
    }

    #[gpui_test]
    fn filter_and_journal_toggle_change_the_rows(cx: &mut TestAppContext) {
        setup(cx);
        let g = graph();
        let (view, cx) = open(cx);
        view.update(cx, |v, cx| v.show(g.handle.clone(), cx));
        settle(cx, &view);
        view.update(cx, |v, cx| {
            v.table()
                .update(cx, |t, _| t.delegate_mut().set_filter("ga"));
        });
        assert_eq!(view.read_with(cx, |v, cx| v.visible_names(cx)), ["Gamma"]);
        view.update(cx, |v, cx| {
            v.table().update(cx, |t, _| t.delegate_mut().set_filter(""));
            v.set_show_journals(true, cx);
        });
        cx.run_until_parked();
        for _ in 0..400 {
            cx.run_until_parked();
            if view.read_with(cx, |v, cx| v.visible_names(cx).len()) == 4 {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        let names = view.read_with(cx, |v, cx| v.visible_names(cx));
        assert!(
            names
                .iter()
                .any(|n| n.contains("2024") || n.contains("Jan")),
            "{names:?}"
        );
        assert_eq!(names.len(), 4);
    }

    #[gpui_test]
    fn opening_a_row_emits_navigate_or_sidebar_events(cx: &mut TestAppContext) {
        setup(cx);
        let g = graph();
        let (view, cx) = open(cx);
        view.update(cx, |v, cx| v.show(g.handle.clone(), cx));
        settle(cx, &view);
        let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let sink = seen.clone();
        let _sub = cx.update(|_, cx| {
            cx.subscribe(&view, move |_, event: &PageEvent, _| {
                sink.borrow_mut().push(event.clone());
            })
        });
        view.update(cx, |v, cx| {
            v.open_row(1, OpenIn::Main, cx);
            v.open_row(2, OpenIn::Sidebar, cx);
            v.open_row(1, OpenIn::NewTab, cx);
        });
        assert_eq!(
            *seen.borrow(),
            vec![
                PageEvent::Navigate(NavTarget::Page("Beta".into())),
                PageEvent::OpenInSidebar(NavTarget::Page("Gamma".into())),
                PageEvent::OpenInNewTab(NavTarget::Page("Beta".into())),
            ]
        );
    }
}
