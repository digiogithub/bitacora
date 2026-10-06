//! Theme switching: light / dark / system plus a bundled theme per mode.
//!
//! The choice is kept in a GPUI global ([`ThemeController`]) and persisted to
//! `<config_dir>/settings.json` after every change.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use rust_i18n::t;

use crate::custom_css::{self, CustomCss, Diagnostic, Overrides, Rgba8};
use crate::settings::{AppSettings, ThemePreference};
use crate::ui::menu::{PopupMenu, PopupMenuItem};
use crate::ui::theme::{Theme, ThemeMode, ThemeRegistry};
use crate::ui::{App, Global, Hsla, Task, Window, WindowAppearance};

/// Themes shipped with the app, in addition to the two GPUI Kit defaults.
const BUNDLED_THEMES: &str = include_str!("../assets/themes/bitacora.json");

/// How often the open graph's `logseq/custom.css` is checked for changes.
const CSS_POLL: Duration = Duration::from_secs(2);

/// The state of the open graph's `custom.css` (BIT-T-0334).
#[derive(Default)]
struct CssState {
    path: Option<PathBuf>,
    stamp: Option<(SystemTime, u64)>,
    parsed: CustomCss,
    poller: Option<Task<()>>,
}

/// Current theme choice and where it is persisted.
pub struct ThemeController {
    settings: AppSettings,
    path: Option<PathBuf>,
    css: CssState,
    /// Problems found while loading user theme files.
    theme_errors: Vec<String>,
}

impl std::fmt::Debug for ThemeController {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ThemeController")
            .field("settings", &self.settings)
            .field("css_path", &self.css.path)
            .finish_non_exhaustive()
    }
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
        css: CssState::default(),
        theme_errors: Vec::new(),
    });
    if let Err(err) = ThemeRegistry::global_mut(cx).load_themes_from_str(BUNDLED_THEMES) {
        tracing::warn!("cannot load the bundled themes: {err}");
    }
    reload_user_themes(cx);
    apply(cx, None);
}

/// `<config_dir>/themes`, next to the settings file.
fn user_themes_dir(cx: &App) -> Option<PathBuf> {
    cx.global::<ThemeController>()
        .path
        .as_deref()
        .and_then(Path::parent)
        .map(|dir| dir.join("themes"))
}

/// Loads `*.json` theme files from the user themes directory (new theme names only; a theme
/// already registered keeps its definition until restart). Returns the load problems, which
/// Settings > Appearance lists.
pub fn reload_user_themes(cx: &mut App) -> Vec<String> {
    let mut errors = Vec::new();
    if let Some(dir) = user_themes_dir(cx)
        && let Ok(entries) = std::fs::read_dir(&dir)
    {
        let mut files: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_file() && p.extension().is_some_and(|e| e == "json"))
            .collect();
        files.sort();
        for file in files {
            let name = file
                .file_name()
                .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
            match std::fs::read_to_string(&file) {
                Ok(text) => {
                    if let Err(err) = ThemeRegistry::global_mut(cx).load_themes_from_str(&text) {
                        errors.push(format!("{name}: {err}"));
                    }
                }
                Err(err) => errors.push(format!("{name}: {err}")),
            }
        }
    }
    for err in &errors {
        tracing::warn!("user theme ignored: {err}");
    }
    cx.global_mut::<ThemeController>().theme_errors = errors.clone();
    errors
}

/// Problems found while loading user theme files.
pub fn user_theme_errors(cx: &App) -> Vec<String> {
    cx.try_global::<ThemeController>()
        .map(|c| c.theme_errors.clone())
        .unwrap_or_default()
}

/// Points the theme at the open graph's `logseq/custom.css` (`None` when no graph is open) and
/// starts watching it. The file is only read, never written.
pub fn set_graph_css(cx: &mut App, graph_root: Option<&Path>) {
    if cx.try_global::<ThemeController>().is_none() {
        return;
    }
    let css = &mut cx.global_mut::<ThemeController>().css;
    css.path = graph_root.map(|root| root.join("logseq").join("custom.css"));
    css.stamp = None;
    css.parsed = CustomCss::default();
    css.poller = None;
    check_css(cx);
    if graph_root.is_some() {
        let poller = cx.spawn(async move |cx| {
            loop {
                cx.background_executor().timer(CSS_POLL).await;
                cx.update(check_css);
            }
        });
        cx.global_mut::<ThemeController>().css.poller = Some(poller);
    }
    apply(cx, None);
    cx.refresh_windows();
}

