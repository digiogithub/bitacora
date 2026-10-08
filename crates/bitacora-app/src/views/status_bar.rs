//! Bottom status bar with sync, MCP and index slots plus the app version on the right.

use crate::views::dims;
use async_channel::Receiver;
use bitacora_runtime::SyncStatusView;
use bitacora_sync::state::SyncState;
use rust_i18n::t;

use crate::events::EventPump;
use crate::theme;
use crate::ui::button::{Button, ButtonVariants as _};
use crate::ui::menu::DropdownMenu as _;
use crate::ui::progress::Progress;
use crate::ui::status_bar::StatusBar;
use crate::ui::theme::ActiveBitacoraTheme as _;
use crate::ui::{
    Context, EventEmitter, FluentBuilder as _, IconName, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, Sizable as _, StatefulInteractiveElement as _, Styled as _, Task,
    Window, div, h_flex, icon,
};

/// Which subsystem a slot describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    /// Git sync.
    Sync,
    /// MCP server.
    Mcp,
    /// SQLite index.
    Index,
}

/// State of a subsystem as shown in its slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SlotState {
    /// Disabled / not configured.
    #[default]
    Off,
    /// Running and healthy.
    Idle,
    /// Working.
    Busy,
    /// Failed.
    Error,
}

impl SlotState {
    fn icon(self) -> IconName {
        match self {
            Self::Off => IconName::Minus,
            Self::Idle => IconName::CircleCheck,
            Self::Busy => IconName::LoaderCircle,
            Self::Error => IconName::CircleAlert,
        }
    }
}

/// What the user asked of the status bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusBarEvent {
    /// The sync indicator was clicked: show the sync panel.
    SyncClicked,
    /// "Sync now" / "Retry" was clicked.
    SyncNow,
}

/// How a sync status looks in the slot: icon and colour class (BIT-T-0294).
pub fn slot_state_for(view: &SyncStatusView) -> SlotState {
    match &view.status.state {
        SyncState::Disabled => SlotState::Off,
        SyncState::Idle | SyncState::Dirty => SlotState::Idle,
        SyncState::Error(_) | SyncState::Conflicted | SyncState::Offline => SlotState::Error,
        _ => SlotState::Busy,
    }
}

/// Events that update the status bar (sent through an `async_channel`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusEvent {
    /// A slot changed state.
    Slot(Slot, SlotState),
    /// Demo heartbeat from a timer-driven producer.
    Heartbeat(u64),
    /// Indexing progress (files done / total); also marks the index slot busy.
    IndexProgress {
        /// Files indexed so far.
        done: usize,
        /// Files to index.
        total: usize,
    },
}

/// The status bar view.
#[derive(Debug)]
pub struct AppStatusBar {
    sync: SlotState,
    mcp: SlotState,
    index: SlotState,
    sync_view: Option<SyncStatusView>,
    heartbeat: u64,
    progress: Option<(usize, usize)>,
    _pump: Task<()>,
}

impl EventEmitter<StatusBarEvent> for AppStatusBar {}

impl AppStatusBar {
    /// Creates the bar; `rx` delivers [`StatusEvent`]s from any thread or task.
    pub fn new(rx: Receiver<StatusEvent>, cx: &mut Context<Self>) -> Self {
        let pump = EventPump::attach(cx, rx, |this: &mut Self, event, _| this.apply(event));
        Self {
            sync: SlotState::Off,
            mcp: SlotState::Off,
            index: SlotState::Off,
            sync_view: None,
            heartbeat: 0,
            progress: None,
            _pump: pump,
        }
    }

    /// Applies one event.
    pub fn apply(&mut self, event: StatusEvent) {
        match event {
            StatusEvent::Slot(slot, state) => self.set_slot_state(slot, state),
            StatusEvent::Heartbeat(n) => self.heartbeat = n,
            StatusEvent::IndexProgress { done, total } => {
                self.index = SlotState::Busy;
                self.progress = Some((done, total));
            }
        }
    }

    fn set_slot_state(&mut self, slot: Slot, state: SlotState) {
        if slot == Slot::Index && state != SlotState::Busy {
            self.progress = None;
        }
        match slot {
            Slot::Sync => self.sync = state,
            Slot::Mcp => self.mcp = state,
            Slot::Index => self.index = state,
        }
    }

    /// Sets the sync slot.
    pub fn set_sync(&mut self, state: SlotState, cx: &mut Context<Self>) {
        self.set_slot_state(Slot::Sync, state);
        cx.notify();
    }

    /// Shows the engine's status in the sync slot (`None`: sync is off).
    pub fn set_sync_view(&mut self, view: Option<SyncStatusView>, cx: &mut Context<Self>) {
        let state = view.as_ref().map_or(SlotState::Off, slot_state_for);
        self.sync_view = view;
        self.set_slot_state(Slot::Sync, state);
        cx.notify();
    }

    /// The status shown in the sync slot.
    pub fn sync_view(&self) -> Option<&SyncStatusView> {
        self.sync_view.as_ref()
    }

    /// Sets the MCP slot.
    pub fn set_mcp(&mut self, state: SlotState, cx: &mut Context<Self>) {
        self.set_slot_state(Slot::Mcp, state);
        cx.notify();
    }

