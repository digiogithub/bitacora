//! The search palette (Mod+K) and the actions palette (Mod+Shift+P), BIT-US-0078.
//!
//! Both are GPUI Kit `Command` palettes shown as a centered overlay. The search palette asks the
//! index (`IndexReader::search`, BIT-SP-0003.R14) on a background thread after a short debounce,
//! cancelling stale queries, and lists pages, blocks (with highlighted snippets) and a "create
//! page" entry. Everything is operable with the keyboard: arrows move, Enter opens, Shift+Enter
//! opens in the right sidebar, Tab cycles the scope, Esc clears and then closes.

use std::time::Duration;

use bitacora_index::search::{Scope, SearchHit, SearchOptions, Snippet};
use rust_i18n::t;

use crate::actions::{ClosePalette, CycleSearchScope, OpenResultInSidebar};
use crate::data::{self, GraphHandle};
use crate::nav::Route;
use crate::ui::button::{Button, ButtonVariants as _};
use crate::ui::command::{Command, CommandGroup, CommandItem, CommandState, IndexPath};
use crate::ui::text_edit::{FontWeight, HighlightStyle, StyledText};
use crate::ui::{
    ActiveTheme as _, App, AppContext as _, Context, Disableable as _, Entity, EventEmitter,
    FocusHandle, Hsla, IconName, InteractiveElement as _, IntoElement, MenuCancel,
    ParentElement as _, Render, Selectable as _, SharedString, Sizable as _, Styled as _,
    Subscription, Task, Window, div, h_flex, icon, px, v_flex,
};

/// Time the palette waits after a keystroke before querying the index.
pub const SEARCH_DEBOUNCE: Duration = Duration::from_millis(30);

/// Results asked from the index.
const RESULT_LIMIT: usize = 30;

/// Which palette is open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteMode {
    /// Search pages and blocks.
    Search,
    /// Run an application command.
    Commands,
}

/// Where a search runs (the scope chips).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SearchScope {
    /// Everything.
    #[default]
    All,
    /// Blocks of the page on screen.
    ThisPage,
    /// Journals only.
    Journals,
    /// Pages only (no journals).
    Pages,
}

impl SearchScope {
    const ALL: [Self; 4] = [Self::All, Self::ThisPage, Self::Journals, Self::Pages];

    fn label(self) -> String {
        match self {
            Self::All => t!("palette.scope_all"),
            Self::ThisPage => t!("palette.scope_page"),
            Self::Journals => t!("palette.scope_journals"),
            Self::Pages => t!("palette.scope_pages"),
        }
        .to_string()
    }

    fn next(self, has_page: bool) -> Self {
        let all = Self::ALL;
        let mut ix = all.iter().position(|s| *s == self).unwrap_or(0);
        loop {
            ix = (ix + 1) % all.len();
            if all[ix] != Self::ThisPage || has_page {
                return all[ix];
            }
        }
    }
}

/// An application command of the actions palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteCommand {
    /// Show the journals feed.
    GoJournals,
    /// Show the table of all pages.
    GoAllPages,
    /// Show the graph view.
    GoGraph,
    /// Show the tasks view (BIT-US-0126).
    GoTasks,
    /// Show or hide the left sidebar.
    ToggleLeftSidebar,
    /// Show or hide the right sidebar.
    ToggleRightSidebar,
    /// Switch between light and dark.
    ToggleTheme,
    /// History back.
    GoBack,
    /// History forward.
    GoForward,
    /// Rebuild the index of the open graph from the files.
    Reindex,
    /// Back to the graph picker.
    SwitchGraph,
    /// Move the page on screen to `logseq/.recycle/` (asks first).
    DeletePage,
    /// Rename the page on screen (keyboard route of the title click, BIT-T-0338).
    RenamePage,
    /// Commit, fetch, merge and push now.
    SyncNow,
    /// Open the sync panel (status, backend, preferences).
    SyncSettings,
    /// Open the history of the page on screen.
    PageHistory,
    /// Open the list of what MCP agents did, with undo.
    AgentActivity,
    /// Open the visual conflict resolver.
    ResolveConflicts,
    /// Open a graph from a git remote (clone).
    CloneGraph,
    /// Check GitHub Releases for a newer version (BIT-US-0100).
    CheckForUpdates,
    /// Open the settings (BIT-US-0107).
    OpenSettings,
    /// Choose a graph folder to open (BIT-US-0165).
    OpenGraph,
    /// Close the open graph and show the picker (BIT-US-0165).
    CloseGraph,
}

