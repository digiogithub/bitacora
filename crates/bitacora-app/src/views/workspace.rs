//! The main window content: left sidebar, dock area (center + right) and status bar.

use std::path::PathBuf;
use std::time::Duration;

use std::rc::Rc;

use async_channel::Sender;
use bitacora_core::date::Date;
use bitacora_core::editor::DayRollover;
use tokio::task::JoinError;

use crate::actions::{
    FocusRightSidebar, GoAllPages, GoBack, GoForward, GoJournals, OpenCommandPalette, OpenSearch,
    Quit, ToggleLeftSidebar, ToggleRightSidebar, ToggleTheme,
};
use crate::credentials::CredentialHub;
use crate::data::{self, GraphHandle};
use crate::editing;
use crate::graph_ops::{self, AssetOutcome};
use crate::graph_state::GraphState;
use crate::layout::{LAYOUT_VERSION, load_layout, save_layout};
use crate::nav::Route;
use crate::recent::{RecentGraphs, graph_name, has_graph_config};
use crate::session::{
    GraphSession, SessionEvent, SessionHandle, SessionLink, SessionNotice, SessionOptions,
    SyncSetup, initial_page,
};
use crate::sync_prefs::SyncPrefs;
use crate::theme;
use crate::tokio_bridge;
use crate::ui::dock::{DockArea, DockEvent, DockLayout, DockPlacement, DockSkin, panel_handle};
use crate::ui::{
    App, AppContext as _, Context, Entity, FluentBuilder as _, FocusHandle, Focusable,
    InteractiveElement as _, IntoElement, KeyDownEvent, ParentElement as _, Render, Styled as _,
    Subscription, Task, Window, div, h_flex, px, v_flex,
};
use crate::ui::{Level, notify};
use crate::views::agent_activity::{AgentActivityEvent, AgentActivityView};
use crate::views::conflicts::{ConflictsEvent, ConflictsView};
use crate::views::credential_dialog::CredentialDialog;
use crate::views::disk_conflict::{
    DiskConflictBanner, DiskConflictEvent, DiskDiffEvent, DiskDiffView,
};
use crate::views::history::{HistoryEvent, HistoryView};
use crate::views::main_view::{MainEvent, MainView};
use crate::views::page_view::PageView;
use crate::views::palette::{Palette, PaletteCommand, PaletteEvent};
use crate::views::panels::{HubEvent, PaneHub, PanelKind, PlaceholderPanel, SharedHub};
use crate::views::picker::{GraphPicker, PickerEvent};
use crate::views::right_sidebar::{RightSidebar, StackEvent};
use crate::views::settings::{SettingsContext, SettingsEvent, SettingsView};
use crate::views::sidebar::{LeftSidebar, SidebarEvent, Target};
use crate::views::status_bar::{AppStatusBar, Slot, SlotState, StatusBarEvent, StatusEvent};
use crate::views::sync_dialog::{SyncDialog, SyncDialogEvent};
use crate::views::sync_panel::{SyncPanel, SyncPanelEvent};
use crate::views::title_bar::AppTitleBar;
use bitacora_core::editor::MergeMode;
use bitacora_core::graph::PageKey;
use bitacora_core::queue::{Keep, Request, Source};
use rust_i18n::t;
use std::sync::Arc;

/// Delay before a layout change is written to disk.
const SAVE_DEBOUNCE: Duration = Duration::from_millis(500);

/// How long the "files not saved" notice stays up before the app quits.
const QUIT_NOTICE_TIME: Duration = Duration::from_secs(4);

/// Initial width of the right dock.
const RIGHT_DOCK_WIDTH: f32 = 280.0;

/// Source of "today" (local calendar date); replaced in tests.
pub type Clock = Rc<dyn Fn() -> Option<Date>>;

/// A [`Clock`] that can sit in a `Debug` struct.
struct ClockFn(Clock);

impl std::fmt::Debug for ClockFn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Clock")
    }
}

/// The day clock re-checks the date at least this often (also after the machine slept through
/// midnight).
const DAY_CHECK_MAX: Duration = Duration::from_secs(3600);

/// What the workspace needs to know about its environment.
#[derive(Debug, Clone, Default)]
pub struct WorkspaceConfig {
    /// Display name of the open graph.
    pub graph_name: Option<String>,
    /// Where the dock layout is persisted (`None` disables persistence).
    pub layout_file: Option<PathBuf>,
    /// Where the recent-graphs list is persisted (`None` disables persistence).
    pub recent_file: Option<PathBuf>,
    /// Explicit data directory for index databases (`None`: platform data dir).
    pub index_data_dir: Option<PathBuf>,
    /// Page to show after opening a graph (file path relative to the graph, or a page name).
    pub initial_page: Option<String>,
    /// Token file of the MCP endpoint (`None` leaves MCP off).
    pub mcp_token_path: Option<PathBuf>,
    /// Global config file override (`None`: platform default; tests point it nowhere).
    pub global_config: Option<PathBuf>,
    /// Directory of the per-graph UI state (recent pages, right sidebar stack); `None` keeps it
    /// in memory only.
    pub state_dir: Option<PathBuf>,
    /// Keep remote passwords in the OS keyring and start the askpass bridge for the system git
    /// (the app sets it; tests leave it off and use an in-memory store).
    pub system_credentials: bool,
    /// Where the user keymap is kept (`None`: the settings do not persist shortcuts).
    pub keymap_file: Option<PathBuf>,
    /// Keychain for MCP token secrets (`None`: the token file holds them).
    pub mcp_secrets: Option<Arc<dyn bitacora_mcp::SecretBackend>>,
}

