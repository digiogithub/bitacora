//! Thin facade over GPUI and GPUI Kit (ADR-001, GPUI Kit risks R1/R2).
//!
//! Rule: **only files under `src/ui/` may name the GPUI Kit crate by path**. Every
//! other module imports GPUI / GPUI Kit types from `crate::ui`. This keeps the
//! blast radius of GPUI Kit's frequent API churn (it is pre-1.0 and re-pins GPUI
//! weekly) inside this directory. The rule is enforced by the
//! `only_ui_names_gpui_kit` test below.
//!
//! Upgrade procedure: bump the exact `gpui-kit` pin in the workspace manifest,
//! fix this facade until the crate compiles, then fix the views.
//!
//! The facade deliberately lists its re-exports explicitly instead of
//! glob-importing the kit root: with `test-support` that glob also pulls in
//! GPUI's `test` attribute macro, which would shadow Rust's built-in `#[test]`.

pub use gpui_kit::prelude::*;
pub use gpui_kit::{
    Action, Anchor, AnyElement, AnyView, AnyWindowHandle, App, AppContext, Application, AsyncApp,
    Bounds, ClickEvent, Context, Entity, EntityId, EventEmitter, FocusHandle, Focusable, Global,
    Hsla, KeyBinding, KeyBindingContextPredicate, Pixels, Point, Render, SharedString, Size,
    Subscription, Task, TitlebarOptions, WeakEntity, Window, WindowAppearance, WindowBounds,
    WindowOptions, div, point, px, size,
};

/// Defines unit actions (re-exported so views never name the kit crate).
pub use gpui_kit::actions;

/// Styled components used by the shell.
pub use gpui_kit::component::{
    ActiveTheme, Icon, IconName, Root, Selectable, Sizable, StyledExt, WindowExt, h_flex, v_flex,
};

/// Buttons.
pub mod button {
    pub use gpui_kit::component::button::{Button, ButtonVariants};
}

/// Menus and dropdowns.
pub mod menu {
    pub use gpui_kit::component::menu::{DropdownMenu, PopupMenu, PopupMenuItem};
}

/// Left sidebar building blocks.
pub mod sidebar {
    pub use gpui_kit::component::sidebar::{
        Sidebar, SidebarCollapsible, SidebarGroup, SidebarHeader, SidebarMenu, SidebarMenuItem,
    };
}

/// Status bar chrome.
pub mod status_bar {
    pub use gpui_kit::component::status_bar::StatusBar;
}

/// Theme registry and modes.
pub mod theme {
    pub use gpui_kit::component::theme::{Theme, ThemeConfig, ThemeMode, ThemeRegistry};
}

/// Dock area and panels.
pub mod dock {
    pub use gpui_kit::component::dock::{
        BasePanel, DockArea, DockAreaState, DockEvent, DockLayout, DockPlacement, DockSkin, Panel,
        PanelEvent, PanelStyle, panel_handle, register_panel,
    };
}

/// Notification severity used by [`notify`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    /// Informational.
    Info,
    /// Operation succeeded.
    Success,
    /// Something needs attention.
    Warning,
    /// Operation failed.
    Error,
}

/// Shows a toast in `window` (requires a `Root`, which [`open_main_window`] provides).
pub fn notify(window: &mut Window, cx: &mut App, level: Level, message: impl Into<SharedString>) {
    use gpui_kit::component::notification::Notification;
    let message = message.into();
    let note = match level {
        Level::Info => Notification::info(message),
        Level::Success => Notification::success(message),
        Level::Warning => Notification::warning(message),
        Level::Error => Notification::error(message),
    };
    window.push_notification(note, cx);
}

/// Builds a key binding from raw keymap data (keystrokes, a built action, optional
/// key context such as `"Workspace"`).
pub fn key_binding(
    keystrokes: &str,
    action: Box<dyn Action>,
    context: Option<&str>,
) -> anyhow::Result<KeyBinding> {
    let predicate = match context {
        Some(c) => Some(std::rc::Rc::new(KeyBindingContextPredicate::parse(c)?)),
        None => None,
    };
    Ok(KeyBinding::load(
        keystrokes,
        action,
        predicate,
        false,
        None,
        &gpui_kit::DummyKeyboardMapper,
    )?)
}

/// Builds an icon element from a Lucide icon name.
pub fn icon(name: IconName) -> Icon {
    Icon::new(name)
}

/// Starts the platform application with the default GPUI Kit assets (icons).
pub fn application() -> Application {
    gpui_kit::application().with_assets(gpui_kit::assets::Assets)
}

/// Initializes GPUI Kit (themes, component keymaps, global state).
pub fn init(cx: &mut App) {
    gpui_kit::init(cx);
}

/// Opens a window whose root view is GPUI Kit's `Root` wrapping the view built by
/// `build` (GPUI Kit 0.7 one-window entry point; dialogs/notifications need the `Root`).
pub fn open_main_window<V: Render>(
    options: WindowOptions,
    cx: &mut App,
    build: impl FnOnce(&mut Window, &mut App) -> Entity<V>,
) -> anyhow::Result<(AnyWindowHandle, Entity<V>)> {
    gpui_kit::open_window(options, cx, build)
}

/// Low-level GPUI text and list APIs for custom elements (the block editor spike and,
/// later, the real block editor). Kept explicit so the kit churn stays in this file.
pub mod text_edit {
    pub use gpui_kit::{
        AvailableSpace, ClipboardItem, CursorStyle, Element, ElementId, ElementInputHandler,
        EntityInputHandler, Font, FontWeight, GlobalElementId, InspectorElementId, IntoElement,
        LayoutId, ListAlignment, ListOffset, ListState, Modifiers, MouseButton, MouseDownEvent,
        MouseMoveEvent, MouseUpEvent, PaintQuad, Rgba, StrikethroughStyle, Style, StyledText,
        TextAlign, TextLayout, TextRun, UTF16Selection, UnderlineStyle, WrappedLine, fill, hsla,
        list, relative, rgba,
    };
}

/// GPUI Kit text inputs (used by the Textarea-per-block probe, BIT-T-0105).
pub mod input {
    pub use gpui_kit::component::input::{
        Backspace, Enter, Indent, InputEvent, MoveDown, MoveUp, Textarea, TextareaState,
    };
}

/// Test helpers (headless windows, simulated input).
#[cfg(test)]
pub mod testing {
    pub use gpui_kit::TestAppContext;
    pub use gpui_kit::VisualTestContext;
    /// The GPUI test attribute (`#[ui::testing::gpui_test]`).
    pub use gpui_kit::test as gpui_test;
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                rust_files(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }

    #[test]
    fn only_ui_names_gpui_kit() {
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let ui_dir = src.join("ui");
        let mut files = Vec::new();
        rust_files(&src, &mut files);
        assert!(!files.is_empty());
        // Built from pieces so this very file does not match its own needle.
        let needle = ["gpui_kit", "::"].concat();
        let mut offenders = Vec::new();
        for file in files.iter().filter(|f| !f.starts_with(&ui_dir)) {
            let text = std::fs::read_to_string(file).unwrap_or_default();
            if text.contains(&needle) {
                offenders.push(file.display().to_string());
            }
        }
        assert!(offenders.is_empty(), "outside src/ui: {offenders:?}");
    }
}
