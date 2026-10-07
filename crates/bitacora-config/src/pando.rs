//! Machine-local settings of the Pando integration (ADR-028/029).
//!
//! These settings live in a JSON file in the platform config directory, never inside a graph and
//! never in git (cf. ADR-019). They hold **no secret**: tokens are kept in the OS keychain by
//! `bitacora-pando`. Everything here is plain data plus validation; the crate has no network or
//! keychain access.
//!
//! URL policy: a Pando endpoint must be loopback (`localhost`, `127.0.0.0/8`, `::1`) unless
//! [`PandoSettings::allow_remote`] is set, and a non-loopback endpoint must use `https`.

use std::collections::BTreeMap;
use std::net::IpAddr;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// Default REST base URL of a local Pando server.
pub const DEFAULT_REST_URL: &str = "http://127.0.0.1:8765";
/// Default AG-UI base URL of a local Pando server.
pub const DEFAULT_AGUI_URL: &str = "http://127.0.0.1:8765";

/// How Bitacora relates to the Pando server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PandoMode {
    /// Bitacora supervises a `pando serve` instance per graph (default, ADR-029).
    #[default]
    Managed,
    /// The user runs Pando; Bitacora only connects to the configured URLs.
    External,
    /// No Pando integration.
    Off,
}

/// A feature that can be switched on or off independently.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PandoFeature {
    /// Index graph blocks in Pando's knowledge base for semantic search.
    SemanticSearch,
    /// Chat with Pando agents from inside Bitacora (AG-UI).
    AgentChat,
    /// Let agents use Bitacora's MCP endpoint through Pando.
    McpBridge,
}

impl PandoFeature {
    /// Every feature, in display order.
    pub const ALL: [Self; 3] = [Self::SemanticSearch, Self::AgentChat, Self::McpBridge];
}

/// What the user agreed to for one graph.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct GraphConsent {
    /// The user accepted that indexed blocks go to Pando's shared KB (visible to any Pando agent).
    pub granted: bool,
    /// Unix seconds of the consent, for display.
    pub granted_at: Option<i64>,
    /// Graph-relative path prefixes or page names never sent to Pando.
    pub exclusions: Vec<String>,
}

/// Pando settings of this machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PandoSettings {
    /// Master switch (off by default: the integration is opt-in).
    pub enabled: bool,
    /// Managed (default), external or off.
    pub mode: PandoMode,
    /// REST (knowledge base) base URL.
    pub rest_url: String,
    /// AG-UI (agent runs) base URL.
    pub agui_url: String,
    /// Allow non-loopback URLs (they then must be `https`).
    pub allow_remote: bool,
    /// Pando profile (agent persona) per feature.
    pub profiles: BTreeMap<PandoFeature, String>,
    /// Feature switches; a missing feature is on when [`enabled`](Self::enabled) is.
    pub features: BTreeMap<PandoFeature, bool>,
    /// Consent and exclusions per graph, keyed by the canonical graph path.
    pub graphs: BTreeMap<String, GraphConsent>,
}

impl Default for PandoSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: PandoMode::Managed,
            rest_url: DEFAULT_REST_URL.to_owned(),
            agui_url: DEFAULT_AGUI_URL.to_owned(),
            allow_remote: false,
            profiles: BTreeMap::new(),
            features: BTreeMap::new(),
            graphs: BTreeMap::new(),
        }
    }
}

/// Which endpoint a URL error is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UrlRole {
    /// REST knowledge-base URL.
    Rest,
    /// AG-UI URL.
    Agui,
}

impl std::fmt::Display for UrlRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Rest => "REST",
            Self::Agui => "AG-UI",
        })
    }
}

/// Why settings are invalid or could not be stored.
#[derive(Debug, thiserror::Error)]
pub enum PandoSettingsError {
    /// A URL is malformed.
    #[error("{role} URL `{url}` is not a valid http(s) URL: {reason}")]
    BadUrl {
        /// Which URL.
        role: UrlRole,
        /// The offending text (`<redacted>` when it may carry credentials).
        url: String,
        /// What is wrong.
        reason: &'static str,
    },
    /// A non-loopback URL without `allow_remote`.
    #[error("{role} URL `{url}` is not loopback; enable remote servers to use it")]
    RemoteNotAllowed {
        /// Which URL.
        role: UrlRole,
        /// The URL.
        url: String,
    },
    /// A non-loopback URL that is not `https`.
    #[error("{role} URL `{url}` is remote and must use https")]
    RemoteNeedsHttps {
        /// Which URL.
        role: UrlRole,
        /// The URL.
        url: String,
    },
    /// The settings file could not be read or written.
    #[error("pando settings file {path}: {source}")]
    Io {
        /// File.
        path: String,
        /// Cause.
        source: std::io::Error,
    },
    /// The settings file is not valid JSON for this model.
    #[error("pando settings file {path}: {source}")]
    Parse {
        /// File.
        path: String,
        /// Cause.
        source: serde_json::Error,
    },
}