/// The root view of the main window.
#[derive(Debug)]
pub struct Workspace {
    config: WorkspaceConfig,
    focus: FocusHandle,
    sidebar: Entity<LeftSidebar>,
    dock: Entity<DockArea>,
    status: Entity<AppStatusBar>,
    status_tx: Sender<StatusEvent>,
    picker: Entity<GraphPicker>,
    main: Entity<MainView>,
    hub: Entity<PaneHub>,
    stack: Entity<RightSidebar>,
    palette: Entity<Palette>,
    sync_dialog: Entity<SyncDialog>,
    credential_dialog: Entity<CredentialDialog>,
    sync_panel: Entity<SyncPanel>,
    settings: Entity<SettingsView>,
    history: Entity<HistoryView>,
    activity: Entity<AgentActivityView>,
    conflicts: Entity<ConflictsView>,
    disk_banner: Entity<DiskConflictBanner>,
    disk_diff: Entity<DiskDiffView>,
    credentials: Option<Arc<CredentialHub>>,
    prompt_task: Option<Task<()>>,
    sync_prefs: SyncPrefs,
    sync_prefs_file: Option<PathBuf>,
    session_handle: Option<SessionHandle>,
    sync_conflicted: bool,
    handle: Option<GraphHandle>,
    graph_state: GraphState,
    graph_state_file: Option<PathBuf>,
    clock: ClockFn,
    rollover: Option<DayRollover>,
    day_task: Option<Task<()>>,
    recents: RecentGraphs,
    graph_root: Option<PathBuf>,
    session: Option<GraphSession>,
    link: Option<SessionLink>,
    session_task: Option<Task<()>>,
    picker_visible: bool,
    save_task: Option<Task<()>>,
    heartbeat_task: Option<Task<Result<(), JoinError>>>,
    probe_task: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl Workspace {
    /// Builds the shell and restores the persisted dock layout when there is one.
    pub fn new(config: WorkspaceConfig, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        let sidebar = cx.new(|_| LeftSidebar::new(config.graph_name.clone()));
        let (status_tx, status_rx) = async_channel::bounded(256);
        let status = cx.new(|cx| AppStatusBar::new(status_rx, cx));
        let recents = config
            .recent_file
            .as_deref()
            .map(RecentGraphs::load)
            .unwrap_or_default();
        let picker = cx.new(|_| GraphPicker::new(recents.graphs().to_vec()));
        let main = cx.new(|cx| MainView::new(window, cx));
        let stack = cx.new(RightSidebar::new);
        let hub = cx.new(|_| PaneHub::new(main.clone(), stack.clone()));
        cx.set_global(SharedHub(hub.clone()));
        let palette = cx.new(|cx| Palette::new(window, cx));
        let sync_dialog = cx.new(|cx| SyncDialog::new(window, cx));
        let credential_dialog = cx.new(|cx| CredentialDialog::new(window, cx));
        let sync_panel = cx.new(|_| SyncPanel::new());
        let settings = cx.new(|cx| SettingsView::new(window, cx));
        let history = cx.new(|_| HistoryView::new());
        let activity = cx.new(|_| AgentActivityView::new());
        let conflicts = cx.new(|cx| ConflictsView::new(window, cx));
        let disk_banner = cx.new(|_| DiskConflictBanner::new());
        let disk_diff = cx.new(|_| DiskDiffView::new());

        let (dock, skin) = DockSkin::dock_area("workspace", Some(LAYOUT_VERSION), window, cx);
        skin.set_toggle_button_visible(false, cx);
        let mut subscriptions = vec![cx.subscribe_in(&dock, window, Self::on_dock_event)];
        subscriptions.push(cx.observe(&sidebar, |_, _, cx| cx.notify()));
        subscriptions.push(cx.observe(&status, |this, _, cx| this.sync_sidebar_footer(cx)));
        subscriptions.push(cx.subscribe_in(&picker, window, Self::on_picker_event));
        subscriptions.push(cx.observe(&main, |_, _, cx| cx.notify()));
        subscriptions.push(cx.subscribe_in(&main, window, Self::on_main_event));
        subscriptions.push(cx.subscribe_in(&hub, window, Self::on_hub_event));
        subscriptions.push(cx.subscribe_in(&stack, window, Self::on_stack_event));
        subscriptions.push(cx.subscribe_in(&palette, window, Self::on_palette_event));
        subscriptions.push(cx.observe(&palette, |_, _, cx| cx.notify()));
        subscriptions.push(cx.subscribe_in(&sidebar, window, Self::on_sidebar_event));
        subscriptions.push(cx.subscribe_in(&status, window, Self::on_status_event));
        subscriptions.push(cx.subscribe_in(&sync_panel, window, Self::on_sync_panel_event));
        subscriptions.push(cx.subscribe_in(&settings, window, Self::on_settings_event));
        subscriptions.push(cx.subscribe_in(&sync_dialog, window, Self::on_sync_dialog_event));
        subscriptions.push(cx.subscribe_in(
            &history,
            window,
            |this, _, _: &HistoryEvent, window, cx| {
                window.focus(&this.focus, cx);
            },
        ));
        subscriptions.push(cx.subscribe_in(
            &activity,
            window,
            |this, _, event: &AgentActivityEvent, window, cx| match event {
                AgentActivityEvent::Closed => window.focus(&this.focus, cx),
                AgentActivityEvent::OpenBlock(uuid) => {
                    this.activity.update(cx, |a, cx| a.close(cx));
                    this.navigate(Route::Block(uuid.clone()), cx);
                }
            },
        ));
        subscriptions.push(cx.subscribe_in(&conflicts, window, Self::on_conflicts_event));
        subscriptions.push(cx.subscribe_in(&disk_banner, window, Self::on_disk_event));
        subscriptions.push(cx.subscribe_in(
            &disk_diff,
            window,
            |this, _, _: &DiskDiffEvent, window, cx| {
                window.focus(&this.focus, cx);
            },
        ));

        let restored = config
            .layout_file
            .as_deref()
            .and_then(load_layout)
            .is_some_and(|state| {
                dock.update(cx, |area, cx| match area.load(state, window, cx) {
                    Ok(()) => true,
                    Err(err) => {
                        tracing::warn!("cannot restore workspace layout: {err}");
                        false
                    }
                })
            });
        if !restored {
            dock.update(cx, |area, cx| Self::default_layout(area, window, cx));
        }

        window.focus(&focus, cx);
        Self {
            config,
            focus,
            sidebar,
            dock,
            status,
            status_tx,
            picker,
            main,
            hub,
            stack,
            palette,
            sync_dialog,
            credential_dialog,
            sync_panel,
            settings,
            history,
            activity,
            conflicts,
            disk_banner,
            disk_diff,
            credentials: None,
            prompt_task: None,
            sync_prefs: SyncPrefs::default(),
            sync_prefs_file: None,
            session_handle: None,
            sync_conflicted: false,
            handle: None,
            graph_state: GraphState::default(),
            graph_state_file: None,
            clock: ClockFn(Rc::new(data::today_local)),
            rollover: None,
            day_task: None,
            recents,
            graph_root: None,
            session: None,
            link: None,
            session_task: None,
            picker_visible: true,
            save_task: None,
            heartbeat_task: None,
            probe_task: None,
            _subscriptions: subscriptions,
        }
    }

    /// The picker entity.
    pub fn picker(&self) -> &Entity<GraphPicker> {
        &self.picker
    }

    /// The reader of the open graph, once the session delivered it.
    pub fn graph_handle(&self) -> Option<&GraphHandle> {
        self.handle.as_ref()
    }

    /// The main area (journals feed and page view with history).
    pub fn main_view(&self) -> &Entity<MainView> {
        &self.main
    }

    /// Every pane (center panel) of the main area, primary first.
    pub fn panes(&self, cx: &App) -> Vec<Entity<MainView>> {
        self.hub.read(cx).panes()
    }

    /// The right sidebar stack.
    pub fn right_sidebar(&self) -> &Entity<RightSidebar> {
        &self.stack
    }

    /// The search / actions palette.
    pub fn palette(&self) -> &Entity<Palette> {
        &self.palette
    }

    /// Recently visited pages of the open graph, newest first.
    pub fn recent_pages(&self) -> &[String] {
        &self.graph_state.recent
    }

    /// The page view entity.
    pub fn page_view(&self, cx: &App) -> Entity<PageView> {
        self.main.read(cx).page().clone()
    }

    /// The open graph folder, if any.
    pub fn graph_root(&self) -> Option<&std::path::Path> {
        self.graph_root.as_deref()
    }

    /// Whether the picker is shown instead of the shell.
    pub fn picker_visible(&self) -> bool {
        self.picker_visible
    }

    /// Opens a graph: closes the current session, remembers the folder, starts the index
    /// session on a background thread and shows the initial page.
    pub fn open_graph(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        if !path.is_dir() {
            let error = format!("{} is not a folder", path.display());
            notify(
                window,
                cx,
                Level::Error,
                t!("picker.open_failed", error = error).to_string(),
            );
            return;
        }
        let path = path.canonicalize().unwrap_or(path);
        if self.graph_root.as_deref() == Some(path.as_path()) && self.session.is_some() {
            // Same graph again: just leave the picker.
            self.picker_visible = false;
            cx.notify();
            return;
        }
        let name = graph_name(&path);
        if !has_graph_config(&path) {
            notify(
                window,
                cx,
                Level::Warning,
                t!("picker.not_logseq", name = name.clone()).to_string(),
            );
        }
        self.close_session(cx);
        self.graph_state_file = self
            .config
            .state_dir
            .as_deref()
            .map(|dir| GraphState::file_for(dir, &path));
        self.graph_state = self
            .graph_state_file
            .as_deref()
            .map(GraphState::load)
            .unwrap_or_default();
        let recent_titles = self.graph_state.recent.clone();
        self.sidebar
            .update(cx, |sidebar, cx| sidebar.set_recent(recent_titles, cx));
        let stack_entries = self.graph_state.right_sidebar.clone();
        self.stack.update(cx, |stack, cx| {
            stack.clear(cx);
            stack.restore(&stack_entries, cx);
        });
        self.sync_prefs_file = self
            .config
            .state_dir
            .as_deref()
            .map(|dir| SyncPrefs::file_for(dir, &path));
        self.sync_prefs = self
            .sync_prefs_file
            .as_deref()
            .map(SyncPrefs::load)
            .unwrap_or_default();
        let sync = if self.sync_prefs.enabled {
            let hub = self.credentials(window, cx);
            Some(SyncSetup {
                branch: self.sync_prefs.branch.clone(),
                device: self.sync_prefs.device.clone(),
                cli: Some(hub.cli_config()),
                credentials: Some(hub.provider()),
                timing: self.sync_prefs.clone(),
            })
        } else {
            None
        };
        let app_settings = theme::try_settings(cx).unwrap_or_default();
        let started = GraphSession::start(
            path.clone(),
            SessionOptions {
                data_dir: self.config.index_data_dir.clone(),
                mcp_token_path: self
                    .config
                    .mcp_token_path
                    .clone()
                    .filter(|_| app_settings.mcp.enabled),
                global_config: self.config.global_config.clone(),
                sync,
                gate: None,
                mcp_secrets: self.config.mcp_secrets.clone(),
                mcp: app_settings.mcp.clone(),
                disable_substring: !app_settings.search.substring,
            },
        );
        let (session, events) = match started {
            Ok(started) => started,
            Err(err) => {
                notify(
                    window,
                    cx,
                    Level::Error,
                    t!("picker.open_failed", error = err.to_string()).to_string(),
                );
                return;
            }
        };
        self.session_handle = session.handle();
        self.session = Some(session);
        self.settings.update(cx, |s, cx| {
            s.set_mcp_endpoint(None, cx);
            s.set_mcp_unavailable(None, cx);
        });
        self.graph_root = Some(path.clone());
        crate::theme::set_graph_css(cx, Some(&path));
        self.picker_visible = false;
        self.sync_conflicted = false;
        self.disk_banner.update(cx, |b, cx| b.clear_all(cx));
        let enabled = self.sync_prefs.enabled;
        self.status.update(cx, |bar, cx| {
            bar.set_sync_view(None, cx);
            if enabled {
                bar.set_sync(SlotState::Busy, cx);
            }
        });
        self.refresh_sync_panel(cx);
        self.recents.touch(&path);
        self.persist_recents(cx);
        self.sidebar.update(cx, |sidebar, cx| {
            sidebar.set_graph_name(Some(name.clone()), cx)
        });
        window.set_window_title(&format!("Bitacora \u{2014} {name}"));
        self.status
            .update(cx, |bar, cx| bar.set_index(SlotState::Busy, cx));

        self.session_task = Some(cx.spawn_in(window, async move |this, cx| {
            while let Ok(event) = events.recv().await {
                let alive = this.update_in(cx, |ws, window, cx| {
                    ws.on_session_event(event, window, cx);
                });
                if alive.is_err() {
                    break;
                }
            }
        }));

        for pane in self.panes(cx) {
            pane.update(cx, |main, cx| main.clear_graph(cx));
        }
        cx.notify();
    }

    /// Saves the recent graphs list and refreshes the picker and the application menus.
    fn persist_recents(&mut self, cx: &mut Context<Self>) {
        if let Some(file) = &self.config.recent_file
            && let Err(err) = self.recents.save(file)
        {
            tracing::warn!("cannot save the recent graphs: {err}");
        }
        let recent_list = self.recents.graphs().to_vec();
        crate::menus::install(cx, &recent_list);
        self.picker
            .update(cx, |picker, cx| picker.set_recents(recent_list, cx));
    }

    /// Startup behaviour (BIT-US-0165): reopens the most recent graph unless the user turned
    /// "reopen last graph" off. A recent graph whose folder is gone is reported and pruned and the
    /// picker stays visible. Returns whether a graph is being opened.
    pub fn open_startup_graph(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let reopen = crate::theme::try_settings(cx).is_none_or(|s| s.reopen_last_graph);
        if !reopen {
            return false;
        }
        let Some(last) = self.recents.graphs().first().cloned() else {
            return false;
        };
        if !last.exists() {
            notify(
                window,
                cx,
                Level::Warning,
                t!(
                    "menu.last_graph_missing",
                    path = last.path.display().to_string()
                )
                .to_string(),
            );
            self.recents.remove(&last.path);
            self.persist_recents(cx);
            return false;
        }
        self.open_graph(last.path, window, cx);
        true
    }

    /// Shows the native folder dialog; the chosen folder is opened (BIT-US-0165).
    pub fn open_graph_dialog(&mut self, cx: &mut Context<Self>) {
        self.picker.update(cx, |picker, cx| picker.browse(cx));
    }

    /// Opens the `ix`-th (0-based) recent graph.
    pub fn open_recent(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(graph) = self.recents.graphs().get(ix).cloned() {
            self.open_graph(graph.path, window, cx);
        }
    }

    /// Closes the open graph and shows the picker.
    pub fn close_graph(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.graph_root.is_none() && self.session.is_none() {
            self.picker_visible = true;
            cx.notify();
            return;
        }
        self.save_now(cx);
        self.close_session(cx);
        self.picker_visible = true;
        self.sidebar
            .update(cx, |sidebar, cx| sidebar.set_graph_name(None, cx));
        window.set_window_title("Bitacora");
        cx.notify();
    }

    /// Stops the current session; the index is closed on a background thread so a still
    /// running reconcile never blocks the UI.
    fn close_session(&mut self, cx: &mut Context<Self>) {
        self.session_task = None;
        if let Some(session) = self.session.take() {
            cx.background_spawn(async move { session.close() }).detach();
        }
        self.link = None;
        self.day_task = None;
        self.rollover = None;
        self.handle = None;
        self.sidebar
            .update(cx, |sidebar, cx| sidebar.set_handle(None, cx));
        self.hub.update(cx, |hub, _| hub.set_handle(None));
        self.graph_root = None;
        crate::theme::set_graph_css(cx, None);
    }

    /// Replaces the clock (tests).
    pub fn set_clock(&mut self, clock: Clock) {
        self.clock = ClockFn(clock);
    }

    /// Makes today's journal available in core and re-checks the date at every local midnight
    /// (BIT-US-0057): the journal is a virtual page until it has content, so nothing is written
    /// here. The journals view lists the new day on its own clock.
    fn start_day_clock(&mut self, cx: &mut Context<Self>) {
        let today = (self.clock.0)();
        self.rollover = today.map(DayRollover::new);
        if let Some(today) = today {
            self.ensure_today(today, cx);
        }
        self.day_task = Some(cx.spawn(async move |this, cx| {
            loop {
                let wait = data::secs_until_midnight().map_or(DAY_CHECK_MAX, |secs| {
                    Duration::from_secs(secs.saturating_add(1)).min(DAY_CHECK_MAX)
                });
                cx.background_executor().timer(wait).await;
                if this.update(cx, |ws, cx| ws.on_day_tick(cx)).is_err() {
                    break;
                }
            }
        }));
    }

    /// Looks at the clock; on a new day today's journal is ensured again.
    pub fn on_day_tick(&mut self, cx: &mut Context<Self>) {
        let Some(now) = (self.clock.0)() else {
            return;
        };
        if let Some(day) = self.rollover.as_mut().and_then(|r| r.observe(now)) {
            self.ensure_today(day, cx);
        }
    }

    fn ensure_today(&mut self, day: Date, cx: &mut Context<Self>) {
        let Some(link) = self.link.clone() else {
            return;
        };
        let handle = self.handle.clone();
        cx.background_spawn(async move {
            let result = match &handle {
                Some(h) => graph_ops::ensure_today_templated(&link.queue, h, &link.config, day),
                None => graph_ops::ensure_today(&link.queue, &link.config, day),
            };
            if let Err(err) = result {
                tracing::warn!("cannot prepare today's journal: {err}");
            }
        })
        .detach();
    }

    /// Asks before moving the page `title` to `logseq/.recycle/`.
    pub fn request_delete_page(
        &mut self,
        title: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let this = cx.entity();
        let name = title.clone();
        let shown = crate::ui::confirm(
            window,
            cx,
            crate::ui::Confirmation {
                title: t!("delete.page_title", name = title).to_string(),
                description: t!("delete.page_description").to_string(),
                ok_text: t!("delete.ok").to_string(),
                cancel_text: t!("delete.cancel").to_string(),
            },
            move |window, cx| {
                let name = name.clone();
                this.update(cx, |ws, cx| ws.delete_page_now(name, window, cx));
            },
        );
        if !shown {
            tracing::debug!("page deletion needs a dialog host");
        }
    }

    /// Deletes the page without asking (the caller confirmed): recycle, favorites, recent list.
    pub fn delete_page_now(&mut self, title: String, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(link), Some(handle)) = (self.link.clone(), self.handle.clone()) else {
            return;
        };
        let name = title.clone();
        let task =
            cx.background_spawn(async move { graph_ops::delete_page(&link.queue, &handle, &name) });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |ws, window, cx| match result {
                Ok(done) => {
                    ws.after_page_deleted(&title, done.favorite_removed, window, cx);
                }
                Err(err) => notify(
                    window,
                    cx,
                    Level::Error,
                    t!("delete.failed", error = err.to_string()).to_string(),
                ),
            });
        })
        .detach();
    }

    fn after_page_deleted(
        &mut self,
        title: &str,
        favorite_removed: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.graph_state.forget_recent(title);
        let recent = self.graph_state.recent.clone();
        self.sidebar.update(cx, |sidebar, cx| {
            sidebar.set_recent(recent, cx);
            if favorite_removed {
                let kept: Vec<String> = sidebar
                    .favorites()
                    .iter()
                    .filter(|f| !f.eq_ignore_ascii_case(title))
                    .cloned()
                    .collect();
                sidebar.set_favorites(kept, cx);
            }
        });
        self.save_graph_state();
        // The page on screen is gone: show the journals instead.
        let showing = matches!(self.main.read(cx).route(), Some(Route::Page(n)) if n.eq_ignore_ascii_case(title));
        if showing {
            self.navigate(Route::Journals, cx);
        }
        notify(
            window,
            cx,
            Level::Success,
            t!("delete.page_done", name = title).to_string(),
        );
    }

    /// Renames the page `from` to `to` (BIT-T-0157). When `to` is an existing page nothing
    /// happens until the user confirms the merge in a dialog that lists what it does.
    pub fn rename_page(
        &mut self,
        from: String,
        to: String,
        merge: MergeMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (Some(link), Some(handle)) = (self.link.clone(), self.handle.clone()) else {
            return;
        };
        let task = {
            let (link, handle, from, to) = (link.clone(), handle, from.clone(), to.clone());
            cx.background_spawn(async move {
                graph_ops::rename_page(
                    &link.queue,
                    &handle,
                    &link.config,
                    link.lookup.clone(),
                    &from,
                    &to,
                    merge,
                )
            })
        };
        cx.spawn_in(window, async move |this, cx| {
            let outcome = task.await;
            let _ = this.update_in(cx, |ws, window, cx| match outcome {
                Ok(graph_ops::RenameOutcome::Renamed(report)) => {
                    ws.after_page_renamed(&from, &to, report.merged, window, cx);
                }
                Ok(graph_ops::RenameOutcome::NeedsMerge(preview)) => {
                    ws.confirm_merge(from, to, &preview, window, cx);
                }
                Err(err) => notify(
                    window,
                    cx,
                    Level::Error,
                    t!("rename.failed", error = err.to_string()).to_string(),
                ),
            });
        })
        .detach();
    }

    fn confirm_merge(
        &mut self,
        from: String,
        to: String,
        preview: &graph_ops::MergePreview,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let this = cx.entity();
        let from_title = from.clone();
        let (f, t_) = (from.clone(), preview.target.clone());
        let shown = crate::ui::confirm(
            window,
            cx,
            crate::ui::Confirmation {
                title: t!("rename.merge_title", from = from, to = preview.target).to_string(),
                description: merge_description(preview, &from_title),
                ok_text: t!("rename.merge_ok").to_string(),
                cancel_text: t!("delete.cancel").to_string(),
            },
            move |window, cx| {
                let (f, t_) = (f.clone(), t_.clone());
                this.update(cx, |ws, cx| {
                    ws.rename_page(
                        f,
                        t_,
                        MergeMode::Merge {
                            keep_aliases: false,
                        },
                        window,
                        cx,
                    );
                });
            },
        );
        if !shown {
            tracing::debug!(%to, "merge confirmation needs a dialog host");
        }
    }

    fn after_page_renamed(
        &mut self,
        from: &str,
        to: &str,
        merged: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.graph_state.forget_recent(from);
        self.graph_state.push_recent(to);
        let recent = self.graph_state.recent.clone();
        self.sidebar
            .update(cx, |sidebar, cx| sidebar.set_recent(recent, cx));
        self.save_graph_state();
        let showing = matches!(self.main.read(cx).route(), Some(Route::Page(n)) if n.eq_ignore_ascii_case(from));
        if showing {
            self.navigate(Route::Page(to.to_owned()), cx);
        }
        let key = if merged {
            "rename.merged"
        } else {
            "rename.done"
        };
        notify(
            window,
            cx,
            Level::Success,
            t!(key, from = from, to = to).to_string(),
        );
    }

    /// The "delete asset" action: refuses (and says so) while another block still uses the file,
    /// otherwise asks and moves it to `logseq/.recycle/`. `except_block` is the block the user is
    /// removing the link from.
    pub fn request_delete_asset(
        &mut self,
        link: String,
        except_block: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (Some(queue), Some(handle)) = (
            self.link.as_ref().map(|l| l.queue.clone()),
            self.handle.clone(),
        ) else {
            return;
        };
        let name = link.rsplit('/').next().unwrap_or(&link).to_owned();
        let check = {
            let (handle, link, except) = (handle.clone(), link.clone(), except_block.clone());
            cx.background_spawn(async move {
                let path = bitacora_core::recycle::asset_path_from_link(&link)?;
                graph_ops::asset_references(&handle, &path, except.as_deref()).ok()
            })
        };
        cx.spawn_in(window, async move |this, cx| {
            let references = check.await;
            let _ = this.update_in(cx, |_, window, cx| match references {
                Some(0) => {
                    let (queue, handle, link, except) = (
                        queue.clone(),
                        handle.clone(),
                        link.clone(),
                        except_block.clone(),
                    );
                    let name_done = name.clone();
                    let shown = crate::ui::confirm(
                        window,
                        cx,
                        crate::ui::Confirmation {
                            title: t!("delete.asset_title", name = name).to_string(),
                            description: t!("delete.asset_description").to_string(),
                            ok_text: t!("delete.ok").to_string(),
                            cancel_text: t!("delete.cancel").to_string(),
                        },
                        move |window, cx| {
                            let result =
                                graph_ops::delete_asset(&queue, &handle, &link, except.as_deref());
                            let (level, text) = match result {
                                Ok(AssetOutcome::Recycled(_)) => (
                                    Level::Success,
                                    t!("delete.asset_done", name = name_done).to_string(),
                                ),
                                Ok(AssetOutcome::Kept { references }) => (
                                    Level::Info,
                                    t!("delete.asset_kept", name = name_done, count = references)
                                        .to_string(),
                                ),
                                Err(err) => (
                                    Level::Error,
                                    t!("delete.failed", error = err.to_string()).to_string(),
                                ),
                            };
                            notify(window, cx, level, text);
                        },
                    );
                    if !shown {
                        tracing::debug!("asset deletion needs a dialog host");
                    }
                }
                Some(count) => notify(
                    window,
                    cx,
                    Level::Info,
                    t!("delete.asset_kept", name = name, count = count).to_string(),
                ),
                None => notify(
                    window,
                    cx,
                    Level::Error,
                    t!("delete.failed", error = name).to_string(),
                ),
            });
        })
        .detach();
    }

    /// The command queue of the open graph's session, once it is up.
    pub fn queue(&self) -> Option<&bitacora_core::queue::CommandQueue> {
        self.link.as_ref().map(|l| &l.queue)
    }

    /// The MCP endpoint of the open graph, when the server runs.
    pub fn mcp_endpoint(&self) -> Option<&str> {
        self.link.as_ref().and_then(|l| l.mcp_endpoint.as_deref())
    }

    fn on_session_event(
        &mut self,
        event: SessionEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            SessionEvent::Live(link) => {
                self.link = Some(link.clone());
                self.settings.update(cx, |s, cx| {
                    s.set_mcp_endpoint(link.mcp_endpoint.clone(), cx)
                });
                self.refresh_settings(window, cx);
                for pane in self.panes(cx) {
                    let link = link.clone();
                    pane.update(cx, |main, cx| main.set_session_link(link, window, cx));
                }
                self.start_day_clock(cx);
            }
            SessionEvent::Notice(notice) => {
                if let SessionNotice::McpUnavailable(why) = &notice {
                    self.settings
                        .update(cx, |s, cx| s.set_mcp_unavailable(Some(why.clone()), cx));
                }
                Self::show_notice(&notice, window, cx);
            }
            SessionEvent::Sync(view) => {
                let view = *view;
                let conflicted = view.status.conflicts > 0;
                if conflicted && !self.sync_conflicted {
                    notify(window, cx, Level::Warning, view.message.clone());
                }
                self.sync_conflicted = conflicted;
                self.status
                    .update(cx, |bar, cx| bar.set_sync_view(Some(view.clone()), cx));
                let prefs = self.sync_prefs.clone();
                self.settings
                    .update(cx, |s, cx| s.set_sync(prefs, Some(view.clone()), cx));
                self.sync_panel
                    .update(cx, |panel, cx| panel.set_view(Some(view), cx));
                if self.conflicts.read(cx).is_open() {
                    self.conflicts.update(cx, |c, cx| c.reload(cx));
                }
                self.refresh_page_conflicts(cx);
            }
            SessionEvent::DiskConflict(notice) => {
                self.disk_banner
                    .update(cx, |b, cx| b.set_notice(notice, cx));
            }
            SessionEvent::EditingConflict(conflict) => {
                for pane in self.panes(cx) {
                    pane.update(cx, |main, cx| main.on_editing_conflict(&conflict, cx));
                }
            }
            SessionEvent::DiskConflictCleared(key) => {
                self.disk_banner.update(cx, |b, cx| b.clear(&key, cx));
            }
            SessionEvent::Reader(handle) => {
                crate::perf::mark("index_opened");
                let initial = self
                    .config
                    .initial_page
                    .as_deref()
                    .map(|req| requested_route(&handle, req));
                self.handle = Some(handle.clone());
                self.hub
                    .update(cx, |hub, _| hub.set_handle(Some(handle.clone())));
                let favorites = handle.settings.config.favorites();
                self.sidebar.update(cx, |sidebar, cx| {
                    sidebar.set_favorites(favorites, cx);
                    sidebar.set_handle(Some(handle.clone()), cx);
                });
                self.stack
                    .update(cx, |stack, cx| stack.set_graph(handle.clone(), cx));
                for (ix, pane) in self.panes(cx).into_iter().enumerate() {
                    let handle = handle.clone();
                    let initial = if ix == 0 {
                        initial.clone()
                    } else {
                        Some(Route::Journals)
                    };
                    pane.update(cx, |main, cx| main.set_graph(handle, initial, cx));
                }
            }
            SessionEvent::Index(event) => {
                crate::views::widgets::on_index_event(&event, cx);
                self.sidebar
                    .update(cx, |sidebar, cx| sidebar.on_index_changed(cx));
                for pane in self.panes(cx) {
                    pane.update(cx, |main, cx| main.on_index_event(&event, cx));
                }
                self.stack
                    .update(cx, |stack, cx| stack.on_index_event(&event, cx));
            }
            SessionEvent::Ready(summary) => {
                crate::perf::mark("index_ready");
                tracing::info!(
                    scanned = summary.scanned,
                    parsed = summary.parsed,
                    errors = summary.errors,
                    cold = summary.cold_build,
                    ms = summary.elapsed_ms,
                    "index ready"
                );
                self.status
                    .update(cx, |bar, cx| bar.set_index(SlotState::Idle, cx));
                let mcp = if self.mcp_endpoint().is_some() {
                    SlotState::Idle
                } else {
                    SlotState::Off
                };
                self.status.update(cx, |bar, cx| bar.set_mcp(mcp, cx));
                // Pages opened while a cold build was still running may have been incomplete.
                for pane in self.panes(cx) {
                    pane.update(cx, |main, cx| main.reload(cx));
                }
                self.stack.update(cx, |stack, cx| stack.reload(cx));
                if summary.cold_build {
                    notify(
                        window,
                        cx,
                        Level::Success,
                        t!("picker.index_summary", count = summary.parsed).to_string(),
                    );
                }
            }
            SessionEvent::Failed(message) => {
                self.status
                    .update(cx, |bar, cx| bar.set_index(SlotState::Error, cx));
                // Without the index the page can still be read straight from its file.
                if let Some(root) = self.graph_root.clone()
                    && let Some((file, title)) =
                        initial_page(&root, self.config.initial_page.as_deref())
                {
                    let page = self.main.read(cx).page().clone();
                    page.update(cx, |page, cx| page.open_file(file, title, cx));
                }
                notify(
                    window,
                    cx,
                    Level::Error,
                    t!("picker.indexing_failed", error = message).to_string(),
                );
            }
        }
    }

    fn show_notice(notice: &SessionNotice, window: &mut Window, cx: &mut Context<Self>) {
        let (level, text) = match notice {
            SessionNotice::WriteFailed(n) => (
                Level::Error,
                t!("notice.write_failed", count = n).to_string(),
            ),
            SessionNotice::Conflict(n) => {
                (Level::Warning, t!("notice.conflict", count = n).to_string())
            }
            SessionNotice::WatcherDegraded => {
                (Level::Warning, t!("notice.watcher_degraded").to_string())
            }
            SessionNotice::ConfigChanged => (Level::Info, t!("notice.config_changed").to_string()),
            SessionNotice::IndexError { path, message } => (
                Level::Warning,
                t!("notice.index_error", path = path, error = message).to_string(),
            ),
            SessionNotice::McpUnavailable(error) => (
                Level::Warning,
                t!("notice.mcp_unavailable", error = error).to_string(),
            ),
            SessionNotice::SyncUnavailable(error) => (
                Level::Warning,
                t!("notice.sync_unavailable", error = error).to_string(),
            ),
            SessionNotice::SyncRecovery(text) => (
                Level::Info,
                t!("notice.sync_recovery", text = text).to_string(),
            ),
            SessionNotice::EditingBlockChanged => (
                Level::Warning,
                t!("notice.editing_block_changed").to_string(),
            ),
        };
        notify(window, cx, level, text);
    }

    fn on_picker_event(
        &mut self,
        _: &Entity<GraphPicker>,
        event: &PickerEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            PickerEvent::Open(path) => self.open_graph(path.clone(), window, cx),
            PickerEvent::CloneFromRemote => self.open_clone_dialog(window, cx),
            PickerEvent::Forget(path) => {
                self.recents.remove(path);
                self.persist_recents(cx);
            }
        }
    }

    fn on_sidebar_event(
        &mut self,
        _: &Entity<LeftSidebar>,
        event: &SidebarEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            SidebarEvent::Navigate(Target::GraphSwitcher) => {
                self.picker_visible = true;
                cx.notify();
            }
            SidebarEvent::Navigate(Target::Journals) => self.navigate(Route::Journals, cx),
            SidebarEvent::Navigate(Target::AllPages) => self.navigate(Route::AllPages, cx),
            SidebarEvent::Navigate(Target::Page(name)) => {
                self.navigate(Route::Page(name.clone()), cx);
            }
            // The Tasks and Graph views are routed by their own stories; until they land the
            // entries only highlight (BIT-US-0123). Wire them here.
            SidebarEvent::Navigate(Target::Tasks | Target::Graph) => {}
            SidebarEvent::OpenInSidebar(name) => {
                self.open_in_right_sidebar(Route::Page(name.clone()), window, cx);
            }
            SidebarEvent::OpenJournalDay(day) => {
                let title = self.handle.as_ref().and_then(|handle| {
                    let date = bitacora_core::date::Date::from_journal_day(*day)?;
                    Some(data::journal_title(handle, date))
                });
                if let Some(title) = title {
                    self.navigate(Route::Page(title), cx);
                }
            }
            SidebarEvent::GoToDate => self
                .palette
                .update(cx, |palette, cx| palette.open_commands(window, cx)),
        }
    }

    /// Mirrors the status bar's MCP and sync slots into the sidebar footer.
    fn sync_sidebar_footer(&mut self, cx: &mut Context<Self>) {
        let (mcp, sync) = {
            let bar = self.status.read(cx);
            (bar.slot(Slot::Mcp), bar.slot(Slot::Sync))
        };
        let endpoint = self.mcp_endpoint().map(str::to_owned);
        self.sidebar
            .update(cx, |s, cx| s.set_footer_status(mcp, endpoint, sync, cx));
    }

    /// Navigates the primary pane.
    pub fn navigate(&mut self, route: Route, cx: &mut Context<Self>) {
        self.main.update(cx, |main, cx| main.navigate(route, cx));
    }

    /// Shows `route` at the top of the right sidebar stack, opening the dock when it is closed.
    pub fn open_in_right_sidebar(
        &mut self,
        route: Route,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.stack.update(cx, |stack, cx| stack.open(route, cx));
        self.ensure_right_dock(window, cx);
    }

    fn ensure_right_dock(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.dock.update(cx, |area, cx| {
            if !area.is_dock_open(DockPlacement::Right) {
                area.toggle_dock(DockPlacement::Right, window, cx);
            }
        });
    }

    fn on_main_event(
        &mut self,
        _: &Entity<MainView>,
        event: &MainEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            MainEvent::Visited(route) => {
                if let Route::Page(name) = route {
                    self.graph_state.push_recent(name);
                    let recent = self.graph_state.recent.clone();
                    self.sidebar
                        .update(cx, |sidebar, cx| sidebar.set_recent(recent, cx));
                    self.save_graph_state();
                }
                self.sidebar
                    .update(cx, |sidebar, cx| sidebar.set_current(Some(route), cx));
                let day = match (route, &self.handle) {
                    (Route::Page(name), Some(handle)) => {
                        bitacora_core::journal::detect_journal(name, &handle.settings.config)
                            .map(|j| j.journal_day)
                    }
                    _ => None,
                };
                self.sidebar
                    .update(cx, |sidebar, cx| sidebar.set_selected_day(day, cx));
                let key = match route {
                    Route::Page(name) => Some(PageKey::from_title(name)),
                    _ => None,
                };
                let on_page = key.is_some();
                self.disk_banner.update(cx, |b, cx| b.set_current(key, cx));
                self.sync_panel
                    .update(cx, |p, cx| p.set_history_available(on_page, cx));
                self.refresh_page_conflicts(cx);
            }
            MainEvent::OpenInSidebar(route) => {
                self.open_in_right_sidebar(route.clone(), window, cx);
            }
            MainEvent::ConflictJump => self.open_conflicts(window, cx),
            MainEvent::RenamePage { from, to } => {
                self.rename_page(from.clone(), to.clone(), MergeMode::Refuse, window, cx);
            }
            MainEvent::DeleteAsset { link, block } => {
                self.request_delete_asset(link.clone(), block.clone(), window, cx);
            }
            MainEvent::BlockFocus(focus) => {
                if let Some(queue) = self.queue() {
                    editing::sync_editing_block(queue, &focus.title, focus.block_index);
                }
            }
        }
    }

    fn on_hub_event(
        &mut self,
        _: &Entity<PaneHub>,
        event: &HubEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let HubEvent::PaneAdded(pane) = event;
        if let Some(link) = self.link.clone() {
            pane.update(cx, |main, cx| main.set_session_link(link, window, cx));
        }
        self._subscriptions
            .push(cx.subscribe_in(pane, window, Self::on_main_event));
        self._subscriptions
            .push(cx.observe(pane, |_, _, cx| cx.notify()));
    }

    fn on_stack_event(
        &mut self,
        _: &Entity<RightSidebar>,
        event: &StackEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            StackEvent::Navigate(target) => {
                use crate::render::inline::NavTarget;
                match target {
                    NavTarget::Page(name) => self.navigate(Route::Page(name.clone()), cx),
                    NavTarget::Block(uuid) => self.navigate(Route::Block(uuid.clone()), cx),
                    NavTarget::Url(url) => cx.open_url(url),
                }
            }
            StackEvent::OpenInMain(route) => self.navigate(route.clone(), cx),
            StackEvent::Changed => {
                self.graph_state.right_sidebar = self.stack.read(cx).entries();
                self.save_graph_state();
            }
        }
    }

    fn save_graph_state(&self) {
        if let Some(file) = &self.graph_state_file
            && let Err(err) = self.graph_state.save(file)
        {
            tracing::warn!("cannot save the graph state: {err}");
        }
    }

    fn on_palette_event(
        &mut self,
        _: &Entity<Palette>,
        event: &PaletteEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            PaletteEvent::Open { route, sidebar } => {
                if *sidebar {
                    self.open_in_right_sidebar(route.clone(), window, cx);
                } else {
                    self.navigate(route.clone(), cx);
                }
            }
            PaletteEvent::Run(command) => self.run_command(*command, window, cx),
            PaletteEvent::Closed => window.focus(&self.focus, cx),
        }
    }

    /// Runs an actions-palette command.
    pub fn run_command(
        &mut self,
        command: PaletteCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match command {
            PaletteCommand::GoJournals => self.navigate(Route::Journals, cx),
            PaletteCommand::GoAllPages => self.navigate(Route::AllPages, cx),
            PaletteCommand::GoBack => self.main.update(cx, |m, cx| m.go_back(cx)),
            PaletteCommand::GoForward => self.main.update(cx, |m, cx| m.go_forward(cx)),
            PaletteCommand::ToggleLeftSidebar => {
                self.sidebar.update(cx, |sidebar, cx| sidebar.toggle(cx));
            }
            PaletteCommand::ToggleRightSidebar => self.dock.update(cx, |area, cx| {
                area.toggle_dock(DockPlacement::Right, window, cx);
            }),
            PaletteCommand::ToggleTheme => theme::toggle(cx, Some(window)),
            PaletteCommand::Reindex => self.reindex(window, cx),
            PaletteCommand::CheckForUpdates => crate::update::check_now(window.window_handle(), cx),
            PaletteCommand::DeletePage => {
                let page = match self.main.read(cx).route() {
                    Some(Route::Page(name)) => Some(name.clone()),
                    _ => None,
                };
                match page {
                    Some(name) => self.request_delete_page(name, window, cx),
                    None => notify(window, cx, Level::Info, t!("delete.not_a_page").to_string()),
                }
            }
            PaletteCommand::RenamePage => {
                let page = self.main.read(cx).page().clone();
                if page.read(cx).can_rename() {
                    page.update(cx, |p, cx| p.start_rename(window, cx));
                } else {
                    notify(window, cx, Level::Info, t!("delete.not_a_page").to_string());
                }
            }
            PaletteCommand::SwitchGraph => {
                self.picker_visible = true;
                cx.notify();
            }
            PaletteCommand::OpenGraph => self.open_graph_dialog(cx),
            PaletteCommand::CloseGraph => self.close_graph(window, cx),
            PaletteCommand::SyncNow => self.sync_now(window, cx),
            PaletteCommand::SyncSettings => self.open_sync_panel(cx),
            PaletteCommand::OpenSettings => self.open_settings(None, window, cx),
            PaletteCommand::PageHistory => self.open_history(window, cx),
            PaletteCommand::AgentActivity => self.open_agent_activity(window, cx),
            PaletteCommand::ResolveConflicts => self.open_conflicts(window, cx),
            PaletteCommand::CloneGraph => self.open_clone_dialog(window, cx),
        }
    }

    // ---- sync (BIT-US-0043, BIT-US-0046, BIT-US-0047, BIT-US-0048, BIT-US-0054, BIT-US-0070) ----

    /// The credential hub, created on first use: prompts from the engine and the onboarding
    /// threads arrive in the credential dialog.
    pub fn credentials(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Arc<CredentialHub> {
        if let Some(hub) = &self.credentials {
            return hub.clone();
        }
        let (tx, rx) = async_channel::unbounded();
        let hub = Arc::new(if self.config.system_credentials {
            CredentialHub::start(tx)
        } else {
            CredentialHub::start_in_memory(tx, None)
        });
        self.prompt_task = Some(cx.spawn_in(window, async move |this, cx| {
            while let Ok(request) = rx.recv().await {
                let alive = this.update_in(cx, |ws, window, cx| {
                    ws.credential_dialog
                        .update(cx, |dialog, cx| dialog.ask(request, window, cx));
                });
                if alive.is_err() {
                    break;
                }
            }
        }));
        self.credentials = Some(hub.clone());
        hub
    }

    fn refresh_sync_panel(&mut self, cx: &mut Context<Self>) {
        let askpass = self.credentials.as_ref().map_or_else(
            || crate::credentials::askpass_helper_path().is_some(),
            |h| h.askpass_active(),
        );
        let (root, prefs) = (self.graph_root.clone(), self.sync_prefs.clone());
        self.sync_panel
            .update(cx, |panel, cx| panel.set_prefs(root, prefs, askpass, cx));
        self.sync_panel
            .update(cx, |panel, cx| panel.set_view(None, cx));
        let prefs = self.sync_prefs.clone();
        self.settings
            .update(cx, |s, cx| s.set_sync(prefs, None, cx));
    }

    /// "Sync now" / "Retry": asks the engine for a cycle.
    pub fn sync_now(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(hub) = &self.credentials {
            hub.reset_cancel();
        }
        let Some(handle) = self.session_handle.clone() else {
            return;
        };
        let rx = handle.run(|s| s.sync_now());
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(false) = rx.recv().await {
                // No engine runs: sync is off for this graph, so show where to turn it on.
                let _ = this.update_in(cx, |ws, window, cx| {
                    notify(window, cx, Level::Info, t!("sync.panel.off").to_string());
                    ws.open_sync_panel(cx);
                });
            }
        })
        .detach();
    }

    /// Shows the sync panel.
    pub fn open_sync_panel(&mut self, cx: &mut Context<Self>) {
        let on_page = matches!(self.main.read(cx).route(), Some(Route::Page(_)));
        self.sync_panel.update(cx, |panel, cx| {
            panel.set_history_available(on_page, cx);
            panel.show(cx);
        });
    }

    /// The sync panel entity.
    pub fn sync_panel(&self) -> &Entity<SyncPanel> {
        &self.sync_panel
    }

    /// The sync dialog entity.
    pub fn sync_dialog(&self) -> &Entity<SyncDialog> {
        &self.sync_dialog
    }

    /// The credential dialog entity.
    pub fn credential_dialog(&self) -> &Entity<CredentialDialog> {
        &self.credential_dialog
    }

    /// The history overlay entity.
    pub fn history(&self) -> &Entity<HistoryView> {
        &self.history
    }

    /// The conflict resolver entity.
    pub fn conflicts(&self) -> &Entity<ConflictsView> {
        &self.conflicts
    }

    /// The "page changed on disk" banner entity.
    pub fn disk_banner(&self) -> &Entity<DiskConflictBanner> {
        &self.disk_banner
    }

    /// The disk diff overlay entity.
    pub fn disk_diff(&self) -> &Entity<DiskDiffView> {
        &self.disk_diff
    }

    /// The sync preferences of the open graph.
    pub fn sync_prefs(&self) -> &SyncPrefs {
        &self.sync_prefs
    }

    /// Opens "Open graph from a remote".
    pub fn open_clone_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let parent = self
            .graph_root
            .as_deref()
            .and_then(|r| r.parent())
            .map(std::path::Path::to_path_buf)
            .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
            .unwrap_or_default();
        let hub = self.credentials(window, cx);
        self.sync_dialog
            .update(cx, |d, cx| d.open_clone(parent, Some(hub), window, cx));
    }

    /// Opens "Enable sync" for the open graph.
    pub fn open_enable_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(root) = self.graph_root.clone() else {
            return;
        };
        let hub = self.credentials(window, cx);
        let prefs = self.sync_prefs.clone();
        self.sync_dialog.update(cx, |d, cx| {
            d.open_enable(root, &prefs, Some(hub), window, cx)
        });
    }

    /// The "Agent activity" overlay entity.
    pub fn agent_activity(&self) -> &Entity<AgentActivityView> {
        &self.activity
    }

    /// Opens "Agent activity": what MCP clients did, with undo (BIT-US-0022).
    pub fn open_agent_activity(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.session_handle.clone() {
            Some(handle) => self.activity.update(cx, |a, cx| a.open_with(handle, cx)),
            None => notify(
                window,
                cx,
                Level::Info,
                t!("activity.no_session").to_string(),
            ),
        }
    }

    /// Opens the history of the page on screen.
    pub fn open_history(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let title = match self.main.read(cx).route() {
            Some(Route::Page(name)) => Some(name.clone()),
            _ => None,
        };
        let (Some(title), Some(handle), Some(graph)) =
            (title, self.session_handle.clone(), self.handle.clone())
        else {
            notify(window, cx, Level::Info, t!("delete.not_a_page").to_string());
            return;
        };
        let rel = graph
            .reader
            .page_by_name(&title)
            .ok()
            .flatten()
            .and_then(|p| p.file_path);
        match rel {
            Some(rel) => self
                .history
                .update(cx, |h, cx| h.open_for(handle, rel, title, cx)),
            None => notify(window, cx, Level::Info, t!("history.empty").to_string()),
        }
    }

    /// Recomputes the sync conflicts of the page on screen: the banner count and the
    /// conflict markers on its blocks.
    fn refresh_page_conflicts(&mut self, cx: &mut Context<Self>) {
        let title = match self.main.read(cx).route() {
            Some(Route::Page(name)) => Some(name.clone()),
            _ => None,
        };
        let rel = title
            .zip(self.handle.as_ref())
            .and_then(|(title, graph)| graph.reader.page_by_name(&title).ok().flatten())
            .and_then(|page| page.file_path);
        let cards = match (&rel, &self.graph_root) {
            (Some(_), Some(root)) if self.sync_conflicted => {
                crate::views::conflicts::load_cards(root)
            }
            _ => Vec::new(),
        };
        let mine: Vec<_> = cards
            .iter()
            .filter(|c| Some(&c.path) == rel.as_ref() && c.resolution.is_none())
            .collect();
        let blocks = mine.iter().filter_map(|c| c.block_key.clone()).collect();
        let count = mine.len();
        self.disk_banner
            .update(cx, |b, cx| b.set_sync_conflicts(count, cx));
        let page = self.main.read(cx).page().clone();
        page.update(cx, |p, cx| p.set_conflict_blocks(blocks, cx));
    }

    /// Opens the conflict resolver.
    pub fn open_conflicts(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(root), Some(handle)) = (self.graph_root.clone(), self.session_handle.clone())
        else {
            return;
        };
        self.conflicts
            .update(cx, |c, cx| c.open_for(root, handle, window, cx));
    }

    /// Stops the session and opens the same graph again (sync preferences changed).
    fn restart_session(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(root) = self.graph_root.clone() else {
            return;
        };
        let session = self.take_session();
        self.hub.update(cx, |hub, _| hub.set_handle(None));
        let task = cx.background_spawn(async move {
            if let Some(session) = session {
                session.close();
            }
            root
        });
        cx.spawn_in(window, async move |this, cx| {
            let root = task.await;
            let _ = this.update_in(cx, |ws, window, cx| {
                ws.graph_root = None;
                ws.open_graph(root, window, cx);
            });
        })
        .detach();
    }

    // ---- settings (BIT-US-0107) ----

    /// Opens the settings on `section` (the last one when `None`).
    pub fn open_settings(
        &mut self,
        section: Option<crate::views::settings::Section>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.refresh_settings(window, cx);
        self.settings
            .update(cx, |settings, cx| settings.show(section, window, cx));
    }

    fn open_settings_action(
        &mut self,
        _: &crate::actions::OpenSettings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_settings(None, window, cx);
    }

    fn open_graph_action(
        &mut self,
        _: &crate::actions::OpenGraph,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_graph_dialog(cx);
    }

    fn close_graph_action(
        &mut self,
        _: &crate::actions::CloseGraph,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_graph(window, cx);
    }

    /// The settings view of the workspace.
    pub fn settings(&self) -> &Entity<SettingsView> {
        &self.settings
    }

    /// Hands the settings the handles of the open graph and, from the session thread, the MCP
    /// tokens and write policy.
    fn refresh_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let ctx = SettingsContext {
            root: self.graph_root.clone(),
            queue: self.link.as_ref().map(|l| l.queue.clone()),
            session: self.session_handle.clone(),
            global_config: self.config.global_config.clone(),
            keymap_file: self.config.keymap_file.clone(),
            tokens: None,
            policy: None,
        };
        let session = ctx.session.clone();
        self.settings
            .update(cx, |settings, cx| settings.set_context(ctx.clone(), cx));
        let Some(session) = session else {
            return;
        };
        let rx = session.run(|s| (s.mcp_tokens(), s.mcp_policy()));
        cx.spawn_in(window, async move |this, cx| {
            if let Ok((tokens, policy)) = rx.recv().await {
                let _ = this.update(cx, |ws, cx| {
                    let ctx = SettingsContext {
                        tokens,
                        policy,
                        ..ctx
                    };
                    ws.settings
                        .update(cx, |settings, cx| settings.set_context(ctx, cx));
                });
            }
        })
        .detach();
    }

    fn on_settings_event(
        &mut self,
        _: &Entity<SettingsView>,
        event: &SettingsEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let close = |ws: &mut Self, cx: &mut Context<Self>| {
            ws.settings.update(cx, |s, cx| s.close(cx));
        };
        match event {
            SettingsEvent::Closed => window.focus(&self.focus, cx),
            SettingsEvent::Reindex => {
                close(self, cx);
                self.reindex(window, cx);
            }
            SettingsEvent::ReopenGraph => {
                close(self, cx);
                self.restart_session(window, cx);
            }
            SettingsEvent::FavoritesChanged(favorites) => {
                let favorites = favorites.clone();
                self.sidebar
                    .update(cx, |sidebar, cx| sidebar.set_favorites(favorites, cx));
            }
            SettingsEvent::SyncNow => self.sync_now(window, cx),
            SettingsEvent::EnableSync => {
                close(self, cx);
                self.open_enable_dialog(window, cx);
            }
            SettingsEvent::DisableSync => {
                self.sync_prefs.enabled = false;
                self.save_sync_prefs();
                close(self, cx);
                self.restart_session(window, cx);
            }
            SettingsEvent::OpenSyncPanel => {
                close(self, cx);
                self.open_sync_panel(cx);
            }
            SettingsEvent::SyncTiming {
                idle,
                max,
                fetch,
                squash,
            } => {
                self.sync_prefs.commit_idle_secs = *idle;
                self.sync_prefs.commit_max_secs = *max;
                self.sync_prefs.fetch_interval_secs = *fetch;
                self.sync_prefs.squash_auto_commits = *squash;
                self.sync_prefs = self.sync_prefs.clone().clamped();
                self.save_sync_prefs();
                if self.sync_prefs.enabled {
                    close(self, cx);
                    self.restart_session(window, cx);
                }
            }
        }
    }

    fn save_sync_prefs(&self) {
        if let Some(file) = &self.sync_prefs_file
            && let Err(err) = self.sync_prefs.save(file)
        {
            tracing::warn!("cannot save the sync preferences: {err}");
        }
    }

    fn on_status_event(
        &mut self,
        _: &Entity<AppStatusBar>,
        event: &StatusBarEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            StatusBarEvent::SyncClicked => self.open_sync_panel(cx),
            StatusBarEvent::SyncNow => self.sync_now(window, cx),
        }
    }

    fn on_sync_panel_event(
        &mut self,
        _: &Entity<SyncPanel>,
        event: &SyncPanelEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            SyncPanelEvent::SyncNow => self.sync_now(window, cx),
            SyncPanelEvent::Enable => {
                self.sync_panel.update(cx, |p, cx| p.close(cx));
                self.open_enable_dialog(window, cx);
            }
            SyncPanelEvent::Disable => {
                self.sync_prefs.enabled = false;
                self.save_sync_prefs();
                self.sync_panel.update(cx, |p, cx| p.close(cx));
                self.restart_session(window, cx);
            }
            SyncPanelEvent::OpenConflicts => {
                self.sync_panel.update(cx, |p, cx| p.close(cx));
                self.open_conflicts(window, cx);
            }
            SyncPanelEvent::OpenHistory => {
                self.sync_panel.update(cx, |p, cx| p.close(cx));
                self.open_history(window, cx);
            }
            SyncPanelEvent::Closed => window.focus(&self.focus, cx),
        }
    }

    fn on_sync_dialog_event(
        &mut self,
        _: &Entity<SyncDialog>,
        event: &SyncDialogEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            SyncDialogEvent::Enabled {
                prefs, needs_merge, ..
            } => {
                self.sync_prefs = prefs.clone();
                self.save_sync_prefs();
                let text = if *needs_merge {
                    t!("sync.dialog.needs_merge").to_string()
                } else {
                    t!("sync.dialog.enabled").to_string()
                };
                notify(window, cx, Level::Success, text);
                self.restart_session(window, cx);
            }
            SyncDialogEvent::Cloned { root, prefs } => {
                let canonical = root.canonicalize().unwrap_or_else(|_| root.clone());
                if let Some(dir) = &self.config.state_dir
                    && let Err(err) = prefs.save(&SyncPrefs::file_for(dir, &canonical))
                {
                    tracing::warn!("cannot save the sync preferences: {err}");
                }
                notify(
                    window,
                    cx,
                    Level::Success,
                    t!("sync.dialog.cloned", name = graph_name(&canonical)).to_string(),
                );
                self.open_graph(canonical, window, cx);
            }
            SyncDialogEvent::Closed => window.focus(&self.focus, cx),
        }
    }

    fn on_conflicts_event(
        &mut self,
        _: &Entity<ConflictsView>,
        event: &ConflictsEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            ConflictsEvent::Closed => window.focus(&self.focus, cx),
            ConflictsEvent::Resolved => {
                for pane in self.panes(cx) {
                    pane.update(cx, |main, cx| main.reload(cx));
                }
                self.refresh_page_conflicts(cx);
            }
        }
    }

    fn on_disk_event(
        &mut self,
        _: &Entity<DiskConflictBanner>,
        event: &DiskConflictEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            DiskConflictEvent::OpenResolver => self.open_conflicts(window, cx),
            DiskConflictEvent::KeepMine(key) => {
                self.resolve_disk(key.clone(), Keep::Mine, window, cx)
            }
            DiskConflictEvent::TakeDisk(key) => {
                self.resolve_disk(key.clone(), Keep::Disk, window, cx)
            }
            DiskConflictEvent::ShowDiff(key) => {
                let notice = self.disk_banner.read(cx).notice(key).cloned();
                if let Some(notice) = notice {
                    let title = self
                        .main
                        .read(cx)
                        .route()
                        .and_then(|r| match r {
                            Route::Page(name) => Some(name.clone()),
                            _ => None,
                        })
                        .unwrap_or_default();
                    self.disk_diff
                        .update(cx, |d, cx| d.show(title, &notice, cx));
                }
            }
        }
    }

    /// Keeps my version (`Keep::Mine`, the disk file is backed up) or loads the disk version
    /// (`Keep::Disk`, my edits are backed up) of a conflicted page through the command queue.
    pub fn resolve_disk(
        &mut self,
        key: PageKey,
        keep: Keep,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(queue) = self.queue().cloned() else {
            return;
        };
        let task_key = key.clone();
        let task = cx.background_spawn(async move {
            queue
                .execute(
                    Source::Ui,
                    Request::Resolve {
                        key: task_key,
                        keep,
                    },
                )
                .map(|_| ())
                .map_err(|e| e.to_string())
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |ws, window, cx| match result {
                Ok(()) => {
                    ws.disk_banner.update(cx, |b, cx| b.clear(&key, cx));
                    ws.disk_diff.update(cx, |d, cx| d.close(cx));
                    let text = match keep {
                        Keep::Mine => t!("disk.kept_mine"),
                        Keep::Disk => t!("disk.took_disk"),
                    };
                    notify(window, cx, Level::Success, text.to_string());
                    for pane in ws.panes(cx) {
                        pane.update(cx, |main, cx| main.reload(cx));
                    }
                }
                Err(error) => notify(
                    window,
                    cx,
                    Level::Error,
                    t!("disk.resolve_failed", error = error).to_string(),
                ),
            });
        })
        .detach();
    }

    /// Closes the topmost sync overlay on Escape; `true` when one was open.
    fn close_topmost_overlay(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.credential_dialog.read(cx).is_open() {
            // Escape declines the credential prompt, like its Cancel button.
            self.credential_dialog
                .update(cx, |d, cx| d.cancel(window, cx));
        } else if self.settings.read(cx).is_open() {
            // Escape first cancels a shortcut recording, then closes the settings.
            self.settings.update(cx, |s, cx| s.escape(cx));
        } else if self.disk_diff.read(cx).is_open() {
            self.disk_diff.update(cx, |d, cx| d.close(cx));
        } else if self.sync_dialog.read(cx).mode().is_some() {
            self.sync_dialog.update(cx, |d, cx| d.close(window, cx));
        } else if self.conflicts.read(cx).is_open() {
            self.conflicts.update(cx, |c, cx| c.close(cx));
        } else if self.activity.read(cx).is_open() {
            self.activity.update(cx, |a, cx| a.close(cx));
        } else if self.history.read(cx).is_open() {
            self.history.update(cx, |h, cx| h.close(cx));
        } else if self.sync_panel.read(cx).is_open() {
            self.sync_panel.update(cx, |p, cx| p.close(cx));
        } else {
            return false;
        }
        true
    }

    /// Rebuilds the index of the open graph from its files: stops the session, deletes the
    /// index database (the SQLite cache is disposable, ADR-005) and opens the graph again.
    pub fn reindex(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(root) = self.graph_root.clone() else {
            return;
        };
        let data_dir = self.config.index_data_dir.clone();
        let session = self.take_session();
        self.hub.update(cx, |hub, _| hub.set_handle(None));
        notify(
            window,
            cx,
            Level::Info,
            t!("palette.reindexing").to_string(),
        );
        let task = cx.background_spawn(async move {
            if let Some(session) = session {
                session.close();
            }
            let location = match &data_dir {
                Some(dir) => bitacora_index::IndexLocation::in_data_dir(dir, &root),
                None => bitacora_index::IndexLocation::for_graph(&root),
            };
            if let Ok(location) = location {
                let db = location.db_path();
                for suffix in ["", "-wal", "-shm"] {
                    let mut file = db.clone().into_os_string();
                    file.push(suffix);
                    // Nothing to delete is fine; a failure shows up as a rebuilt-from-garbage
                    // index on open.
                    let _ = std::fs::remove_file(file);
                }
            }
            root
        });
        cx.spawn_in(window, async move |this, cx| {
            let root = task.await;
            let _ = this.update_in(cx, |ws, window, cx| {
                ws.graph_root = None;
                ws.open_graph(root, window, cx);
            });
        })
        .detach();
    }

    fn open_search(&mut self, _: &OpenSearch, window: &mut Window, cx: &mut Context<Self>) {
        if self.picker_visible {
            return;
        }
        let page_id = self.main.read(cx).current_page_id(cx);
        let handle = self.handle.clone();
        let recent = self.graph_state.recent.clone();
        self.palette.update(cx, |palette, cx| {
            palette.open_search(handle, page_id, recent, window, cx);
        });
    }

    fn open_command_palette(
        &mut self,
        _: &OpenCommandPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.picker_visible {
            return;
        }
        self.palette
            .update(cx, |palette, cx| palette.open_commands(window, cx));
    }

    fn go_journals(&mut self, _: &GoJournals, _: &mut Window, cx: &mut Context<Self>) {
        self.navigate(Route::Journals, cx);
    }

    fn go_all_pages(&mut self, _: &GoAllPages, _: &mut Window, cx: &mut Context<Self>) {
        self.navigate(Route::AllPages, cx);
    }

    fn focus_right_sidebar(
        &mut self,
        _: &FocusRightSidebar,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ensure_right_dock(window, cx);
        self.stack
            .update(cx, |stack, cx| stack.focus_stack(window, cx));
    }

    /// The default layout: a page host in the center and the right sidebar dock.
    fn default_layout(area: &mut DockArea, window: &mut Window, cx: &mut Context<DockArea>) {
        let page = cx.new(|cx| PlaceholderPanel::new(PanelKind::PageHost, window, cx));
        area.set_center(
            DockLayout::tabs().panel_view(panel_handle(page), cx),
            window,
            cx,
        );
        let right = cx.new(|cx| PlaceholderPanel::new(PanelKind::RightSidebar, window, cx));
        area.set_dock(
            DockPlacement::Right,
            DockLayout::tabs().panel_view(panel_handle(right), cx),
            window,
            cx,
        );
        area.set_dock_size(DockPlacement::Right, px(RIGHT_DOCK_WIDTH), window, cx);
        area.set_dock_collapsible(DockPlacement::Right, true, window, cx);
    }

    /// The sidebar entity.
    pub fn sidebar(&self) -> &Entity<LeftSidebar> {
        &self.sidebar
    }

    /// The dock area entity.
    pub fn dock(&self) -> &Entity<DockArea> {
        &self.dock
    }

    /// The status bar entity.
    pub fn status_bar(&self) -> &Entity<AppStatusBar> {
        &self.status
    }

    /// A sender for status events (services push into it from any thread).
    pub fn status_sender(&self) -> Sender<StatusEvent> {
        self.status_tx.clone()
    }

    /// Starts the demo producers: a tokio interval (proves tokio -> channel -> GPUI)
    /// and a background task (proves background -> UI). Requires `tokio_bridge::init`.
    pub fn start_demo_producers(&mut self, cx: &mut Context<Self>) {
        let tx = self.status_tx.clone();
        self.heartbeat_task = Some(tokio_bridge::spawn(cx, async move {
            let mut ticker = tokio::time::interval(Duration::from_secs(1));
            let mut count = 0_u64;
            loop {
                ticker.tick().await;
                count += 1;
                if tx.send(StatusEvent::Heartbeat(count)).await.is_err() {
                    break;
                }
            }
        }));
        let tx = self.status_tx.clone();
        self.probe_task = Some(cx.background_spawn(async move {
            // Stand-in for real work (indexing); proves the background -> UI path.
            let _ = tx
                .send(StatusEvent::Slot(Slot::Index, SlotState::Busy))
                .await;
            let _ = tx
                .send(StatusEvent::Slot(Slot::Index, SlotState::Idle))
                .await;
        }));
    }

    fn on_dock_event(
        &mut self,
        _: &Entity<DockArea>,
        event: &DockEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(event, DockEvent::LayoutChanged) {
            self.schedule_save(cx);
        }
    }

    /// Debounced write of the dock layout; a newer change replaces the pending one.
    fn schedule_save(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.config.layout_file.clone() else {
            return;
        };
        let dock = self.dock.clone();
        self.save_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SAVE_DEBOUNCE).await;
            // Errors here mean the workspace is gone; nothing left to save.
            let _ = this.update(cx, |_, cx| Self::write_layout(&dock, &path, cx));
        }));
    }

    fn write_layout(dock: &Entity<DockArea>, path: &std::path::Path, cx: &App) {
        let state = dock.read(cx).dump(cx);
        if let Err(err) = save_layout(path, &state) {
            tracing::warn!(path = %path.display(), "cannot save workspace layout: {err}");
        }
    }

    /// Writes the layout immediately (used on quit).
    pub fn save_now(&self, cx: &App) {
        if let Some(path) = &self.config.layout_file {
            Self::write_layout(&self.dock, path, cx);
        }
    }

    fn toggle_left(&mut self, _: &ToggleLeftSidebar, _: &mut Window, cx: &mut Context<Self>) {
        self.sidebar.update(cx, |sidebar, cx| sidebar.toggle(cx));
    }

    fn toggle_right(
        &mut self,
        _: &ToggleRightSidebar,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.dock.update(cx, |area, cx| {
            area.toggle_dock(DockPlacement::Right, window, cx)
        });
    }

    /// Hands the session out for the final shutdown (app quit); afterwards the workspace has no
    /// session left.
    pub fn take_session(&mut self) -> Option<GraphSession> {
        self.link = None;
        self.session_handle = None;
        self.session_task = None;
        self.session.take()
    }

    /// `Quit`: runs the ordered shutdown first and shows a notice when files stayed unwritten.
    fn quit(&mut self, _: &Quit, window: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = self.take_session() else {
            cx.quit();
            return;
        };
        let task = cx.background_spawn(async move {
            session.shutdown_with_report(crate::session::SHUTDOWN_BUDGET)
        });
        cx.spawn_in(window, async move |this, cx| {
            let report = task.await;
            let problem = report.as_ref().and_then(shutdown_problem);
            if let Some(text) = problem {
                let _ = this.update_in(cx, |_, window, cx| {
                    notify(window, cx, Level::Error, text);
                });
                // Leave the notice on screen for a moment before the window goes away.
                cx.background_executor().timer(QUIT_NOTICE_TIME).await;
            }
            let _ = cx.update(|_, cx| cx.quit());
        })
        .detach();
    }

    fn go_back(&mut self, _: &GoBack, _: &mut Window, cx: &mut Context<Self>) {
        self.main.update(cx, |main, cx| main.go_back(cx));
    }

    fn go_forward(&mut self, _: &GoForward, _: &mut Window, cx: &mut Context<Self>) {
        self.main.update(cx, |main, cx| main.go_forward(cx));
    }

    fn toggle_theme(&mut self, _: &ToggleTheme, window: &mut Window, cx: &mut Context<Self>) {
        theme::toggle(cx, Some(window));
    }
}