/// Re-reads `custom.css` when its modification time or size changed and re-applies the theme.
/// Returns whether anything was reloaded.
pub fn check_css(cx: &mut App) -> bool {
    let Some(controller) = cx.try_global::<ThemeController>() else {
        return false;
    };
    let Some(path) = controller.css.path.clone() else {
        return false;
    };
    let stamp = std::fs::metadata(&path)
        .ok()
        .and_then(|m| Some((m.modified().ok()?, m.len())));
    if stamp == controller.css.stamp {
        return false;
    }
    let parsed = match &stamp {
        Some(_) => match std::fs::read(&path) {
            Ok(bytes) => custom_css::parse(&String::from_utf8_lossy(&bytes)),
            Err(err) => {
                tracing::warn!(path = %path.display(), "cannot read custom.css: {err}");
                CustomCss::default()
            }
        },
        None => CustomCss::default(),
    };
    let css = &mut cx.global_mut::<ThemeController>().css;
    css.stamp = stamp;
    css.parsed = parsed;
    apply(cx, None);
    cx.refresh_windows();
    true
}

/// What the open graph's `custom.css` contributed: the ignored constructs, how many values were
/// applied and the file path when it exists (for Settings > Appearance).
pub fn css_report(cx: &App) -> (Vec<Diagnostic>, usize, Option<PathBuf>) {
    cx.try_global::<ThemeController>().map_or_else(
        || (Vec::new(), 0, None),
        |c| {
            (
                c.css.parsed.diagnostics.clone(),
                c.css.parsed.applied_count(),
                c.css.path.clone().filter(|_| c.css.stamp.is_some()),
            )
        },
    )
}

/// The block bullet colour requested by `custom.css` for the current mode, if any. Kept outside
/// the GPUI `Theme` (which has no slot for it) so block rows can read it without an `App`.
pub fn bullet_color() -> Option<Hsla> {
    BULLET.lock().ok().and_then(|guard| *guard).map(hsla_of)
}

static BULLET: std::sync::Mutex<Option<Rgba8>> = std::sync::Mutex::new(None);

fn hsla_of(c: Rgba8) -> Hsla {
    crate::ui::Rgba {
        r: f32::from(c[0]) / 255.0,
        g: f32::from(c[1]) / 255.0,
        b: f32::from(c[2]) / 255.0,
        a: f32::from(c[3]) / 255.0,
    }
    .into()
}

/// Lays the `custom.css` values over the resolved theme.
fn apply_overrides(theme: &mut Theme, ov: &Overrides, font_size_free: bool) {
    let c = &mut theme.colors;
    if let Some(v) = ov.background {
        let v = hsla_of(v);
        c.background = v;
        c.popover = v;
        c.list = v;
    }
    if let Some(v) = ov.secondary_background {
        let v = hsla_of(v);
        c.sidebar = v;
        c.title_bar = v;
        c.status_bar = v;
    }
    if let Some(v) = ov.tertiary_background {
        let v = hsla_of(v);
        c.muted = v;
        c.secondary = v;
    }
    if let Some(v) = ov.text {
        let v = hsla_of(v);
        c.foreground = v;
        c.popover_foreground = v;
        c.sidebar_foreground = v;
    }
    if let Some(v) = ov.secondary_text {
        c.muted_foreground = hsla_of(v);
    }
    if let Some(v) = ov.link {
        let v = hsla_of(v);
        c.link = v;
        c.link_hover = v;
        c.link_active = v;
    }
    if let Some(v) = ov.border {
        let v = hsla_of(v);
        c.border = v;
        c.sidebar_border = v;
    }
    if let Some(v) = ov.selection {
        c.selection = hsla_of(v);
    }
    if let Some(family) = &ov.font_family {
        theme.font_family = family.clone().into();
    }
    if let (Some(px), true) = (ov.font_size, font_size_free) {
        theme.font_size = crate::ui::px(px);
    }
}

