//! Auto-update (BIT-US-0100, ADR-018).
//!
//! * [`release`]: pure logic (feed parsing, channels, schedule), unit-tested;
//! * [`service`]: blocking Velopack / GitHub calls, run on a background thread;
//! * this module: persisted state, settings and the notifications that tie them together.
//!
//! Flow: at startup (and on "Check for updates...") a background check runs at most once a day.
//! With a Velopack install the update downloads in the background and a notice offers
//! "Restart now"; the updater applies it only after this process has exited through the normal
//! ordered shutdown (pending writes flushed, sync stopped). Any other install gets a notice with
//! a download link. Checks can be turned off with `updates.enabled = false` in `settings.json`;
//! the only request made is to the GitHub API.

pub mod release;
pub mod service;

use std::path::{Path, PathBuf};

use rust_i18n::t;
use serde::{Deserialize, Serialize};

use crate::actions::Quit;
use crate::settings::AppSettings;
use crate::ui::{AnyWindowHandle, App, Global, Level, notify, notify_action};
use release::{CHECK_INTERVAL, Channel, is_due};
use service::{CheckOutcome, UpdateError};

/// User settings of the updater (`updates` object in `settings.json`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct UpdateSettings {
    /// Check for updates automatically (once a day).
    pub enabled: bool,
    /// Stable releases only, or also pre-releases.
    pub channel: Channel,
}

impl Default for UpdateSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            channel: Channel::Stable,
        }
    }
}

/// What the updater remembers between runs (`update-state.json` in the data dir).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct UpdateState {
    /// Unix time of the last automatic check.
    pub last_check_unix: Option<i64>,
    /// Version the user was already told about (no daily nagging).
    pub notified_version: Option<String>,
}

