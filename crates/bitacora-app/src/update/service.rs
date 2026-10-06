//! Blocking update operations (run them on a background thread).
//!
//! Two paths (ADR-018):
//! * **Velopack**, when the app was installed by a Velopack package: the feed is the project's
//!   GitHub Releases, the update is downloaded in the background and applied on restart;
//! * **Notice only**, for every other install (AppImage, deb, MSI/NSIS from cargo-packager,
//!   `cargo run`): the GitHub releases API is asked once and the user gets a link.
//!
//! The only network traffic is the GitHub request (no telemetry, no identifiers).

use std::time::Duration;

use semver::Version;
use velopack::sources::GithubSource;
use velopack::{UpdateCheck, UpdateInfo, UpdateManager, UpdateOptions};

use super::release::{self, Channel, Release};

/// Repository the releases come from.
pub const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");

/// Result of a check.
#[derive(Debug, Clone)]
pub enum CheckOutcome {
    /// Nothing newer.
    UpToDate,
    /// A Velopack update that can be downloaded and applied by the updater.
    Velopack {
        /// Target version.
        version: String,
        /// Feed data needed to download and apply.
        info: Box<UpdateInfo>,
    },
    /// A newer release exists; the user installs it from the release page.
    Notice(Release),
}

/// Errors of a check.
#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    /// Network or feed failure.
    #[error("{0}")]
    Check(String),
    /// Applying the update failed.
    #[error("{0}")]
    Apply(String),
}

fn manager(channel: Channel) -> Option<UpdateManager> {
    let source = GithubSource::new(REPOSITORY, None, channel.includes_prereleases());
    UpdateManager::new(source, Some(UpdateOptions::default()), None).ok()
}

/// Whether this process runs from a Velopack install.
pub fn is_velopack_install(channel: Channel) -> bool {
    manager(channel).is_some()
}

/// Checks for a newer release.
pub fn check(channel: Channel) -> Result<CheckOutcome, UpdateError> {
    if let Some(manager) = manager(channel) {
        return match manager
            .check_for_updates()
            .map_err(|e| UpdateError::Check(e.to_string()))?
        {
            UpdateCheck::UpdateAvailable(info) => Ok(CheckOutcome::Velopack {
                version: info.TargetFullRelease.Version.clone(),
                info,
            }),
            UpdateCheck::NoUpdateAvailable | UpdateCheck::RemoteIsEmpty => {
                Ok(CheckOutcome::UpToDate)
            }
        };
    }
    check_notice_only(channel)
}

fn check_notice_only(channel: Channel) -> Result<CheckOutcome, UpdateError> {
    let slug = release::repo_slug(REPOSITORY)
        .ok_or_else(|| UpdateError::Check(format!("not a GitHub repository: {REPOSITORY}")))?;
    let current =
        Version::parse(env!("CARGO_PKG_VERSION")).map_err(|e| UpdateError::Check(e.to_string()))?;
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(15)))
        .build()
        .into();
    let body = agent
        .get(&format!(
            "https://api.github.com/repos/{slug}/releases?per_page=20"
        ))
        .header("Accept", "application/vnd.github+json")
        .header(
            "User-Agent",
            concat!("bitacora/", env!("CARGO_PKG_VERSION")),
        )
        .call()
        .map_err(|e| UpdateError::Check(e.to_string()))?
        .body_mut()
        .read_to_string()
        .map_err(|e| UpdateError::Check(e.to_string()))?;
    let releases = release::parse_releases(&body);
    Ok(match release::pick_update(&current, channel, &releases) {
        Some(release) => CheckOutcome::Notice(release),
        None => CheckOutcome::UpToDate,
    })
}

/// Downloads the update packages (delta when available) without applying anything.
pub fn download(channel: Channel, info: &UpdateInfo) -> Result<(), UpdateError> {
    let manager =
        manager(channel).ok_or_else(|| UpdateError::Apply("not a Velopack install".into()))?;
    manager
        .download_updates(info, None)
        .map_err(|e| UpdateError::Apply(e.to_string()))
}

/// Hands the downloaded update to the Velopack updater, which waits for this process to exit,
/// applies it and restarts the app. The caller then quits through the normal shutdown, so
/// pending writes are flushed and sync is stopped before anything is replaced.
pub fn apply_after_exit(channel: Channel, info: &UpdateInfo) -> Result<(), UpdateError> {
    let manager =
        manager(channel).ok_or_else(|| UpdateError::Apply("not a Velopack install".into()))?;
    manager
        .wait_exit_then_apply_updates(&info.TargetFullRelease, true, true, Vec::<String>::new())
        .map_err(|e| UpdateError::Apply(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_development_build_is_not_a_velopack_install() {
        assert!(!is_velopack_install(Channel::Stable));
    }

    #[test]
    fn repository_is_a_github_url() {
        assert!(release::repo_slug(REPOSITORY).is_some());
    }
}