/// The notice text for a shutdown that left files unwritten or steps unfinished.
pub fn shutdown_problem(report: &bitacora_runtime::ShutdownReport) -> Option<String> {
    let unwritten = report.flush.as_ref().map_or(0, |f| {
        f.unwritten.len().max(f.conflicts.len() + f.failed.len())
    });
    if unwritten > 0 {
        return Some(t!("notice.unwritten_on_quit", count = unwritten).to_string());
    }
    if !report.timed_out.is_empty() {
        return Some(t!("notice.shutdown_slow", steps = report.timed_out.join(", ")).to_string());
    }
    None
}

/// The route for `--page <req>`: a graph-relative file path (derived to its page title) or a
/// page name.
fn requested_route(handle: &GraphHandle, req: &str) -> Route {
    let cfg = &handle.settings.config;
    let is_file = req.ends_with(".md") && handle.root.join(req).is_file();
    if !is_file {
        return Route::Page(req.to_owned());
    }
    let title = bitacora_core::naming::derive_title(req, None, cfg);
    match bitacora_core::journal::detect_journal(&title, cfg) {
        Some(journal) => Route::Page(journal.title),
        None => Route::Page(title),
    }
}

impl Focusable for Workspace {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for Workspace {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let sidebar_visible = self.sidebar.read(cx).is_visible();
        let main = if self.picker_visible {
            div().flex_1().min_h_0().child(self.picker.clone())
        } else {
            div().flex_1().min_h_0().child(
                v_flex().size_full().child(self.disk_banner.clone()).child(
                    div().flex_1().min_h_0().child(
                        h_flex()
                            .size_full()
                            .child(
                                div()
                                    .h_full()
                                    .when(sidebar_visible, |d| d.child(self.sidebar.clone())),
                            )
                            .child(div().flex_1().min_w_0().h_full().child(self.dock.clone())),
                    ),
                ),
            )
        };
        let content = v_flex()
            .id("workspace")
            .key_context("Workspace")
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::open_graph_action))
            .on_action(cx.listener(Self::close_graph_action))
            .on_action(
                cx.listener(|this, _: &crate::actions::OpenRecentGraph1, window, cx| {
                    this.open_recent(0, window, cx);
                }),
            )
            .on_action(
                cx.listener(|this, _: &crate::actions::OpenRecentGraph2, window, cx| {
                    this.open_recent(1, window, cx);
                }),
            )
            .on_action(
                cx.listener(|this, _: &crate::actions::OpenRecentGraph3, window, cx| {
                    this.open_recent(2, window, cx);
                }),
            )
            .on_action(
                cx.listener(|this, _: &crate::actions::OpenRecentGraph4, window, cx| {
                    this.open_recent(3, window, cx);
                }),
            )
            .on_action(
                cx.listener(|this, _: &crate::actions::OpenRecentGraph5, window, cx| {
                    this.open_recent(4, window, cx);
                }),
            )
            .on_action(
                cx.listener(|this, _: &crate::actions::OpenRecentGraph6, window, cx| {
                    this.open_recent(5, window, cx);
                }),
            )
            .on_action(
                cx.listener(|this, _: &crate::actions::OpenRecentGraph7, window, cx| {
                    this.open_recent(6, window, cx);
                }),
            )
            .on_action(
                cx.listener(|this, _: &crate::actions::OpenRecentGraph8, window, cx| {
                    this.open_recent(7, window, cx);
                }),
            )
            .on_action(
                cx.listener(|this, _: &crate::actions::OpenRecentGraph9, window, cx| {
                    this.open_recent(8, window, cx);
                }),
            )
            .on_action(
                cx.listener(|this, _: &crate::actions::OpenRecentGraph10, window, cx| {
                    this.open_recent(9, window, cx);
                }),
            )
            .on_action(cx.listener(Self::toggle_left))
            .on_action(cx.listener(Self::toggle_right))
            .on_action(cx.listener(Self::toggle_theme))
            .on_action(cx.listener(Self::quit))
            .on_action(cx.listener(Self::open_search))
            .on_action(cx.listener(Self::open_command_palette))
            .on_action(cx.listener(Self::go_journals))
            .on_action(cx.listener(Self::go_all_pages))
            .on_action(cx.listener(Self::focus_right_sidebar))
            .on_action(cx.listener(Self::go_back))
            .on_action(cx.listener(Self::go_forward))
            .on_action(cx.listener(Self::open_settings_action))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" {
                    this.close_topmost_overlay(window, cx);
                }
            }))
            .size_full()
            .child(AppTitleBar::new())
            .child(main)
            .child(self.status.clone())
            .child(self.palette.clone())
            .child(self.sync_panel.clone())
            .child(self.settings.clone())
            .child(self.history.clone())
            .child(self.activity.clone())
            .child(self.conflicts.clone())
            .child(self.disk_diff.clone())
            .child(self.sync_dialog.clone())
            .child(self.credential_dialog.clone())
            .when(crate::perf::enabled(), |d| d.child(crate::perf::FrameEnd));
        // Client-side shadow ring, resize bands and tiling insets (ADR-033); a no-op under
        // server decorations.
        crate::ui::frameless::window_border().child(content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keymap;
    use crate::settings::{AppSettings, ThemePreference};
    use crate::ui::testing::{TestAppContext, VisualTestContext, gpui_test};
    use crate::ui::theme::Theme;
    use crate::views::sidebar::{SidebarEvent, Target};
    use crate::views::status_bar::StatusEvent;

    fn setup(cx: &mut TestAppContext) {
        cx.update(|cx| {
            crate::ui::init(cx);
            crate::views::panels::register_panels(cx);
            theme::install(cx, AppSettings::default(), None);
            keymap::load_with_user(cx, None).expect("default keymap");
        });
    }

    fn open(
        cx: &mut TestAppContext,
        layout_file: Option<PathBuf>,
    ) -> (Entity<Workspace>, &mut VisualTestContext) {
        cx.add_window_view(|window, cx| {
            Workspace::new(
                WorkspaceConfig {
                    graph_name: Some("demo".into()),
                    layout_file,
                    ..WorkspaceConfig::default()
                },
                window,
                cx,
            )
        })
    }

    #[gpui_test]
    fn toggle_left_sidebar_action_flips_visibility(cx: &mut TestAppContext) {
        setup(cx);
        let (ws, cx) = open(cx, None);
        let sidebar = ws.read_with(cx, |w, _| w.sidebar().clone());
        assert!(sidebar.read_with(cx, |s, _| s.is_visible()));
        cx.dispatch_action(ToggleLeftSidebar);
        assert!(!sidebar.read_with(cx, |s, _| s.is_visible()));
        cx.dispatch_action(ToggleLeftSidebar);
        assert!(sidebar.read_with(cx, |s, _| s.is_visible()));
    }

    #[gpui_test]
    fn keybindings_from_the_keymap_file_trigger_actions(cx: &mut TestAppContext) {
        setup(cx);
        let (ws, cx) = open(cx, None);
        let sidebar = ws.read_with(cx, |w, _| w.sidebar().clone());
        // `secondary-b` is cmd-b on macOS and ctrl-b elsewhere; the test platform
        // resolves it the same way.
        cx.simulate_keystrokes("secondary-b");
        assert!(!sidebar.read_with(cx, |s, _| s.is_visible()));
        let dock = ws.read_with(cx, |w, _| w.dock().clone());
        assert!(dock.read_with(cx, |d, _| d.is_dock_open(DockPlacement::Right)));
        cx.simulate_keystrokes("secondary-shift-b");
        assert!(!dock.read_with(cx, |d, _| d.is_dock_open(DockPlacement::Right)));
    }

    #[gpui_test]
    fn toggle_theme_action_switches_between_light_and_dark(cx: &mut TestAppContext) {
        setup(cx);
        let (_ws, cx) = open(cx, None);
        cx.update(|_, cx| theme::set_preference(cx, None, ThemePreference::Light));
        assert!(!cx.read(|cx| Theme::global(cx).is_dark()));
        cx.dispatch_action(ToggleTheme);
        assert!(cx.read(|cx| Theme::global(cx).is_dark()));
        cx.dispatch_action(ToggleTheme);
        assert!(!cx.read(|cx| Theme::global(cx).is_dark()));
    }

    #[gpui_test]
    fn sidebar_selection_emits_a_navigate_event(cx: &mut TestAppContext) {
        setup(cx);
        let (ws, cx) = open(cx, None);
        let sidebar = ws.read_with(cx, |w, _| w.sidebar().clone());
        let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let sink = events.clone();
        let _sub = cx.update(|_, cx| {
            cx.subscribe(&sidebar, move |_, event: &SidebarEvent, _| {
                sink.borrow_mut().push(event.clone());
            })
        });
        sidebar.update(cx, |s, cx| s.select(Target::AllPages, cx));
        assert_eq!(
            *events.borrow(),
            vec![SidebarEvent::Navigate(Target::AllPages)]
        );
        assert_eq!(
            sidebar.read_with(cx, |s, _| s.active().cloned()),
            Some(Target::AllPages)
        );
    }

    #[gpui_test]
    fn status_events_update_the_status_bar(cx: &mut TestAppContext) {
        setup(cx);
        let (ws, cx) = open(cx, None);
        let tx = ws.read_with(cx, |w, _| w.status_sender());
        let bar = ws.read_with(cx, |w, _| w.status_bar().clone());
        tx.try_send(StatusEvent::Heartbeat(3)).expect("send");
        tx.try_send(StatusEvent::Slot(Slot::Sync, SlotState::Busy))
            .expect("send");
        cx.run_until_parked();
        assert_eq!(bar.read_with(cx, |b, _| b.heartbeat()), 3);
        assert_eq!(
            bar.read_with(cx, |b, _| b.slot(Slot::Sync)),
            SlotState::Busy
        );
    }

    #[gpui_test]
    fn right_dock_state_survives_a_restart(cx: &mut TestAppContext) {
        setup(cx);
        let tmp = tempfile::tempdir().expect("tempdir");
        let file = tmp.path().join("workspace.json");
        {
            let (ws, cx) = open(cx, Some(file.clone()));
            let dock = ws.read_with(cx, |w, _| w.dock().clone());
            assert!(dock.read_with(cx, |d, _| d.is_dock_open(DockPlacement::Right)));
            cx.dispatch_action(ToggleRightSidebar);
            assert!(!dock.read_with(cx, |d, _| d.is_dock_open(DockPlacement::Right)));
            ws.read_with(cx, |w, cx| w.save_now(cx));
        }
        assert!(file.is_file(), "layout was written");
        let (ws, cx) = open(cx, Some(file));
        let dock = ws.read_with(cx, |w, _| w.dock().clone());
        assert!(dock.read_with(cx, |d, _| d.has_dock(DockPlacement::Right)));
        assert!(!dock.read_with(cx, |d, _| d.is_dock_open(DockPlacement::Right)));
    }

    /// Lets the session thread finish indexing so tearing the test down does not race it.
    fn settle(ws: &Entity<Workspace>, cx: &mut VisualTestContext) {
        cx.executor().allow_parking();
        let bar = ws.read_with(cx, |w, _| w.status_bar().clone());
        for _ in 0..200 {
            cx.run_until_parked();
            if bar.read_with(cx, |b, _| b.slot(Slot::Index)) == SlotState::Idle {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    fn workspace_with_recents(
        cx: &mut TestAppContext,
        recent_file: PathBuf,
        data: PathBuf,
    ) -> (Entity<Workspace>, &mut VisualTestContext) {
        cx.add_window_view(|window, cx| {
            Workspace::new(
                WorkspaceConfig {
                    index_data_dir: Some(data),
                    recent_file: Some(recent_file),
                    initial_page: Some("Home".into()),
                    ..WorkspaceConfig::default()
                },
                window,
                cx,
            )
        })
    }

    #[gpui_test]
    fn startup_reopens_the_last_graph_without_the_picker(cx: &mut TestAppContext) {
        setup(cx);
        let data = tempfile::tempdir().expect("data");
        let recent_file = data.path().join("recent.json");
        let g = graph();
        let mut recents = crate::recent::RecentGraphs::default();
        recents.touch(g.path());
        recents.save(&recent_file).expect("save recents");
        let (ws, cx) = workspace_with_recents(cx, recent_file, data.path().to_path_buf());
        assert!(ws.read_with(cx, |w, _| w.picker_visible()));
        let opened = ws.update_in(cx, |w, window, cx| w.open_startup_graph(window, cx));
        assert!(opened);
        assert!(!ws.read_with(cx, |w, _| w.picker_visible()));
        settle(&ws, cx);
        assert_eq!(
            ws.read_with(cx, |w, _| w.graph_root().map(std::path::Path::to_path_buf)),
            g.path().canonicalize().ok()
        );
        drop(ws);
        cx.run_until_parked();
    }

    #[gpui_test]
    fn startup_with_a_missing_last_graph_shows_the_picker_and_prunes(cx: &mut TestAppContext) {
        setup(cx);
        let data = tempfile::tempdir().expect("data");
        let recent_file = data.path().join("recent.json");
        let mut recents = crate::recent::RecentGraphs::default();
        recents.touch(&data.path().join("gone"));
        recents.save(&recent_file).expect("save recents");
        let (ws, cx) = workspace_with_recents(cx, recent_file.clone(), data.path().to_path_buf());
        let opened = ws.update_in(cx, |w, window, cx| w.open_startup_graph(window, cx));
        assert!(!opened);
        assert!(ws.read_with(cx, |w, _| w.picker_visible()));
        assert!(ws.read_with(cx, |w, _| w.graph_root().is_none()));
        assert!(
            crate::recent::RecentGraphs::load(&recent_file)
                .graphs()
                .is_empty()
        );
    }

    #[gpui_test]
    fn startup_without_recents_or_with_the_setting_off_shows_the_picker(cx: &mut TestAppContext) {
        setup(cx);
        let data = tempfile::tempdir().expect("data");
        let recent_file = data.path().join("recent.json");
        let (ws, cx) = workspace_with_recents(cx, recent_file.clone(), data.path().to_path_buf());
        assert!(!ws.update_in(cx, |w, window, cx| w.open_startup_graph(window, cx)));
        drop(ws);
        let g = graph();
        let mut recents = crate::recent::RecentGraphs::default();
        recents.touch(g.path());
        recents.save(&recent_file).expect("save recents");
        cx.update(|_, cx| {
            theme::edit_settings(cx, None, |s| s.reopen_last_graph = false);
        });
        let (ws, cx) = workspace_with_recents(cx, recent_file, data.path().to_path_buf());
        assert!(!ws.update_in(cx, |w, window, cx| w.open_startup_graph(window, cx)));
        assert!(ws.read_with(cx, |w, _| w.picker_visible()));
    }

    #[gpui_test]
    fn graph_menu_actions_open_recent_and_close(cx: &mut TestAppContext) {
        setup(cx);
        let data = tempfile::tempdir().expect("data");
        let recent_file = data.path().join("recent.json");
        let g = graph();
        let mut recents = crate::recent::RecentGraphs::default();
        recents.touch(g.path());
        recents.save(&recent_file).expect("save recents");
        let (ws, cx) = workspace_with_recents(cx, recent_file, data.path().to_path_buf());
        ws.update_in(cx, |w, window, cx| w.focus_handle(cx).focus(window, cx));
        cx.dispatch_action(crate::actions::OpenRecentGraph1);
        assert!(!ws.read_with(cx, |w, _| w.picker_visible()));
        assert!(ws.read_with(cx, |w, _| w.graph_root().is_some()));
        settle(&ws, cx);
        cx.dispatch_action(crate::actions::CloseGraph);
        assert!(ws.read_with(cx, |w, _| w.picker_visible()));
        assert!(ws.read_with(cx, |w, _| w.graph_root().is_none()));
        drop(ws);
        cx.run_until_parked();
    }

    fn graph() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().expect("tmp");
        std::fs::create_dir_all(tmp.path().join("pages")).expect("pages");
        std::fs::write(tmp.path().join("pages/Home.md"), "- hello [[World]]\n").expect("page");
        tmp
    }

    #[gpui_test]
    fn opening_a_graph_hides_the_picker_indexes_and_shows_a_page(cx: &mut TestAppContext) {
        setup(cx);
        let data = tempfile::tempdir().expect("data");
        let recent_file = data.path().join("recent.json");
        let g = graph();
        let (ws, cx) = cx.add_window_view(|window, cx| {
            Workspace::new(
                WorkspaceConfig {
                    index_data_dir: Some(data.path().to_path_buf()),
                    recent_file: Some(recent_file.clone()),
                    initial_page: Some("Home".into()),
                    ..WorkspaceConfig::default()
                },
                window,
                cx,
            )
        });
        assert!(ws.read_with(cx, |w, _| w.picker_visible()));
        let path = g.path().to_path_buf();
        ws.update_in(cx, |w, window, cx| w.open_graph(path, window, cx));
        assert!(!ws.read_with(cx, |w, _| w.picker_visible()));
        // The session thread and the page read run off the foreground executor.
        cx.executor().allow_parking();
        let bar = ws.read_with(cx, |w, _| w.status_bar().clone());
        for _ in 0..200 {
            cx.run_until_parked();
            if bar.read_with(cx, |b, _| b.slot(Slot::Index)) == SlotState::Idle {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(
            bar.read_with(cx, |b, _| b.slot(Slot::Index)),
            SlotState::Idle
        );
        assert_eq!(bar.read_with(cx, |b, _| b.index_progress()), None);
        let page = ws.read_with(cx, |w, cx| w.page_view(cx));
        for _ in 0..200 {
            cx.run_until_parked();
            if page.read_with(cx, |p, _| p.rows().len() == 1) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(page.read_with(cx, |p, _| p.rows().len()), 1);
        assert_eq!(
            page.read_with(cx, |p, _| p.title().map(str::to_owned)),
            Some("Home".into())
        );
        // The graph was remembered.
        let recents = crate::recent::RecentGraphs::load(&recent_file);
        assert_eq!(recents.graphs().len(), 1);
        // The switcher brings the picker back.
        ws.update(cx, |w, cx| {
            w.sidebar()
                .update(cx, |s, cx| s.select(Target::GraphSwitcher, cx));
        });
        assert!(ws.read_with(cx, |w, _| w.picker_visible()));
        // Dropping the workspace closes the session without hanging.
        drop(ws);
        cx.run_until_parked();
    }

    #[gpui_test]
    fn opening_a_missing_folder_keeps_the_picker(cx: &mut TestAppContext) {
        setup(cx);
        let tmp = tempfile::tempdir().expect("tmp");
        let (ws, cx) = open(cx, None);
        let path = tmp.path().join("nope");
        ws.update_in(cx, |w, window, cx| w.open_graph(path, window, cx));
        assert!(ws.read_with(cx, |w, _| w.picker_visible()));
        assert!(ws.read_with(cx, |w, _| w.graph_root().is_none()));
    }

    #[gpui_test]
    fn index_progress_events_drive_the_status_bar(cx: &mut TestAppContext) {
        setup(cx);
        let (ws, cx) = open(cx, None);
        let tx = ws.read_with(cx, |w, _| w.status_sender());
        let bar = ws.read_with(cx, |w, _| w.status_bar().clone());
        tx.try_send(StatusEvent::IndexProgress { done: 3, total: 10 })
            .expect("send");
        cx.run_until_parked();
        assert_eq!(bar.read_with(cx, |b, _| b.index_progress()), Some((3, 10)));
        assert_eq!(
            bar.read_with(cx, |b, _| b.slot(Slot::Index)),
            SlotState::Busy
        );
        tx.try_send(StatusEvent::Slot(Slot::Index, SlotState::Idle))
            .expect("send");
        cx.run_until_parked();
        assert_eq!(bar.read_with(cx, |b, _| b.index_progress()), None);
    }

    #[gpui_test]
    fn corrupt_layout_file_falls_back_to_the_default_layout(cx: &mut TestAppContext) {
        setup(cx);
        let tmp = tempfile::tempdir().expect("tempdir");
        let file = tmp.path().join("workspace.json");
        std::fs::write(&file, b"\x00 not json").expect("write");
        let (ws, cx) = open(cx, Some(file));
        let dock = ws.read_with(cx, |w, _| w.dock().clone());
        assert!(dock.read_with(cx, |d, _| d.is_dock_open(DockPlacement::Right)));
    }

    fn open_in_graph<'a>(
        cx: &'a mut TestAppContext,
        data: &tempfile::TempDir,
        graph: &tempfile::TempDir,
    ) -> (Entity<Workspace>, &'a mut VisualTestContext) {
        let (ws, cx) = cx.add_window_view(|window, cx| {
            Workspace::new(
                WorkspaceConfig {
                    index_data_dir: Some(data.path().to_path_buf()),
                    state_dir: Some(data.path().join("state")),
                    global_config: Some(data.path().join("no-global.edn")),
                    initial_page: Some("Home".into()),
                    ..WorkspaceConfig::default()
                },
                window,
                cx,
            )
        });
        let path = graph.path().to_path_buf();
        ws.update_in(cx, |w, window, cx| w.open_graph(path, window, cx));
        cx.executor().allow_parking();
        let page = ws.read_with(cx, |w, cx| w.page_view(cx));
        for _ in 0..400 {
            cx.run_until_parked();
            if page.read_with(cx, |p, _| p.title() == Some("Home")) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(
            page.read_with(cx, |p, _| p.title().map(str::to_owned)),
            Some("Home".into())
        );
        (ws, cx)
    }

    #[gpui_test]
    fn runtime_session_feeds_favorites_recent_and_the_right_sidebar_per_graph(
        cx: &mut TestAppContext,
    ) {
        use crate::views::main_view::MainEvent;
        setup(cx);
        let data = tempfile::tempdir().expect("data");
        let g = graph();
        std::fs::create_dir_all(g.path().join("logseq")).expect("logseq");
        std::fs::write(
            g.path().join("logseq/config.edn"),
            "{:favorites [\"Home\" \"World\"]}",
        )
        .expect("config");
        let (ws, cx) = open_in_graph(cx, &data, &g);
        // The session thread exposes the command queue and the MCP state once it is up.
        for _ in 0..200 {
            cx.run_until_parked();
            if ws.read_with(cx, |w, _| w.queue().is_some()) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(ws.read_with(cx, |w, _| w.queue().is_some()));
        let sidebar = ws.read_with(cx, |w, _| w.sidebar().clone());
        assert_eq!(
            sidebar.read_with(cx, |s, _| s.favorites().to_vec()),
            ["Home", "World"]
        );
        // The initial page was recorded as recent.
        assert_eq!(ws.read_with(cx, |w, _| w.recent_pages().to_vec()), ["Home"]);
        assert_eq!(
            sidebar.read_with(cx, |s, _| s.active().cloned()),
            Some(Target::Page("Home".into()))
        );

        // A Shift+click on a reference asks the host for the right sidebar.
        let main = ws.read_with(cx, |w, _| w.main_view().clone());
        main.update(cx, |_, cx| {
            cx.emit(MainEvent::OpenInSidebar(Route::Page("World".into())));
        });
        let stack = ws.read_with(cx, |w, _| w.right_sidebar().clone());
        assert_eq!(
            stack.read_with(cx, |s, _| s.routes()),
            vec![Route::Page("World".into())]
        );
        let dock = ws.read_with(cx, |w, _| w.dock().clone());
        assert!(dock.read_with(cx, |d, _| d.is_dock_open(DockPlacement::Right)));
        // ...and the stack and the recent pages are remembered for this graph.
        let root = g.path().canonicalize().expect("canonical");
        let saved = crate::graph_state::GraphState::load(
            &crate::graph_state::GraphState::file_for(&data.path().join("state"), &root),
        );
        assert_eq!(saved.recent, ["Home"]);
        assert_eq!(
            saved.right_sidebar,
            vec![crate::graph_state::StackEntry {
                route: crate::graph_state::StoredRoute::Page("World".into()),
                collapsed: false
            }]
        );
    }

    #[gpui_test]
    fn mod_k_opens_the_search_palette_and_escape_returns_the_focus(cx: &mut TestAppContext) {
        setup(cx);
        let data = tempfile::tempdir().expect("data");
        let g = graph();
        let (ws, cx) = open_in_graph(cx, &data, &g);
        // Wait for the index handle (the search needs it).
        cx.run_until_parked();
        let palette = ws.read_with(cx, |w, _| w.palette().clone());
        cx.simulate_keystrokes("secondary-k");
        assert!(palette.read_with(cx, |p, _| p.is_open()));
        // Frames draw the overlay and move the focus into its query field.
        for _ in 0..3 {
            cx.update(|window, cx| {
                let needed = window.draw(cx);
                needed.clear(cx);
            });
            cx.run_until_parked();
        }
        let focused = cx.update(|window, cx| palette.read(cx).has_focus(window, cx));
        assert!(focused, "focus must be inside the palette");
        cx.simulate_keystrokes("escape");
        assert!(!palette.read_with(cx, |p, _| p.is_open()));
        cx.run_until_parked();
        cx.simulate_keystrokes("secondary-shift-p");
        for _ in 0..3 {
            cx.update(|window, cx| {
                let needed = window.draw(cx);
                needed.clear(cx);
            });
            cx.run_until_parked();
        }
        assert_eq!(
            palette.read_with(cx, |p, _| p.mode()),
            Some(crate::views::palette::PaletteMode::Commands)
        );
        cx.simulate_keystrokes("escape");
        assert!(!palette.read_with(cx, |p, _| p.is_open()));
    }

    #[gpui_test]
    fn each_dock_panel_owns_its_own_pane(cx: &mut TestAppContext) {
        setup(cx);
        let (ws, cx) = open(cx, None);
        let first = ws.read_with(cx, |w, cx| w.panes(cx));
        assert_eq!(first.len(), 1);
        // A second page host (a split) gets a pane of its own instead of sharing the first.
        let hub = ws.read_with(cx, |w, _| w.hub.clone());
        let second = cx.update(|window, cx| hub.update(cx, |hub, cx| hub.claim_pane(window, cx)));
        assert_ne!(first[0].entity_id(), second.entity_id());
        assert_eq!(ws.read_with(cx, |w, cx| w.panes(cx)).len(), 2);
        // The primary pane is the one the workspace navigates.
        assert_eq!(
            ws.read_with(cx, |w, _| w.main_view().entity_id()),
            first[0].entity_id()
        );
    }

    fn wait_until(
        cx: &mut VisualTestContext,
        what: &str,
        mut done: impl FnMut(&mut VisualTestContext) -> bool,
    ) {
        cx.executor().allow_parking();
        for _ in 0..600 {
            cx.run_until_parked();
            if done(cx) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("timed out waiting for {what}");
    }

    #[gpui_test]
    fn todays_journal_is_ensured_on_open_and_at_the_midnight_rollover(cx: &mut TestAppContext) {
        use bitacora_core::graph::PageKey;
        setup(cx);
        let data = tempfile::tempdir().expect("data");
        let g = graph();
        let day = std::rc::Rc::new(std::cell::Cell::new(Date::new(2025, 3, 9).expect("date")));
        let clock_day = day.clone();
        let (ws, cx) = cx.add_window_view(|window, cx| {
            let mut ws = Workspace::new(
                WorkspaceConfig {
                    index_data_dir: Some(data.path().to_path_buf()),
                    global_config: Some(data.path().join("no-global.edn")),
                    ..WorkspaceConfig::default()
                },
                window,
                cx,
            );
            // A mocked clock stands in for the system date.
            ws.set_clock(Rc::new(move || Some(clock_day.get())));
            ws
        });
        let path = g.path().to_path_buf();
        ws.update_in(cx, |w, window, cx| w.open_graph(path, window, cx));
        wait_until(cx, "the session link", |cx| {
            ws.read_with(cx, |w, _| w.queue().is_some())
        });
        let (queue, config) = ws.read_with(cx, |w, _| {
            let link = w.link.clone().expect("link");
            (link.queue, link.config)
        });
        let key_of =
            |d: Date| PageKey::from_title(&bitacora_core::journal::journal_page(d, &config).title);
        let first = key_of(day.get());
        wait_until(cx, "today's journal in core", |_| {
            queue.snapshot(&first).is_some()
        });
        // It is virtual: nothing is written for an untouched day.
        let _ = queue
            .flush(bitacora_core::queue::Source::Ui)
            .expect("flush");
        let journals = g.path().join("journals");
        assert!(!journals.exists() || std::fs::read_dir(&journals).expect("dir").count() == 0);
        // Past midnight: the next tick ensures the new day, still without writing.
        day.set(Date::new(2025, 3, 10).expect("date"));
        ws.update(cx, |w, cx| w.on_day_tick(cx));
        let second = key_of(day.get());
        wait_until(cx, "the new day in core", |_| {
            queue.snapshot(&second).is_some()
        });
        let _ = queue
            .flush(bitacora_core::queue::Source::Ui)
            .expect("flush");
        assert!(!journals.exists() || std::fs::read_dir(&journals).expect("dir").count() == 0);
        // A tick on the same day does nothing more.
        ws.update(cx, |w, cx| w.on_day_tick(cx));
    }

    #[gpui_test]
    fn deleting_the_open_page_recycles_it_and_updates_favorites_and_recent(
        cx: &mut TestAppContext,
    ) {
        setup(cx);
        let data = tempfile::tempdir().expect("data");
        let g = graph();
        std::fs::create_dir_all(g.path().join("logseq")).expect("logseq");
        std::fs::write(
            g.path().join("logseq/config.edn"),
            "{:favorites [\"Home\"]}",
        )
        .expect("config");
        let (ws, cx) = open_in_graph(cx, &data, &g);
        wait_until(cx, "the session link", |cx| {
            ws.read_with(cx, |w, _| w.queue().is_some())
        });
        ws.update_in(cx, |w, window, cx| {
            w.delete_page_now("Home".into(), window, cx);
        });
        let root = g.path().canonicalize().expect("root");
        wait_until(cx, "the page to be recycled", |_| {
            root.join("logseq/.recycle/pages_Home.md").exists()
        });
        assert!(!root.join("pages/Home.md").exists());
        wait_until(cx, "the sidebar to refresh", |cx| {
            ws.read_with(cx, |w, cx| w.sidebar().read(cx).favorites().is_empty())
        });
        assert!(ws.read_with(cx, |w, _| w.recent_pages().is_empty()));
        // The page that was on screen gave way to the journals.
        assert_eq!(
            ws.read_with(cx, |w, cx| w.main_view().read(cx).route().cloned()),
            Some(Route::Journals)
        );
        let cfg = std::fs::read_to_string(root.join("logseq/config.edn")).expect("config");
        assert!(!cfg.contains("Home"), "{cfg}");
    }
}

/// The text of the merge confirmation: what moves and which aliases are dropped (BIT-T-0157).
pub fn merge_description(preview: &graph_ops::MergePreview, from: &str) -> String {
    let mut text = t!(
        "rename.merge_blocks",
        source = preview.source_blocks,
        from = from,
        to = preview.target,
        target = preview.target_blocks
    )
    .to_string();
    if !preview.dropped_aliases.is_empty() {
        text.push(' ');
        text.push_str(&t!(
            "rename.merge_aliases",
            from = from,
            aliases = preview.dropped_aliases.join(", ")
        ));
    }
    text
}

#[cfg(test)]
mod merge_dialog_tests {
    use super::*;

    #[test]
    fn the_merge_dialog_lists_block_counts_and_dropped_aliases() {
        let preview = graph_ops::MergePreview {
            target: "Bar".into(),
            source_blocks: 2,
            target_blocks: 5,
            dropped_aliases: vec!["Fu".into(), "Fuu".into()],
        };
        let text = merge_description(&preview, "Foo");
        assert!(text.contains("2 block(s) of \"Foo\""), "{text}");
        assert!(text.contains("\"Bar\", which already has 5"), "{text}");
        assert!(text.contains("Fu, Fuu"), "{text}");
        let none = graph_ops::MergePreview {
            dropped_aliases: Vec::new(),
            ..preview
        };
        assert!(!merge_description(&none, "Foo").contains("Aliases"));
    }
}
