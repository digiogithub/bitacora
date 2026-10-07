//! Minimal app settings persisted as JSON in `<config_dir>/settings.json`.
//!
//! The settings view (`views/settings`, BIT-US-0107) edits these; `config.edn` keys of the open
//! graph are edited separately, comment-preserving, through the command queue.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// Which appearance the app uses.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemePreference {
    /// Follow the operating system appearance.
    #[default]
    System,
    /// Always light.
    Light,
    /// Always dark.
    Dark,
}

/// Current layout of `settings.json`. 1.x files carry no `settings_version` (read as 1);
/// [`crate::migrate`] upgrades them once and stamps this value.
pub const SETTINGS_VERSION: u32 = 2;

/// Persisted application settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    /// File format version (see [`SETTINGS_VERSION`]).
    pub settings_version: u32,
    /// Light / dark / system.
    pub mode: ThemePreference,
    /// Keep the process (and the MCP server) alive when the last window closes (BIT-T-0155).
    pub keep_running_in_background: bool,
    /// Update checks (BIT-US-0100).
    pub updates: crate::update::UpdateSettings,
    /// UI font size in pixels (`None`: the theme default).
    pub font_size: Option<u16>,
    /// Search and index options (BIT-US-0107).
    pub search: SearchSettings,
    /// MCP server options (BIT-US-0016, BIT-US-0107).
    pub mcp: McpSettings,
    /// UI language tag (`en`, `es`); `None` follows the operating system (BIT-T-0335).
    pub language: Option<String>,
    /// Reduce motion: no caret blinking, spinners and transitions are drawn static
    /// (BIT-T-0338). Off by default; GPUI cannot read the operating system preference.
    pub reduce_motion: bool,
    /// Reopen the most recent graph at startup instead of showing the picker (BIT-US-0165).
    /// On by default.
    pub reopen_last_graph: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            settings_version: SETTINGS_VERSION,
            mode: ThemePreference::default(),
            keep_running_in_background: false,
            updates: crate::update::UpdateSettings::default(),
            font_size: None,
            search: SearchSettings::default(),
            mcp: McpSettings::default(),
            language: None,
            reduce_motion: false,
            reopen_last_graph: true,
        }
    }
}

/// Smallest and largest UI font size the settings accept.
pub const FONT_SIZE_RANGE: (u16, u16) = (12, 24);

/// Search and index options.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SearchSettings {
    /// Keep the trigram block index for substring and CJK search (`search.substring`); turning
    /// it off roughly halves the index size.
    pub substring: bool,
}

impl Default for SearchSettings {
    fn default() -> Self {
        Self { substring: true }
    }
}

/// MCP server options. Toggles apply to the running server at once; the endpoint-shaping ones
/// (`api_enabled`, `allowed_origins`, `writes_per_minute`) need the server to restart.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct McpSettings {
    /// Run the MCP server while a graph is open (on by default).
    pub enabled: bool,
    /// TCP port on 127.0.0.1 (`0` picks a free one; used by tests).
    pub port: u16,
    /// Agents may create and edit (off by default, ADR-010).
    pub allow_writes: bool,
    /// Agents may remove blocks and delete or rename pages (off by default).
    pub allow_deletes: bool,
    /// Serve the Logseq-compatible `POST /api` endpoint (off by default).
    pub api_enabled: bool,
    /// Extra browser origins allowed (empty by default).
    pub allowed_origins: Vec<String>,
    /// Namespaces agents cannot write to.
    pub protected_namespaces: Vec<String>,
    /// Write calls allowed per token and minute.
    pub writes_per_minute: usize,
}

impl Default for McpSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            port: bitacora_mcp::DEFAULT_PORT,
            allow_writes: false,
            allow_deletes: false,
            api_enabled: false,
            allowed_origins: Vec::new(),
            protected_namespaces: Vec::new(),
            writes_per_minute: bitacora_mcp::DEFAULT_WRITES_PER_MINUTE,
        }
    }
}

impl McpSettings {
    /// The server configuration these settings describe (port stays the default; the port is not
    /// configurable yet).
    pub fn to_config(&self) -> bitacora_mcp::McpConfig {
        bitacora_mcp::McpConfig {
            port: self.port,
            allow_writes: self.allow_writes,
            allow_deletes: self.allow_deletes,
            api_enabled: self.api_enabled,
            allowed_origins: self.allowed_origins.clone(),
            protected_namespaces: self.protected_namespaces.clone(),
            writes_per_minute: self.writes_per_minute.max(1),
            ..bitacora_mcp::McpConfig::default()
        }
    }
}

impl AppSettings {
    /// Loads the settings; a missing file yields defaults silently, an unreadable
    /// or invalid file yields defaults with a warning (never an error).
    pub fn load(path: &Path) -> Self {
        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Self::default(),
            Err(err) => {
                tracing::warn!(path = %path.display(), "cannot read settings: {err}");
                return Self::default();
            }
        };
        match serde_json::from_slice(&bytes) {
            Ok(settings) => settings,
            Err(err) => {
                tracing::warn!(path = %path.display(), "invalid settings, using defaults: {err}");
                Self::default()
            }
        }
    }

    /// Writes the settings atomically (temp file in the same directory, then rename).
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let json = serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?;
        write_atomic(path, &json)
    }
}

/// Atomic write for app-owned files (not graph files, which go through core).
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write as _;
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(dir)?;
    let mut tmp = tempfile_in(dir, path)?;
    tmp.0.write_all(bytes)?;
    tmp.0.sync_all()?;
    std::fs::rename(&tmp.1, path)?;
    Ok(())
}

/// Creates `<name>.tmp-<pid>` next to `target`.
fn tempfile_in(dir: &Path, target: &Path) -> std::io::Result<(std::fs::File, std::path::PathBuf)> {
    let name = target
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "file".to_owned());
    let tmp_path = dir.join(format!("{name}.tmp-{}", std::process::id()));
    let file = std::fs::File::create(&tmp_path)?;
    Ok((file, tmp_path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_gives_defaults() {
        let tmp = tempfile::tempdir().expect("tempdir");
        assert_eq!(
            AppSettings::load(&tmp.path().join("settings.json")),
            AppSettings::default()
        );
    }

    #[test]
    fn roundtrip_and_partial_files() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("nested/settings.json");
        let settings = AppSettings {
            settings_version: SETTINGS_VERSION,
            mode: ThemePreference::Dark,
            keep_running_in_background: false,
            updates: crate::update::UpdateSettings::default(),
            font_size: Some(14),
            search: SearchSettings { substring: false },
            mcp: McpSettings {
                allow_writes: true,
                allowed_origins: vec!["http://localhost:3000".into()],
                ..McpSettings::default()
            },
            language: Some("es".into()),
            reduce_motion: true,
            reopen_last_graph: false,
        };
        settings.save(&path).expect("save");
        assert_eq!(AppSettings::load(&path), settings);
        std::fs::write(&path, br#"{"mode":"light"}"#).expect("write");
        assert_eq!(AppSettings::load(&path).mode, ThemePreference::Light);
        // A file written before the setting existed still reopens the last graph.
        assert!(AppSettings::load(&path).reopen_last_graph);
    }

    #[test]
    fn corrupt_file_gives_defaults() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("settings.json");
        std::fs::write(&path, b"not json").expect("write");
        assert_eq!(AppSettings::load(&path), AppSettings::default());
    }
}
