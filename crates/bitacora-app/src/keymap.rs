//! Keymap loading from JSON.
//!
//! `assets/keymaps/default.json` is embedded in the binary. Keystrokes use GPUI
//! syntax; `secondary-` means `cmd-` on macOS and `ctrl-` everywhere else. A user
//! keymap can be layered on top with [`load_with_user`] (later bindings win).

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::ui::{self, App};

/// The embedded default keymap.
pub const DEFAULT_KEYMAP: &str = include_str!("../assets/keymaps/default.json");

/// One block of bindings sharing a key context.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Section {
    /// Key context predicate (for example `Workspace`); `None` binds globally.
    #[serde(default)]
    pub context: Option<String>,
    /// Keystrokes to fully qualified action names (`bitacora::ToggleTheme`).
    #[serde(default)]
    pub bindings: BTreeMap<String, String>,
    /// Keystrokes whose default binding in this context is switched off (user keymap only).
    #[serde(default)]
    pub unbind: Vec<String>,
}

/// Errors loading a keymap.
#[derive(Debug, thiserror::Error)]
pub enum KeymapError {
    /// The JSON is malformed.
    #[error("invalid keymap JSON: {0}")]
    Json(#[from] serde_json::Error),
}

/// Parses keymap JSON.
pub fn parse(json: &str) -> Result<Vec<Section>, KeymapError> {
    Ok(serde_json::from_str(json)?)
}

/// What loading a keymap did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LoadReport {
    /// Bindings installed.
    pub bound: usize,
    /// Entries that were reported and ignored (unknown action, invalid keystroke or context).
    pub problems: Vec<String>,
}

/// Binds every entry of `json`; entries that fail (unknown action, bad keystroke) are logged,
/// listed in the report and skipped.
pub fn load_checked(cx: &mut App, json: &str) -> Result<LoadReport, KeymapError> {
    let sections = parse(json)?;
    let mut bindings = Vec::new();
    let mut problems = Vec::new();
    for section in &sections {
        for (keys, action_name) in &section.bindings {
            let action = match cx.build_action(action_name, None) {
                Ok(action) => action,
                Err(err) => {
                    tracing::warn!(action = %action_name, "keymap: unknown action: {err}");
                    problems.push(format!("`{keys}`: unknown action {action_name}"));
                    continue;
                }
            };
            match ui::key_binding(keys, action, section.context.as_deref()) {
                Ok(binding) => bindings.push(binding),
                Err(err) => {
                    tracing::warn!(keys = %keys, "keymap: invalid binding: {err}");
                    problems.push(format!("`{keys}`: {err}"));
                }
            }
        }
        for keys in &section.unbind {
            match ui::key_binding(keys, Box::new(ui::NoAction), section.context.as_deref()) {
                Ok(binding) => bindings.push(binding),
                Err(err) => problems.push(format!("`{keys}`: {err}")),
            }
        }
    }
    let bound = bindings.len();
    cx.bind_keys(bindings);
    Ok(LoadReport { bound, problems })
}

/// Binds every entry of `json`; returns the number of bindings installed.
pub fn load(cx: &mut App, json: &str) -> Result<usize, KeymapError> {
    load_checked(cx, json).map(|r| r.bound)
}

/// Loads the default keymap, then an optional user keymap on top of it (later bindings win).
/// Problems of the user keymap are reported in the result and ignored.
pub fn load_with_user_report(
    cx: &mut App,
    user_json: Option<&str>,
) -> Result<LoadReport, KeymapError> {
    let mut report = load_checked(cx, DEFAULT_KEYMAP)?;
    crate::editor::bind_platform_keys(cx);
    if let Some(user) = user_json {
        match load_checked(cx, user) {
            Ok(r) => {
                report.bound += r.bound;
                report.problems.extend(r.problems);
            }
            Err(err) => {
                tracing::warn!("ignoring user keymap: {err}");
                report
                    .problems
                    .push(format!("keymap.json is not valid: {err}"));
            }
        }
    }
    remember_installed(cx, user_json);
    Ok(report)
}

/// Loads the default keymap, then an optional user keymap on top of it.
pub fn load_with_user(cx: &mut App, user_json: Option<&str>) -> Result<usize, KeymapError> {
    load_with_user_report(cx, user_json).map(|r| r.bound)
}

/// Where a binding applies: the key context (`None` = global) and the keystrokes.
pub type BindingKey = (Option<String>, String);

/// What every binding currently does: `(context, keystrokes)` to the action name.
pub type Effective = BTreeMap<BindingKey, String>;

