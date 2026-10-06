//! The data side of the keymap editor (BIT-T-0332): which action has which keystrokes, what the
//! user changed, conflicts within a key context, and the `keymap.json` that records the changes.
//!
//! Pure logic, no GPUI: the view in `keymap.rs` drives it and applies the result live with
//! [`crate::keymap::apply_live`].

use std::collections::BTreeMap;

use crate::keymap::{self, BindingKey, Effective, Section, normalize_keys};

/// One line of the editor: an action in a key context and the keystrokes it answers to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeymapRow {
    /// Key context the binding lives in (`None`: global).
    pub context: Option<String>,
    /// Fully qualified action name (`bitacora::ToggleTheme`).
    pub action: String,
    /// Effective keystrokes (platform-normalised, `ctrl-` / `cmd-`).
    pub keys: Vec<String>,
    /// Keystrokes of the default keymap.
    pub default_keys: Vec<String>,
}

impl KeymapRow {
    /// Whether the user changed this action's bindings.
    pub fn customised(&self) -> bool {
        self.keys != self.default_keys
    }

    /// Short name shown first (`ToggleTheme`).
    pub fn short_name(&self) -> &str {
        self.action.rsplit("::").next().unwrap_or(&self.action)
    }
}

fn section_for<'a>(
    map: &'a mut BTreeMap<Option<String>, Section>,
    ctx: &Option<String>,
) -> &'a mut Section {
    map.entry(ctx.clone()).or_insert_with(|| Section {
        context: ctx.clone(),
        bindings: BTreeMap::new(),
        unbind: Vec::new(),
    })
}

/// The editable keymap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeymapModel {
    defaults: Effective,
    current: Effective,
    /// Every `(context, action)` that may get a binding, bound or not.
    universe: Vec<(Option<String>, String)>,
}

/// Key context offered for an action that has no binding yet.
fn default_context(action: &str) -> Option<String> {
    if action.starts_with("outliner::") {
        Some("Outliner".to_owned())
    } else {
        Some("Workspace".to_owned())
    }
}

impl KeymapModel {
    /// Builds the model from the default and user keymap JSON. `registered` lists action names
    /// that exist but may have no default binding.
    pub fn new(default_json: &str, user_json: Option<&str>, registered: &[String]) -> Self {
        let defaults_sections = keymap::parse(default_json).unwrap_or_default();
        let user_sections = user_json
            .and_then(|j| keymap::parse(j).ok())
            .unwrap_or_default();
        let defaults = keymap::effective(&[&defaults_sections]);
        let current = keymap::effective(&[&defaults_sections, &user_sections]);
        let mut universe: Vec<(Option<String>, String)> = defaults
            .iter()
            .chain(current.iter())
            .map(|((ctx, _), action)| (ctx.clone(), action.clone()))
            .collect();
        for action in registered {
            if !universe.iter().any(|(_, a)| a == action) {
                universe.push((default_context(action), action.clone()));
            }
        }
        universe.sort();
        universe.dedup();
        Self {
            defaults,
            current,
            universe,
        }
    }

    /// The effective bindings (what `apply_live` installs).
    pub fn effective(&self) -> &Effective {
        &self.current
    }

    fn keys_of(map: &Effective, context: &Option<String>, action: &str) -> Vec<String> {
        map.iter()
            .filter(|((ctx, _), a)| ctx == context && a.as_str() == action)
            .map(|((_, keys), _)| keys.clone())
            .collect()
    }

    /// Rows whose action or keystrokes contain `filter` (case-insensitive; empty: all).
    pub fn rows(&self, filter: &str) -> Vec<KeymapRow> {
        let needle = filter.trim().to_lowercase();
        let mut rows: Vec<KeymapRow> = self
            .universe
            .iter()
            .map(|(context, action)| KeymapRow {
                context: context.clone(),
                action: action.clone(),
                keys: Self::keys_of(&self.current, context, action),
                default_keys: Self::keys_of(&self.defaults, context, action),
            })
            .filter(|row| {
                needle.is_empty()
                    || row.action.to_lowercase().contains(&needle)
                    || row.keys.iter().any(|k| k.to_lowercase().contains(&needle))
                    || row
                        .context
                        .as_deref()
                        .is_some_and(|c| c.to_lowercase().contains(&needle))
            })
            .collect();
        rows.sort_by(|a, b| {
            (a.context.as_deref(), a.action.as_str())
                .cmp(&(b.context.as_deref(), b.action.as_str()))
        });
        rows
    }