/// A validated endpoint URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PandoUrl {
    /// `http` or `https`.
    pub scheme: String,
    /// Host without brackets.
    pub host: String,
    /// Explicit port, if any.
    pub port: Option<u16>,
    /// Whether the host is loopback.
    pub loopback: bool,
    /// The normalised URL (lowercase scheme, no trailing slash).
    pub normalized: String,
}

fn is_loopback_host(host: &str) -> bool {
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    host.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback())
}

/// Parses `raw` and applies the loopback / `https` policy.
///
/// # Errors
/// [`PandoSettingsError`] when the URL is malformed, carries credentials, is remote without
/// `allow_remote`, or is remote without `https`.
pub fn validate_pando_url(
    role: UrlRole,
    raw: &str,
    allow_remote: bool,
) -> Result<PandoUrl, PandoSettingsError> {
    let raw_trim = raw.trim();
    let bad = |reason: &'static str| PandoSettingsError::BadUrl {
        role,
        // Never echo what may be a credential.
        url: if raw_trim.contains('@') {
            "<redacted>".to_owned()
        } else {
            raw_trim.to_owned()
        },
        reason,
    };
    let (scheme, rest) = raw_trim
        .split_once("://")
        .ok_or_else(|| bad("missing scheme"))?;
    let scheme = scheme.to_ascii_lowercase();
    if scheme != "http" && scheme != "https" {
        return Err(bad("scheme must be http or https"));
    }
    let (authority, path) = match rest.find(['/', '?', '#']) {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, ""),
    };
    if authority.contains('@') {
        return Err(bad("credentials in the URL are not allowed"));
    }
    let (host, port) = if let Some(r) = authority.strip_prefix('[') {
        let (h, after) = r
            .split_once(']')
            .ok_or_else(|| bad("unterminated IPv6 host"))?;
        let port = match after.strip_prefix(':') {
            Some(p) => Some(p),
            None if after.is_empty() => None,
            None => return Err(bad("garbage after IPv6 host")),
        };
        (h, port)
    } else {
        match authority.rsplit_once(':') {
            Some((h, p)) => (h, Some(p)),
            None => (authority, None),
        }
    };
    if host.is_empty() {
        return Err(bad("missing host"));
    }
    if host
        .chars()
        .any(|c| c.is_whitespace() || c == '[' || c == ']')
    {
        return Err(bad("invalid host"));
    }
    let port = match port {
        Some(p) => Some(p.parse::<u16>().map_err(|_| bad("invalid port"))?),
        None => None,
    };
    let loopback = is_loopback_host(host);
    if !loopback {
        if !allow_remote {
            return Err(PandoSettingsError::RemoteNotAllowed {
                role,
                url: raw_trim.to_owned(),
            });
        }
        if scheme != "https" {
            return Err(PandoSettingsError::RemoteNeedsHttps {
                role,
                url: raw_trim.to_owned(),
            });
        }
    }
    let host_text = if host.contains(':') {
        format!("[{host}]")
    } else {
        host.to_owned()
    };
    let port_text = port.map(|p| format!(":{p}")).unwrap_or_default();
    let normalized = format!(
        "{scheme}://{host_text}{port_text}{}",
        path.trim_end_matches('/')
    );
    Ok(PandoUrl {
        scheme,
        host: host.to_owned(),
        port,
        loopback,
        normalized,
    })
}

impl PandoSettings {
    /// Validates both URLs against the loopback / `https` policy. Skipped for [`PandoMode::Off`].
    ///
    /// # Errors
    /// The first [`PandoSettingsError`] found.
    pub fn validate(&self) -> Result<(), PandoSettingsError> {
        if self.mode == PandoMode::Off {
            return Ok(());
        }
        validate_pando_url(UrlRole::Rest, &self.rest_url, self.allow_remote)?;
        validate_pando_url(UrlRole::Agui, &self.agui_url, self.allow_remote)?;
        Ok(())
    }

