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
    Hsla, KeyBinding, KeyBindingContextPredicate, KeyDownEvent, NoAction, PathPromptOptions,
    Pixels, Point, Render, Rgba, SharedString, Size, StyledImage, Subscription, Task,
    TitlebarOptions, WeakEntity, Window, WindowAppearance, WindowBounds, WindowOptions, deferred,
    div, point, px, size,
};
pub use gpui_kit::{rgb, rgba};

/// The kit's "cancel" action (Escape in menus and command palettes).
pub use gpui_kit::base::actions::Cancel as MenuCancel;

/// Defines unit actions (re-exported so views never name the kit crate).
pub use gpui_kit::actions;

/// Styled components used by the shell.
pub use gpui_kit::component::{
    ActiveTheme, Disableable, Icon, IconName, Root, Selectable, Sizable, StyledExt, WindowExt,
    h_flex, v_flex,
};

/// Buttons.
pub mod button {
    pub use gpui_kit::component::button::{Button, ButtonVariants};
}

/// Toggle switch.
pub mod switch {
    pub use gpui_kit::component::switch::Switch;
}

/// Popovers.
pub mod popover {
    pub use gpui_kit::component::popover::{Popover, PopoverState};
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

/// Progress bar.
pub mod progress {
    pub use gpui_kit::component::progress::Progress;
}

/// Theme registry and modes.
pub mod theme {
    pub mod palette;

    pub use palette::Palette;

    #[cfg(test)]
    mod palette_tests;

    pub use gpui_kit::component::theme::{Theme, ThemeConfig, ThemeMode, ThemeRegistry};
}

/// The base calendar (date picking in the block editor, BIT-US-0105).
pub mod calendar {
    use chrono::{Datelike as _, NaiveDate};

    pub use gpui_kit::base::{
        Calendar, CalendarEvent, CalendarItem, CalendarItemKind, CalendarItemState, CalendarState,
        Date as CalendarDate,
    };

    /// The calendar value of a year/month/day.
    #[must_use]
    pub fn date_value(year: i32, month: u32, day: u32) -> Option<CalendarDate> {
        NaiveDate::from_ymd_opt(year, month, day).map(CalendarDate::from)
    }

    /// Year, month and day of a single selected value.
    #[must_use]
    pub fn ymd(date: &CalendarDate) -> Option<(i32, u32, u32)> {
        let d = date.start()?;
        Some((d.year(), d.month(), d.day()))
    }

