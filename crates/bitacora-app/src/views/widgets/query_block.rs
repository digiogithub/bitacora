//! Live query results of one block (BIT-US-0102): grouped blocks or a table, kept fresh by
//! index events.
//!
//! The query runs on a background task; the view shows "running" until the first result and
//! keeps the previous result on screen while a refresh runs. Index events re-run the query
//! after a 300 ms pause, but only while the widget is on screen: a widget that was not drawn
//! for a while only marks itself stale and re-runs when drawn again.

use std::time::{Duration, Instant};

use rust_i18n::t;

use crate::data::{self, GraphHandle};
use crate::nav::OpenIn;
use crate::render::inline::{NavTarget, NoBlocks, layout_line};
use crate::render::model::Row;
use crate::render::query::table::{PageNames, RowTarget, Table, table_of};
use crate::render::query::{self, Body, Failure, Output, Scope};
use crate::render::widget::{QueryKind, QueryProps, QuerySpec};
use crate::ui::{
    ActiveTheme as _, AnyElement, App, Context, FluentBuilder as _, IconName,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, SharedString,
    StatefulInteractiveElement as _, Styled as _, Task, Window, div, h_flex, icon, px, v_flex,
};
use crate::views::block_view::{Action, Nav, RowActions, render_block_row, text_element_owned};

/// Pause after the last index event before a visible query runs again.
pub const REFRESH_DEBOUNCE: Duration = Duration::from_millis(300);

/// A widget drawn within this time counts as on screen.
const VISIBLE_WITHIN: Duration = Duration::from_secs(2);

/// Blocks whose breadcrumbs are looked up (the rest show none).
const CRUMB_CAP: usize = 200;

/// Whether an index event can change the result of a query. A file replaced without any page
/// or block touched changed nothing a query can see.
pub fn is_relevant(event: &bitacora_index::IndexEvent) -> bool {
    use bitacora_index::IndexEvent;
    match event {
        IndexEvent::FileReplaced {
            page_ids_touched,
            block_uuids_added,
            block_uuids_removed,
            ..
        } => {
            !(page_ids_touched.is_empty()
                && block_uuids_added.is_empty()
                && block_uuids_removed.is_empty())
        }
        _ => true,
    }
}

/// One page's blocks in the grouped list.
#[derive(Debug, Clone, PartialEq)]
struct Group {
    page: String,
    rows: Vec<usize>,
}

/// A finished query with everything the list view needs.
#[derive(Debug, Clone, PartialEq)]
struct Loaded {
    output: Output,
    pages: PageNames,
    rows: Vec<Row>,
    crumbs: Vec<Vec<String>>,
    groups: Vec<Group>,
}

#[derive(Debug, Clone, PartialEq)]
enum State {
    Running,
    Ready(Box<Loaded>),
    Failed(Failure),
}

/// Runs the query and prepares the rows (blocking: call from a background task).
fn load(h: &GraphHandle, spec: &QuerySpec, scope: &Scope) -> Result<Loaded, Failure> {
    let today = data::today_local().ok_or_else(|| Failure::Index("no local date".into()))?;
    let now = jiff::Timestamp::now().as_millisecond();
    let output = query::run(&h.reader, spec, scope, today, now)?;
    let mut pages = PageNames::new();
    let mut rows = Vec::new();
    let mut crumbs = Vec::new();
    let mut groups: Vec<Group> = Vec::new();
    if let Body::Blocks(blocks) = &output.body {
        rows = data::rows_from_blocks(h, blocks, 0);
        for (i, b) in blocks.iter().enumerate() {
            let name = match pages.get(&b.page_id) {
                Some(n) => n.clone(),
                None => {
                    let n = h
                        .reader
                        .page_by_id(b.page_id)
                        .ok()
                        .flatten()
                        .map(|p| p.original_name)
                        .unwrap_or_default();
                    pages.insert(b.page_id, n.clone());
                    n
                }
            };
            match groups.iter_mut().find(|g| g.page == name) {
                Some(g) => g.rows.push(i),
                None => groups.push(Group {
                    page: name,
                    rows: vec![i],
                }),
            }
            let trail = if i < CRUMB_CAP && b.depth > 1 {
                h.reader
                    .ancestors(&b.uuid)
                    .map(|a| a.into_iter().map(|x| x.title).collect())
                    .unwrap_or_default()
            } else {
                Vec::new()
            };
            crumbs.push(trail);
        }
        // Results are flat: every block shows at the left edge and without a fold arrow.
        for row in &mut rows {
            row.depth = 0;
            row.has_children = false;
        }
    }
    Ok(Loaded {
        output,
        pages,
        rows,
        crumbs,
        groups,
    })
}

