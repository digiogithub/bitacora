//! The main window content: left sidebar, dock area (center + right) and status bar.

use std::path::PathBuf;
use std::time::Duration;

use async_channel::Sender;
use tokio::task::JoinError;

use crate::actions::{ToggleLeftSidebar, ToggleRightSidebar, ToggleTheme};
use crate::layout::{LAYOUT_VERSION, load_layout, save_layout};
use crate::recent::{RecentGraphs, graph_name, has_graph_config};
use crate::render::inline::NavTarget;
use crate::session::{GraphSession, SessionEvent, SessionOptions, initial_page};
use crate::theme;
use crate::tokio_bridge;
use crate::ui::dock::{DockArea, DockEvent, DockLayout, DockPlacement, DockSkin, panel_handle};
use crate::ui::{
    App, AppContext as _, Context, Entity, FluentBuilder as _, FocusHandle, Focusable,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, Styled as _, Subscription,
    Task, Window, div, h_flex, px, v_flex,
};
use crate::ui::{Level, notify};
use crate::views::page_view::{PageEvent, PageView};
use crate::views::panels::{PanelKind, PlaceholderPanel, SharedPageView};
use crate::views::picker::{GraphPicker, PickerEvent};
use crate::views::sidebar::{LeftSidebar, SidebarEvent, Target};
use crate::views::status_bar::{AppStatusBar, Slot, SlotState, StatusEvent};
use rust_i18n::t;

/// Delay before a layout change is written to disk.
const SAVE_DEBOUNCE: Duration = Duration::from_millis(500);

/// Initial width of the right dock.
const RIGHT_DOCK_WIDTH: f32 = 280.0;

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
    page: Entity<PageView>,
    recents: RecentGraphs,
    graph_root: Option<PathBuf>,
    session: Option<GraphSession>,
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
        let page = cx.new(PageView::new);
        cx.set_global(SharedPageView(page.clone()));

        let (dock, skin) = DockSkin::dock_area("workspace", Some(LAYOUT_VERSION), window, cx);
        skin.set_toggle_button_visible(false, cx);
        let mut subscriptions = vec![cx.subscribe_in(&dock, window, Self::on_dock_event)];
        subscriptions.push(cx.observe(&sidebar, |_, _, cx| cx.notify()));
        subscriptions.push(cx.subscribe_in(&picker, window, Self::on_picker_event));
        subscriptions.push(cx.subscribe_in(&page, window, Self::on_page_event));
        subscriptions.push(cx.subscribe_in(&sidebar, window, Self::on_sidebar_event));

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
            page,
            recents,
            graph_root: None,
            session: None,
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

    /// The page view entity.
    pub fn page_view(&self) -> &Entity<PageView> {
        &self.page
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
        let started = GraphSession::start(
            path.clone(),
            SessionOptions {
                data_dir: self.config.index_data_dir.clone(),
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
        self.session = Some(session);
        self.graph_root = Some(path.clone());
        self.picker_visible = false;
        self.recents.touch(&path);
        if let Some(file) = &self.config.recent_file
            && let Err(err) = self.recents.save(file)
        {
            tracing::warn!("cannot save the recent graphs: {err}");
        }
        let recent_list = self.recents.graphs().to_vec();
        self.picker
            .update(cx, |picker, cx| picker.set_recents(recent_list, cx));
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

        if let Some((file, title)) = initial_page(&path, self.config.initial_page.as_deref()) {
            self.page.update(cx, |page, cx| {
                page.open_file(path.clone(), file, title, cx);
            });
        }
        cx.notify();
    }

    /// Stops the current session; the index is closed on a background thread so a still
    /// running reconcile never blocks the UI.
    fn close_session(&mut self, cx: &mut Context<Self>) {
        self.session_task = None;
        if let Some(session) = self.session.take() {
            cx.background_spawn(async move { session.close() }).detach();
        }
        self.graph_root = None;
    }

    fn on_session_event(
        &mut self,
        event: SessionEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            SessionEvent::Opened { rebuilt, total } => {
                if rebuilt {
                    notify(
                        window,
                        cx,
                        Level::Info,
                        t!("picker.index_rebuilt").to_string(),
                    );
                }
                self.status.update(cx, |bar, cx| {
                    bar.apply(StatusEvent::IndexProgress { done: 0, total });
                    cx.notify();
                });
            }
            SessionEvent::Progress { done, total } => {
                self.status.update(cx, |bar, cx| {
                    bar.apply(StatusEvent::IndexProgress { done, total });
                    cx.notify();
                });
            }
            SessionEvent::Ready(summary) => {
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
                notify(
                    window,
                    cx,
                    Level::Error,
                    t!("picker.indexing_failed", error = message).to_string(),
                );
            }
        }
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
            PickerEvent::Forget(path) => {
                self.recents.remove(path);
                if let Some(file) = &self.config.recent_file
                    && let Err(err) = self.recents.save(file)
                {
                    tracing::warn!("cannot save the recent graphs: {err}");
                }
            }
        }
    }

    fn on_page_event(
        &mut self,
        _: &Entity<PageView>,
        event: &PageEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let PageEvent::Navigate(target) = event;
        match target {
            NavTarget::Url(url) => cx.open_url(url),
            // Page and block navigation arrive with BIT-US-0075.
            NavTarget::Page(name) => tracing::debug!(page = %name, "navigate to page"),
            NavTarget::Block(id) => tracing::debug!(block = %id, "navigate to block"),
        }
    }

    fn on_sidebar_event(
        &mut self,
        _: &Entity<LeftSidebar>,
        event: &SidebarEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if *event == SidebarEvent::Navigate(Target::GraphSwitcher) {
            self.picker_visible = true;
            cx.notify();
        }
    }

    /// The default layout: a page host in the center and the right sidebar dock.
    fn default_layout(area: &mut DockArea, window: &mut Window, cx: &mut Context<DockArea>) {
        let page = cx.new(|cx| PlaceholderPanel::new(PanelKind::PageHost, cx));
        area.set_center(
            DockLayout::tabs().panel_view(panel_handle(page), cx),
            window,
            cx,
        );
        let right = cx.new(|cx| PlaceholderPanel::new(PanelKind::RightSidebar, cx));
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

    fn toggle_theme(&mut self, _: &ToggleTheme, window: &mut Window, cx: &mut Context<Self>) {
        theme::toggle(cx, Some(window));
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
                h_flex()
                    .size_full()
                    .child(
                        div()
                            .h_full()
                            .when(sidebar_visible, |d| d.child(self.sidebar.clone())),
                    )
                    .child(div().flex_1().min_w_0().h_full().child(self.dock.clone())),
            )
        };
        v_flex()
            .id("workspace")
            .key_context("Workspace")
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::toggle_left))
            .on_action(cx.listener(Self::toggle_right))
            .on_action(cx.listener(Self::toggle_theme))
            .size_full()
            .child(main)
            .child(self.status.clone())
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
                sink.borrow_mut().push(*event);
            })
        });
        sidebar.update(cx, |s, cx| s.select(Target::AllPages, cx));
        assert_eq!(
            *events.borrow(),
            vec![SidebarEvent::Navigate(Target::AllPages)]
        );
        assert_eq!(
            sidebar.read_with(cx, |s, _| s.active()),
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
        let page = ws.read_with(cx, |w, _| w.page_view().clone());
        assert_eq!(page.read_with(cx, |p, _| p.model().rows.len()), 1);
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
}
