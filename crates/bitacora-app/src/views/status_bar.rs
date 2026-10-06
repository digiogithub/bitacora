//! Bottom status bar with sync, MCP and index slots plus a heartbeat demo slot.

use async_channel::Receiver;
use rust_i18n::t;

use crate::events::EventPump;
use crate::theme;
use crate::ui::button::{Button, ButtonVariants as _};
use crate::ui::menu::DropdownMenu as _;
use crate::ui::status_bar::StatusBar;
use crate::ui::{
    Context, IconName, IntoElement, ParentElement as _, Render, Sizable as _, Styled as _, Task,
    Window, h_flex, icon,
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

/// Events that update the status bar (sent through an `async_channel`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusEvent {
    /// A slot changed state.
    Slot(Slot, SlotState),
    /// Demo heartbeat from a timer-driven producer.
    Heartbeat(u64),
}

/// The status bar view.
#[derive(Debug)]
pub struct AppStatusBar {
    sync: SlotState,
    mcp: SlotState,
    index: SlotState,
    heartbeat: u64,
    _pump: Task<()>,
}

impl AppStatusBar {
    /// Creates the bar; `rx` delivers [`StatusEvent`]s from any thread or task.
    pub fn new(rx: Receiver<StatusEvent>, cx: &mut Context<Self>) -> Self {
        let pump = EventPump::attach(cx, rx, |this: &mut Self, event, _| this.apply(event));
        Self {
            sync: SlotState::Off,
            mcp: SlotState::Off,
            index: SlotState::Off,
            heartbeat: 0,
            _pump: pump,
        }
    }

    /// Applies one event.
    pub fn apply(&mut self, event: StatusEvent) {
        match event {
            StatusEvent::Slot(slot, state) => self.set_slot_state(slot, state),
            StatusEvent::Heartbeat(n) => self.heartbeat = n,
        }
    }

    fn set_slot_state(&mut self, slot: Slot, state: SlotState) {
        match slot {
            Slot::Sync => self.sync = state,
            Slot::Mcp => self.mcp = state,
            Slot::Index => self.index = state,
        }
    }

    /// Sets the sync slot.
    pub fn set_sync(&mut self, state: SlotState, cx: &mut Context<Self>) {
        self.sync = state;
        cx.notify();
    }

    /// Sets the MCP slot.
    pub fn set_mcp(&mut self, state: SlotState, cx: &mut Context<Self>) {
        self.mcp = state;
        cx.notify();
    }

    /// Sets the index slot.
    pub fn set_index(&mut self, state: SlotState, cx: &mut Context<Self>) {
        self.index = state;
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
        h_flex()
            .gap_1()
            .child(icon(state.icon()).xsmall())
            .child(Self::slot_label(slot, state))
    }
}

impl Render for AppStatusBar {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let theme_button = Button::new("theme-menu")
            .ghost()
            .xsmall()
            .icon(IconName::Palette)
            .label(t!("status.theme_button").to_string())
            .dropdown_menu(|menu, _, cx| theme::build_menu(menu, cx));
        StatusBar::new()
            .left(self.slot_element(Slot::Sync))
            .left(self.slot_element(Slot::Mcp))
            .left(self.slot_element(Slot::Index))
            .right(t!("status.heartbeat", count = self.heartbeat).to_string())
            .right(theme_button)
    }
}
