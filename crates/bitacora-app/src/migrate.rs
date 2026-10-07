//! One-shot migration of the machine-local files written by Bitacora 1.x (BIT-US-0162).
//!
//! Inventory of what 1.x wrote outside the graph and what 2.0 does with it:
//!
//! | File | Where | 2.0 |
//! |---|---|---|
//! | `settings.json` | config dir | versioned (`settings_version`); `light_theme` / `dark_theme` are dropped (only the Bitacora Light/Dark themes exist), everything else is kept; old file backed up |
//! | `workspace.json` | data dir | dock layout `version` 1 is incompatible (right dock width): backed up and removed so the default layout is built |
//! | `themes/*.json` | config dir | no longer loaded; left in place, the user is told (colour-scheme files return after 2.0, BIT-US-0164) |
//! | `recent-graphs.json` | config dir | same format, untouched |
//! | `keymap.json` | config dir | same format, untouched (bad entries are reported at startup) |
//! | `mcp-tokens.json` | config dir | owned by `bitacora-mcp` (v1 and v2 files both load), untouched |
//! | `logseq/custom.css` | the graph | user data: never touched, the 2.0 subset keeps being applied |
//!
//! The graph folder is never read or written here. Every file that is rewritten or removed is
//! copied to `<name>.1x.bak` first (an existing backup is never overwritten, so a second run
//! cannot lose the original). The migration is idempotent: `settings_version` marks it done.

use std::path::{Path, PathBuf};

use rust_i18n::t;
use serde_json::Value;

use crate::layout::LAYOUT_VERSION;
use crate::paths::AppDirs;
use crate::settings::{SETTINGS_VERSION, write_atomic};

/// Theme names 1.x used when no theme was chosen; selecting them is not worth a notice.
const DEFAULT_1X_THEMES: [&str; 2] = ["Paper", "Midnight"];

/// Something the user should hear about after the upgrade.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Notice {
    /// A non-default 1.x theme selection was replaced by the Bitacora theme.
    ThemeReplaced,
    /// User theme files exist that 2.0 no longer loads.
    CustomThemes {
        /// Number of `*.json` files in the themes folder.
        count: usize,
    },
    /// The saved dock layout belonged to 1.x and was reset.
    LayoutReset,
    /// `settings.json` could not be parsed; defaults are used and the file was kept as a backup.
    SettingsUnreadable,
}

/// What a migration run did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    /// Notices for the user, in order.
    pub notices: Vec<Notice>,
    /// Backups created by this run.
    pub backups: Vec<PathBuf>,
}

impl Report {
    /// Whether there is nothing to tell the user.
    pub fn is_empty(&self) -> bool {
        self.notices.is_empty()
    }

    /// Localized, user-facing messages (one per notice).
    pub fn messages(&self) -> Vec<String> {
        self.notices
            .iter()
            .map(|notice| match notice {
                Notice::ThemeReplaced => t!("migrate.theme_replaced").to_string(),
                Notice::CustomThemes { count } => {
                    t!("migrate.custom_themes", count = count).to_string()
                }
                Notice::LayoutReset => t!("migrate.layout_reset").to_string(),
                Notice::SettingsUnreadable => t!("migrate.settings_unreadable").to_string(),
            })
            .collect()
    }
}

/// Migrates the 1.x files under `dirs`. Never fails: every problem is logged and the affected
/// file keeps working through its own fallbacks (defaults, default layout).
pub fn run(dirs: &AppDirs) -> Report {
    let mut report = Report::default();
    migrate_settings(&dirs.settings_file(), &mut report);
    migrate_layout(&dirs.workspace_file(), &mut report);
    note_custom_themes(&dirs.themes_dir(), &mut report);
    if !report.backups.is_empty() || !report.notices.is_empty() {
        tracing::info!(
            backups = ?report.backups,
            notices = report.notices.len(),
            "migrated 1.x app files"
        );
    }
    report
}

/// `<file>.1x.bak` next to `file`.
fn backup_path(file: &Path) -> PathBuf {
    let mut name = file.as_os_str().to_owned();
    name.push(".1x.bak");
    PathBuf::from(name)
}

/// Copies `file` to its backup unless one exists already (the first backup is the original).
fn backup(file: &Path, report: &mut Report) -> std::io::Result<()> {
    let bak = backup_path(file);
    if bak.exists() {
        return Ok(());
    }
    std::fs::copy(file, &bak)?;
    report.backups.push(bak);
    Ok(())
}

