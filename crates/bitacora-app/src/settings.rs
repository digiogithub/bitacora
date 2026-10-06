//! Minimal app settings persisted as JSON in `<config_dir>/settings.json`.
//!
//! The full settings UI is out of scope here (BIT-EP-0013); this holds only the
//! theme choice.

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

/// Persisted application settings.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    /// Light / dark / system.
    pub mode: ThemePreference,
    /// Name of the bundled theme used in light mode (`None` = default).
    pub light_theme: Option<String>,
    /// Name of the bundled theme used in dark mode (`None` = default).
    pub dark_theme: Option<String>,
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
            mode: ThemePreference::Dark,
            light_theme: None,
            dark_theme: Some("Ayu Dark".into()),
        };
        settings.save(&path).expect("save");
        assert_eq!(AppSettings::load(&path), settings);
        std::fs::write(&path, br#"{"mode":"light"}"#).expect("write");
        assert_eq!(AppSettings::load(&path).mode, ThemePreference::Light);
    }

    #[test]
    fn corrupt_file_gives_defaults() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("settings.json");
        std::fs::write(&path, b"not json").expect("write");
        assert_eq!(AppSettings::load(&path), AppSettings::default());
    }
}