    /// Activates a day as a click would (tests and the keyboard path).
    pub fn activate(
        state: &mut CalendarState,
        (y, m, d): (i32, u32, u32),
        cx: &mut gpui_kit::Context<CalendarState>,
    ) -> bool {
        NaiveDate::from_ymd_opt(y, m, d).is_some_and(|day| state.activate_date(day, cx))
    }
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
    // Windows without a `Root` (headless tests) cannot show toasts; keep the message in the log.
    if window.root::<Root>().flatten().is_none() {
        tracing::info!(?level, %message, "notification without a Root window");
        return;
    }
    let note = match level {
        Level::Info => Notification::info(message),
        Level::Success => Notification::success(message),
        Level::Warning => Notification::warning(message),
        Level::Error => Notification::error(message),
    };
    window.push_notification(note, cx);
}

/// Like [`notify`] but sticky, with one action button; `on_click` runs when the user presses it
/// (the toast closes itself). Windows without a `Root` only log the message.
pub fn notify_action(
    window: &mut Window,
    cx: &mut App,
    level: Level,
    message: impl Into<SharedString>,
    label: impl Into<SharedString>,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
) {
    use gpui_kit::component::notification::Notification;
    let message = message.into();
    if window.root::<Root>().flatten().is_none() {
        tracing::info!(?level, %message, "action notification without a Root window");
        return;
    }
    let note = match level {
        Level::Info => Notification::info(message),
        Level::Success => Notification::success(message),
        Level::Warning => Notification::warning(message),
        Level::Error => Notification::error(message),
    };
    let label: SharedString = label.into();
    let on_click = std::rc::Rc::new(on_click);
    let note = note.action(move |_, _, cx| {
        let this = cx.entity();
        let on_click = on_click.clone();
        button::Button::new("notice-action")
            .label(label.clone())
            .on_click(move |_, window, cx| {
                on_click(window, cx);
                this.update(cx, |note, cx| note.dismiss(window, cx));
            })
    });
    window.push_notification(note, cx);
}

/// What to ask in a confirmation dialog.
#[derive(Debug, Clone)]
pub struct Confirmation {
    /// Dialog title.
    pub title: String,
    /// Explanation under the title.
    pub description: String,
    /// Label of the confirming button.
    pub ok_text: String,
    /// Label of the cancelling button.
    pub cancel_text: String,
}

/// Opens a confirmation dialog (OK / Cancel); `on_ok` runs only when the user confirms. Returns
/// `false` without running anything when the window has no `Root` (headless tests), so a
/// destructive action can never proceed without the question having been shown.
pub fn confirm(
    window: &mut Window,
    cx: &mut App,
    question: Confirmation,
    on_ok: impl Fn(&mut Window, &mut App) + 'static,
) -> bool {
    if window.root::<Root>().flatten().is_none() {
        tracing::info!(title = %question.title, "confirmation without a Root window");
        return false;
    }
    let on_ok = std::rc::Rc::new(on_ok);
    window.open_alert_dialog(cx, move |alert, _, _| {
        let on_ok = on_ok.clone();
        alert
            .confirm()
            .title(question.title.clone())
            .description(question.description.clone())
            .ok_text(question.ok_text.clone())
            .cancel_text(question.cancel_text.clone())
            .on_ok(move |_, window, cx| {
                on_ok(window, cx);
                true
            })
    });
    true
}

/// Opens a two-button dialog; `on_ok` runs on the primary button, `on_cancel` on the secondary
/// one (Escape closes without either). Returns `false` when the window has no `Root`.
pub fn choose(
    window: &mut Window,
    cx: &mut App,
    question: Confirmation,
    on_ok: impl Fn(&mut Window, &mut App) + 'static,
    on_cancel: impl Fn(&mut Window, &mut App) + 'static,
) -> bool {
    if window.root::<Root>().flatten().is_none() {
        tracing::info!(title = %question.title, "dialog without a Root window");
        return false;
    }
    let on_ok = std::rc::Rc::new(on_ok);
    let on_cancel = std::rc::Rc::new(on_cancel);
    window.open_alert_dialog(cx, move |alert, _, _| {
        let (on_ok, on_cancel) = (on_ok.clone(), on_cancel.clone());
        alert
            .confirm()
            .title(question.title.clone())
            .description(question.description.clone())
            .ok_text(question.ok_text.clone())
            .cancel_text(question.cancel_text.clone())
            .on_ok(move |_, window, cx| {
                on_ok(window, cx);
                true
            })
            .on_cancel(move |_, window, cx| {
                on_cancel(window, cx);
                true
            })
    });
    true
}

/// Installs the application menu: one menu named `name` with a settings entry and a quit entry
/// (the macOS app menu; platforms without a global menu ignore it).
pub fn set_app_menu(
    cx: &mut App,
    name: &str,
    settings: (&str, impl Action),
    quit: (&str, impl Action),
) {
    use gpui_kit::{Menu, MenuItem};
    cx.set_menus([Menu {
        name: name.to_owned().into(),
        items: vec![
            MenuItem::action(settings.0.to_owned(), settings.1),
            MenuItem::separator(),
            MenuItem::action(quit.0.to_owned(), quit.1),
        ],
        disabled: false,
    }]);
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
        AvailableSpace, ClipboardEntry, ClipboardItem, CursorStyle, Element, ElementId,
        ElementInputHandler, EntityInputHandler, ExternalPaths, Font, FontStyle, FontWeight,
        GlobalElementId, HighlightStyle, Image, ImageFormat, InspectorElementId, InteractiveText,
        IntoElement, LayoutId, ListAlignment, ListOffset, ListState, Modifiers, MouseButton,
        MouseDownEvent, MouseMoveEvent, MouseUpEvent, ObjectFit, PaintQuad, Rgba,
        StrikethroughStyle, Style, StyledImage, StyledText, TextAlign, TextLayout, TextRun,
        UTF16Selection, UnderlineStyle, WrappedLine, fill, hsla, img, list, relative, rgba,
    };
}

/// Accessibility roles and states (GPUI exposes them through AccessKit; BIT-T-0338). Elements
/// need an id and a role to appear in the accessibility tree: `div().id(..).role(Role::Button)
/// .aria_label(..)`.
pub mod a11y {
    pub use gpui_kit::gpui::{Orientation, Role, Toggled};
}

/// Drag and drop of blocks (BIT-US-0106).
pub mod drag {
    pub use gpui_kit::DragMoveEvent;
}

/// Command palette (search and actions palettes).
pub mod command {
    pub use gpui_kit::component::IndexPath;
    pub use gpui_kit::component::command::{Command, CommandGroup, CommandItem, CommandState};
}

/// Data table (all pages view).
pub mod table {
    pub use gpui_kit::component::table::{
        Column, ColumnSort, DataTable, TableDelegate, TableEvent, TableState,
    };
}

/// GPUI Kit text inputs (used by the Textarea-per-block probe, BIT-T-0105).
pub mod input {
    pub use gpui_kit::component::input::{
        Backspace, Enter, Indent, Input, InputEvent, InputState, MoveDown, MoveUp, Textarea,
        TextareaState,
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