/// The view of one query block.
pub struct QueryBlock {
    handle: GraphHandle,
    spec: QuerySpec,
    scope: Scope,
    nav: Nav,
    edit: Option<Action>,
    state: State,
    /// The user flipped the collapse state (`None`: the query's `:collapsed?`).
    collapsed: Option<bool>,
    /// The user flipped list/table (`None`: `query-table::`).
    table: Option<bool>,
    /// The user clicked a column header.
    sort: Option<(String, bool)>,
    /// The user picked columns.
    columns: Option<Vec<String>>,
    picker: bool,
    generation: u64,
    task: Option<Task<()>>,
    debounce: Option<Task<()>>,
    stale: bool,
    last_drawn: Option<Instant>,
}

impl std::fmt::Debug for QueryBlock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QueryBlock")
            .field("source", &self.spec.source)
            .field("state", &self.state)
            .finish_non_exhaustive()
    }
}

impl QueryBlock {
    /// A query block that starts running at once.
    pub fn new(
        handle: GraphHandle,
        spec: QuerySpec,
        scope: Scope,
        nav: Nav,
        edit: Option<Action>,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut this = Self {
            handle,
            spec,
            scope,
            nav,
            edit,
            state: State::Running,
            collapsed: None,
            table: None,
            sort: None,
            columns: None,
            picker: false,
            generation: 0,
            task: None,
            debounce: None,
            stale: false,
            last_drawn: None,
        };
        this.run(cx);
        this
    }

    /// Updates what the host knows; the query runs again when the query or graph changed.
    pub fn configure(
        &mut self,
        handle: GraphHandle,
        spec: QuerySpec,
        scope: Scope,
        nav: Nav,
        edit: Option<Action>,
        cx: &mut Context<Self>,
    ) {
        self.nav = nav;
        self.edit = edit;
        let changed = handle != self.handle || spec != self.spec || scope != self.scope;
        if spec.props != self.spec.props {
            // New `query-*` properties win over what the user toggled.
            self.table = None;
            self.sort = None;
            self.columns = None;
        }
        self.handle = handle;
        self.spec = spec;
        self.scope = scope;
        if changed {
            self.run(cx);
        }
    }