/// Sets the UI language (`None` follows the system).
pub fn set_language(cx: &mut App, window: Option<&mut Window>, language: Option<&str>) {
    update(cx, window, |s| s.language = language.map(str::to_owned));
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
    crate::i18n::apply(settings.language.as_deref());
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
    let overrides = {
        let dark = Theme::global(cx).is_dark();
        cx.global::<ThemeController>().css.parsed.effective(dark)
    };
    if let Ok(mut bullet) = BULLET.lock() {
        *bullet = overrides.bullet;
    }
    if !overrides.is_empty() {
        let font_free = settings.font_size.is_none();
        // `Theme::update` carries the colour edits into the component tokens too.
        Theme::update(cx, |theme| apply_overrides(theme, &overrides, font_free));
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

    fn rgb(c: Hsla) -> [u8; 3] {
        let rgba = crate::ui::Rgba::from(c);
        [rgba.r, rgba.g, rgba.b].map(|v| (v * 255.0).round() as u8)
    }

    #[gpui_test]
    fn bundled_and_user_themes_are_selectable_and_switch_live(cx: &mut TestAppContext) {
        let tmp = tempfile::tempdir().expect("tempdir");
        let themes = tmp.path().join("themes");
        std::fs::create_dir_all(&themes).expect("mkdir");
        std::fs::write(
            themes.join("mine.json"),
            r##"{"name":"Mine","themes":[{"name":"Mine Dark","mode":"dark","colors":{"background":"#102030"}}]}"##,
        )
        .expect("write");
        std::fs::write(themes.join("broken.json"), "{nope").expect("write");
        let path = tmp.path().join("settings.json");
        cx.update(|cx| {
            crate::ui::init(cx);
            install(cx, AppSettings::default(), Some(path));
            let registry = ThemeRegistry::global(cx);
            for name in [
                "Paper",
                "Solarized Light",
                "Midnight",
                "Solarized Dark",
                "Mine Dark",
            ] {
                assert!(registry.themes().contains_key(name), "{name} registered");
            }
            assert_eq!(
                user_theme_errors(cx).len(),
                1,
                "the broken file is reported"
            );

            set_preference(cx, None, ThemePreference::Light);
            let default_bg = Theme::global(cx).colors.background;
            set_theme_name(cx, None, ThemeMode::Light, "Paper");
            assert_ne!(Theme::global(cx).colors.background, default_bg);
            assert_eq!(rgb(Theme::global(cx).colors.background), [0xfb, 0xf7, 0xee]);

            set_preference(cx, None, ThemePreference::Dark);
            set_theme_name(cx, None, ThemeMode::Dark, "Mine Dark");
            assert!(Theme::global(cx).is_dark());
            assert_eq!(rgb(Theme::global(cx).colors.background), [0x10, 0x20, 0x30]);
        });
    }

    fn write_css(root: &Path, css: &str) {
        std::fs::create_dir_all(root.join("logseq")).expect("mkdir");
        std::fs::write(root.join("logseq/custom.css"), css).expect("write css");
    }

    #[gpui_test]
    fn custom_css_maps_onto_the_theme_and_hot_reloads(cx: &mut TestAppContext) {
        let graph = tempfile::tempdir().expect("tempdir");
        let css = "
:root { --ls-primary-background-color: #fdf6e3; --ls-link-text-color: #ff0000;
        --ls-block-bullet-color: #00ff00; --ls-odd-thing: 1; }
.dark-theme { --ls-primary-background-color: #001122; }
body { font-family: 'Fira Sans', sans-serif; font-size: 20px; }
.weird > .selector { color: red }
";
        write_css(graph.path(), css);
        let before = std::fs::read(graph.path().join("logseq/custom.css")).expect("read");
        cx.update(|cx| {
            crate::ui::init(cx);
            install(
                cx,
                AppSettings {
                    mode: ThemePreference::Light,
                    ..AppSettings::default()
                },
                None,
            );
            set_graph_css(cx, Some(graph.path()));
            let theme = Theme::global(cx);
            assert_eq!(rgb(theme.colors.background), [0xfd, 0xf6, 0xe3]);
            assert_eq!(rgb(theme.colors.link), [0xff, 0, 0]);
            assert_eq!(theme.font_family.as_ref(), "Fira Sans");
            assert_eq!(f32::from(theme.font_size), 20.0);
            assert_eq!(bullet_color().map(rgb), Some([0, 0xff, 0]));
            let (diagnostics, applied, path) = css_report(cx);
            assert!(applied >= 5 && path.is_some());
            assert!(
                diagnostics
                    .iter()
                    .any(|d| d.subject.contains("--ls-odd-thing"))
            );
            assert!(diagnostics.iter().any(|d| d.subject.contains(".weird")));

            // Dark mode picks the `.dark-theme` override.
            set_preference(cx, None, ThemePreference::Dark);
            assert_eq!(rgb(Theme::global(cx).colors.background), [0, 0x11, 0x22]);

            // The font size set in Settings wins over the stylesheet.
            edit_settings(cx, None, |s| s.font_size = Some(14));
            assert_eq!(f32::from(Theme::global(cx).font_size), 14.0);

            // Editing the file is picked up by the poller.
            write_css(
                graph.path(),
                ":root { --ls-primary-background-color: #123456; } /* changed */",
            );
            assert!(check_css(cx));
            assert!(!check_css(cx), "unchanged file is not re-read");
            set_preference(cx, None, ThemePreference::Light);
            assert_eq!(rgb(Theme::global(cx).colors.background), [0x12, 0x34, 0x56]);
            assert_eq!(bullet_color(), None);

            // Closing the graph drops the overrides.
            set_graph_css(cx, None);
            assert_ne!(rgb(Theme::global(cx).colors.background), [0x12, 0x34, 0x56]);
        });
        // The stylesheet is only ever read by the app.
        assert!(before.starts_with(b"\n:root"));
    }
}