impl PaletteCommand {
    /// Every command, in palette order.
    pub const ALL: [Self; 23] = [
        Self::GoJournals,
        Self::GoAllPages,
        Self::GoGraph,
        Self::GoTasks,
        Self::GoBack,
        Self::GoForward,
        Self::ToggleLeftSidebar,
        Self::ToggleRightSidebar,
        Self::ToggleTheme,
        Self::Reindex,
        Self::DeletePage,
        Self::RenamePage,
        Self::SyncNow,
        Self::SyncSettings,
        Self::PageHistory,
        Self::AgentActivity,
        Self::ResolveConflicts,
        Self::CloneGraph,
        Self::SwitchGraph,
        Self::CheckForUpdates,
        Self::OpenSettings,
        Self::OpenGraph,
        Self::CloseGraph,
    ];

    /// The label shown (and matched against the query).
    pub fn label(self) -> String {
        match self {
            Self::GoJournals => t!("palette.cmd_journals"),
            Self::GoAllPages => t!("palette.cmd_all_pages"),
            Self::GoGraph => t!("palette.cmd_graph_view"),
            Self::GoTasks => t!("palette.cmd_tasks"),
            Self::GoBack => t!("palette.cmd_back"),
            Self::GoForward => t!("palette.cmd_forward"),
            Self::ToggleLeftSidebar => t!("palette.cmd_left_sidebar"),
            Self::ToggleRightSidebar => t!("palette.cmd_right_sidebar"),
            Self::ToggleTheme => t!("palette.cmd_theme"),
            Self::Reindex => t!("palette.cmd_reindex"),
            Self::SwitchGraph => t!("palette.cmd_switch_graph"),
            Self::DeletePage => t!("palette.cmd_delete_page"),
            Self::RenamePage => t!("palette.cmd_rename_page"),
            Self::SyncNow => t!("palette.cmd_sync_now"),
            Self::SyncSettings => t!("palette.cmd_sync_settings"),
            Self::OpenSettings => t!("settings.cmd_open"),
            Self::PageHistory => t!("palette.cmd_page_history"),
            Self::AgentActivity => t!("palette.cmd_agent_activity"),
            Self::ResolveConflicts => t!("palette.cmd_conflicts"),
            Self::CloneGraph => t!("palette.cmd_clone"),
            Self::OpenGraph => t!("palette.cmd_open_graph"),
            Self::CloseGraph => t!("palette.cmd_close_graph"),
            Self::CheckForUpdates => t!("update.check_updates"),
        }
        .to_string()
    }

    fn icon(self) -> IconName {
        match self {
            Self::GoJournals => IconName::Calendar,
            Self::GoAllPages => IconName::FileText,
            Self::GoGraph => IconName::Network,
            Self::GoTasks => IconName::CircleCheck,
            Self::GoBack => IconName::ArrowLeft,
            Self::GoForward => IconName::ArrowRight,
            Self::ToggleLeftSidebar | Self::ToggleRightSidebar => IconName::PanelLeft,
            Self::ToggleTheme => IconName::Sun,
            Self::Reindex => IconName::LoaderCircle,
            Self::SwitchGraph | Self::OpenGraph | Self::CloseGraph => IconName::Folder,
            Self::DeletePage => IconName::Close,
            Self::RenamePage => IconName::Replace,
            Self::SyncNow => IconName::RefreshCw,
            Self::SyncSettings | Self::OpenSettings => IconName::Settings,
            Self::PageHistory => IconName::Undo2,
            Self::AgentActivity => IconName::Bot,
            Self::ResolveConflicts => IconName::TriangleAlert,
            Self::CloneGraph => IconName::FolderOpen,
            Self::CheckForUpdates => IconName::LoaderCircle,
        }
    }
}

