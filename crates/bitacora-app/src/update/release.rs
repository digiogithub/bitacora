//! Pure update logic: GitHub release parsing, channel filtering, version comparison and the
//! once-a-day schedule. No I/O, so it is fully unit-tested (BIT-US-0100).

use std::time::Duration;

use semver::Version;
use serde::Deserialize;

/// How often the automatic check may run.
pub const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);

/// Which releases the user follows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    /// Published, non-prerelease releases only.
    #[default]
    Stable,
    /// Also pre-releases.
    Beta,
}

impl Channel {
    /// Whether pre-releases are followed.
    pub fn includes_prereleases(self) -> bool {
        self == Self::Beta
    }
}

/// A GitHub release, reduced to what the notice needs.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Release {
    /// Git tag, e.g. `v0.2.0`.
    pub tag_name: String,
    /// Release title.
    #[serde(default)]
    pub name: Option<String>,
    /// Release page (notes and downloads).
    pub html_url: String,
    /// Marked as pre-release.
    #[serde(default)]
    pub prerelease: bool,
    /// Draft releases are never offered.
    #[serde(default)]
    pub draft: bool,
}

impl Release {
    /// The semantic version of the tag (`v` prefix tolerated).
    pub fn version(&self) -> Option<Version> {
        parse_version(&self.tag_name)
    }
}

/// Parses `v1.2.3` / `1.2.3` / `1.2.3-beta.1`.
pub fn parse_version(text: &str) -> Option<Version> {
    Version::parse(text.trim().trim_start_matches('v')).ok()
}

/// Parses the body of `GET /repos/{owner}/{repo}/releases`; invalid entries are skipped.
pub fn parse_releases(json: &str) -> Vec<Release> {
    let Ok(values) = serde_json::from_str::<Vec<serde_json::Value>>(json) else {
        return Vec::new();
    };
    values
        .into_iter()
        .filter_map(|v| serde_json::from_value(v).ok())
        .collect()
}

/// The newest release that is newer than `current` and allowed by `channel`.
pub fn pick_update(current: &Version, channel: Channel, releases: &[Release]) -> Option<Release> {
    releases
        .iter()
        .filter(|r| !r.draft && (channel.includes_prereleases() || !r.prerelease))
        .filter_map(|r| Some((r.version()?, r)))
        .filter(|(v, _)| v > current)
        .max_by(|a, b| a.0.cmp(&b.0))
        .map(|(_, r)| r.clone())
}

/// `owner/repo` of a GitHub repository URL.
pub fn repo_slug(repository_url: &str) -> Option<String> {
    let rest = repository_url
        .trim_end_matches('/')
        .strip_prefix("https://github.com/")?;
    let mut parts = rest.split('/');
    let (owner, repo) = (parts.next()?, parts.next()?);
    (!owner.is_empty() && !repo.is_empty()).then(|| format!("{owner}/{repo}"))
}

/// Whether an automatic check is due: never run, clock went backwards, or `interval` elapsed.
pub fn is_due(last_check_unix: Option<i64>, now_unix: i64, interval: Duration) -> bool {
    match last_check_unix {
        None => true,
        Some(last) if last > now_unix => true,
        Some(last) => now_unix - last >= i64::try_from(interval.as_secs()).unwrap_or(i64::MAX),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FEED: &str = r#"[
        {"tag_name":"v0.3.0-beta.1","html_url":"https://x/b","prerelease":true,"draft":false},
        {"tag_name":"v0.2.0","name":"Two","html_url":"https://x/2","prerelease":false,"draft":false},
        {"tag_name":"v0.9.0","html_url":"https://x/d","prerelease":false,"draft":true},
        {"tag_name":"nightly","html_url":"https://x/n","prerelease":false,"draft":false},
        {"broken":true}
    ]"#;

    fn v(s: &str) -> Version {
        parse_version(s).expect("version")
    }

    #[test]
    fn parses_valid_entries_and_skips_the_rest() {
        let releases = parse_releases(FEED);
        assert_eq!(releases.len(), 4);
        assert!(parse_releases("not json").is_empty());
    }

    #[test]
    fn stable_channel_ignores_prereleases_drafts_and_non_semver_tags() {
        let releases = parse_releases(FEED);
        let pick = pick_update(&v("0.1.0"), Channel::Stable, &releases).expect("update");
        assert_eq!(pick.tag_name, "v0.2.0");
        assert!(pick_update(&v("0.2.0"), Channel::Stable, &releases).is_none());
    }

    #[test]
    fn beta_channel_offers_the_newest_prerelease() {
        let releases = parse_releases(FEED);
        let pick = pick_update(&v("0.2.0"), Channel::Beta, &releases).expect("update");
        assert_eq!(pick.tag_name, "v0.3.0-beta.1");
        assert!(pick_update(&v("0.3.0"), Channel::Beta, &releases).is_none());
    }

    #[test]
    fn slug_and_schedule() {
        assert_eq!(
            repo_slug("https://github.com/digio-es/bitacora/").as_deref(),
            Some("digio-es/bitacora")
        );
        assert_eq!(repo_slug("https://example.com/a/b"), None);
        let day = CHECK_INTERVAL;
        assert!(is_due(None, 1000, day));
        assert!(!is_due(Some(1000), 1000 + 3600, day));
        assert!(is_due(Some(1000), 1000 + 86_400, day));
        assert!(is_due(Some(5000), 1000, day), "clock moved back");
    }
}