    /// Actions in the same context that already use `keys` (other than `action`). A conflict must
    /// be confirmed before [`rebind`](Self::rebind) takes the keystroke away from them.
    pub fn conflicts(&self, context: &Option<String>, action: &str, keys: &str) -> Vec<String> {
        let key: BindingKey = (context.clone(), normalize_keys(keys));
        match self.current.get(&key) {
            Some(other) if other != action => vec![other.clone()],
            _ => Vec::new(),
        }
    }

    /// Gives `action` exactly the keystroke `keys` in `context` (its other keystrokes go away); a
    /// conflicting action loses `keys`.
    pub fn rebind(&mut self, context: &Option<String>, action: &str, keys: &str) {
        self.current
            .retain(|(ctx, _), a| !(ctx == context && a == action));
        self.current
            .insert((context.clone(), normalize_keys(keys)), action.to_owned());
    }

    /// Removes every keystroke of `action` in `context`.
    pub fn unbind(&mut self, context: &Option<String>, action: &str) {
        self.current
            .retain(|(ctx, _), a| !(ctx == context && a == action));
    }

    /// Restores the default keystrokes of `action` in `context`.
    pub fn reset(&mut self, context: &Option<String>, action: &str) {
        self.unbind(context, action);
        for (key, a) in &self.defaults {
            if &key.0 == context && a == action {
                self.current.insert(key.clone(), a.clone());
            }
        }
    }

    /// Restores every default.
    pub fn reset_all(&mut self) {
        self.current = self.defaults.clone();
    }

    /// Number of customised actions.
    pub fn customised_count(&self) -> usize {
        self.rows("").iter().filter(|r| r.customised()).count()
    }

    /// The user keymap: bindings that differ from the defaults, plus `unbind` entries for default
    /// keystrokes that no longer do what they did.
    pub fn user_sections(&self) -> Vec<Section> {
        let mut by_context: BTreeMap<Option<String>, Section> = BTreeMap::new();
        for (key, action) in &self.current {
            if self.defaults.get(key) != Some(action) {
                section_for(&mut by_context, &key.0)
                    .bindings
                    .insert(key.1.clone(), action.clone());
            }
        }
        for key in self.defaults.keys() {
            if !self.current.contains_key(key) {
                section_for(&mut by_context, &key.0)
                    .unbind
                    .push(key.1.clone());
            }
        }
        by_context.into_values().collect()
    }

    /// `keymap.json` text of [`user_sections`](Self::user_sections); `None` when nothing differs
    /// from the defaults (the file can be removed).
    pub fn user_json(&self) -> Option<String> {
        let sections = self.user_sections();
        if sections.is_empty() {
            return None;
        }
        let array: Vec<serde_json::Value> = sections
            .iter()
            .map(|s| {
                let mut obj = serde_json::Map::new();
                if let Some(ctx) = &s.context {
                    obj.insert("context".into(), ctx.clone().into());
                }
                obj.insert(
                    "bindings".into(),
                    serde_json::to_value(&s.bindings).unwrap_or_default(),
                );
                if !s.unbind.is_empty() {
                    obj.insert(
                        "unbind".into(),
                        serde_json::to_value(&s.unbind).unwrap_or_default(),
                    );
                }
                serde_json::Value::Object(obj)
            })
            .collect();
        serde_json::to_string_pretty(&array).ok()
    }
}