impl UpdateState {
    /// Loads the state; missing or invalid files give the default.
    pub fn load(path: &Path) -> Self {
        std::fs::read(path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    /// Saves the state, logging failures (it is only a convenience).
    pub fn save(&self, path: &Path) {
        let result = serde_json::to_vec_pretty(self)
            .map_err(std::io::Error::other)
            .and_then(|json| crate::settings::write_atomic(path, &json));
        if let Err(err) = result {
            tracing::warn!("cannot save the update state: {err}");
        }
    }
}

/// Files the updater reads, kept as an app global.
#[derive(Debug, Clone)]
struct UpdateContext {
    settings_file: PathBuf,
    state_file: PathBuf,
}

impl Global for UpdateContext {}

fn now_unix() -> i64 {
    jiff::Timestamp::now().as_second()
}

/// Must run first in `main`: lets Velopack handle its install / uninstall / update hooks.
/// A no-op for anything that is not a Velopack install.
pub fn init_velopack() {
    velopack::VelopackApp::build().run();
}

/// Registers the updater and runs the automatic check when it is due.
pub fn start(handle: AnyWindowHandle, dirs: &crate::paths::AppDirs, cx: &mut App) {
    cx.set_global(UpdateContext {
        settings_file: dirs.settings_file(),
        state_file: dirs.data_dir.join("update-state.json"),
    });
    run_check(handle, false, cx);
}

/// "Check for updates...": always checks and always reports the result.
pub fn check_now(handle: AnyWindowHandle, cx: &mut App) {
    run_check(handle, true, cx);
}

fn run_check(handle: AnyWindowHandle, manual: bool, cx: &mut App) {
    let Some(ctx) = cx.try_global::<UpdateContext>().cloned() else {
        return;
    };
    let settings = AppSettings::load(&ctx.settings_file).updates;
    let state = UpdateState::load(&ctx.state_file);
    if !manual && (!settings.enabled || !is_due(state.last_check_unix, now_unix(), CHECK_INTERVAL))
    {
        return;
    }
    let channel = settings.channel;
    cx.spawn(async move |cx| {
        let outcome = cx
            .background_executor()
            .spawn(async move { service::check(channel) })
            .await;
        let mut state = UpdateState::load(&ctx.state_file);
        state.last_check_unix = Some(now_unix());
        cx.update(|cx| report(handle, manual, channel, outcome, &mut state, cx));
        state.save(&ctx.state_file);
    })
    .detach();
}

fn report(
    handle: AnyWindowHandle,
    manual: bool,
    channel: Channel,
    outcome: Result<CheckOutcome, UpdateError>,
    state: &mut UpdateState,
    cx: &mut App,
) {
    let _ = handle.update(cx, |_, window, cx| match outcome {
        Err(err) => {
            tracing::warn!("update check failed: {err}");
            if manual {
                notify(
                    window,
                    cx,
                    Level::Warning,
                    t!("update.check_failed", error = err.to_string()).to_string(),
                );
            }
        }
        Ok(CheckOutcome::UpToDate) => {
            if manual {
                notify(window, cx, Level::Info, t!("update.up_to_date").to_string());
            }
        }
        Ok(CheckOutcome::Notice(release)) => {
            if manual || state.notified_version.as_deref() != Some(release.tag_name.as_str()) {
                state.notified_version = Some(release.tag_name.clone());
                let url = release.html_url.clone();
                notify_action(
                    window,
                    cx,
                    Level::Info,
                    t!("update.available", version = release.tag_name).to_string(),
                    t!("update.download").to_string(),
                    move |_, cx| cx.open_url(&url),
                );
            }
        }
        Ok(CheckOutcome::Velopack { version, info }) => {
            state.notified_version = Some(version.clone());
            notify(
                window,
                cx,
                Level::Info,
                t!("update.downloading", version = version.clone()).to_string(),
            );
            download_in_background(handle, channel, version, *info, cx);
        }
    });
}

fn download_in_background(
    handle: AnyWindowHandle,
    channel: Channel,
    version: String,
    info: velopack::UpdateInfo,
    cx: &mut App,
) {
    cx.spawn(async move |cx| {
        let target = info.clone();
        let result = cx
            .background_executor()
            .spawn(async move { service::download(channel, &target) })
            .await;
        cx.update(|cx| {
            let _ = handle.update(cx, |_, window, cx| match result {
                Ok(()) => {
                    let info = info.clone();
                    notify_action(
                        window,
                        cx,
                        Level::Success,
                        t!("update.ready", version = version).to_string(),
                        t!("update.restart").to_string(),
                        move |window, cx| match service::apply_after_exit(channel, &info) {
                            // The updater now waits for this process; the normal quit path
                            // flushes pending writes and stops sync before it exits.
                            Ok(()) => window.dispatch_action(Box::new(Quit), cx),
                            Err(err) => notify(
                                window,
                                cx,
                                Level::Error,
                                t!("update.apply_failed", error = err.to_string()).to_string(),
                            ),
                        },
                    );
                }
                Err(err) => {
                    tracing::warn!("update download failed: {err}");
                    notify(
                        window,
                        cx,
                        Level::Warning,
                        t!("update.check_failed", error = err.to_string()).to_string(),
                    );
                }
            });
        });
    })
    .detach();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_default_to_enabled_stable_and_roundtrip_through_app_settings() {
        let defaults = UpdateSettings::default();
        assert!(defaults.enabled);
        assert_eq!(defaults.channel, Channel::Stable);
        let parsed: AppSettings =
            serde_json::from_str(r#"{"updates":{"enabled":false,"channel":"beta"}}"#)
                .expect("parse");
        assert!(!parsed.updates.enabled);
        assert_eq!(parsed.updates.channel, Channel::Beta);
        let none: AppSettings = serde_json::from_str("{}").expect("parse");
        assert_eq!(none.updates, defaults);
    }

    #[test]
    fn state_roundtrips_and_tolerates_garbage() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("update-state.json");
        assert_eq!(UpdateState::load(&path), UpdateState::default());
        let state = UpdateState {
            last_check_unix: Some(42),
            notified_version: Some("v1.0.0".into()),
        };
        state.save(&path);
        assert_eq!(UpdateState::load(&path), state);
        std::fs::write(&path, b"{{").expect("write");
        assert_eq!(UpdateState::load(&path), UpdateState::default());
    }
}
