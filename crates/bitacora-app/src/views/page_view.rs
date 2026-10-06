//! Page view: a lazily loaded, virtualized outline with header, breadcrumbs and references
//! (BIT-US-0075, BIT-US-0077).
//!
//! The view reads pages through the index ([`crate::data`]): the first 50 blocks arrive with the
//! header, 25 more are fetched as the user scrolls near the end. Everything shown (header, blocks,
//! linked and unlinked references) is one flat list of [`Item`]s inside a single GPUI `list`, so
//! long pages and large reference sets stay virtualized. Collapse state is view-only.

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use bitacora_index::{IndexEvent, RefFilters};
use bitacora_markdown::properties::PropertyConfig;
use rust_i18n::t;

use crate::data::{
    self, CHUNK, FIRST_CHUNK, GraphHandle, Link, PageHeader, PageLoad, RefGroupModel, RefsLoad,
};
use crate::nav::{Route, Scroll};
use crate::render::inline::{NavTarget, NoBlocks};
use crate::render::model::{
    PageModel, Row, apply_overrides, collapse_overrides, toggle_row, visible_rows,
};
use crate::ui::button::{Button, ButtonVariants as _};
use crate::ui::popover::Popover;
use crate::ui::text_edit::{FontWeight, ListAlignment, ListOffset, ListState, list};
use crate::ui::{
    ActiveTheme as _, Anchor, AnyElement, App, Context, EventEmitter, FluentBuilder as _, IconName,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, Sizable as _,
    StatefulInteractiveElement as _, Styled as _, Task, Window, div, h_flex, icon, px, v_flex,
};
use crate::views::block_view::{Nav, RowActions, properties_table, render_block_row};

/// Pause after the last index event before the page is reloaded.
const REFRESH_DEBOUNCE: Duration = Duration::from_millis(250);

/// Between the ancestors of a reference hit.
const CRUMB_SEPARATOR: &str = "\u{203a}";

/// Between the segments of a namespaced title.
const NAMESPACE_SEPARATOR: &str = "/";

/// Events emitted by the page view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PageEvent {
    /// The user clicked a ref, tag, link or breadcrumb.
    Navigate(NavTarget),
}

/// What the view currently shows.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum LoadState {
    /// No page.
    #[default]
    Empty,
    /// The page is being read.
    Loading,
    /// The page is rendered.
    Loaded,
    /// Reading failed.
    Failed(String),
}

/// Which reference list an item belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefKind {
    /// Blocks that link to the page.
    Linked,
    /// Blocks that mention the page name as plain text.
    Unlinked,
}

impl RefKind {
    fn tag(self) -> usize {
        match self {
            Self::Linked => 1,
            Self::Unlinked => 2,
        }
    }
}

/// One entry of the flat list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Item {
    /// Title, breadcrumbs, properties, namespace children.
    Header,
    /// A block of the page (index into the rows).
    Block(usize),
    /// "No content yet".
    Empty,
    /// "Linked references (N)" or "Unlinked references".
    RefsHeading(RefKind),
    /// Page title of a reference group.
    RefGroup(RefKind, usize),
    /// Breadcrumb of a reference hit.
    RefCrumbs(RefKind, usize, usize),
    /// A row of a reference hit.
    RefBlock(RefKind, usize, usize, usize),
}

/// One references section.
#[derive(Debug, Default)]
struct RefsSection {
    expanded: bool,
    loading: bool,
    load: Option<RefsLoad>,
}

/// The page view.
pub struct PageView {
    handle: Option<GraphHandle>,
    route: Option<Route>,
    header: PageHeader,
    page_id: Option<i64>,
    rows: Vec<Row>,
    fetched: usize,
    has_more: bool,
    loading_more: bool,
    state: LoadState,
    items: Vec<Item>,
    list_state: ListState,
    cfg: PropertyConfig,
    generation: u64,
    refs_generation: u64,
    linked: RefsSection,
    unlinked: RefsSection,
    filters: RefFilters,
    filter_candidates: BTreeSet<String>,
    load_task: Option<Task<()>>,
    more_task: Option<Task<()>>,
    refs_task: Option<Task<()>>,
    unlinked_task: Option<Task<()>>,
    refresh_task: Option<Task<()>>,
    rendered_rows: usize,
}

impl std::fmt::Debug for PageView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PageView")
            .field("title", &self.header.title)
            .field("rows", &self.rows.len())
            .field("state", &self.state)
            .finish_non_exhaustive()
    }
}

impl EventEmitter<PageEvent> for PageView {}

pub use crate::views::block_view::resolve_asset;

impl PageView {
    /// An empty view.
    pub fn new(_cx: &mut Context<Self>) -> Self {
        Self {
            handle: None,
            route: None,
            header: PageHeader::default(),
            page_id: None,
            rows: Vec::new(),
            fetched: 0,
            has_more: false,
            loading_more: false,
            state: LoadState::Empty,
            items: Vec::new(),
            list_state: ListState::new(0, ListAlignment::Top, px(400.)),
            cfg: PropertyConfig::default(),
            generation: 0,
            refs_generation: 0,
            linked: RefsSection {
                expanded: true,
                ..RefsSection::default()
            },
            unlinked: RefsSection::default(),
            filters: RefFilters::default(),
            filter_candidates: BTreeSet::new(),
            load_task: None,
            more_task: None,
            refs_task: None,
            unlinked_task: None,
            refresh_task: None,
            rendered_rows: 0,
        }
    }

    /// Current load state.
    pub fn state(&self) -> &LoadState {
        &self.state
    }

    /// The loaded rows (collapsed subtrees included; see [`visible_rows`]).
    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    /// Rows currently drawn in the list (collapsed subtrees skipped).
    pub fn visible_count(&self) -> usize {
        visible_rows(&self.rows).len()
    }

    /// The list items (header, blocks, references).
    pub fn items(&self) -> &[Item] {
        &self.items
    }

    /// The header data.
    pub fn header(&self) -> &PageHeader {
        &self.header
    }

    /// Page title shown above the blocks.
    pub fn title(&self) -> Option<&str> {
        (!self.header.title.is_empty()).then_some(self.header.title.as_str())
    }

    /// The route being shown.
    pub fn route(&self) -> Option<&Route> {
        self.route.as_ref()
    }

    /// Whether more blocks can be fetched from the index.
    pub fn has_more(&self) -> bool {
        self.has_more
    }

    /// Blocks fetched from the index so far.
    pub fn fetched(&self) -> usize {
        self.fetched
    }

    /// Linked references loaded for the page, if any.
    pub fn linked(&self) -> Option<&RefsLoad> {
        self.linked.load.as_ref()
    }