/// What the palette asks of its host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaletteEvent {
    /// Open a page or block (in the right sidebar when `sidebar`).
    Open {
        /// Target.
        route: Route,
        /// Show it in the right sidebar.
        sidebar: bool,
    },
    /// Run an application command.
    Run(PaletteCommand),
    /// The palette closed (restore the previous focus).
    Closed,
}

/// One search result as the palette shows it.
#[derive(Debug, Clone, PartialEq)]
pub enum Hit {
    /// A page matched by title.
    Page {
        /// Display title.
        title: String,
        /// Title with highlight ranges.
        snippet: Snippet,
        /// A journal page.
        is_journal: bool,
    },
    /// A block matched by content.
    Block {
        /// Block UUID.
        uuid: String,
        /// Title of the containing page.
        page: String,
        /// Content window with highlight ranges.
        snippet: Snippet,
    },
}

impl Hit {
    fn from_index(hit: SearchHit) -> Self {
        match hit {
            SearchHit::Page {
                title,
                snippet,
                is_journal,
                ..
            } => Self::Page {
                title,
                snippet,
                is_journal,
            },
            SearchHit::Block {
                uuid,
                page_title,
                snippet,
                ..
            } => Self::Block {
                uuid,
                page: page_title,
                snippet,
            },
        }
    }

    fn route(&self) -> Route {
        match self {
            Self::Page { title, .. } => Route::Page(title.clone()),
            Self::Block { uuid, .. } => Route::Block(uuid.clone()),
        }
    }
}

/// Runs a search; blocking, call from a background thread.
pub fn run_search(
    handle: &GraphHandle,
    query: &str,
    scope: SearchScope,
    current_page: Option<i64>,
) -> Result<Vec<Hit>, String> {
    let scope = match scope {
        SearchScope::All => Scope::All,
        SearchScope::ThisPage => current_page.map_or(Scope::All, Scope::Page),
        SearchScope::Journals => Scope::Journals,
        SearchScope::Pages => Scope::Pages,
    };
    let opts = SearchOptions {
        limit: RESULT_LIMIT,
        scope,
        today: data::today_key(),
        ..SearchOptions::default()
    };
    handle
        .reader
        .search(query, &opts)
        .map(|hits| hits.into_iter().map(Hit::from_index).collect())
        .map_err(|e| e.to_string())
}

/// What a row of the palette does when confirmed.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Pick {
    Route(Route),
    Create(String),
    Command(PaletteCommand),
}

/// The overlay view.
pub struct Palette {
    state: Entity<CommandState>,
    mode: Option<PaletteMode>,
    handle: Option<GraphHandle>,
    current_page: Option<i64>,
    scope: SearchScope,
    query: String,
    results: Vec<Hit>,
    recent: Vec<String>,
    searching: bool,
    generation: u64,
    search_task: Option<Task<()>>,
    layout: Vec<Vec<Pick>>,
    focus_stage: u8,
    focus: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl std::fmt::Debug for Palette {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Palette")
            .field("mode", &self.mode)
            .field("query", &self.query)
            .finish_non_exhaustive()
    }
}

impl EventEmitter<PaletteEvent> for Palette {}

