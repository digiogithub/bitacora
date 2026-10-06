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
pub mod crash;
pub mod data;
pub mod editor;
pub mod events;
pub mod graph_ops;
pub mod graph_state;
pub mod instance;
pub mod keymap;
pub mod layout;
pub mod logging;
pub mod nav;
pub mod paths;
pub mod recent;
pub mod render;
pub mod session;
pub mod settings;
pub mod spike;
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
}
