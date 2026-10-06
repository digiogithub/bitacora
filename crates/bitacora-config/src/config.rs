//! Effective graph config: built-in defaults, optional global config and the graph config.
//!
//! Behaviour follows `docs/analysis/logseq/01-file-graph-layout.md` section 2: later sources
//! win, maps are shallow-merged, and a config without `:file/name-format` means legacy naming.

use std::path::{Path, PathBuf};

use crate::edn::{Edn, read_str};
use crate::error::{Diagnostic, DiagnosticKind};

/// Path of the graph config relative to the graph root.
pub const GRAPH_CONFIG_REL_PATH: &str = "logseq/config.edn";

/// Location of the optional global config (`~/.logseq/config/config.edn`).
pub fn global_config_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    Some(
        PathBuf::from(home)
            .join(".logseq")
            .join("config")
            .join("config.edn"),
    )
}

/// Built-in defaults applied under every other source.
pub fn default_config() -> Edn {
    Edn::Map(vec![
        (
            Edn::kw("feature/enable-search-remove-accents?"),
            Edn::Bool(true),
        ),
        (Edn::kw("ui/auto-expand-block-refs?"), Edn::Bool(true)),
        (Edn::kw("file/name-format"), Edn::kw("legacy")),
    ])
}

/// Merges `over` on top of `base`: later values win, but when both sides hold a map the two maps
/// are merged one level deep (entries of `over` win).
pub fn merge_configs(base: Edn, over: Edn) -> Edn {
    let (Edn::Map(mut entries), Edn::Map(over)) = (base, over) else {
        return Edn::Map(Vec::new());
    };
    for (k, v) in over {
        match entries.iter_mut().find(|(ek, _)| *ek == k) {
            Some((_, existing)) => {
                *existing = match (std::mem::replace(existing, Edn::Nil), v) {
                    (Edn::Map(mut a), Edn::Map(b)) => {
                        for (bk, bv) in b {
                            match a.iter_mut().find(|(ak, _)| *ak == bk) {
                                Some((_, av)) => *av = bv,
                                None => a.push((bk, bv)),
                            }
                        }
                        Edn::Map(a)
                    }
                    (_, v) => v,
                };
            }
            None => entries.push((k, v)),
        }
    }
    Edn::Map(entries)
}

/// The merged config plus the problems found while loading its sources.
#[derive(Debug, Clone, PartialEq)]
pub struct EffectiveConfig {
    pub(crate) value: Edn,
    diagnostics: Vec<Diagnostic>,
}

impl Default for EffectiveConfig {
    fn default() -> Self {
        EffectiveConfig::from_texts(None, None)
    }
}

impl EffectiveConfig {
    /// Computes the effective config from the texts of the global and graph configs.
    ///
    /// A source that is invalid EDN (including duplicate keys) or whose root is not a map is
    /// skipped and reported in [`EffectiveConfig::diagnostics`]; the others still apply.
    pub fn from_texts(global: Option<&str>, graph: Option<&str>) -> Self {
        let mut diagnostics = Vec::new();
        let mut value = default_config();
        for text in [global, graph].into_iter().flatten() {
            match parse_source(text) {
                Ok(m) => value = merge_configs(value, m),
                Err(d) => diagnostics.push(d),
            }
        }
        EffectiveConfig { value, diagnostics }
    }

    /// Loads `<graph_root>/logseq/config.edn` over the optional global config file.
    ///
    /// Missing files are fine; unreadable or invalid ones become diagnostics tagged with their
    /// path.
    pub fn load(graph_root: &Path, global_path: Option<&Path>) -> Self {
        let graph_path = graph_root.join(GRAPH_CONFIG_REL_PATH);
        let mut diagnostics = Vec::new();
        let mut value = default_config();
        let sources = global_path.into_iter().chain(std::iter::once(&*graph_path));
        for path in sources {
            let text = match std::fs::read(path) {
                Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => {
                    diagnostics.push(
                        Diagnostic::at("", 0, DiagnosticKind::Io, e.to_string())
                            .with_path(path.to_owned()),
                    );
                    continue;
                }
            };
            match parse_source(&text) {
                Ok(m) => value = merge_configs(value, m),
                Err(d) => diagnostics.push(d.with_path(path.to_owned())),
            }
        }
        EffectiveConfig { value, diagnostics }
    }

    /// Problems found while loading (empty when every source was valid).
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// The merged config as an EDN map.
    pub fn value(&self) -> &Edn {
        &self.value
    }

    /// Looks a top-level key up (name without the leading `:`).
    pub fn get(&self, key: &str) -> Option<&Edn> {
        self.value.get(key)
    }
}

fn parse_source(text: &str) -> Result<Edn, Diagnostic> {
    // A UTF-8 BOM is not EDN whitespace.
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    match read_str(text)? {
        None => Ok(Edn::Map(Vec::new())),
        Some(m @ Edn::Map(_)) => Ok(m),
        Some(_) => Err(Diagnostic::at(
            text,
            0,
            DiagnosticKind::NotAMap,
            "config root must be a map",
        )),
    }
}

/// Text of a minimal `config.edn` Bitacora writes for a new graph (authored for Bitacora, not
/// Logseq's template; ADR-015). It selects the triple-lowbar file name format.
pub const DEFAULT_CONFIG_EDN: &str = include_str!("default_config.edn");
