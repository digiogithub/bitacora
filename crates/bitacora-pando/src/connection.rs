//! Helpers of the settings page: "Test connection" and "does the managed instance share my KB".
//!
//! Both are blocking, free of UI types and never log or return a secret.

use std::path::{Path, PathBuf};
use std::time::Duration;

use bitacora_config::{PandoSettings, UrlRole, validate_pando_url};
use pando::{PandoClient, PandoConfig};

use crate::credentials::{PandoCredentials, TokenKind};
use crate::managed::{DEFAULT_MIN_VERSION, parse_version};

/// What a successful `GET /health` told us.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionReport {
    /// Server version string.
    pub version: String,
    /// Startup mode reported by the server (may be empty).
    pub startup_mode: String,
    /// Whether the version satisfies [`DEFAULT_MIN_VERSION`] (unknown versions count as ok).
    pub version_ok: bool,
    /// The minimum version Bitacora needs.
    pub min_version: &'static str,
}

/// Whether `version` is at least `min` (an unparsable `version` is accepted: dev builds).
#[must_use]
pub fn version_at_least(version: &str, min: &str) -> bool {
    match (parse_version(version), parse_version(min)) {
        (Some(v), Some(m)) => v >= m,
        _ => true,
    }
}

/// Probes the REST endpoint of `settings` with the stored REST token.
///
/// # Errors
/// A user-presentable message: invalid URL, unreachable server, bad token.
pub fn test_connection(
    settings: &PandoSettings,
    credentials: &PandoCredentials,
    timeout: Duration,
) -> Result<ConnectionReport, String> {
    let url = validate_pando_url(UrlRole::Rest, &settings.rest_url, settings.allow_remote)
        .map_err(|e| e.to_string())?;
    let mut cfg = PandoConfig::new(url.normalized)
        .with_timeout(timeout)
        .with_connect_timeout(timeout);
    if let Some((token, _)) = credentials.resolve(TokenKind::Rest) {
        cfg = cfg.with_token(token);
    }
    let client = PandoClient::new(cfg).map_err(|e| e.to_string())?;
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    let info = rt.block_on(client.info()).map_err(|e| e.to_string())?;
    Ok(ConnectionReport {
        version_ok: version_at_least(&info.version, DEFAULT_MIN_VERSION),
        version: info.version,
        startup_mode: info.startup_mode,
        min_version: DEFAULT_MIN_VERSION,
    })
}

/// Whether a managed instance shares the user's own Pando knowledge base (design 5.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KbSharing {
    /// The user's global Pando config sets an absolute `Data.Directory`: one database for all.
    Shared(PathBuf),
    /// The default relative directory: the managed instance keeps a private database.
    Private,
}

/// `<pando config dir>/.pando.toml`, the user's global Pando configuration.
#[must_use]
pub fn default_global_pando_config() -> Option<PathBuf> {
    let base = match std::env::var_os("XDG_CONFIG_HOME").filter(|v| !v.is_empty()) {
        Some(x) => PathBuf::from(x),
        None => PathBuf::from(std::env::var_os("HOME")?).join(".config"),
    };
    Some(base.join("pando").join(".pando.toml"))
}

/// Reads the global Pando config at `path` (`None`: the default location); a missing or
/// unreadable file means the default relative data directory, hence [`KbSharing::Private`].
#[must_use]
pub fn kb_sharing(path: Option<&Path>) -> KbSharing {
    let Some(path) = path
        .map(Path::to_path_buf)
        .or_else(default_global_pando_config)
    else {
        return KbSharing::Private;
    };
    let Ok(text) = std::fs::read_to_string(path) else {
        return KbSharing::Private;
    };
    let Ok(doc) = text.parse::<toml::Table>() else {
        return KbSharing::Private;
    };
    let dir = doc
        .get("Data")
        .and_then(toml::Value::as_table)
        .and_then(|t| t.get("Directory"))
        .and_then(toml::Value::as_str)
        .map(str::trim);
    match dir {
        Some(d) if Path::new(d).is_absolute() => KbSharing::Shared(PathBuf::from(d)),
        _ => KbSharing::Private,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn version_comparison() {
        assert!(version_at_least("v1.2.11", "1.2.0"));
        assert!(version_at_least("1.2.0", "1.2.0"));
        assert!(!version_at_least("1.1.9", "1.2.0"));
        assert!(version_at_least("dev", "1.2.0"));
    }

    #[test]
    fn sharing_needs_an_absolute_data_directory() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join(".pando.toml");
        assert_eq!(kb_sharing(Some(&f)), KbSharing::Private);
        std::fs::write(&f, "[Data]\nDirectory = '.pando'\n").unwrap();
        assert_eq!(kb_sharing(Some(&f)), KbSharing::Private);
        let abs = dir.path().join("kb");
        std::fs::write(&f, format!("[Data]\nDirectory = '{}'\n", abs.display())).unwrap();
        assert_eq!(kb_sharing(Some(&f)), KbSharing::Shared(abs));
        std::fs::write(&f, "not toml [[").unwrap();
        assert_eq!(kb_sharing(Some(&f)), KbSharing::Private);
    }

    #[test]
    fn invalid_url_is_reported_without_network() {
        let s = PandoSettings {
            rest_url: "nope".into(),
            ..PandoSettings::default()
        };
        let c = PandoCredentials::new(None, |_| None);
        assert!(test_connection(&s, &c, Duration::from_millis(200)).is_err());
    }
}