impl Palette {
    /// A closed palette.
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let state = cx.new(|cx| CommandState::new(window, cx));
        Self {
            state,
            mode: None,
            handle: None,
            current_page: None,
            scope: SearchScope::All,
            query: String::new(),
            results: Vec::new(),
            recent: Vec::new(),
            searching: false,
            generation: 0,
            search_task: None,
            layout: Vec::new(),
            focus_stage: 0,
            focus: cx.focus_handle(),
            _subscriptions: Vec::new(),
        }
    }

    /// Whether a palette is showing.
    pub fn is_open(&self) -> bool {
        self.mode.is_some()
    }

    /// Whether the keyboard focus is inside the palette.
    pub fn has_focus(&self, window: &Window, cx: &App) -> bool {
        self.focus.contains_focused(window, cx)
    }

    /// The mode shown.
    pub fn mode(&self) -> Option<PaletteMode> {
        self.mode
    }

    /// The scope chip selected.
    pub fn scope(&self) -> SearchScope {
        self.scope
    }

    /// The results of the last finished query.
    pub fn results(&self) -> &[Hit] {
        &self.results
    }

    /// What is selected (tests).
    pub fn command_state(&self) -> &Entity<CommandState> {
        &self.state
    }

    /// Opens the search palette. `current_page` is the index id of the page on screen (the "This
    /// page" scope) and `recent` the pages suggested while the query is empty.
    pub fn open_search(
        &mut self,
        handle: Option<GraphHandle>,
        current_page: Option<i64>,
        recent: Vec<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.handle = handle;
        self.current_page = current_page;
        self.recent = recent;
        self.open(PaletteMode::Search, window, cx);
    }

    /// Opens the actions palette.
    pub fn open_commands(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open(PaletteMode::Commands, window, cx);
    }

    fn open(&mut self, mode: PaletteMode, window: &mut Window, cx: &mut Context<Self>) {
        self.mode = Some(mode);
        self.query.clear();
        self.results.clear();
        self.searching = false;
        self.search_task = None;
        self.scope = SearchScope::All;
        self.generation += 1;
        self.focus_stage = 1;
        self.state
            .update(cx, |state, cx| state.set_query("", window, cx));
        // Once the `Command` has been rendered the query field exists and can take the focus
        // right away (keystrokes typed immediately after the shortcut are not lost); the first
        // opening relies on the staged focus in `render`.
        self.state.update(cx, |state, cx| state.focus(window, cx));
        cx.notify();
    }

    /// Closes the palette and tells the host to restore the focus.
    pub fn close(&mut self, cx: &mut Context<Self>) {
        if self.mode.take().is_some() {
            self.search_task = None;
            self.generation += 1;
            cx.emit(PaletteEvent::Closed);
            cx.notify();
        }
    }

    /// The query changed (typed by the user): search after the debounce.
    pub fn set_query(&mut self, query: &str, cx: &mut Context<Self>) {
        if self.mode != Some(PaletteMode::Search) || self.query == query {
            return;
        }
        self.query = query.to_owned();
        self.start_search(cx);
    }

    /// Selects a scope chip and searches again.
    pub fn set_scope(&mut self, scope: SearchScope, cx: &mut Context<Self>) {
        let scope = if scope == SearchScope::ThisPage && self.current_page.is_none() {
            SearchScope::All
        } else {
            scope
        };
        if self.scope != scope {
            self.scope = scope;
            self.start_search(cx);
            cx.notify();
        }
    }

    fn start_search(&mut self, cx: &mut Context<Self>) {
        self.generation += 1;
        let generation = self.generation;
        let query = self.query.trim().to_owned();
        // Replacing the task cancels the previous (stale) query.
        self.search_task = None;
        let Some(handle) = self.handle.clone().filter(|_| !query.is_empty()) else {
            self.results.clear();
            self.searching = false;
            cx.notify();
            return;
        };
        self.searching = true;
        let scope = self.scope;
        let page = self.current_page;
        self.search_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SEARCH_DEBOUNCE).await;
            let result = cx
                .background_executor()
                .spawn(async move { run_search(&handle, &query, scope, page) })
                .await;
            let _ = this.update(cx, |palette, cx| {
                if palette.generation != generation {
                    return;
                }
                palette.searching = false;
                match result {
                    Ok(hits) => palette.results = hits,
                    Err(message) => {
                        tracing::warn!("search failed: {message}");
                        palette.results.clear();
                    }
                }
                cx.notify();
            });
        }));
    }

    fn confirm(&mut self, path: IndexPath, shift: bool, cx: &mut Context<Self>) {
        let Some(pick) = self
            .layout
            .get(path.section)
            .and_then(|g| g.get(path.row))
            .cloned()
        else {
            return;
        };
        self.accept(pick, shift, cx);
    }

    fn accept(&mut self, pick: Pick, sidebar: bool, cx: &mut Context<Self>) {
        match pick {
            Pick::Route(route) => cx.emit(PaletteEvent::Open { route, sidebar }),
            Pick::Create(name) => cx.emit(PaletteEvent::Open {
                route: Route::Page(name),
                sidebar,
            }),
            Pick::Command(cmd) => cx.emit(PaletteEvent::Run(cmd)),
        }
        self.close(cx);
    }

    fn open_in_sidebar(&mut self, _: &OpenResultInSidebar, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(path) = self.state.read(cx).selected_index() {
            self.confirm(path, true, cx);
        }
    }

    fn cycle_scope(&mut self, _: &CycleSearchScope, _: &mut Window, cx: &mut Context<Self>) {
        if self.mode == Some(PaletteMode::Search) {
            let next = self.scope.next(self.current_page.is_some());
            self.set_scope(next, cx);
        }
    }

    fn close_action(&mut self, _: &ClosePalette, window: &mut Window, cx: &mut Context<Self>) {
        let _ = window;
        self.close(cx);
    }

    fn cancel_action(&mut self, _: &MenuCancel, _: &mut Window, cx: &mut Context<Self>) {
        // The command state already cleared a non-empty query; reaching here means it was empty.
        self.close(cx);
    }

    fn snippet_text(snippet: &Snippet, accent: Hsla) -> StyledText {
        let highlights: Vec<_> = snippet
            .highlights
            .iter()
            .map(|r| {
                (
                    r.clone(),
                    HighlightStyle {
                        color: Some(accent),
                        font_weight: Some(FontWeight::BOLD),
                        ..HighlightStyle::default()
                    },
                )
            })
            .collect();
        StyledText::new(SharedString::from(snippet.text.clone())).with_highlights(highlights)
    }

    fn page_item(title: &str, snippet: &Snippet, is_journal: bool, accent: Hsla) -> CommandItem {
        let snippet = snippet.clone();
        CommandItem::new()
            .label(title.to_owned())
            .child(move |_, _| {
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(icon(if is_journal {
                        IconName::Calendar
                    } else {
                        IconName::FileText
                    }))
                    .child(Self::snippet_text(&snippet, accent))
            })
    }

    fn block_item(page: &str, snippet: &Snippet, accent: Hsla, muted: Hsla) -> CommandItem {
        let snippet = snippet.clone();
        let page = page.to_owned();
        CommandItem::new()
            .label(snippet.text.clone())
            .child(move |_, _| {
                v_flex()
                    .gap(px(1.))
                    .child(Self::snippet_text(&snippet, accent))
                    .child(div().text_xs().text_color(muted).child(page.clone()))
            })
    }

    /// Builds the groups for the current state and records what each row does.
    fn build(&mut self, accent: Hsla, muted: Hsla) -> Vec<CommandGroup> {
        let mut groups = Vec::new();
        let mut layout: Vec<Vec<Pick>> = Vec::new();
        match self.mode {
            Some(PaletteMode::Commands) => {
                let mut group = CommandGroup::new().label(t!("palette.commands").to_string());
                let mut picks = Vec::new();
                for cmd in PaletteCommand::ALL {
                    group = group.item(CommandItem::new().label(cmd.label()).icon(cmd.icon()));
                    picks.push(Pick::Command(cmd));
                }
                groups.push(group);
                layout.push(picks);
            }
            Some(PaletteMode::Search) if self.query.trim().is_empty() => {
                if !self.recent.is_empty() {
                    let mut group = CommandGroup::new().label(t!("palette.recent").to_string());
                    let mut picks = Vec::new();
                    for title in &self.recent {
                        group = group.item(
                            CommandItem::new()
                                .label(title.clone())
                                .icon(IconName::FileText),
                        );
                        picks.push(Pick::Route(Route::Page(title.clone())));
                    }
                    groups.push(group);
                    layout.push(picks);
                }
            }
            Some(PaletteMode::Search) => {
                let mut pages = CommandGroup::new().label(t!("palette.pages").to_string());
                let mut page_picks = Vec::new();
                let mut blocks = CommandGroup::new().label(t!("palette.blocks").to_string());
                let mut block_picks = Vec::new();
                for hit in &self.results {
                    match hit {
                        Hit::Page {
                            title,
                            snippet,
                            is_journal,
                        } => {
                            pages =
                                pages.item(Self::page_item(title, snippet, *is_journal, accent));
                            page_picks.push(Pick::Route(hit.route()));
                        }
                        Hit::Block { page, snippet, .. } => {
                            blocks = blocks.item(Self::block_item(page, snippet, accent, muted));
                            block_picks.push(Pick::Route(hit.route()));
                        }
                    }
                }
                if !page_picks.is_empty() {
                    groups.push(pages);
                    layout.push(page_picks);
                }
                if !block_picks.is_empty() {
                    groups.push(blocks);
                    layout.push(block_picks);
                }
                let name = self.query.trim().to_owned();
                let exists = self.results.iter().any(
                    |h| matches!(h, Hit::Page { title, .. } if title.eq_ignore_ascii_case(&name)),
                );
                if !exists && self.handle.is_some() {
                    groups.push(
                        CommandGroup::new()
                            .label(t!("palette.create").to_string())
                            .item(
                                CommandItem::new()
                                    .label(t!("palette.create_page", name = name).to_string())
                                    .icon(IconName::Plus),
                            ),
                    );
                    layout.push(vec![Pick::Create(name)]);
                }
            }
            None => {}
        }
        self.layout = layout;
        groups
    }
}

