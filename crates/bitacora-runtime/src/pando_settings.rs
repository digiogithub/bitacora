//! Machine-local Pando settings file shared by the desktop app and the headless CLI.

use std::path::{Path, PathBuf};

use bitacora_config::{PandoSettings, PandoSettingsError};
use bitacora_pando::PandoOptions;

/// File name inside the platform config directory.
pub const PANDO_SETTINGS_FILE: &str = "pando.json";

/// `<platform config dir>/pando.json`, or `None` without a home directory.
#[must_use]
pub fn default_pando_settings_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("es", "digio", "Bitacora")
        .map(|d| d.config_dir().join(PANDO_SETTINGS_FILE))
}

/// Loads the settings at `path` (a missing file gives the defaults, which are inactive).
///
/// # Errors
/// The file exists but cannot be read or parsed.
pub fn load_pando_settings(path: &Path) -> Result<PandoSettings, PandoSettingsError> {
    PandoSettings::load(path)
}

/// Pando options for `graph` from the settings at `path`; `None` when the integration is not
/// active (missing file, disabled or mode `off`), so callers leave `RuntimeConfig::pando` unset.
///
/// # Errors
/// The settings file is unreadable or invalid.
pub fn pando_options_from_file(
    path: &Path,
    graph: &Path,
) -> Result<Option<PandoOptions>, PandoSettingsError> {
    let settings = load_pando_settings(path)?;
    if !settings.is_active() {
        return Ok(None);
    }
    Ok(Some(PandoOptions::new(settings, graph)))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn missing_or_disabled_settings_give_no_options() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pando.json");
        assert!(
            pando_options_from_file(&path, dir.path())
                .unwrap()
                .is_none()
        );
        let s = PandoSettings {
            enabled: true,
            ..PandoSettings::default()
        };
        s.save(&path).unwrap();
        let o = pando_options_from_file(&path, dir.path()).unwrap().unwrap();
        assert!(o.settings.enabled);
        std::fs::write(&path, "{ nope").unwrap();
        assert!(pando_options_from_file(&path, dir.path()).is_err());
    }
}