    /// Whether the integration should run at all.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.enabled && self.mode != PandoMode::Off
    }

    /// Whether `feature` is switched on (requires [`is_active`](Self::is_active) too).
    #[must_use]
    pub fn feature_enabled(&self, feature: PandoFeature) -> bool {
        self.is_active() && self.features.get(&feature).copied().unwrap_or(true)
    }

    /// The Pando profile configured for `feature`, if any.
    #[must_use]
    pub fn profile(&self, feature: PandoFeature) -> Option<&str> {
        self.profiles.get(&feature).map(String::as_str)
    }

    /// Consent record of a graph (default: none given).
    #[must_use]
    pub fn consent(&self, graph: &str) -> GraphConsent {
        self.graphs.get(graph).cloned().unwrap_or_default()
    }

    /// Whether the user consented to send data of `graph` to Pando.
    #[must_use]
    pub fn has_consent(&self, graph: &str) -> bool {
        self.graphs.get(graph).is_some_and(|c| c.granted)
    }

    /// Records consent for `graph`.
    pub fn grant_consent(&mut self, graph: &str, now_unix: i64) {
        let c = self.graphs.entry(graph.to_owned()).or_default();
        c.granted = true;
        c.granted_at = Some(now_unix);
    }

    /// Withdraws consent for `graph` (exclusions stay).
    pub fn revoke_consent(&mut self, graph: &str) {
        if let Some(c) = self.graphs.get_mut(graph) {
            c.granted = false;
            c.granted_at = None;
        }
    }

    /// Reads settings from `path`; a missing file yields the defaults.
    ///
    /// # Errors
    /// [`PandoSettingsError::Io`] / [`PandoSettingsError::Parse`].
    pub fn load(path: &Path) -> Result<Self, PandoSettingsError> {
        match std::fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text).map_err(|source| PandoSettingsError::Parse {
                path: path.display().to_string(),
                source,
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(source) => Err(PandoSettingsError::Io {
                path: path.display().to_string(),
                source,
            }),
        }
    }

    /// Writes settings atomically (temp file in the same directory, fsync, rename).
    ///
    /// # Errors
    /// [`PandoSettingsError::Io`] / [`PandoSettingsError::Parse`].
    pub fn save(&self, path: &Path) -> Result<(), PandoSettingsError> {
        use std::io::Write as _;
        let io = |source| PandoSettingsError::Io {
            path: path.display().to_string(),
            source,
        };
        let text =
            serde_json::to_string_pretty(self).map_err(|source| PandoSettingsError::Parse {
                path: path.display().to_string(),
                source,
            })?;
        let dir = path.parent().unwrap_or_else(|| Path::new("."));
        std::fs::create_dir_all(dir).map_err(io)?;
        let tmp = path.with_extension("json.tmp");
        let mut f = std::fs::File::create(&tmp).map_err(io)?;
        f.write_all(text.as_bytes()).map_err(io)?;
        f.write_all(b"\n").map_err(io)?;
        f.sync_all().map_err(io)?;
        drop(f);
        std::fs::rename(&tmp, path).map_err(io)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;

    fn check(raw: &str, remote: bool) -> Result<PandoUrl, PandoSettingsError> {
        validate_pando_url(UrlRole::Rest, raw, remote)
    }

    #[test]
    fn loopback_hosts_are_accepted_over_http() {
        for u in [
            "http://127.0.0.1:8765",
            "http://localhost",
            "http://LOCALHOST:1/x/",
            "http://[::1]:9000",
            "http://127.1.2.3",
            "https://localhost:8443",
        ] {
            let p = check(u, false).unwrap_or_else(|e| panic!("{u}: {e}"));
            assert!(p.loopback, "{u}");
        }
    }

    #[test]
    fn normalisation_strips_trailing_slash_and_brackets_ipv6() {
        assert_eq!(
            check("http://[::1]:9000/", false).unwrap().normalized,
            "http://[::1]:9000"
        );
        assert_eq!(
            check("HTTP://localhost:80/api/", false).unwrap().normalized,
            "http://localhost:80/api"
        );
    }

    #[test]
    fn remote_needs_allow_remote_then_https() {
        assert!(matches!(
            check("https://pando.example.com", false),
            Err(PandoSettingsError::RemoteNotAllowed { .. })
        ));
        assert!(matches!(
            check("http://pando.example.com", true),
            Err(PandoSettingsError::RemoteNeedsHttps { .. })
        ));
        assert!(check("https://pando.example.com:8443", true).is_ok());
        assert!(matches!(
            check("http://10.0.0.5", true),
            Err(PandoSettingsError::RemoteNeedsHttps { .. })
        ));
        // Lookalikes are not loopback.
        assert!(matches!(
            check("http://localhost.evil.com", false),
            Err(PandoSettingsError::RemoteNotAllowed { .. })
        ));
        assert!(matches!(
            check("http://127.0.0.1.evil.com", false),
            Err(PandoSettingsError::RemoteNotAllowed { .. })
        ));
    }

    #[test]
    fn malformed_urls_are_rejected_without_echoing_credentials() {
        for u in [
            "",
            "localhost:8765",
            "ftp://localhost",
            "http://",
            "http://localhost:99999",
            "http://[::1",
            "http://local host",
        ] {
            assert!(
                matches!(check(u, true), Err(PandoSettingsError::BadUrl { .. })),
                "{u}"
            );
        }
        let err = check("http://user:secret@localhost", true).unwrap_err();
        let text = err.to_string();
        assert!(!text.contains("secret"), "{text}");
    }

    #[test]
    fn defaults_are_off_managed_and_valid() {
        let s = PandoSettings::default();
        assert!(!s.enabled);
        assert_eq!(s.mode, PandoMode::Managed);
        assert!(!s.is_active());
        assert!(s.validate().is_ok());
        assert!(!s.has_consent("/g"));
    }

    #[test]
    fn validate_checks_external_urls_and_skips_off() {
        let mut s = PandoSettings {
            enabled: true,
            mode: PandoMode::External,
            rest_url: "http://example.com".into(),
            ..PandoSettings::default()
        };
        assert!(s.validate().is_err());
        s.mode = PandoMode::Off;
        assert!(s.validate().is_ok());
        s.mode = PandoMode::External;
        s.allow_remote = true;
        assert!(s.validate().is_err(), "remote http still refused");
        s.rest_url = "https://example.com".into();
        assert!(s.validate().is_ok());
    }

    #[test]
    fn features_default_on_when_active() {
        let mut s = PandoSettings::default();
        assert!(!s.feature_enabled(PandoFeature::AgentChat));
        s.enabled = true;
        assert!(s.feature_enabled(PandoFeature::AgentChat));
        s.features.insert(PandoFeature::AgentChat, false);
        assert!(!s.feature_enabled(PandoFeature::AgentChat));
        assert!(s.feature_enabled(PandoFeature::SemanticSearch));
    }

    #[test]
    fn consent_roundtrip_keeps_exclusions() {
        let mut s = PandoSettings::default();
        s.grant_consent("/g", 5);
        s.graphs
            .entry("/g".into())
            .or_default()
            .exclusions
            .push("pages/private".into());
        assert!(s.has_consent("/g"));
        s.revoke_consent("/g");
        assert!(!s.has_consent("/g"));
        assert_eq!(s.consent("/g").exclusions, vec!["pages/private"]);
    }

    #[test]
    fn json_roundtrip_and_partial_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("pando.json");
        assert_eq!(
            PandoSettings::load(&path).unwrap(),
            PandoSettings::default()
        );
        let mut s = PandoSettings {
            enabled: true,
            mode: PandoMode::External,
            ..PandoSettings::default()
        };
        s.profiles
            .insert(PandoFeature::AgentChat, "bitacora-chat".into());
        s.grant_consent("/g", 1);
        s.save(&path).unwrap();
        assert_eq!(PandoSettings::load(&path).unwrap(), s);
        // Missing keys fall back to defaults.
        std::fs::write(&path, r#"{"enabled": true, "mode": "off"}"#).unwrap();
        let p = PandoSettings::load(&path).unwrap();
        assert!(p.enabled && p.mode == PandoMode::Off);
        assert_eq!(p.rest_url, DEFAULT_REST_URL);
        std::fs::write(&path, "nope").unwrap();
        assert!(matches!(
            PandoSettings::load(&path),
            Err(PandoSettingsError::Parse { .. })
        ));
    }

    #[test]
    fn serialized_settings_contain_no_token_field() {
        let json = serde_json::to_string(&PandoSettings::default()).unwrap();
        assert!(!json.to_ascii_lowercase().contains("token"));
    }
}