fn migrate_settings(file: &Path, report: &mut Report) {
    let bytes = match std::fs::read(file) {
        Ok(bytes) => bytes,
        Err(err) => {
            if err.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!(path = %file.display(), "cannot read settings: {err}");
            }
            return;
        }
    };
    let Ok(Value::Object(mut map)) = serde_json::from_slice::<Value>(&bytes) else {
        // Unreadable: keep the bytes, the loader falls back to defaults.
        if backup(file, report).is_ok() {
            report.notices.push(Notice::SettingsUnreadable);
        }
        return;
    };
    let version = map
        .get("settings_version")
        .and_then(Value::as_u64)
        .unwrap_or(1);
    if version >= u64::from(SETTINGS_VERSION) {
        return;
    }
    if let Err(err) = backup(file, report) {
        // Without a backup the file is left as it is: the loader ignores unknown keys.
        tracing::warn!(path = %file.display(), "cannot back up settings, not migrating: {err}");
        return;
    }
    let mut replaced = false;
    for key in ["light_theme", "dark_theme"] {
        if let Some(Value::String(name)) = map.remove(key)
            && !DEFAULT_1X_THEMES.contains(&name.as_str())
        {
            replaced = true;
        }
    }
    if replaced {
        report.notices.push(Notice::ThemeReplaced);
    }
    map.insert("settings_version".into(), SETTINGS_VERSION.into());
    match serde_json::to_vec_pretty(&Value::Object(map)) {
        Ok(json) => {
            if let Err(err) = write_atomic(file, &json) {
                tracing::warn!(path = %file.display(), "cannot write migrated settings: {err}");
            }
        }
        Err(err) => tracing::warn!("cannot serialize migrated settings: {err}"),
    }
}

fn migrate_layout(file: &Path, report: &mut Report) {
    let Ok(bytes) = std::fs::read(file) else {
        return;
    };
    let version = serde_json::from_slice::<Value>(&bytes)
        .ok()
        .and_then(|v| v.get("version").and_then(Value::as_u64));
    // Only older layouts are ours to retire; a newer one belongs to a newer app.
    let Some(version) = version else { return };
    if version >= LAYOUT_VERSION as u64 {
        return;
    }
    if backup(file, report).is_err() {
        return;
    }
    match std::fs::remove_file(file) {
        Ok(()) => report.notices.push(Notice::LayoutReset),
        Err(err) => tracing::warn!(path = %file.display(), "cannot reset layout: {err}"),
    }
}

fn note_custom_themes(dir: &Path, report: &mut Report) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let count = entries
        .flatten()
        .filter(|e| e.path().is_file() && e.path().extension().is_some_and(|x| x == "json"))
        .count();
    // The files are the user's, so they stay; a marker keeps the notice to one showing until
    // colour-scheme files come back (BIT-US-0164).
    let marker = dir.join(".2.0-notice-shown");
    if count == 0 || marker.exists() {
        return;
    }
    if let Err(err) = std::fs::write(&marker, b"") {
        tracing::warn!(path = %marker.display(), "cannot write notice marker: {err}");
    }
    report.notices.push(Notice::CustomThemes { count });
}

#[cfg(test)]
mod tests {
    use super::*;

    const SETTINGS_1X: &str = include_str!("../tests/fixtures/settings-1.x.json");
    const WORKSPACE_1X: &str = include_str!("../tests/fixtures/workspace-1.x.json");

    fn dirs() -> (tempfile::TempDir, AppDirs) {
        let tmp = tempfile::tempdir().expect("tempdir");
        let dirs = AppDirs::with_root(tmp.path());
        dirs.ensure().expect("dirs");
        (tmp, dirs)
    }

    #[test]
    fn settings_1x_are_migrated_backed_up_and_keep_their_values() {
        let (_tmp, dirs) = dirs();
        let file = dirs.settings_file();
        std::fs::write(&file, SETTINGS_1X).expect("write");
        let report = run(&dirs);
        assert_eq!(report.notices, vec![Notice::ThemeReplaced]);
        assert_eq!(
            std::fs::read_to_string(backup_path(&file)).expect("backup"),
            SETTINGS_1X
        );
        let text = std::fs::read_to_string(&file).expect("read");
        assert!(!text.contains("light_theme") && !text.contains("Solarized"));
        let settings = crate::settings::AppSettings::load(&file);
        assert_eq!(settings.mode, crate::settings::ThemePreference::Dark);
        assert_eq!(settings.font_size, Some(15));
        assert!(settings.keep_running_in_background);
        assert!(settings.mcp.allow_writes);
        assert_eq!(settings.mcp.port, 7777);
        assert_eq!(settings.mcp.allowed_origins, vec!["http://localhost:3000"]);
        assert_eq!(settings.language.as_deref(), Some("es"));
        assert_eq!(settings.settings_version, SETTINGS_VERSION);
        assert!(
            settings.reopen_last_graph,
            "new settings take their default"
        );
    }

