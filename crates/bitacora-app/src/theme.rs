//! Theme switching: light / dark / system plus a bundled theme per mode.
//!
//! The choice is kept in a GPUI global ([`ThemeController`]) and persisted to
//! `<config_dir>/settings.json` after every change.

use std::path::PathBuf;

use rust_i18n::t;

use crate::settings::{AppSettings, ThemePreference};
use crate::ui::menu::{PopupMenu, PopupMenuItem};
use crate::ui::theme::{Theme, ThemeMode, ThemeRegistry};
use crate::ui::{App, Global, Window, WindowAppearance};

/// Current theme choice and where it is persisted.
#[derive(Debug)]
pub struct ThemeController {
    settings: AppSettings,
    path: Option<PathBuf>,
}

impl Global for ThemeController {}

fn mode_of(appearance: WindowAppearance) -> ThemeMode {
    ThemeMode::from(appearance)
}

/// Installs the controller and applies the saved theme. `settings_path = None`
/// disables persistence (tests).
pub fn install(cx: &mut App, settings: AppSettings, settings_path: Option<PathBuf>) {
    cx.set_global(ThemeController {
        settings,
        path: settings_path,
    });
    apply(cx, None);
}

/// The current settings.
pub fn settings(cx: &App) -> AppSettings {
    cx.global::<ThemeController>().settings.clone()
}

/// The current settings, `None` before [`install`] (headless tests).
pub fn try_settings(cx: &App) -> Option<AppSettings> {
    cx.try_global::<ThemeController>()
        .map(|c| c.settings.clone())
}

/// Re-applies the stored choice (call again when the OS appearance changes).
pub fn apply(cx: &mut App, window: Option<&mut Window>) {
    let settings = settings(cx);
    resolve_theme_names(cx, &settings);
    let mode = match settings.mode {
        ThemePreference::Light => ThemeMode::Light,
        ThemePreference::Dark => ThemeMode::Dark,
        ThemePreference::System => match &window {
            Some(window) => mode_of(window.appearance()),
            None => mode_of(cx.window_appearance()),
        },
    };
    Theme::change(mode, window, cx);
    if let Some(size) = settings.font_size {
        let (min, max) = crate::settings::FONT_SIZE_RANGE;
        Theme::global_mut(cx).font_size = crate::ui::px(f32::from(size.clamp(min, max)));
    }
}

/// Points the theme's light/dark slots at the configured bundled themes; unknown
/// names log a warning and fall back to the default theme.
fn resolve_theme_names(cx: &mut App, settings: &AppSettings) {
    let registry = ThemeRegistry::global(cx);
    let pick = |name: &Option<String>, mode: ThemeMode| {
        let default = match mode {
            ThemeMode::Light => registry.default_light_theme().clone(),
            ThemeMode::Dark => registry.default_dark_theme().clone(),
        };
        match name {
            None => default,
            Some(name) => match registry.themes().get(name.as_str()) {
                Some(theme) if theme.mode == mode => theme.clone(),
                _ => {
                    tracing::warn!(theme = %name, "unknown {} theme, using the default", mode.name());
                    default
                }
            },
        }
    };
    let light = pick(&settings.light_theme, ThemeMode::Light);
    let dark = pick(&settings.dark_theme, ThemeMode::Dark);
    let theme = Theme::global_mut(cx);
    theme.light_theme = light;
    theme.dark_theme = dark;
}

/// Edits the settings, persists them and re-applies the theme (the settings view calls this for
/// every app-level change). Does nothing before [`install`].
pub fn edit_settings(
    cx: &mut App,
    window: Option<&mut Window>,
    edit: impl FnOnce(&mut AppSettings),
) {
    if cx.try_global::<ThemeController>().is_some() {
        update(cx, window, edit);
    }
}