    /// Sets the index slot.
    pub fn set_index(&mut self, state: SlotState, cx: &mut Context<Self>) {
        self.set_slot_state(Slot::Index, state);
        cx.notify();
    }

    /// Current state of a slot.
    pub fn slot(&self, slot: Slot) -> SlotState {
        match slot {
            Slot::Sync => self.sync,
            Slot::Mcp => self.mcp,
            Slot::Index => self.index,
        }
    }

    /// Indexing progress `(done, total)` while the index is being built.
    pub fn index_progress(&self) -> Option<(usize, usize)> {
        self.progress
    }

    /// Latest heartbeat value.
    pub fn heartbeat(&self) -> u64 {
        self.heartbeat
    }

    fn slot_label(slot: Slot, state: SlotState) -> String {
        match (slot, state) {
            (Slot::Sync, SlotState::Off) => t!("status.sync.off"),
            (Slot::Sync, SlotState::Idle) => t!("status.sync.idle"),
            (Slot::Sync, SlotState::Busy) => t!("status.sync.busy"),
            (Slot::Sync, SlotState::Error) => t!("status.sync.error"),
            (Slot::Mcp, SlotState::Off) => t!("status.mcp.off"),
            (Slot::Mcp, SlotState::Idle) => t!("status.mcp.idle"),
            (Slot::Mcp, SlotState::Busy) => t!("status.mcp.busy"),
            (Slot::Mcp, SlotState::Error) => t!("status.mcp.error"),
            (Slot::Index, SlotState::Off) => t!("status.index.off"),
            (Slot::Index, SlotState::Idle) => t!("status.index.idle"),
            (Slot::Index, SlotState::Busy) => t!("status.index.busy"),
            (Slot::Index, SlotState::Error) => t!("status.index.error"),
        }
        .to_string()
    }

    fn slot_element(&self, slot: Slot) -> impl IntoElement {
        let state = self.slot(slot);
        let progress = (slot == Slot::Index).then_some(self.progress).flatten();
        let label = match progress {
            Some((done, total)) => {
                t!("status.index.progress", done = done, total = total).to_string()
            }
            None => Self::slot_label(slot, state),
        };
        h_flex()
            .gap_1()
            .child(icon(state.icon()).xsmall())
            .child(label)
            .when_some(progress, |row, (done, total)| {
                let percent = if total == 0 {
                    100.0
                } else {
                    done as f32 * 100.0 / total as f32
                };
                row.child(
                    Progress::new("index-progress")
                        .w(dims::PX_80)
                        .value(percent),
                )
            })
    }
}

impl AppStatusBar {
    /// The sync slot: icon and message, clickable, with "Sync now" / "Retry" beside it.
    fn sync_element(&self, cx: &mut Context<Self>) -> crate::ui::AnyElement {
        let Some(view) = &self.sync_view else {
            let this = cx.entity();
            return h_flex()
                .id("sync-slot")
                .gap_1()
                .cursor_pointer()
                .on_click(move |_, _, cx| {
                    this.update(cx, |_, cx| cx.emit(StatusBarEvent::SyncClicked))
                })
                .child(icon(self.sync.icon()).xsmall())
                .child(Self::slot_label(Slot::Sync, self.sync))
                .into_any_element();
        };
        let state = slot_state_for(view);
        let open = cx.entity();
        let now = cx.entity();
        let busy = state == SlotState::Busy;
        h_flex()
            .gap_1()
            .child(
                h_flex()
                    .id("sync-slot")
                    .gap_1()
                    .cursor_pointer()
                    .on_click(move |_, _, cx| {
                        open.update(cx, |_, cx| cx.emit(StatusBarEvent::SyncClicked));
                    })
                    .child(icon(state.icon()).xsmall())
                    .child(view.message.clone()),
            )
            .when(!busy, |row| {
                row.child(
                    Button::new("sync-now")
                        .ghost()
                        .xsmall()
                        .icon(IconName::RefreshCw)
                        .label(if view.can_retry {
                            t!("status.sync.retry").to_string()
                        } else {
                            t!("status.sync.now").to_string()
                        })
                        .on_click(move |_, _, cx| {
                            now.update(cx, |_, cx| cx.emit(StatusBarEvent::SyncNow));
                        }),
                )
            })
            .into_any_element()
    }
}

/// The running app version as shown in the footer, e.g. `v2.0.2` (BIT-US-0174).
#[must_use]
pub fn version_label() -> String {
    format!("v{}", env!("CARGO_PKG_VERSION"))
}

impl Render for AppStatusBar {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.bitacora().colors.muted;
        let theme_button = Button::new("theme-menu")
            .ghost()
            .xsmall()
            .icon(IconName::Palette)
            .label(t!("status.theme_button").to_string())
            .dropdown_menu(|menu, _, cx| theme::build_menu(menu, cx));
        StatusBar::new()
            .left(self.sync_element(cx))
            .left(self.slot_element(Slot::Mcp))
            .left(self.slot_element(Slot::Index))
            .right(div().text_color(muted).child(version_label()))
            .right(theme_button)
    }
}

#[cfg(test)]
mod version_tests {
    use super::version_label;

    #[test]
    fn the_footer_shows_the_cargo_package_version() {
        let label = version_label();
        assert_eq!(label, concat!("v", env!("CARGO_PKG_VERSION")));
        assert!(label.starts_with('v') && label.len() > 1);
    }
}