    /// Reacts to an index change (see the module docs).
    pub fn on_index_event(&mut self, event: &bitacora_index::IndexEvent, cx: &mut Context<Self>) {
        if !is_relevant(event) {
            return;
        }
        self.stale = true;
        if !self.on_screen() {
            return;
        }
        self.debounce = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(REFRESH_DEBOUNCE).await;
            let _ = this.update(cx, |this, cx| {
                if this.stale {
                    this.run(cx);
                }
            });
        }));
    }

    fn on_screen(&self) -> bool {
        self.last_drawn
            .is_some_and(|t| t.elapsed() < VISIBLE_WITHIN)
    }

    /// Runs the query in the background; the previous result stays until the new one arrives.
    pub fn run(&mut self, cx: &mut Context<Self>) {
        self.generation += 1;
        self.stale = false;
        let generation = self.generation;
        let (handle, spec, scope) = (self.handle.clone(), self.spec.clone(), self.scope.clone());
        if !matches!(self.state, State::Ready(_)) {
            self.state = State::Running;
        }
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { load(&handle, &spec, &scope) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.state = match result {
                    Ok(l) => State::Ready(Box::new(l)),
                    Err(f) => State::Failed(f),
                };
                this.task = None;
                cx.notify();
            });
        }));
        cx.notify();
    }

    /// Whether a result (or an error) is showing.
    pub fn is_settled(&self) -> bool {
        !matches!(self.state, State::Running)
    }

    /// The finished result's body, when there is one.
    pub fn body(&self) -> Option<&Body> {
        match &self.state {
            State::Ready(l) => Some(&l.output.body),
            _ => None,
        }
    }

    /// The failure, when the query failed.
    pub fn failure(&self) -> Option<&Failure> {
        match &self.state {
            State::Failed(f) => Some(f),
            _ => None,
        }
    }

    /// Whether the results are folded away.
    pub fn is_collapsed(&self) -> bool {
        self.collapsed.unwrap_or(match &self.state {
            State::Ready(l) => l.output.collapsed,
            _ => false,
        })
    }

    /// Whether the table view is on.
    pub fn is_table(&self) -> bool {
        match &self.state {
            State::Ready(l) if matches!(l.output.body, Body::Rows { .. }) => true,
            _ => self.table.or(self.spec.props.table).unwrap_or(false),
        }
    }

    /// Folds or unfolds the results.
    pub fn toggle_collapsed(&mut self, cx: &mut Context<Self>) {
        self.collapsed = Some(!self.is_collapsed());
        cx.notify();
    }

    /// Switches between the grouped list and the table.
    pub fn toggle_table(&mut self, cx: &mut Context<Self>) {
        self.table = Some(!self.is_table());
        cx.notify();
    }

    /// Sorts by `column`: first ascending, then descending on a second click.
    pub fn sort_by(&mut self, column: &str, cx: &mut Context<Self>) {
        let current = self.effective_props();
        let desc =
            current.sort_by.as_deref() == Some(column) && !current.sort_desc.unwrap_or(false);
        self.sort = Some((column.to_owned(), desc));
        cx.notify();
    }

    /// Shows or hides a column; the picker keeps the order of the available columns.
    pub fn toggle_column(&mut self, column: &str, cx: &mut Context<Self>) {
        let Some(table) = self.table_model() else {
            return;
        };
        let mut shown = table.columns.clone();
        if let Some(pos) = shown.iter().position(|c| c == column) {
            if shown.len() > 1 {
                shown.remove(pos);
            }
        } else {
            shown.push(column.to_owned());
        }
        shown.sort_by_key(|c| table.available.iter().position(|a| a == c));
        self.columns = Some(shown);
        cx.notify();
    }

    /// Opens or closes the column picker.
    pub fn toggle_picker(&mut self, cx: &mut Context<Self>) {
        self.picker = !self.picker;
        cx.notify();
    }

    fn effective_props(&self) -> QueryProps {
        let mut p = self.spec.props.clone();
        if let Some(c) = &self.columns {
            p.properties = Some(c.clone());
        }
        if let Some((col, desc)) = &self.sort {
            p.sort_by = Some(col.clone());
            p.sort_desc = Some(*desc);
        }
        p
    }

    /// The table of the current result with the user's column and sort choices.
    pub fn table_model(&self) -> Option<Table> {
        match &self.state {
            State::Ready(l) => Some(table_of(&l.output.body, &l.pages, &self.effective_props())),
            _ => None,
        }
    }

    fn title(&self) -> String {
        match &self.state {
            State::Ready(l) => l.output.title.clone(),
            _ => None,
        }
        .unwrap_or_else(|| t!("widgets.query.title").to_string())
    }

    fn render_header(
        &self,
        eid: u64,
        theme: &crate::ui::theme::Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let collapsed = self.is_collapsed();
        let count = match &self.state {
            State::Ready(l) => Some(l.output.body.len()),
            _ => None,
        };
        let can_table =
            matches!(&self.state, State::Ready(l) if !matches!(l.output.body, Body::Rows { .. }));
        let table = self.is_table();
        let id = |part| crate::views::widgets::element_id(eid, part);
        let mut bar = h_flex()
            .gap_2()
            .items_center()
            .px_2()
            .py(px(3.))
            .child(
                div()
                    .id(("qb-fold", id(1)))
                    .cursor_pointer()
                    .child(
                        icon(if collapsed {
                            IconName::ChevronRight
                        } else {
                            IconName::ChevronDown
                        })
                        .size(px(14.)),
                    )
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_collapsed(cx))),
            )
            .child(
                div()
                    .id(("qb-title", id(2)))
                    .cursor_pointer()
                    .font_weight(crate::ui::text_edit::FontWeight::SEMIBOLD)
                    .child(self.title())
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_collapsed(cx))),
            );
        if let Some(n) = count {
            bar = bar.child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(t!("widgets.query.count", count = n).to_string()),
            );
        }
        bar = bar.child(div().flex_1());
        if matches!(self.state, State::Running) {
            bar = bar.child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(t!("widgets.query.running").to_string()),
            );
        }
        if can_table {
            bar = bar.child(chip(
                ("qb-view", id(3)),
                if table {
                    t!("widgets.query.view_list").to_string()
                } else {
                    t!("widgets.query.view_table").to_string()
                },
                theme,
                cx.listener(|this, _, _, cx| this.toggle_table(cx)),
            ));
        }
        if table && self.is_settled() {
            bar = bar.child(chip(
                ("qb-columns", id(4)),
                t!("widgets.query.columns").to_string(),
                theme,
                cx.listener(|this, _, _, cx| this.toggle_picker(cx)),
            ));
        }
        bar = bar.child(chip(
            ("qb-refresh", id(5)),
            t!("widgets.query.refresh").to_string(),
            theme,
            cx.listener(|this, _, _, cx| this.run(cx)),
        ));
        if let Some(edit) = self.edit.clone() {
            bar = bar.child(chip(
                ("qb-edit", id(6)),
                t!("widgets.edit_source").to_string(),
                theme,
                move |_: &crate::ui::ClickEvent, window: &mut Window, cx: &mut App| {
                    edit(window, cx)
                },
            ));
        }
        bar.into_any_element()
    }

    fn render_message(
        &self,
        text: String,
        color: crate::ui::Hsla,
        theme: &crate::ui::theme::Theme,
    ) -> AnyElement {
        h_flex()
            .gap_2()
            .items_start()
            .px_2()
            .py(px(4.))
            .text_sm()
            .text_color(color)
            .child(icon(IconName::TriangleAlert).size(px(14.)))
            .child(div().flex_1().min_w_0().child(text))
            .border_t_1()
            .border_color(theme.border)
            .into_any_element()
    }

    fn render_list(
        &self,
        eid: u64,
        loaded: &Loaded,
        theme: &crate::ui::theme::Theme,
        root: Option<&std::path::Path>,
    ) -> AnyElement {
        let nav = self.nav.clone();
        let mut col = v_flex().gap_1().px_2().pb_2();
        match &loaded.output.body {
            Body::Blocks(_) => {
                for (g, group) in loaded.groups.iter().enumerate() {
                    let page = group.page.clone();
                    let nav_page = nav.clone();
                    col = col.child(
                        div()
                            .id(("qb-group", crate::views::widgets::element_id(eid, 100 + g)))
                            .text_sm()
                            .text_color(theme.info)
                            .font_weight(crate::ui::text_edit::FontWeight::SEMIBOLD)
                            .cursor_pointer()
                            .child(page.clone())
                            .on_click(move |_, window, cx| {
                                nav_page(
                                    NavTarget::Page(page.clone()),
                                    OpenIn::from_shift(window.modifiers().shift),
                                    cx,
                                );
                            }),
                    );
                    for &i in &group.rows {
                        let Some(row) = loaded.rows.get(i) else {
                            continue;
                        };
                        let trail = loaded.crumbs.get(i).filter(|c| !c.is_empty());
                        if let Some(trail) = trail {
                            col = col.child(
                                div()
                                    .pl(px(22.))
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child(trail.join(" \u{203a} ")),
                            );
                        }
                        let actions = RowActions::nav_only(nav.clone());
                        col = col.child(render_block_row(
                            crate::views::widgets::element_id(eid, 10_000 + i),
                            row,
                            root,
                            theme,
                            &actions,
                        ));
                    }
                }
            }
            Body::Pages(pages) => {
                for (i, p) in pages.iter().enumerate() {
                    let name = p.original_name.clone();
                    let nav = nav.clone();
                    col = col.child(
                        div()
                            .id(("qb-page", crate::views::widgets::element_id(eid, 200 + i)))
                            .text_color(theme.info)
                            .cursor_pointer()
                            .child(name.clone())
                            .on_click(move |_, window, cx| {
                                nav(
                                    NavTarget::Page(name.clone()),
                                    OpenIn::from_shift(window.modifiers().shift),
                                    cx,
                                );
                            }),
                    );
                }
            }
            Body::Rows { .. } => {}
        }
        col.into_any_element()
    }

    fn render_table(
        &self,
        eid: u64,
        table: &Table,
        theme: &crate::ui::theme::Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let props = self.effective_props();
        let mut out = v_flex().gap_0().px_2().pb_2();
        if self.picker {
            let mut chips = h_flex().gap_1().flex_wrap().pb_1();
            for (i, name) in table.available.iter().enumerate() {
                let on = table.columns.contains(name);
                let col = name.clone();
                chips = chips.child(
                    div()
                        .id(("qb-pick", crate::views::widgets::element_id(eid, 300 + i)))
                        .px(px(6.))
                        .rounded(px(4.))
                        .text_xs()
                        .cursor_pointer()
                        .border_1()
                        .border_color(if on { theme.primary } else { theme.border })
                        .text_color(if on {
                            theme.foreground
                        } else {
                            theme.muted_foreground
                        })
                        .child(format!("{} {}", if on { "\u{2713}" } else { "+" }, name))
                        .on_click(cx.listener(move |this, _, _, cx| this.toggle_column(&col, cx))),
                );
            }
            out = out.child(chips);
        }
        let mut head = h_flex()
            .gap_2()
            .py(px(2.))
            .border_b_1()
            .border_color(theme.border);
        for (c, name) in table.columns.iter().enumerate() {
            let col = name.clone();
            let marker = if props.sort_by.as_deref() == Some(name.as_str()) {
                if props.sort_desc.unwrap_or(false) {
                    " \u{25BE}"
                } else {
                    " \u{25B4}"
                }
            } else {
                ""
            };
            head = head.child(
                div()
                    .id(("qb-head", crate::views::widgets::element_id(eid, 400 + c)))
                    .flex_1()
                    .min_w(px(60.))
                    .when(c == 0, |d| d.min_w(px(220.)))
                    .text_xs()
                    .font_weight(crate::ui::text_edit::FontWeight::SEMIBOLD)
                    .text_color(theme.muted_foreground)
                    .cursor_pointer()
                    .child(format!("{name}{marker}"))
                    .on_click(cx.listener(move |this, _, _, cx| this.sort_by(&col, cx))),
            );
        }
        out = out.child(head);
        for (r, row) in table.rows.iter().enumerate() {
            let mut line = h_flex()
                .gap_2()
                .py(px(2.))
                .items_start()
                .border_b_1()
                .border_color(theme.border.opacity(0.4));
            for (c, text) in row.cells.iter().enumerate() {
                let cell_id = crate::views::widgets::element_id(eid, 1000 + r * 64 + c.min(63));
                let is_page_col = table.columns.get(c).is_some_and(|n| n == "page");
                let target = match (&row.target, c, is_page_col) {
                    (RowTarget::Block(u), 0, _) => Some(NavTarget::Block(u.clone())),
                    (_, _, true) => row.page.clone().map(NavTarget::Page),
                    (RowTarget::Page(p), 0, _) => Some(NavTarget::Page(p.clone())),
                    _ => None,
                };
                let layout = layout_line(text, &NoBlocks);
                let nav = self.nav.clone();
                let cell = match target {
                    Some(t) => {
                        let nav = nav.clone();
                        div()
                            .id(("qb-cell", cell_id))
                            .cursor_pointer()
                            .child(SharedString::from(
                                if is_page_col || matches!(row.target, RowTarget::Page(_)) {
                                    text.clone()
                                } else {
                                    layout.text.clone()
                                },
                            ))
                            .text_color(if is_page_col {
                                theme.info
                            } else {
                                theme.foreground
                            })
                            .on_click(move |_, window, cx| {
                                nav(t.clone(), OpenIn::from_shift(window.modifiers().shift), cx);
                            })
                            .into_any_element()
                    }
                    None => text_element_owned(
                        ("qb-text", cell_id),
                        layout,
                        theme,
                        false,
                        Some(nav),
                        None,
                    ),
                };
                line = line.child(
                    div()
                        .flex_1()
                        .min_w(px(60.))
                        .text_sm()
                        .when(c == 0, |d| d.min_w(px(220.)))
                        .child(cell),
                );
            }
            out = out.child(line);
        }
        out.into_any_element()
    }
}