impl Render for Palette {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(mode) = self.mode else {
            return div().into_any_element();
        };
        let theme = cx.theme().clone();
        // The `Command` installs its model (so that the query field exists) while it is
        // rendered, i.e. after this function: focus it right after this frame is drawn.
        if self.focus_stage == 1 {
            self.focus_stage = 0;
            let state = self.state.clone();
            window.on_next_frame(move |window, cx| {
                state.update(cx, |state, cx| state.focus(window, cx));
            });
        }
        let groups = self.build(theme.info, theme.muted_foreground);
        let this = cx.entity();
        let query_this = this.clone();
        let confirm_this = this.clone();
        let cancel_this = this.clone();

        let mut command = Command::new(&self.state)
            .filterable(mode == PaletteMode::Commands)
            .placeholder(match mode {
                PaletteMode::Search => t!("palette.search_placeholder").to_string(),
                PaletteMode::Commands => t!("palette.commands_placeholder").to_string(),
            })
            .max_h(px(420.))
            .bordered(false)
            .on_query(move |query, _, cx| {
                let query = query.to_owned();
                query_this.update(cx, |p, cx| p.set_query(&query, cx));
            })
            .on_confirm(move |path, window, cx| {
                let shift = window.modifiers().shift;
                confirm_this.update(cx, |p, cx| p.confirm(path, shift, cx));
            })
            .on_cancel(move |_, cx| {
                cancel_this.update(cx, |p, cx| p.close(cx));
            })
            .empty({
                let searching = self.searching;
                let query_empty = self.query.trim().is_empty() && mode == PaletteMode::Search;
                let muted = theme.muted_foreground;
                move |_, _, _| {
                    div()
                        .p(px(16.))
                        .text_sm()
                        .text_color(muted)
                        .child(if searching {
                            t!("palette.searching").to_string()
                        } else if query_empty {
                            t!("palette.type_to_search").to_string()
                        } else {
                            t!("palette.no_results").to_string()
                        })
                }
            });
        for group in groups {
            command = command.group(group);
        }