    /// Unlinked references, once computed.
    pub fn unlinked(&self) -> Option<&RefsLoad> {
        self.unlinked.load.as_ref()
    }

    /// Whether the unlinked section is open.
    pub fn unlinked_expanded(&self) -> bool {
        self.unlinked.expanded
    }

    /// The reference filters in effect (in-memory; persisted as `filters::` once editing
    /// exists).
    pub fn filters(&self) -> &RefFilters {
        &self.filters
    }

    /// Sets the property settings (from `config.edn`) used when building models from files.
    pub fn set_property_config(&mut self, cfg: PropertyConfig) {
        self.cfg = cfg;
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
        let last = self.items.len().saturating_sub(1);
        self.list_state.scroll_to(ListOffset {
            item_ix: scroll.item_ix.min(last),
            offset_in_item: px(scroll.offset_px),
        });
    }

    /// How many rows have been drawn so far (tests and diagnostics).
    pub fn rendered_rows(&self) -> usize {
        self.rendered_rows
    }

    /// Reports a click on a rendered ref.
    pub fn activate(&mut self, target: NavTarget, cx: &mut Context<Self>) {
        cx.emit(PageEvent::Navigate(target));
    }

    // ---- loading -------------------------------------------------------------------------

    /// Shows `route` (a page or a zoomed block) read through the index. Showing the route that
    /// is already open reloads it, keeping the scroll position and the view-only collapse state.
    pub fn show(
        &mut self,
        handle: GraphHandle,
        route: Route,
        restore: Option<Scroll>,
        cx: &mut Context<Self>,
    ) {
        let reload = self.route.as_ref() == Some(&route) && self.state == LoadState::Loaded;
        let anchor = restore.or_else(|| reload.then(|| self.scroll()));
        let overrides = if reload {
            collapse_overrides(&self.rows)
        } else {
            HashMap::new()
        };
        let mut limit = FIRST_CHUNK;
        if let Some(a) = anchor {
            limit = limit.max(a.item_ix + CHUNK);
        }
        if reload {
            limit = limit.max(self.fetched);
        }
        if !reload {
            self.state = LoadState::Loading;
            self.more_task = None;
            self.loading_more = false;
        }
        self.route = Some(route.clone());
        self.handle = Some(handle.clone());
        self.generation += 1;
        let generation = self.generation;
        cx.notify();
        self.load_task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    match &route {
                        Route::Page(name) => data::open_page(&handle, name, limit),
                        Route::Block(uuid) => data::zoom_block(&handle, uuid),
                        Route::Journals => Err("not a page".to_owned()),
                    }
                })
                .await;
            // The view may have been closed meanwhile; nothing to update then.
            let _ = this.update(cx, |view, cx| {
                view.finish_load(generation, result, &overrides, anchor, cx);
            });
        }));
    }

    fn finish_load(
        &mut self,
        generation: u64,
        result: Result<PageLoad, String>,
        overrides: &HashMap<String, bool>,
        anchor: Option<Scroll>,
        cx: &mut Context<Self>,
    ) {
        if generation != self.generation {
            return;
        }
        match result {
            Err(message) => {
                self.state = LoadState::Failed(message);
                cx.notify();
            }
            Ok(load) => {
                let new_page = self.page_id != load.header.page_id || self.page_id.is_none();
                self.page_id = load.header.page_id;
                self.header = load.header;
                self.rows = load.rows;
                apply_overrides(&mut self.rows, overrides);
                self.fetched = load.fetched;
                self.has_more = load.has_more;
                self.state = LoadState::Loaded;
                if new_page {
                    self.linked = RefsSection {
                        expanded: true,
                        ..RefsSection::default()
                    };
                    self.unlinked = RefsSection::default();
                    self.filters = RefFilters::default();
                    self.filter_candidates.clear();
                }
                self.items = self.build_items();
                self.list_state.reset(self.items.len());
                if let Some(a) = anchor {
                    self.restore_scroll(a);
                }
                cx.notify();
                if self.is_page_route() {
                    self.load_linked(new_page, cx);
                    if self.unlinked.expanded {
                        self.load_unlinked(cx);
                    }
                }
            }
        }
    }

    fn is_page_route(&self) -> bool {
        matches!(self.route, Some(Route::Page(_))) && self.page_id.is_some()
    }

    /// Fetches the next chunk of blocks when the user nears the end of the list.
    pub fn request_more(&mut self, cx: &mut Context<Self>) {
        let (Some(handle), Some(page_id)) = (self.handle.clone(), self.page_id) else {
            return;
        };
        if !self.has_more || self.loading_more || !matches!(self.route, Some(Route::Page(_))) {
            return;
        }
        self.loading_more = true;
        let generation = self.generation;
        let offset = self.fetched;
        self.more_task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { data::page_chunk(&handle, page_id, offset, CHUNK) })
                .await;
            let _ = this.update(cx, |view, cx| {
                if generation != view.generation {
                    return;
                }
                view.loading_more = false;
                match result {
                    Ok((rows, fetched, more)) => {
                        view.rows.extend(rows);
                        view.fetched = fetched;
                        view.has_more = more;
                        view.resync_items();
                        cx.notify();
                        // Collapsed subtrees may leave the list short: keep going.
                        if view.has_more && view.items.len() < FIRST_CHUNK {
                            view.request_more(cx);
                        }
                    }
                    Err(message) => {
                        tracing::warn!("cannot load more blocks: {message}");
                        view.has_more = false;
                    }
                }
            });
        }));
    }

    /// Reloads the current route (after the index finished, for instance).
    pub fn reload(&mut self, cx: &mut Context<Self>) {
        if let (Some(handle), Some(route)) = (self.handle.clone(), self.route.clone()) {
            self.show(handle, route, None, cx);
        }
    }

    /// Reacts to an index change: reloads the page when it was touched (debounced), otherwise
    /// only refreshes the references, which may have gained or lost a block.
    pub fn on_index_event(&mut self, event: &IndexEvent, cx: &mut Context<Self>) {
        if self.handle.is_none() || self.state != LoadState::Loaded {
            return;
        }
        let touched = data::event_touches(event, self.page_id, None);
        let refs_only = !touched && self.is_page_route();
        if !touched && !refs_only {
            return;
        }
        self.refresh_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(REFRESH_DEBOUNCE).await;
            let _ = this.update(cx, |view, cx| {
                if touched {
                    view.reload(cx);
                } else {
                    view.load_linked(false, cx);
                    if view.unlinked.expanded {
                        view.load_unlinked(cx);
                    }
                }
            });
        }));
    }

    // ---- references ---------------------------------------------------------------------

    fn load_linked(&mut self, initial: bool, cx: &mut Context<Self>) {
        let (Some(handle), Some(page_id)) = (self.handle.clone(), self.page_id) else {
            return;
        };
        self.linked.loading = true;
        self.refs_generation += 1;
        let generation = self.refs_generation;
        let filters = (!initial).then(|| self.filters.clone());
        self.refs_task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let filters = match filters {
                        Some(f) => f,
                        None => data::page_filters(&handle, page_id)?,
                    };
                    data::load_linked(&handle, page_id, &filters).map(|r| (r, filters))
                })
                .await;
            let _ = this.update(cx, |view, cx| {
                if generation != view.refs_generation {
                    return;
                }
                view.linked.loading = false;
                match result {
                    Ok((load, filters)) => {
                        for g in &load.groups {
                            view.filter_candidates.insert(g.page.clone());
                        }
                        view.filter_candidates
                            .extend(filters.include.iter().chain(&filters.exclude).cloned());
                        view.filters = filters;
                        view.linked.load = Some(load);
                    }
                    Err(message) => tracing::warn!("cannot load linked references: {message}"),
                }
                view.resync_refs();
                cx.notify();
            });
        }));
    }

    fn load_unlinked(&mut self, cx: &mut Context<Self>) {
        let (Some(handle), Some(page_id)) = (self.handle.clone(), self.page_id) else {
            return;
        };
        self.unlinked.loading = true;
        let generation = self.generation;
        self.unlinked_task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { data::load_unlinked(&handle, page_id) })
                .await;
            let _ = this.update(cx, |view, cx| {
                if generation != view.generation {
                    return;
                }
                view.unlinked.loading = false;
                match result {
                    Ok(load) => view.unlinked.load = Some(load),
                    Err(message) => tracing::warn!("cannot load unlinked references: {message}"),
                }
                view.resync_refs();
                cx.notify();
            });
        }));
    }

    /// Opens or closes a references section; the unlinked one is computed on first open.
    pub fn toggle_refs(&mut self, kind: RefKind, cx: &mut Context<Self>) {
        match kind {
            RefKind::Linked => self.linked.expanded = !self.linked.expanded,
            RefKind::Unlinked => {
                self.unlinked.expanded = !self.unlinked.expanded;
                if self.unlinked.expanded && self.unlinked.load.is_none() && !self.unlinked.loading
                {
                    self.load_unlinked(cx);
                }
            }
        }
        self.resync_refs();
        cx.notify();
    }

    /// Cycles a page through "no filter", "include" and "exclude" in the linked references.
    pub fn cycle_filter(&mut self, page: &str, cx: &mut Context<Self>) {
        let key = |s: &String| s.eq_ignore_ascii_case(page);
        if self.filters.include.iter().any(key) {
            self.filters
                .include
                .retain(|s| !s.eq_ignore_ascii_case(page));
            self.filters.exclude.push(page.to_owned());
        } else if self.filters.exclude.iter().any(key) {
            self.filters
                .exclude
                .retain(|s| !s.eq_ignore_ascii_case(page));
        } else {
            self.filters.include.push(page.to_owned());
        }
        self.load_linked(false, cx);
        cx.notify();
    }

    /// Removes every reference filter.
    pub fn clear_filters(&mut self, cx: &mut Context<Self>) {
        self.filters = RefFilters::default();
        self.load_linked(false, cx);
        cx.notify();
    }

    // ---- items ---------------------------------------------------------------------------

    fn ref_load(&self, kind: RefKind) -> Option<&RefsLoad> {
        match kind {
            RefKind::Linked => self.linked.load.as_ref(),
            RefKind::Unlinked => self.unlinked.load.as_ref(),
        }
    }

    fn ref_section(&self, kind: RefKind) -> &RefsSection {
        match kind {
            RefKind::Linked => &self.linked,
            RefKind::Unlinked => &self.unlinked,
        }
    }

    fn build_items(&self) -> Vec<Item> {
        let mut items = vec![Item::Header];
        let zoomed = !self.header.zoom.is_empty();
        let visible = visible_rows(&self.rows);
        if visible.is_empty() && !zoomed && self.state == LoadState::Loaded {
            items.push(Item::Empty);
        }
        items.extend(visible.into_iter().map(Item::Block));
        if self.is_page_route() {
            for kind in [RefKind::Linked, RefKind::Unlinked] {
                self.push_refs(kind, &mut items);
            }
        }
        items
    }

    fn push_refs(&self, kind: RefKind, items: &mut Vec<Item>) {
        items.push(Item::RefsHeading(kind));
        let section = self.ref_section(kind);
        if !section.expanded {
            return;
        }
        let Some(load) = &section.load else { return };
        for (g, group) in load.groups.iter().enumerate() {
            items.push(Item::RefGroup(kind, g));
            for (h, hit) in group.hits.iter().enumerate() {
                if !hit.crumbs.is_empty() {
                    items.push(Item::RefCrumbs(kind, g, h));
                }
                items.extend(
                    visible_rows(&hit.rows)
                        .into_iter()
                        .map(|r| Item::RefBlock(kind, g, h, r)),
                );
            }
        }
    }

    /// Recomputes the items and tells the list what changed (common prefix and suffix are kept,
    /// so the scroll position survives toggles and appended chunks).
    fn resync_items(&mut self) {
        let new = self.build_items();
        let old = &self.items;
        let mut prefix = 0;
        while prefix < old.len() && prefix < new.len() && old[prefix] == new[prefix] {
            prefix += 1;
        }
        let mut suffix = 0;
        while suffix < old.len() - prefix
            && suffix < new.len() - prefix
            && old[old.len() - 1 - suffix] == new[new.len() - 1 - suffix]
        {
            suffix += 1;
        }
        self.list_state
            .splice(prefix..old.len() - suffix, new.len() - prefix - suffix);
        self.items = new;
    }

    /// Like [`Self::resync_items`] but forgets the measured heights of every reference item,
    /// whose content may have changed under the same identity.
    fn resync_refs(&mut self) {
        let new = self.build_items();
        let from = self
            .items
            .iter()
            .position(|i| matches!(i, Item::RefsHeading(_)))
            .unwrap_or(self.items.len());
        self.list_state
            .splice(from..self.items.len(), new.len().saturating_sub(from));
        self.items = new;
    }

    fn item_position(&self, item: Item) -> Option<usize> {
        self.items.iter().position(|i| *i == item)
    }

    /// Flips the view-only collapse state of block row `ix`.
    pub fn toggle_block(&mut self, ix: usize, cx: &mut Context<Self>) {
        if toggle_row(&mut self.rows, ix) {
            self.resync_items();
            cx.notify();
        }
    }

    /// Flips the view-only collapse state of a row of a reference hit.
    pub fn toggle_ref_row(
        &mut self,
        kind: RefKind,
        g: usize,
        h: usize,
        r: usize,
        cx: &mut Context<Self>,
    ) {
        let section = match kind {
            RefKind::Linked => &mut self.linked,
            RefKind::Unlinked => &mut self.unlinked,
        };
        let Some(hit) = section
            .load
            .as_mut()
            .and_then(|l| l.groups.get_mut(g))
            .and_then(|g| g.hits.get_mut(h))
        else {
            return;
        };
        if toggle_row(&mut hit.rows, r) {
            self.resync_items();
            cx.notify();
        }
    }

    /// Opens or closes the list of blocks that reference block row `ix`.
    pub fn toggle_referrers(&mut self, ix: usize, cx: &mut Context<Self>) {
        let Some(row) = self.rows.get_mut(ix) else {
            return;
        };
        if row.referrers.take().is_some() {
            self.remeasure_block(ix);
            cx.notify();
            return;
        }
        let (Some(handle), Some(uuid)) = (self.handle.clone(), row.uuid.clone()) else {
            return;
        };
        let generation = self.generation;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { data::load_referrers(&handle, &uuid) })
                .await;
            let _ = this.update(cx, |view, cx| {
                if generation != view.generation {
                    return;
                }
                match result {
                    Ok(list) => {
                        if let Some(row) = view.rows.get_mut(ix) {
                            row.referrers = Some(list);
                        }
                        view.remeasure_block(ix);
                        cx.notify();
                    }
                    Err(message) => tracing::warn!("cannot load referrers: {message}"),
                }
            });
        })
        .detach();
    }

    fn remeasure_block(&self, ix: usize) {
        if let Some(pos) = self.item_position(Item::Block(ix)) {
            self.list_state.remeasure_items(pos..pos + 1);
        }
    }

    // ---- file fallback -------------------------------------------------------------------

    /// Reads `file` on a background thread and shows it without the index (used when the index
    /// is unavailable).
    pub fn open_file(&mut self, file: PathBuf, title: String, cx: &mut Context<Self>) {
        self.state = LoadState::Loading;
        self.header = PageHeader {
            title: title.clone(),
            ..PageHeader::default()
        };
        self.generation += 1;
        cx.notify();
        self.load_task = Some(cx.spawn(async move |this, cx| {
            let read = cx
                .background_executor()
                .spawn(async move { std::fs::read(&file).map_err(|e| e.to_string()) })
                .await;
            let _ = this.update(cx, |view, cx| match read {
                Ok(bytes) => view.set_source(&title, &bytes, cx),
                Err(message) => {
                    view.state = LoadState::Failed(message);
                    cx.notify();
                }
            });
        }));
    }

    /// Shows a page from its bytes (no index: block refs resolve inside the page only).
    pub fn set_source(&mut self, title: &str, source: &[u8], cx: &mut Context<Self>) {
        let mut model = PageModel::from_source(source, &self.cfg, &NoBlocks);
        let known: HashMap<String, String> = model
            .rows
            .iter()
            .filter_map(|r| Some((r.block.id.clone()?, r.block.title.text.clone())))
            .collect();
        if !known.is_empty() {
            model = PageModel::from_source(source, &self.cfg, &known);
        }
        self.header = PageHeader {
            title: title.to_owned(),
            ..PageHeader::default()
        };
        self.page_id = None;
        self.route = None;
        self.rows = model.rows;
        self.has_more = false;
        self.state = LoadState::Loaded;
        self.items = self.build_items();
        self.list_state.reset(self.items.len());
        cx.notify();
    }

    // ---- rendering -----------------------------------------------------------------------

    fn nav_for(&self, cx: &mut Context<Self>) -> Nav {
        let this = cx.entity();
        Rc::new(move |target: NavTarget, cx: &mut App| {
            this.update(cx, |view, cx| view.activate(target, cx));
        })
    }

    fn render_item(&mut self, ix: usize, _: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let Some(item) = self.items.get(ix).copied() else {
            return div().into_any_element();
        };
        if ix + 8 >= self.items.len() && self.has_more {
            self.request_more(cx);
        }
        self.rendered_rows += 1;
        let theme = cx.theme().clone();
        let nav = self.nav_for(cx);
        let root = self.handle.as_ref().map(|h| h.root.clone());
        let element = match item {
            Item::Header => self.render_header(&nav, cx),
            Item::Empty => div()
                .px(px(24.))
                .py(px(8.))
                .text_color(theme.muted_foreground)
                .child(t!("page.empty").to_string())
                .into_any_element(),
            Item::Block(r) => match self.rows.get(r) {
                Some(row) => {
                    let this = cx.entity();
                    let toggle_this = this.clone();
                    let actions = RowActions {
                        nav: nav.clone(),
                        toggle: Some(Rc::new(move |_, cx| {
                            toggle_this.update(cx, |v, cx| v.toggle_block(r, cx));
                        })),
                        referrers: Some(Rc::new(move |_, cx| {
                            this.update(cx, |v, cx| v.toggle_referrers(r, cx));
                        })),
                    };
                    render_block_row(r, row, root.as_deref(), &theme, &actions)
                }
                None => div().into_any_element(),
            },
            Item::RefsHeading(kind) => self.render_refs_heading(kind, cx),
            Item::RefGroup(kind, g) => match self.ref_group(kind, g) {
                Some(group) => self.render_ref_group(&group.page, &nav, &theme),
                None => div().into_any_element(),
            },
            Item::RefCrumbs(kind, g, h) => {
                match self.ref_group(kind, g).and_then(|x| x.hits.get(h)) {
                    Some(hit) => {
                        let mut line = h_flex()
                            .gap_1()
                            .px(px(24.))
                            .text_xs()
                            .flex_wrap()
                            .text_color(theme.muted_foreground);
                        for (n, (uuid, title)) in hit.crumbs.iter().enumerate() {
                            let nav = nav.clone();
                            let uuid = uuid.clone();
                            if n > 0 {
                                line = line.child(CRUMB_SEPARATOR);
                            }
                            line = line.child(
                                div()
                                    .id(("crumb", ix * 100 + n))
                                    .cursor_pointer()
                                    .child(title.clone())
                                    .on_click(move |_, _, cx| {
                                        nav(NavTarget::Block(uuid.clone()), cx)
                                    }),
                            );
                        }
                        line.into_any_element()
                    }
                    None => div().into_any_element(),
                }
            }
            Item::RefBlock(kind, g, h, r) => {
                let row = self
                    .ref_group(kind, g)
                    .and_then(|x| x.hits.get(h))
                    .and_then(|hit| hit.rows.get(r));
                match row {
                    Some(row) => {
                        let this = cx.entity();
                        let actions = RowActions {
                            nav: nav.clone(),
                            toggle: Some(Rc::new(move |_, cx| {
                                this.update(cx, |v, cx| v.toggle_ref_row(kind, g, h, r, cx));
                            })),
                            referrers: None,
                        };
                        let id = (kind.tag() << 40)
                            | ((g & 0xFFF) << 28)
                            | ((h & 0x3FFF) << 14)
                            | (r & 0x3FFF);
                        render_block_row(id, row, root.as_deref(), &theme, &actions)
                    }
                    None => div().into_any_element(),
                }
            }
        };
        div()
            .w_full()
            .flex()
            .justify_center()
            .child(div().w_full().max_w(px(900.)).child(element))
            .into_any_element()
    }

    fn ref_group(&self, kind: RefKind, g: usize) -> Option<&RefGroupModel> {
        self.ref_load(kind).and_then(|l| l.groups.get(g))
    }

    fn render_ref_group(
        &self,
        page: &str,
        nav: &Nav,
        theme: &crate::ui::theme::Theme,
    ) -> AnyElement {
        let nav = nav.clone();
        let target = page.to_owned();
        div()
            .id(("refgroup", page.len()))
            .px(px(24.))
            .pt(px(10.))
            .pb(px(2.))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(theme.info)
            .cursor_pointer()
            .child(page.to_owned())
            .on_click(move |_, _, cx| nav(NavTarget::Page(target.clone()), cx))
            .into_any_element()
    }

    fn render_refs_heading(&self, kind: RefKind, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let section = self.ref_section(kind);
        let total = section.load.as_ref().map(|l| l.total);
        let label = match (kind, total) {
            (RefKind::Linked, Some(n)) => t!("refs.linked", count = n).to_string(),
            (RefKind::Linked, None) => t!("refs.linked_loading").to_string(),
            (RefKind::Unlinked, Some(n)) => t!("refs.unlinked_count", count = n).to_string(),
            (RefKind::Unlinked, None) => t!("refs.unlinked").to_string(),
        };
        let this = cx.entity();
        let toggle = this.clone();
        let mut line = h_flex()
            .px(px(24.))
            .pt(px(24.))
            .pb(px(4.))
            .gap_2()
            .items_center()
            .child(
                h_flex()
                    .id(("refs-heading", kind.tag()))
                    .gap_1()
                    .items_center()
                    .cursor_pointer()
                    .text_color(theme.muted_foreground)
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(
                        icon(if section.expanded {
                            IconName::ChevronDown
                        } else {
                            IconName::ChevronRight
                        })
                        .size(px(14.)),
                    )
                    .child(label)
                    .on_click(move |_, _, cx| {
                        toggle.update(cx, |v, cx| v.toggle_refs(kind, cx));
                    }),
            );
        if kind == RefKind::Linked {
            line = line.child(self.render_filter_popover(this));
        }
        line.into_any_element()
    }

    fn render_filter_popover(&self, this: crate::ui::Entity<Self>) -> AnyElement {
        let candidates: Vec<(String, Option<bool>)> = self
            .filter_candidates
            .iter()
            .map(|name| {
                let state = if self
                    .filters
                    .include
                    .iter()
                    .any(|f| f.eq_ignore_ascii_case(name))
                {
                    Some(true)
                } else if self
                    .filters
                    .exclude
                    .iter()
                    .any(|f| f.eq_ignore_ascii_case(name))
                {
                    Some(false)
                } else {
                    None
                };
                (name.clone(), state)
            })
            .collect();
        let active = !self.filters.is_empty();
        Popover::new("ref-filters")
            .anchor(Anchor::TopLeft)
            .trigger(
                Button::new("ref-filters-button")
                    .ghost()
                    .small()
                    .icon(IconName::Settings2)
                    .when(active, |b| b.label(t!("refs.filters_active").to_string())),
            )
            .content(move |_, _, cx| {
                let theme = cx.theme().clone();
                let mut body = v_flex().gap_1().p_2().min_w(px(220.)).max_h(px(320.));
                body = body.child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(t!("refs.filters_hint").to_string()),
                );
                if candidates.is_empty() {
                    body = body.child(t!("refs.filters_none").to_string());
                }
                for (n, (name, state)) in candidates.iter().enumerate() {
                    let this = this.clone();
                    let page = name.clone();
                    let mark = match state {
                        Some(true) => Some((IconName::Check, theme.success)),
                        Some(false) => Some((IconName::Ban, theme.danger)),
                        None => None,
                    };
                    body =
                        body.child(
                            h_flex()
                                .id(("filter", n))
                                .gap_2()
                                .px_1()
                                .rounded(px(4.))
                                .cursor_pointer()
                                .hover(|d| d.bg(theme.muted))
                                .child(div().w(px(14.)).children(
                                    mark.map(|(i, c)| icon(i).size(px(14.)).text_color(c)),
                                ))
                                .child(name.clone())
                                .on_click(move |_, _, cx| {
                                    this.update(cx, |v, cx| v.cycle_filter(&page, cx));
                                }),
                        );
                }
                if active {
                    let this = this.clone();
                    body = body.child(
                        Button::new("ref-filters-clear")
                            .ghost()
                            .small()
                            .label(t!("refs.filters_clear").to_string())
                            .on_click(move |_, _, cx| {
                                this.update(cx, |v, cx| v.clear_filters(cx));
                            }),
                    );
                }
                body
            })
            .into_any_element()
    }

    fn render_link_row(
        &self,
        links: &[Link],
        nav: &Nav,
        theme: &crate::ui::theme::Theme,
        base: usize,
    ) -> AnyElement {
        let mut line = h_flex().gap_1().flex_wrap().items_center().text_sm();
        for (n, link) in links.iter().enumerate() {
            let nav = nav.clone();
            let target = match &link.target {
                Route::Page(name) => NavTarget::Page(name.clone()),
                Route::Block(uuid) => NavTarget::Block(uuid.clone()),
                Route::Journals => NavTarget::Page(link.label.clone()),
            };
            if n > 0 {
                line = line.child(
                    div()
                        .text_color(theme.muted_foreground)
                        .child(NAMESPACE_SEPARATOR),
                );
            }
            line = line.child(
                div()
                    .id(("link", base + n))
                    .text_color(theme.info)
                    .cursor_pointer()
                    .child(link.label.clone())
                    .on_click(move |_, _, cx| nav(target.clone(), cx)),
            );
        }
        line.into_any_element()
    }

    fn render_header(&self, nav: &Nav, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let header = &self.header;
        let mut col = v_flex().px(px(24.)).pt(px(16.)).pb(px(8.)).gap_2();
        if !header.zoom.is_empty() {
            col = col.child(self.render_link_row(&header.zoom, nav, &theme, 0));
        } else {
            if !header.namespace.is_empty() {
                col = col.child(self.render_link_row(&header.namespace, nav, &theme, 100));
            }
            col = col.child(
                div()
                    .text_size(px(26.))
                    .font_weight(FontWeight::BOLD)
                    .child(header.title.clone()),
            );
            if let Some(from) = &header.redirected_from {
                col = col.child(
                    div()
                        .text_sm()
                        .text_color(theme.muted_foreground)
                        .child(t!("page.redirected_from", name = from).to_string()),
                );
            }
        }
        if !header.properties.is_empty() {
            col = col.child(properties_table(
                usize::MAX / 4096,
                &header.properties,
                &theme,
                Some(nav.clone()),
            ));
        }
        if !header.children.is_empty() {
            col = col
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(t!("page.namespace_children").to_string()),
                )
                .child(self.render_child_links(&header.children, nav, &theme));
        }
        col.into_any_element()
    }

    fn render_child_links(
        &self,
        links: &[Link],
        nav: &Nav,
        theme: &crate::ui::theme::Theme,
    ) -> AnyElement {
        let mut line = h_flex().gap_3().flex_wrap().text_sm();
        for (n, link) in links.iter().enumerate() {
            let nav = nav.clone();
            let target = match &link.target {
                Route::Page(name) => NavTarget::Page(name.clone()),
                Route::Block(uuid) => NavTarget::Block(uuid.clone()),
                Route::Journals => NavTarget::Page(link.label.clone()),
            };
            line = line.child(
                div()
                    .id(("child", n))
                    .text_color(theme.info)
                    .cursor_pointer()
                    .child(link.label.clone())
                    .on_click(move |_, _, cx| nav(target.clone(), cx)),
            );
        }
        line.into_any_element()
    }
}