fn chip(
    id: (&'static str, usize),
    label: String,
    theme: &crate::ui::theme::Theme,
    on_click: impl Fn(&crate::ui::ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    div()
        .id(id)
        .px(px(6.))
        .rounded(px(4.))
        .text_xs()
        .cursor_pointer()
        .text_color(theme.muted_foreground)
        .hover(|d| d.bg(theme.muted))
        .child(label)
        .on_click(on_click)
        .into_any_element()
}

impl Render for QueryBlock {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.last_drawn = Some(Instant::now());
        if self.stale && self.task.is_none() {
            self.run(cx);
        }
        let theme = cx.theme().clone();
        let eid = cx.entity_id().as_u64();
        let root = Some(self.handle.root.clone());
        let mut frame = v_flex()
            .w_full()
            .rounded(px(6.))
            .border_1()
            .border_color(theme.border)
            .bg(theme.background)
            // Presses inside the widget must not put the host block in edit mode.
            .on_mouse_down(crate::ui::text_edit::MouseButton::Left, |_, _, cx| {
                cx.stop_propagation();
            })
            .child(self.render_header(eid, &theme, cx));
        if matches!(self.spec.kind, QueryKind::Simple) {
            frame = frame.child(
                div()
                    .px_2()
                    .pb(px(2.))
                    .text_xs()
                    .font_family(theme.mono_font_family.clone())
                    .text_color(theme.muted_foreground)
                    .child(self.spec.source.clone()),
            );
        }
        if self.is_collapsed() {
            return frame.into_any_element();
        }
        match &self.state {
            State::Running => {}
            State::Failed(f) => {
                let (key, color) = match f {
                    Failure::Unsupported(_) => ("widgets.query.unsupported", theme.warning),
                    _ => ("widgets.query.error", theme.danger),
                };
                let text = t!(key, message = f.message()).to_string();
                frame = frame.child(self.render_message(text, color, &theme));
            }
            State::Ready(loaded) => {
                for w in &loaded.output.warnings {
                    frame = frame.child(self.render_message(w.clone(), theme.warning, &theme));
                }
                if loaded.output.truncated {
                    frame = frame.child(self.render_message(
                        t!("widgets.query.truncated", count = query::RESULT_CAP).to_string(),
                        theme.muted_foreground,
                        &theme,
                    ));
                }
                let loaded = loaded.clone();
                if loaded.output.body.is_empty() {
                    frame = frame.child(
                        div()
                            .px_2()
                            .pb_2()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .child(t!("widgets.query.empty").to_string()),
                    );
                } else if self.is_table() {
                    if let Some(table) = self.table_model() {
                        frame = frame.child(self.render_table(eid, &table, &theme, cx));
                    }
                } else {
                    frame = frame.child(self.render_list(eid, &loaded, &theme, root.as_deref()));
                }
            }
        }
        frame.into_any_element()
    }
}
