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
    pub bindings: BTreeMap<String, String>,
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
    Ok(report)
}

/// Loads the default keymap, then an optional user keymap on top of it.
pub fn load_with_user(cx: &mut App, user_json: Option<&str>) -> Result<usize, KeymapError> {
    load_with_user_report(cx, user_json).map(|r| r.bound)
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn malformed_json_is_an_error() {
        assert!(parse("{").is_err());
    }
}