    #[test]
    fn migration_is_one_shot_and_never_overwrites_the_backup() {
        let (_tmp, dirs) = dirs();
        let file = dirs.settings_file();
        std::fs::write(&file, SETTINGS_1X).expect("write");
        run(&dirs);
        let migrated = std::fs::read(&file).expect("read");
        assert!(run(&dirs).is_empty());
        assert_eq!(std::fs::read(&file).expect("read"), migrated);
        // A file that regresses to 1.x (an older app ran again) keeps the first backup.
        std::fs::write(&file, br#"{"mode":"light"}"#).expect("write");
        run(&dirs);
        assert_eq!(
            std::fs::read_to_string(backup_path(&file)).expect("backup"),
            SETTINGS_1X
        );
    }

    #[test]
    fn default_1x_theme_names_do_not_warn() {
        let (_tmp, dirs) = dirs();
        std::fs::write(
            dirs.settings_file(),
            br#"{"mode":"light","light_theme":"Paper","dark_theme":null}"#,
        )
        .expect("write");
        assert!(run(&dirs).is_empty());
    }

    #[test]
    fn corrupt_settings_are_kept_and_reported() {
        let (_tmp, dirs) = dirs();
        std::fs::write(dirs.settings_file(), b"{ nope").expect("write");
        let report = run(&dirs);
        assert_eq!(report.notices, vec![Notice::SettingsUnreadable]);
        assert_eq!(
            std::fs::read(dirs.settings_file()).expect("read"),
            b"{ nope"
        );
    }

    #[test]
    fn missing_files_migrate_to_nothing() {
        let (_tmp, dirs) = dirs();
        assert_eq!(run(&dirs), Report::default());
        assert!(!dirs.settings_file().exists());
    }

    #[test]
    fn old_layout_is_backed_up_and_reset_but_current_one_stays() {
        let (_tmp, dirs) = dirs();
        let file = dirs.workspace_file();
        std::fs::write(&file, WORKSPACE_1X).expect("write");
        let report = run(&dirs);
        assert_eq!(report.notices, vec![Notice::LayoutReset]);
        assert!(!file.exists());
        assert_eq!(
            std::fs::read_to_string(backup_path(&file)).expect("backup"),
            WORKSPACE_1X
        );
        let current = format!(r#"{{"version":{LAYOUT_VERSION}}}"#);
        std::fs::write(&file, &current).expect("write");
        assert!(run(&dirs).is_empty());
        assert_eq!(std::fs::read_to_string(&file).expect("read"), current);
        let newer = format!(r#"{{"version":{}}}"#, LAYOUT_VERSION + 1);
        std::fs::write(&file, &newer).expect("write");
        assert!(run(&dirs).is_empty());
        assert_eq!(std::fs::read_to_string(&file).expect("read"), newer);
    }

    #[test]
    fn user_theme_files_stay_and_are_announced_once() {
        let (_tmp, dirs) = dirs();
        let themes = dirs.themes_dir();
        std::fs::create_dir_all(&themes).expect("dir");
        std::fs::write(themes.join("mine.json"), "{}").expect("write");
        std::fs::write(themes.join("notes.txt"), "x").expect("write");
        let report = run(&dirs);
        assert_eq!(report.notices, vec![Notice::CustomThemes { count: 1 }]);
        assert!(themes.join("mine.json").is_file());
        assert!(run(&dirs).is_empty());
    }

    #[test]
    fn recents_keymap_and_tokens_are_untouched() {
        let (_tmp, dirs) = dirs();
        let files = [
            (dirs.recent_graphs_file(), r#"{"graphs":[{"path":"/g"}]}"#),
            (dirs.keymap_file(), r#"[{"bindings":{}}]"#),
            (dirs.config_dir.join("mcp-tokens.json"), r#"{"version":1}"#),
        ];
        for (file, text) in &files {
            std::fs::write(file, text).expect("write");
        }
        assert!(run(&dirs).is_empty());
        for (file, text) in &files {
            assert_eq!(&std::fs::read_to_string(file).expect("read"), text);
            assert!(!backup_path(file).exists());
        }
        let recents = crate::recent::RecentGraphs::load(&dirs.recent_graphs_file());
        assert_eq!(recents.graphs().len(), 1);
    }
}
