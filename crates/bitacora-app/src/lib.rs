//! `bitacora-app`: the GPUI Kit desktop application (views, keymaps, themes).
//!
//! The only crate that depends on `gpui-kit`; all GPUI access goes through [`ui`].

use bitacora_config as _;
use bitacora_core as _;
use bitacora_index as _;
use bitacora_mcp as _;
use bitacora_runtime as _;
use bitacora_sync as _;
use bitacora_watch as _;

// Loads `assets/locales/*.yml`; shared with GPUI Kit (same `rust-i18n` instance).
rust_i18n::i18n!("assets/locales", fallback = "en");

pub mod actions;
pub mod app;
pub mod cli;
pub mod contrast;
pub mod crash;
pub mod credentials;
pub mod custom_css;
pub mod data;
pub mod editing;
pub mod editor;
pub mod events;
pub mod graph_ops;
pub mod graph_state;
pub mod i18n;
pub mod instance;
pub mod keymap;
pub mod layout;
pub mod logging;
pub mod nav;
pub mod paths;
pub mod perf;
pub mod recent;
pub mod render;
pub mod session;
pub mod settings;
pub mod spike;
pub mod sync_prefs;
#[cfg(test)]
pub mod testing;
pub mod theme;
pub mod tokio_bridge;
pub mod ui;
pub mod update;
pub mod views;

#[cfg(test)]
mod i18n_tests {
    use std::path::{Path, PathBuf};

    fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                rust_files(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }

    fn sources() -> Vec<(PathBuf, String)> {
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        rust_files(&src, &mut files);
        files
            .into_iter()
            .map(|f| {
                let text = std::fs::read_to_string(&f).unwrap_or_default();
                (f, text)
            })
            .collect()
    }

    #[test]
    fn english_locale_resolves() {
        assert_eq!(rust_i18n::t!("app.name", locale = "en"), "Bitacora");
    }

    #[test]
    fn every_translation_key_used_in_code_exists() {
        let mut checked = 0;
        for (file, text) in sources() {
            for (at, _) in text.match_indices("t!(\"") {
                // Skip `format!(` and friends: the macro name must start at `t`.
                let before = text[..at].chars().next_back();
                if before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                    continue;
                }
                let Some(key) = text[at + 4..].split('"').next() else {
                    continue;
                };
                if !key.contains('.') {
                    continue;
                }
                let value = rust_i18n::t!(key, locale = "en");
                assert_ne!(
                    value,
                    key,
                    "missing translation `{key}` used in {}",
                    file.display()
                );
                checked += 1;
            }
        }
        assert!(
            checked > 10,
            "expected to find the t!() call sites, found {checked}"
        );
    }

    #[test]
    fn views_have_no_hardcoded_visible_strings() {
        let views = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/views");
        for (file, text) in sources().into_iter().filter(|(f, _)| f.starts_with(&views)) {
            for pattern in [".child(\"", ".label(\"", ".left(\"", ".right(\""] {
                assert!(
                    !text.contains(pattern),
                    "hard-coded string `{pattern}` in {}; use t!()",
                    file.display()
                );
            }
        }
    }

    /// Flattens one of our simple locale files (nested maps, one scalar per line) into
    /// `dotted.key -> value`.
    fn flatten(text: &str) -> std::collections::BTreeMap<String, String> {
        let mut out = std::collections::BTreeMap::new();
        let mut path: Vec<String> = Vec::new();
        for line in text.lines() {
            if line.trim().is_empty() || line.trim_start().starts_with('#') {
                continue;
            }
            let indent = (line.len() - line.trim_start().len()) / 2;
            let Some((key, value)) = line.trim_start().split_once(':') else {
                continue;
            };
            path.truncate(indent);
            path.push(key.trim().to_owned());
            let value = value.trim();
            if !value.is_empty() {
                out.insert(path.join("."), value.to_owned());
            }
        }
        out
    }

    fn placeholders(value: &str) -> Vec<String> {
        let mut found: Vec<String> = value
            .match_indices("%{")
            .filter_map(|(at, _)| value[at + 2..].split('}').next().map(str::to_owned))
            .collect();
        found.sort();
        found
    }

    /// Every locale file of the first language has a counterpart for each other language with
    /// exactly the same keys and placeholders (BIT-T-0335).
    #[test]
    fn every_key_exists_in_all_locales_with_the_same_placeholders() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/locales");
        let languages: Vec<&str> = crate::i18n::LANGUAGES.iter().map(|(t, _)| *t).collect();
        let load = |lang: &str| {
            let mut all = std::collections::BTreeMap::new();
            for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                let own = name == format!("{lang}.yml") || name.ends_with(&format!(".{lang}.yml"));
                if own {
                    let text = std::fs::read_to_string(entry.path()).unwrap_or_default();
                    all.extend(flatten(&text));
                }
            }
            all
        };
        let base = load("en");
        assert!(
            base.len() > 300,
            "english locale not found ({})",
            base.len()
        );
        for lang in languages.iter().filter(|l| **l != "en") {
            let other = load(lang);
            let missing: Vec<_> = base.keys().filter(|k| !other.contains_key(*k)).collect();
            let extra: Vec<_> = other.keys().filter(|k| !base.contains_key(*k)).collect();
            assert!(missing.is_empty(), "{lang}: missing keys {missing:?}");
            assert!(extra.is_empty(), "{lang}: keys not in en {extra:?}");
            for (key, value) in &base {
                assert_eq!(
                    placeholders(value),
                    placeholders(&other[key]),
                    "{lang}: placeholders of `{key}` differ"
                );
            }
        }
    }

    #[test]
    fn spanish_translations_resolve_through_rust_i18n() {
        assert_eq!(
            rust_i18n::t!("settings.section.appearance", locale = "es"),
            "Apariencia"
        );
        assert_eq!(
            rust_i18n::t!("editor.refusal.read_only", locale = "es"),
            "Esta página es de solo lectura."
        );
    }
}