        let mut chips = h_flex().gap_1().px(px(8.)).py(px(6.)).items_center();
        if mode == PaletteMode::Search {
            for scope in SearchScope::ALL {
                let enabled = scope != SearchScope::ThisPage || self.current_page.is_some();
                let chip_this = this.clone();
                chips = chips.child(
                    Button::new(("scope", scope as usize))
                        .small()
                        .ghost()
                        .selected(self.scope == scope)
                        .label(scope.label())
                        .disabled(!enabled)
                        .on_click(move |_, _, cx| {
                            chip_this.update(cx, |p, cx| p.set_scope(scope, cx));
                        }),
                );
            }
        }
        let backdrop = this.clone();
        div()
            .id("palette-overlay")
            .absolute()
            .inset_0()
            .flex()
            .justify_center()
            .items_start()
            .pt(px(72.))
            .on_mouse_down(crate::ui::text_edit::MouseButton::Left, move |_, _, cx| {
                backdrop.update(cx, |p, cx| p.close(cx));
            })
            .child(
                v_flex()
                    .id("palette")
                    .key_context("Palette")
                    .track_focus(&self.focus)
                    .on_action(cx.listener(Self::open_in_sidebar))
                    .on_action(cx.listener(Self::cycle_scope))
                    .on_action(cx.listener(Self::close_action))
                    .on_action(cx.listener(Self::cancel_action))
                    // Clicks inside the card must not reach the backdrop.
                    .on_mouse_down(crate::ui::text_edit::MouseButton::Left, |_, _, cx| {
                        cx.stop_propagation();
                    })
                    .w(px(680.))
                    .max_w_full()
                    .bg(theme.background)
                    .border_1()
                    .border_color(theme.border)
                    .rounded(px(8.))
                    .shadow_lg()
                    .child(chips)
                    .child(command),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TestGraph;
    use crate::ui::testing::{TestAppContext, VisualTestContext, gpui_test};
    use crate::{keymap, settings::AppSettings, theme};

    fn setup(cx: &mut TestAppContext) {
        cx.update(|cx| {
            crate::ui::init(cx);
            theme::install(cx, AppSettings::default(), None);
            keymap::load_with_user(cx, None).expect("keymap");
        });
    }

    fn open_palette(cx: &mut TestAppContext) -> (Entity<Palette>, &mut VisualTestContext) {
        cx.add_window_view(Palette::new)
    }

    fn graph() -> TestGraph {
        TestGraph::new(&[
            ("pages/Rust.md", "- the language\n"),
            ("pages/Notes.md", "- I like rust a lot\n- unrelated\n"),
            ("journals/2024_01_01.md", "- rusty nails\n"),
        ])
    }

    fn settle(cx: &mut VisualTestContext, palette: &Entity<Palette>, want: usize) {
        cx.executor().allow_parking();
        for _ in 0..400 {
            // The debounce is a timer on the (virtual) test clock.
            cx.executor().advance_clock(SEARCH_DEBOUNCE);
            cx.run_until_parked();
            if palette.read_with(cx, |p, _| p.results().len()) >= want {
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("search did not return {want} results");
    }

    #[test]
    fn scope_cycle_skips_this_page_without_a_page() {
        assert_eq!(SearchScope::All.next(false), SearchScope::Journals);
        assert_eq!(SearchScope::All.next(true), SearchScope::ThisPage);
        assert_eq!(SearchScope::Pages.next(true), SearchScope::All);
    }

    #[test]
    fn run_search_ranks_the_exact_page_first_and_honours_scopes() {
        let g = graph();
        let hits = run_search(&g.handle, "rust", SearchScope::All, None).expect("search");
        assert!(
            matches!(&hits[0], Hit::Page { title, .. } if title == "Rust"),
            "{hits:?}"
        );
        assert!(
            hits.iter()
                .any(|h| matches!(h, Hit::Block { page, .. } if page == "Notes"))
        );
        let journals = run_search(&g.handle, "rust", SearchScope::Journals, None).expect("search");
        assert!(!journals.is_empty());
        assert!(journals.iter().all(|h| match h {
            Hit::Block { page, .. } => page.contains("2024") || page.contains("Jan"),
            Hit::Page { is_journal, .. } => *is_journal,
        }));
        let pages = run_search(&g.handle, "rust", SearchScope::Pages, None).expect("search");
        assert!(pages.iter().all(|h| !matches!(
            h,
            Hit::Page {
                is_journal: true,
                ..
            }
        )));
    }

    #[gpui_test]
    fn typing_searches_and_enter_opens_the_selected_result(cx: &mut TestAppContext) {
        setup(cx);
        let g = graph();
        let (palette, cx) = open_palette(cx);
        let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let sink = events.clone();
        let _sub = cx.update(|_, cx| {
            cx.subscribe(&palette, move |_, e: &PaletteEvent, _| {
                sink.borrow_mut().push(e.clone());
            })
        });
        palette.update_in(cx, |p, window, cx| {
            p.open_search(
                Some(g.handle.clone()),
                None,
                vec!["Recent".into()],
                window,
                cx,
            );
        });
        cx.run_until_parked();
        assert!(palette.read_with(cx, |p, _| p.is_open()));
        cx.simulate_input("rust");
        settle(cx, &palette, 2);
        cx.run_until_parked();
        // The first row is the exact page; Enter opens it.
        cx.simulate_keystrokes("enter");
        assert_eq!(
            events.borrow().first(),
            Some(&PaletteEvent::Open {
                route: Route::Page("Rust".into()),
                sidebar: false
            }),
            "{:?}",
            events.borrow()
        );
        assert!(!palette.read_with(cx, |p, _| p.is_open()));
    }

    #[gpui_test]
    fn shift_enter_opens_in_the_sidebar_and_escape_closes(cx: &mut TestAppContext) {
        setup(cx);
        let g = graph();
        let (palette, cx) = open_palette(cx);
        let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let sink = events.clone();
        let _sub = cx.update(|_, cx| {
            cx.subscribe(&palette, move |_, e: &PaletteEvent, _| {
                sink.borrow_mut().push(e.clone());
            })
        });
        palette.update_in(cx, |p, window, cx| {
            p.open_search(Some(g.handle.clone()), None, Vec::new(), window, cx);
        });
        cx.run_until_parked();
        cx.simulate_input("rust");
        settle(cx, &palette, 2);
        cx.run_until_parked();
        cx.simulate_keystrokes("shift-enter");
        assert!(matches!(
            events.borrow().first(),
            Some(PaletteEvent::Open { sidebar: true, .. })
        ));
        palette.update_in(cx, |p, window, cx| {
            p.open_search(Some(g.handle.clone()), None, Vec::new(), window, cx);
        });
        cx.run_until_parked();
        cx.simulate_keystrokes("escape");
        assert!(!palette.read_with(cx, |p, _| p.is_open()));
        assert_eq!(events.borrow().last(), Some(&PaletteEvent::Closed));
    }

    #[gpui_test]
    fn commands_palette_filters_and_runs_a_command(cx: &mut TestAppContext) {
        setup(cx);
        let (palette, cx) = open_palette(cx);
        let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let sink = events.clone();
        let _sub = cx.update(|_, cx| {
            cx.subscribe(&palette, move |_, e: &PaletteEvent, _| {
                sink.borrow_mut().push(e.clone());
            })
        });
        palette.update_in(cx, |p, window, cx| p.open_commands(window, cx));
        cx.run_until_parked();
        cx.simulate_input("all pages");
        cx.run_until_parked();
        cx.simulate_keystrokes("enter");
        assert_eq!(
            events.borrow().first(),
            Some(&PaletteEvent::Run(PaletteCommand::GoAllPages))
        );
    }

    #[gpui_test]
    fn a_query_without_an_exact_page_offers_to_create_it(cx: &mut TestAppContext) {
        setup(cx);
        let g = graph();
        let (palette, cx) = open_palette(cx);
        let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let sink = events.clone();
        let _sub = cx.update(|_, cx| {
            cx.subscribe(&palette, move |_, e: &PaletteEvent, _| {
                sink.borrow_mut().push(e.clone());
            })
        });
        palette.update_in(cx, |p, window, cx| {
            p.open_search(Some(g.handle.clone()), None, Vec::new(), window, cx);
        });
        cx.run_until_parked();
        cx.simulate_input("zzzqqq");
        cx.executor().allow_parking();
        for _ in 0..100 {
            cx.run_until_parked();
            std::thread::sleep(Duration::from_millis(5));
        }
        cx.simulate_keystrokes("enter");
        assert_eq!(
            events.borrow().first(),
            Some(&PaletteEvent::Open {
                route: Route::Page("zzzqqq".into()),
                sidebar: false
            })
        );
    }
}
