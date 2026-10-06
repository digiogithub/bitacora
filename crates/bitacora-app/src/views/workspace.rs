//! The main window content: left sidebar, dock area (center + right) and status bar.

use std::path::PathBuf;
use std::time::Duration;

use async_channel::Sender;
use tokio::task::JoinError;

use crate::actions::{ToggleLeftSidebar, ToggleRightSidebar, ToggleTheme};
use crate::layout::{LAYOUT_VERSION, load_layout, save_layout};
use crate::theme;
use crate::tokio_bridge;
use crate::ui::dock::{DockArea, DockEvent, DockLayout, DockPlacement, DockSkin, panel_handle};
use crate::ui::{
    App, AppContext as _, Context, Entity, FluentBuilder as _, FocusHandle, Focusable,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, Styled as _, Subscription,
    Task, Window, div, h_flex, px, v_flex,
};
use crate::views::panels::{PanelKind, PlaceholderPanel};
use crate::views::sidebar::LeftSidebar;
use crate::views::status_bar::{AppStatusBar, Slot, SlotState, StatusEvent};

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

        let (dock, skin) = DockSkin::dock_area("workspace", Some(LAYOUT_VERSION), window, cx);
        skin.set_toggle_button_visible(false, cx);
        let mut subscriptions = vec![cx.subscribe_in(&dock, window, Self::on_dock_event)];
        subscriptions.push(cx.observe(&sidebar, |_, _, cx| cx.notify()));

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
            save_task: None,
            heartbeat_task: None,
            probe_task: None,
            _subscriptions: subscriptions,
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
        v_flex()
            .id("workspace")
            .key_context("Workspace")
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::toggle_left))
            .on_action(cx.listener(Self::toggle_right))
            .on_action(cx.listener(Self::toggle_theme))
            .size_full()
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .child(
                        div()
                            .h_full()
                            .when(sidebar_visible, |d| d.child(self.sidebar.clone())),
                    )
                    .child(div().flex_1().min_w_0().h_full().child(self.dock.clone())),
            )
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