impl Render for PageView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let body: AnyElement = match &self.state {
            LoadState::Empty => centered(t!("panel.page_host.empty").to_string()),
            LoadState::Loading => centered(t!("page.loading").to_string()),
            LoadState::Failed(message) => {
                centered(t!("page.load_failed", error = message).to_string())
            }
            LoadState::Loaded => list(
                self.list_state.clone(),
                cx.processor(|this, ix: usize, window, cx| this.render_item(ix, window, cx)),
            )
            .size_full()
            .into_any_element(),
        };
        v_flex()
            .size_full()
            .bg(theme.background)
            .text_color(theme.foreground)
            .child(div().flex_1().min_h_0().child(body))
    }
}

fn centered(text: String) -> AnyElement {
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .child(text)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TestGraph;
    use crate::ui::Entity;
    use crate::ui::testing::{TestAppContext, VisualTestContext, gpui_test};
    use crate::{settings::AppSettings, theme};
    use std::path::Path;

    fn setup(cx: &mut TestAppContext) {
        cx.update(|cx| {
            crate::ui::init(cx);
            theme::install(cx, AppSettings::default(), None);
        });
    }

    fn open(cx: &mut TestAppContext) -> (Entity<PageView>, &mut VisualTestContext) {
        cx.add_window_view(|_, cx| PageView::new(cx))
    }

    const SAMPLE: &str = "title:: T\n\n- TODO [#A] hi [[Bob]] #tag\n  SCHEDULED: <2024-05-01 Wed>\n- code\n  ```rust\n  fn x() {}\n  ```\n\t- child ![a](../assets/a.png)\n";

    /// Runs the executor until `done` holds (index reads happen on background threads).
    fn settle(
        cx: &mut VisualTestContext,
        view: &Entity<PageView>,
        done: impl Fn(&PageView) -> bool,
    ) {
        cx.executor().allow_parking();
        for _ in 0..400 {
            cx.run_until_parked();
            if view.read_with(cx, |v, _| done(v)) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("view did not settle");
    }

    fn page_route(name: &str) -> Route {
        Route::Page(name.into())
    }

    #[gpui_test]
    fn shows_the_empty_state_then_renders_rows_from_source(cx: &mut TestAppContext) {
        setup(cx);
        let (view, cx) = open(cx);
        assert_eq!(
            view.read_with(cx, |v, _| v.state().clone()),
            LoadState::Empty
        );
        view.update(cx, |v, cx| v.set_source("T", SAMPLE.as_bytes(), cx));
        cx.run_until_parked();
        assert_eq!(
            view.read_with(cx, |v, _| v.state().clone()),
            LoadState::Loaded
        );
        assert_eq!(view.read_with(cx, |v, _| v.rows().len()), 4);
        assert!(view.read_with(cx, |v, _| v.rendered_rows()) > 0);
    }

    #[gpui_test]
    fn clicking_a_ref_emits_navigation(cx: &mut TestAppContext) {
        setup(cx);
        let (view, cx) = open(cx);
        let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let sink = events.clone();
        let _sub = cx.update(|_, cx| {
            cx.subscribe(&view, move |_, event: &PageEvent, _| {
                sink.borrow_mut().push(event.clone());
            })
        });
        view.update(cx, |v, cx| v.set_source("T", SAMPLE.as_bytes(), cx));
        let target = view.read_with(cx, |v, _| v.rows()[1].block.title.links[0].1.clone());
        view.update(cx, |v, cx| v.activate(target, cx));
        assert_eq!(
            *events.borrow(),
            vec![PageEvent::Navigate(NavTarget::Page("Bob".into()))]
        );
    }

    #[gpui_test]
    fn open_file_loads_in_the_background(cx: &mut TestAppContext) {
        setup(cx);
        let tmp = tempfile::tempdir().expect("tmp");
        let file = tmp.path().join("page.md");
        std::fs::write(&file, SAMPLE).expect("write");
        let (view, cx) = open(cx);
        view.update(cx, |v, cx| v.open_file(file.clone(), "Page".into(), cx));
        assert_eq!(
            view.read_with(cx, |v, _| v.state().clone()),
            LoadState::Loading
        );
        cx.executor().allow_parking();
        cx.run_until_parked();
        assert_eq!(
            view.read_with(cx, |v, _| v.state().clone()),
            LoadState::Loaded
        );
        view.update(cx, |v, cx| {
            v.open_file(tmp.path().join("missing.md"), "X".into(), cx);
        });
        cx.run_until_parked();
        assert!(matches!(
            view.read_with(cx, |v, _| v.state().clone()),
            LoadState::Failed(_)
        ));
    }

    #[test]
    fn asset_paths_stay_inside_the_graph() {
        let root = Path::new("/g");
        assert_eq!(
            resolve_asset(root, "../assets/a.png"),
            Some(PathBuf::from("/g/assets/a.png"))
        );
        assert_eq!(resolve_asset(root, "../../etc/passwd"), None);
        assert_eq!(resolve_asset(root, "/etc/passwd"), None);
        assert_eq!(resolve_asset(root, "https://x.org/a.png"), None);
    }

    /// Every fixture page builds a model without panicking, and a sample of them is drawn.
    #[gpui_test]
    fn renders_every_fixture_page(cx: &mut TestAppContext) {
        setup(cx);
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/graphs");
        let mut pages = Vec::new();
        let mut stack = vec![root];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|e| e == "md") {
                    pages.push(path);
                }
            }
        }
        pages.sort();
        assert!(pages.len() > 300, "fixtures found: {}", pages.len());
        let (view, cx) = open(cx);
        let mut total_rows = 0;
        for page in &pages {
            let bytes = std::fs::read(page).expect("read fixture");
            view.update(cx, |v, cx| v.set_source("fixture", &bytes, cx));
            total_rows += view.read_with(cx, |v, _| v.rows().len());
        }
        cx.run_until_parked();
        assert!(total_rows > 1000, "rows: {total_rows}");
        assert!(view.read_with(cx, |v, _| v.rendered_rows()) > 0);
    }

    fn long_page(blocks: usize) -> String {
        let mut s = String::from("title:: Long\n\n");
        for n in 0..blocks {
            s.push_str(&format!("- block {n}\n"));
        }
        s
    }

    #[gpui_test]
    fn long_pages_load_in_chunks_as_the_list_scrolls(cx: &mut TestAppContext) {
        setup(cx);
        let g = TestGraph::new(&[("pages/Long.md", &long_page(200))]);
        let (view, cx) = open(cx);
        view.update(cx, |v, cx| {
            v.show(g.handle.clone(), page_route("Long"), None, cx);
        });
        settle(cx, &view, |v| *v.state() == LoadState::Loaded);
        // Drawing near the end of the list pulls more chunks, but never the whole page.
        for _ in 0..5 {
            cx.run_until_parked();
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let fetched = view.read_with(cx, |v, _| v.fetched());
        assert!(
            fetched >= FIRST_CHUNK,
            "first chunk is {FIRST_CHUNK}: {fetched}"
        );
        assert_eq!((fetched - FIRST_CHUNK) % CHUNK, 0, "then {CHUNK} per step");
        assert!(fetched < 200, "the page is not loaded at once: {fetched}");
        assert!(view.read_with(cx, |v, _| v.has_more()));
        // Header properties came out of the pre-block, not out of the rows.
        assert!(view.read_with(cx, |v, _| v.rows().iter().all(|r| r.block_index.is_some())));
        view.update(cx, |v, cx| v.request_more(cx));
        settle(cx, &view, |v| v.fetched() >= fetched + CHUNK);
        // The page-properties pre-block is fetched but is not a row.
        assert_eq!(
            view.read_with(cx, |v, _| v.rows().len()) + 1,
            view.read_with(cx, |v, _| v.fetched())
        );
        // Drain the rest.
        for _ in 0..10 {
            view.update(cx, |v, cx| v.request_more(cx));
            cx.run_until_parked();
            std::thread::sleep(std::time::Duration::from_millis(20));
            cx.run_until_parked();
        }
        assert_eq!(view.read_with(cx, |v, _| v.rows().len()), 200);
        assert!(!view.read_with(cx, |v, _| v.has_more()));
    }

    #[gpui_test]
    fn collapsed_blocks_hide_children_and_toggle_in_view_state(cx: &mut TestAppContext) {
        setup(cx);
        let g = TestGraph::new(&[(
            "pages/Tree.md",
            "- a\n  collapsed:: true\n\t- hidden\n\t\t- deeper\n- b\n",
        )]);
        let (view, cx) = open(cx);
        view.update(cx, |v, cx| {
            v.show(g.handle.clone(), page_route("Tree"), None, cx);
        });
        settle(cx, &view, |v| *v.state() == LoadState::Loaded);
        assert_eq!(view.read_with(cx, |v, _| v.rows().len()), 4);
        assert_eq!(view.read_with(cx, |v, _| v.visible_count()), 2);
        let blocks = |cx: &mut VisualTestContext| {
            view.read_with(cx, |v, _| {
                v.items()
                    .iter()
                    .filter(|i| matches!(i, Item::Block(_)))
                    .count()
            })
        };
        assert_eq!(blocks(cx), 2);
        view.update(cx, |v, cx| v.toggle_block(0, cx));
        assert_eq!(blocks(cx), 4);
        view.update(cx, |v, cx| v.toggle_block(0, cx));
        assert_eq!(blocks(cx), 2);
        // The file was not touched by the toggle (view-only state).
        let text = std::fs::read_to_string(g.path().join("pages/Tree.md")).expect("read");
        assert!(text.contains("collapsed:: true"));
    }

    #[gpui_test]
    fn header_shows_properties_namespaces_and_alias_redirects(cx: &mut TestAppContext) {
        setup(cx);
        let g = TestGraph::new(&[
            ("pages/Proj.md", "alias:: Project\ntags:: work\n\n- main\n"),
            ("pages/Project.md", ""),
            ("pages/Sub.md", "title:: Proj/Sub\n\n- sub page\n"),
        ]);
        let (view, cx) = open(cx);
        view.update(cx, |v, cx| {
            v.show(g.handle.clone(), page_route("Proj"), None, cx);
        });
        settle(cx, &view, |v| *v.state() == LoadState::Loaded);
        let header = view.read_with(cx, |v, _| v.header().clone());
        let keys: Vec<_> = header.properties.iter().map(|p| p.key.as_str()).collect();
        assert_eq!(keys, ["alias", "tags"]);
        assert_eq!(header.children.len(), 1);
        assert_eq!(header.children[0].label, "Proj/Sub");
        // The sub page shows its namespace breadcrumb.
        view.update(cx, |v, cx| {
            v.show(g.handle.clone(), page_route("Proj/Sub"), None, cx);
        });
        settle(cx, &view, |v| v.header().title == "Proj/Sub");
        let ns = view.read_with(cx, |v, _| v.header().namespace.clone());
        assert_eq!(ns.len(), 1);
        assert_eq!(ns[0].target, page_route("Proj"));
        // The empty alias page redirects to its target.
        view.update(cx, |v, cx| {
            v.show(g.handle.clone(), page_route("Project"), None, cx);
        });
        settle(cx, &view, |v| v.header().title == "Proj");
        assert_eq!(
            view.read_with(cx, |v, _| v.header().redirected_from.clone()),
            Some("Project".into())
        );
    }

    #[gpui_test]
    fn placeholder_pages_say_no_content_and_list_their_references(cx: &mut TestAppContext) {
        setup(cx);
        let g = TestGraph::new(&[("pages/Home.md", "- see [[Ghost]]\n- other\n")]);
        let (view, cx) = open(cx);
        view.update(cx, |v, cx| {
            v.show(g.handle.clone(), page_route("Ghost"), None, cx);
        });
        settle(cx, &view, |v| {
            *v.state() == LoadState::Loaded && v.linked().is_some()
        });
        assert!(view.read_with(cx, |v, _| v.header().placeholder));
        assert!(view.read_with(cx, |v, _| v.items().contains(&Item::Empty)));
        assert_eq!(
            view.read_with(cx, |v, _| v.linked().map(|l| l.total)),
            Some(1)
        );
        // A name the index has never seen is an empty placeholder too.
        view.update(cx, |v, cx| {
            v.show(g.handle.clone(), page_route("Never seen"), None, cx);
        });
        settle(cx, &view, |v| v.header().title == "Never seen");
        assert!(view.read_with(cx, |v, _| v.items().contains(&Item::Empty)));
    }

    #[gpui_test]
    fn block_refs_resolve_across_pages_and_zoom_shows_breadcrumbs(cx: &mut TestAppContext) {
        setup(cx);
        let id = "6500c1a4-0000-4000-8000-000000000001";
        let g = TestGraph::new(&[
            (
                "pages/Source.md",
                &format!("- parent\n\t- the answer is 42\n\t  id:: {id}\n\t\t- child of it\n"),
            ),
            ("pages/User.md", &format!("- quoting (({id}))\n")),
        ]);
        let (view, cx) = open(cx);
        view.update(cx, |v, cx| {
            v.show(g.handle.clone(), page_route("User"), None, cx);
        });
        settle(cx, &view, |v| *v.state() == LoadState::Loaded);
        let shown = view.read_with(cx, |v, _| v.rows()[0].block.title.text.clone());
        assert!(shown.contains("the answer is 42"), "{shown}");
        // Zooming into the referenced block gives breadcrumbs and the subtree.
        view.update(cx, |v, cx| {
            v.show(g.handle.clone(), Route::Block(id.into()), None, cx);
        });
        settle(cx, &view, |v| !v.header().zoom.is_empty());
        let labels: Vec<_> = view.read_with(cx, |v, _| {
            v.header().zoom.iter().map(|l| l.label.clone()).collect()
        });
        assert_eq!(labels, ["Source", "parent"]);
        assert_eq!(view.read_with(cx, |v, _| v.rows().len()), 2);
        assert_eq!(view.read_with(cx, |v, _| v.rows()[1].depth), 1);
        // The referenced block shows its reference count; opening it lists the referrer.
        assert_eq!(view.read_with(cx, |v, _| v.rows()[0].ref_count), 1);
        view.update(cx, |v, cx| v.toggle_referrers(0, cx));
        settle(cx, &view, |v| v.rows()[0].referrers.is_some());
        let refs = view.read_with(cx, |v, _| v.rows()[0].referrers.clone().unwrap_or_default());
        assert_eq!(refs[0].page, "User");
        view.update(cx, |v, cx| v.toggle_referrers(0, cx));
        assert!(view.read_with(cx, |v, _| v.rows()[0].referrers.is_none()));
    }

    #[gpui_test]
    fn linked_references_group_by_page_filter_and_unlinked_on_expand(cx: &mut TestAppContext) {
        setup(cx);
        let g = TestGraph::new(&[
            ("pages/Target.md", "- the target\n"),
            (
                "pages/A.md",
                "- one [[Target]]\n\t- child\n- two [[Target]]\n",
            ),
            ("pages/B.md", "- tagged [[Other]] [[Target]]\n"),
            ("pages/C.md", "- plain Target mention\n"),
        ]);
        let (view, cx) = open(cx);
        view.update(cx, |v, cx| {
            v.show(g.handle.clone(), page_route("Target"), None, cx);
        });
        settle(cx, &view, |v| v.linked().is_some());
        let groups: Vec<_> = view.read_with(cx, |v, _| {
            v.linked()
                .map(|l| {
                    l.groups
                        .iter()
                        .map(|g| (g.page.clone(), g.hits.len()))
                        .collect()
                })
                .unwrap_or_default()
        });
        assert_eq!(groups, [("A".to_owned(), 2), ("B".to_owned(), 1)]);
        assert_eq!(
            view.read_with(cx, |v, _| v.linked().map(|l| l.total)),
            Some(3)
        );
        // Unlinked references are computed only when the section opens.
        assert!(view.read_with(cx, |v, _| v.unlinked().is_none()));
        view.update(cx, |v, cx| v.toggle_refs(RefKind::Unlinked, cx));
        settle(cx, &view, |v| v.unlinked().is_some());
        let unlinked: Vec<_> = view.read_with(cx, |v, _| {
            v.unlinked()
                .map(|l| l.groups.iter().map(|g| g.page.clone()).collect())
                .unwrap_or_default()
        });
        assert_eq!(unlinked, ["C"]);
        // Excluding page A drops its group; the filter is in memory only.
        view.update(cx, |v, cx| v.cycle_filter("A", cx)); // include A
        settle(cx, &view, |v| {
            v.linked().is_some_and(|l| l.groups.len() == 1)
        });
        view.update(cx, |v, cx| v.cycle_filter("A", cx)); // exclude A
        settle(cx, &view, |v| {
            v.linked()
                .is_some_and(|l| l.groups.len() == 1 && l.groups[0].page == "B")
        });
        view.update(cx, |v, cx| v.clear_filters(cx));
        settle(cx, &view, |v| {
            v.linked().is_some_and(|l| l.groups.len() == 2)
        });
        assert!(
            !std::fs::read_to_string(g.path().join("pages/Target.md"))
                .expect("read")
                .contains("filters")
        );
    }

    #[gpui_test]
    fn page_refreshes_on_index_events_keeping_view_state(cx: &mut TestAppContext) {
        setup(cx);
        let g = TestGraph::new(&[("pages/Live.md", "- one\n- two\n\t- under two\n")]);
        let (view, cx) = open(cx);
        view.update(cx, |v, cx| {
            v.show(g.handle.clone(), page_route("Live"), None, cx);
        });
        settle(cx, &view, |v| *v.state() == LoadState::Loaded);
        // Collapse "two" in the view, then change the file on disk behind the index.
        view.update(cx, |v, cx| v.toggle_block(1, cx));
        assert_eq!(view.read_with(cx, |v, _| v.visible_count()), 2);
        let event = g.rewrite("pages/Live.md", "- one\n- two\n\t- under two\n- three\n");
        view.update(cx, |v, cx| v.on_index_event(&event, cx));
        cx.executor().advance_clock(REFRESH_DEBOUNCE);
        settle(cx, &view, |v| v.rows().len() == 4);
        // `two` is still collapsed after the refresh.
        assert_eq!(view.read_with(cx, |v, _| v.visible_count()), 3);
    }
}