/// Keystrokes with the platform meaning of `secondary-` spelled out, so `secondary-k` and
/// `ctrl-k` compare equal on Linux and Windows (and `cmd-k` on macOS).
pub fn normalize_keys(keys: &str) -> String {
    let primary = if cfg!(target_os = "macos") {
        "cmd-"
    } else {
        "ctrl-"
    };
    keys.split(' ')
        .map(|stroke| {
            let mut mods: Vec<&str> = Vec::new();
            let mut key = stroke;
            // Modifiers are everything before the last `-` that is not the key itself (`-`).
            while let Some((head, rest)) = key.split_once('-') {
                if rest.is_empty() {
                    break;
                }
                mods.push(if head == "secondary" {
                    primary.trim_end_matches('-')
                } else {
                    head
                });
                key = rest;
            }
            mods.sort_unstable();
            mods.dedup();
            let mut out = mods.join("-");
            if !out.is_empty() {
                out.push('-');
            }
            out.push_str(key);
            out
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The effective bindings of `sections` layered in order (later sections win, `unbind` removes).
pub fn effective(layers: &[&[Section]]) -> Effective {
    let mut out = Effective::new();
    for layer in layers {
        for section in *layer {
            for keys in &section.unbind {
                out.remove(&(section.context.clone(), normalize_keys(keys)));
            }
            for (keys, action) in &section.bindings {
                out.insert(
                    (section.context.clone(), normalize_keys(keys)),
                    action.clone(),
                );
            }
        }
    }
    out
}

/// Bindings that differ between `old` and `new`: `Some(action)` binds it, `None` switches the
/// keystroke off.
pub fn diff(old: &Effective, new: &Effective) -> Vec<(BindingKey, Option<String>)> {
    let mut changes = Vec::new();
    for (key, action) in new {
        if old.get(key) != Some(action) {
            changes.push((key.clone(), Some(action.clone())));
        }
    }
    for key in old.keys() {
        if !new.contains_key(key) {
            changes.push((key.clone(), None));
        }
    }
    changes
}

/// The effective bindings the app has installed, kept to re-bind live changes as a diff.
#[derive(Debug, Default)]
pub struct InstalledKeymap(pub Effective);

impl ui::Global for InstalledKeymap {}

/// Records what `load_with_user_report` installed so the settings can diff against it.
pub fn remember_installed(cx: &mut App, user_json: Option<&str>) {
    let defaults = parse(DEFAULT_KEYMAP).unwrap_or_default();
    let user = user_json.and_then(|j| parse(j).ok()).unwrap_or_default();
    cx.set_global(InstalledKeymap(effective(&[&defaults, &user])));
}

/// Makes the bindings of `new` active without a restart: only changed keystrokes are re-bound
/// (a later binding wins; a removed one is bound to `NoAction`). Returns problems (unknown
/// actions, invalid keystrokes).
pub fn apply_live(cx: &mut App, new: Effective) -> Vec<String> {
    let old = cx
        .try_global::<InstalledKeymap>()
        .map(|g| g.0.clone())
        .unwrap_or_default();
    let mut problems = Vec::new();
    let mut bindings = Vec::new();
    for ((context, keys), action_name) in diff(&old, &new) {
        let action: Box<dyn ui::Action> = match &action_name {
            Some(name) => match cx.build_action(name, None) {
                Ok(action) => action,
                Err(err) => {
                    problems.push(format!("`{keys}`: unknown action {name} ({err})"));
                    continue;
                }
            },
            None => Box::new(ui::NoAction),
        };
        match ui::key_binding(&keys, action, context.as_deref()) {
            Ok(binding) => bindings.push(binding),
            Err(err) => problems.push(format!("`{keys}`: {err}")),
        }
    }
    cx.bind_keys(bindings);
    cx.set_global(InstalledKeymap(new));
    problems
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::testing::{TestAppContext, gpui_test};

    #[test]
    fn default_keymap_parses_and_names_every_baseline_action() {
        let sections = parse(DEFAULT_KEYMAP).expect("default keymap parses");
        let actions: Vec<&str> = sections
            .iter()
            .flat_map(|s| s.bindings.values().map(String::as_str))
            .collect();
        for expected in [
            "bitacora::ToggleLeftSidebar",
            "bitacora::ToggleRightSidebar",
            "bitacora::ToggleTheme",
            "bitacora::GoBack",
            "bitacora::GoForward",
            "bitacora::Quit",
        ] {
            assert!(actions.contains(&expected), "{expected} unbound");
        }
    }

    /// Actions that need no default binding, with the keyboard route they have instead.
    const NO_DEFAULT_BINDING: &[(&str, &str)] = &[
        // The palette lists the commands; these two open the palettes themselves (bound).
        (
            "bitacora::ClosePalette",
            "bound to Escape in the Palette context",
        ),
        // Reached from the application menu / the Actions palette, not a key of their own.
        (
            "outliner::ShowCharacterPalette",
            "platform character palette via the menu",
        ),
        (
            "outliner::InsertNewline",
            "Shift+Enter is bound in the BlockEditor context",
        ),
        (
            "outliner::CopyEmbed",
            "Actions palette and the block context",
        ),
        (
            "outliner::DismissCompletion",
            "Escape inside the autocomplete popup",
        ),
    ];

    #[gpui_test]
    fn every_action_is_reachable_from_the_keyboard(cx: &mut TestAppContext) {
        // BIT-T-0338: no action may be mouse-only. Every action either has a default binding
        // or an entry above naming the keyboard route it has instead.
        let sections = parse(DEFAULT_KEYMAP).expect("default keymap parses");
        let bound: std::collections::BTreeSet<&str> = sections
            .iter()
            .flat_map(|s| s.bindings.values().map(String::as_str))
            .collect();
        // Platform-specific word motion bindings are added in code (editor::actions).
        let platform: std::collections::BTreeSet<String> =
            crate::editor::actions::platform_bindings()
                .into_iter()
                .map(|(_, action, _)| action.to_owned())
                .collect();
        let all: Vec<String> = cx.update(|cx| {
            cx.all_action_names()
                .iter()
                .filter(|n| n.starts_with("bitacora::") || n.starts_with("outliner::"))
                .map(|n| (*n).to_owned())
                .collect()
        });
        assert!(all.len() > 50, "the action registry looks empty: {all:?}");
        let mut missing = Vec::new();
        for name in &all {
            let exempt = NO_DEFAULT_BINDING.iter().any(|(n, _)| n == name);
            if !bound.contains(name.as_str()) && !platform.contains(name) && !exempt {
                missing.push(name.clone());
            }
        }
        assert!(
            missing.is_empty(),
            "actions with neither a default binding nor a documented keyboard route: {missing:?}"
        );
    }

    #[test]
    fn malformed_json_is_an_error() {
        assert!(parse("{").is_err());
    }
}