fn update(cx: &mut App, window: Option<&mut Window>, edit: impl FnOnce(&mut AppSettings)) {
    let controller = cx.global_mut::<ThemeController>();
    edit(&mut controller.settings);
    if let Some(path) = &controller.path
        && let Err(err) = controller.settings.save(path)
    {
        tracing::warn!(path = %path.display(), "cannot save settings: {err}");
    }
    apply(cx, window);
    cx.refresh_windows();
}

/// Sets light / dark / system.
pub fn set_preference(cx: &mut App, window: Option<&mut Window>, pref: ThemePreference) {
    update(cx, window, |s| s.mode = pref);
}

/// Selects a bundled theme for `mode`.
pub fn set_theme_name(cx: &mut App, window: Option<&mut Window>, mode: ThemeMode, name: &str) {
    update(cx, window, |s| match mode {
        ThemeMode::Light => s.light_theme = Some(name.to_owned()),
        ThemeMode::Dark => s.dark_theme = Some(name.to_owned()),
    });
}

/// Flips between light and dark (an explicit choice, leaving "System").
pub fn toggle(cx: &mut App, window: Option<&mut Window>) {
    let next = if Theme::global(cx).is_dark() {
        ThemePreference::Light
    } else {
        ThemePreference::Dark
    };
    set_preference(cx, window, next);
}

/// Fills the "Theme" dropdown: mode choices followed by the bundled themes.
pub fn build_menu(menu: PopupMenu, cx: &App) -> PopupMenu {
    let current = settings(cx);
    let mut menu = menu;
    for (pref, label) in [
        (ThemePreference::System, t!("theme.system")),
        (ThemePreference::Light, t!("theme.light")),
        (ThemePreference::Dark, t!("theme.dark")),
    ] {
        menu = menu.item(
            PopupMenuItem::new(label.to_string())
                .checked(current.mode == pref)
                .on_click(move |_, window, cx| set_preference(cx, Some(window), pref)),
        );
    }
    let registry = ThemeRegistry::global(cx);
    for (mode, heading, selected) in [
        (
            ThemeMode::Light,
            t!("theme.light_themes"),
            current.light_theme.clone(),
        ),
        (
            ThemeMode::Dark,
            t!("theme.dark_themes"),
            current.dark_theme.clone(),
        ),
    ] {
        menu = menu
            .separator()
            .item(PopupMenuItem::label(heading.to_string()));
        for config in registry
            .sorted_themes()
            .into_iter()
            .filter(|config| config.mode == mode)
        {
            let name = config.name.to_string();
            let is_selected = match &selected {
                Some(s) => *s == name,
                None => config.is_default,
            };
            menu = menu.item(
                PopupMenuItem::new(name.clone())
                    .checked(is_selected)
                    .on_click(move |_, window, cx| set_theme_name(cx, Some(window), mode, &name)),
            );
        }
    }
    menu
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::testing::{TestAppContext, gpui_test};

    #[gpui_test]
    fn preference_drives_theme_mode_and_unknown_theme_falls_back(cx: &mut TestAppContext) {
        cx.update(|cx| {
            crate::ui::init(cx);
            install(
                cx,
                AppSettings {
                    mode: ThemePreference::Dark,
                    light_theme: Some("No Such Theme".into()),
                    dark_theme: None,
                    ..AppSettings::default()
                },
                None,
            );
            assert!(Theme::global(cx).is_dark());
            // Unknown name fell back to the default light theme.
            assert!(Theme::global(cx).light_theme.is_default);

            set_preference(cx, None, ThemePreference::Light);
            assert!(!Theme::global(cx).is_dark());
            toggle(cx, None);
            assert!(Theme::global(cx).is_dark());
            assert_eq!(settings(cx).mode, ThemePreference::Dark);
        });
    }

    #[gpui_test]
    fn choice_is_persisted(cx: &mut TestAppContext) {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("settings.json");
        cx.update(|cx| {
            crate::ui::init(cx);
            install(cx, AppSettings::default(), Some(path.clone()));
            set_preference(cx, None, ThemePreference::Dark);
        });
        assert_eq!(AppSettings::load(&path).mode, ThemePreference::Dark);
    }
}