/// Writes (or removes, when `json` is `None`) the user keymap file.
///
/// # Errors
/// I/O errors.
pub fn save_user_keymap(path: &std::path::Path, json: Option<&str>) -> std::io::Result<()> {
    match json {
        Some(json) => {
            let mut text = json.to_owned();
            text.push('\n');
            crate::settings::write_atomic(path, text.as_bytes())
        }
        None => match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEFAULTS: &str = r#"[
      {"context": "Workspace", "bindings": {
        "secondary-b": "bitacora::ToggleLeftSidebar",
        "secondary-k": "bitacora::OpenSearch"}},
      {"context": "Outliner", "bindings": {"secondary-z": "outliner::Undo"}}
    ]"#;

    fn model(user: Option<&str>) -> KeymapModel {
        KeymapModel::new(
            DEFAULTS,
            user,
            &["bitacora::GoBack".to_owned(), "outliner::Undo".to_owned()],
        )
    }

    fn ws() -> Option<String> {
        Some("Workspace".to_owned())
    }

    #[test]
    fn rows_list_bound_and_unbound_actions() {
        let m = model(None);
        let rows = m.rows("");
        assert_eq!(rows.len(), 4, "{rows:?}");
        let back = rows
            .iter()
            .find(|r| r.action == "bitacora::GoBack")
            .expect("row");
        assert!(back.keys.is_empty() && back.default_keys.is_empty());
        assert_eq!(
            back.context,
            ws(),
            "unbound actions default to their namespace's context"
        );
        let toggle = rows
            .iter()
            .find(|r| r.action == "bitacora::ToggleLeftSidebar")
            .expect("row");
        assert_eq!(toggle.keys, vec![normalize_keys("secondary-b")]);
        assert!(!toggle.customised());
        assert_eq!(m.rows("search").len(), 1);
        assert_eq!(m.rows("outliner").len(), 1);
        assert_eq!(m.rows("ctrl-b").len() + m.rows("cmd-b").len(), 1);
    }

    #[test]
    fn rebinding_replaces_the_old_keystroke_and_records_an_unbind() {
        let mut m = model(None);
        assert!(
            m.conflicts(&ws(), "bitacora::ToggleLeftSidebar", "secondary-j")
                .is_empty()
        );
        m.rebind(&ws(), "bitacora::ToggleLeftSidebar", "secondary-j");
        let rows = m.rows("ToggleLeftSidebar");
        assert_eq!(rows[0].keys, vec![normalize_keys("secondary-j")]);
        assert!(rows[0].customised());
        let sections = m.user_sections();
        assert_eq!(sections.len(), 1);
        assert_eq!(
            sections[0]
                .bindings
                .get(&normalize_keys("secondary-j"))
                .map(String::as_str),
            Some("bitacora::ToggleLeftSidebar")
        );
        assert_eq!(sections[0].unbind, vec![normalize_keys("secondary-b")]);
        // The saved file reproduces the same model.
        let json = m.user_json().expect("json");
        let again = model(Some(&json));
        assert_eq!(again.effective(), m.effective());
        assert_eq!(again.customised_count(), 1);
    }

    #[test]
    fn conflicts_are_found_within_a_context_and_resolved_by_taking_the_key() {
        let mut m = model(None);
        let found = m.conflicts(&ws(), "bitacora::GoBack", "secondary-k");
        assert_eq!(found, vec!["bitacora::OpenSearch".to_owned()]);
        // Same keystroke in another context is not a conflict.
        assert!(
            m.conflicts(
                &Some("Outliner".to_owned()),
                "outliner::Undo",
                "secondary-k"
            )
            .is_empty()
        );
        // Rebinding to the action's own key is not a conflict.
        assert!(m.conflicts(&ws(), "bitacora::OpenSearch", "ctrl-k").len() <= 1);
        m.rebind(&ws(), "bitacora::GoBack", "secondary-k");
        let search = &m.rows("OpenSearch")[0];
        assert!(
            search.keys.is_empty(),
            "the conflicting action lost the key: {search:?}"
        );
        // A user binding for the keystroke already overrides the default: no unbind needed.
        let json = m.user_json().expect("json");
        assert!(
            json.contains("bitacora::GoBack") && !json.contains("OpenSearch"),
            "{json}"
        );
    }

    #[test]
    fn reset_and_unbind() {
        let mut m = model(None);
        m.rebind(&ws(), "bitacora::OpenSearch", "secondary-shift-k");
        m.unbind(&ws(), "bitacora::ToggleLeftSidebar");
        assert_eq!(m.customised_count(), 2);
        m.reset(&ws(), "bitacora::OpenSearch");
        assert_eq!(m.customised_count(), 1);
        m.reset_all();
        assert_eq!(m.customised_count(), 0);
        assert!(m.user_json().is_none());
    }

    #[test]
    fn normalisation_makes_secondary_and_platform_keys_equal() {
        let primary = if cfg!(target_os = "macos") {
            "cmd"
        } else {
            "ctrl"
        };
        assert_eq!(
            normalize_keys("secondary-shift-k"),
            normalize_keys(&format!("shift-{primary}-k"))
        );
        assert_eq!(normalize_keys("g j"), "g j");
        assert_eq!(normalize_keys("secondary-."), format!("{primary}-."));
    }

    #[test]
    fn save_writes_and_removes_the_file() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("keymap.json");
        let mut m = model(None);
        m.rebind(&ws(), "bitacora::GoBack", "alt-left");
        save_user_keymap(&path, m.user_json().as_deref()).expect("save");
        let text = std::fs::read_to_string(&path).expect("read");
        assert!(text.contains("bitacora::GoBack"));
        save_user_keymap(&path, None).expect("remove");
        assert!(!path.exists());
        save_user_keymap(&path, None).expect("removing nothing is fine");
    }
}
