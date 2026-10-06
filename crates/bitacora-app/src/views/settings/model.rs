//! Settings keys, how each takes effect, and the small parsers behind the form fields.

use bitacora_mcp::Scope;

pub use super::graph_config::ApplyMode;

/// An app-level setting (stored in `settings.json`; graph keys are [`super::GraphEdit`]s).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppKey {
    /// `search.substring`: the trigram block index.
    SearchSubstring,
    /// `mcp.enabled`.
    McpEnabled,
    /// `mcp.port`.
    McpPort,
    /// `mcp.allow_writes`.
    McpAllowWrites,
    /// `mcp.allow_deletes`.
    McpAllowDeletes,
    /// `mcp.api_enabled`.
    McpApi,
    /// `mcp.allowed_origins`.
    McpOrigins,
    /// `mcp.protected_namespaces`.
    McpProtectedNamespaces,
    /// `mcp.writes_per_minute`.
    McpRate,
    /// Light / dark / system.
    ThemeMode,
    /// Bundled theme per mode.
    ThemeName,
    /// UI font size.
    FontSize,
    /// Keep running (and serving MCP) when the last window closes.
    KeepRunning,
    /// Automatic update checks.
    UpdatesEnabled,
    /// Update channel.
    UpdatesChannel,
    /// Sync timing (commit idle / max, fetch interval, squash).
    SyncTiming,
}

impl AppKey {
    /// When a change of this setting takes effect.
    pub fn apply_mode(self) -> ApplyMode {
        match self {
            Self::SearchSubstring => ApplyMode::ReindexRequired,
            Self::McpEnabled
            | Self::McpPort
            | Self::McpApi
            | Self::McpOrigins
            | Self::McpRate
            | Self::SyncTiming => ApplyMode::RestartRequired,
            Self::McpAllowWrites
            | Self::McpAllowDeletes
            | Self::McpProtectedNamespaces
            | Self::ThemeMode
            | Self::ThemeName
            | Self::FontSize
            | Self::KeepRunning
            | Self::UpdatesEnabled
            | Self::UpdatesChannel => ApplyMode::Live,
        }
    }
}

/// Splits a comma or line separated list, trimming, dropping empties and duplicates (order kept).
pub fn parse_list(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for item in text.split([',', '\n']) {
        let item = item.trim();
        if !item.is_empty() && !out.iter().any(|o| o == item) {
            out.push(item.to_owned());
        }
    }
    out
}

/// Checks a browser origin (`scheme://host[:port]`, no path); `null` is never accepted.
///
/// # Errors
/// A user-presentable message.
pub fn validate_origin(origin: &str) -> Result<(), String> {
    let rest = origin
        .strip_prefix("http://")
        .or_else(|| origin.strip_prefix("https://"))
        .ok_or_else(|| format!("`{origin}` must start with http:// or https://"))?;
    if rest.is_empty() || rest.contains('/') || rest.contains(' ') {
        return Err(format!(
            "`{origin}` must be scheme://host[:port] without a path"
        ));
    }
    Ok(())
}

/// Parses the rate limit field (writes per minute and token).
///
/// # Errors
/// A user-presentable message.
pub fn parse_rate(text: &str) -> Result<usize, String> {
    match text.trim().parse::<usize>() {
        Ok(n) if (1..=10_000).contains(&n) => Ok(n),
        _ => Err("enter a number of writes per minute between 1 and 10000".to_owned()),
    }
}

/// Validates a token name: 1-64 characters of letters, digits, `-`, `_`, `.`, ` `.
///
/// # Errors
/// A user-presentable message.
pub fn validate_token_name(name: &str) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("give the token a name (the client that will use it)".to_owned());
    }
    if name.chars().count() > 64
        || !name
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.' | ' '))
    {
        return Err(
            "use letters, digits, spaces, `-`, `_` or `.` (up to 64 characters)".to_owned(),
        );
    }
    Ok(())
}

/// Scope names for display.
pub fn scope_label(scope: Scope) -> &'static str {
    match scope {
        Scope::Read => "read",
        Scope::Write => "write",
        Scope::Delete => "delete",
    }
}

/// Scopes for a new token: read always, plus write and delete when asked.
pub fn scopes_for(write: bool, delete: bool) -> Vec<Scope> {
    let mut scopes = vec![Scope::Read];
    if write {
        scopes.push(Scope::Write);
    }
    if delete {
        scopes.push(Scope::Delete);
    }
    scopes
}

/// JSON client entry for Claude Desktop / Claude Code (`mcpServers`), with the token inline.
pub fn client_config_json(endpoint: &str, secret: &str) -> String {
    let value = serde_json::json!({
        "mcpServers": {
            "bitacora": {
                "type": "http",
                "url": endpoint,
                "headers": { "Authorization": format!("Bearer {secret}") }
            }
        }
    });
    serde_json::to_string_pretty(&value).unwrap_or_default()
}

/// The `claude mcp add` command that registers the server with Claude Code.
pub fn client_config_command(endpoint: &str, secret: &str) -> String {
    format!(
        "claude mcp add --transport http bitacora {endpoint} --header \"Authorization: Bearer {secret}\""
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn substring_reports_reindex_required_and_mcp_endpoint_keys_a_restart() {
        assert_eq!(
            AppKey::SearchSubstring.apply_mode(),
            ApplyMode::ReindexRequired
        );
        assert_eq!(AppKey::McpApi.apply_mode(), ApplyMode::RestartRequired);
        assert_eq!(AppKey::McpAllowWrites.apply_mode(), ApplyMode::Live);
        assert_eq!(AppKey::ThemeMode.apply_mode(), ApplyMode::Live);
    }

    #[test]
    fn list_parsing_trims_and_dedupes() {
        assert_eq!(
            parse_list(" a, b\nc ,, a ,"),
            vec!["a".to_owned(), "b".to_owned(), "c".to_owned()]
        );
        assert!(parse_list("  ").is_empty());
    }

    #[test]
    fn origins_rates_and_names_are_validated() {
        assert!(validate_origin("http://localhost:3000").is_ok());
        assert!(validate_origin("https://app.example").is_ok());
        assert!(validate_origin("null").is_err());
        assert!(validate_origin("ftp://x").is_err());
        assert!(validate_origin("http://x/path").is_err());
        assert!(validate_origin("http://").is_err());
        assert_eq!(parse_rate(" 30 "), Ok(30));
        assert!(parse_rate("0").is_err());
        assert!(parse_rate("abc").is_err());
        assert!(validate_token_name("Claude Desktop").is_ok());
        assert!(validate_token_name("").is_err());
        assert!(validate_token_name("bad/name").is_err());
    }

    #[test]
    fn scopes_always_include_read() {
        assert_eq!(scopes_for(false, false), vec![Scope::Read]);
        assert_eq!(
            scopes_for(true, true),
            vec![Scope::Read, Scope::Write, Scope::Delete]
        );
    }

    #[test]
    fn config_snippets_carry_url_and_bearer() {
        let json = client_config_json("http://127.0.0.1:12316/mcp", "bit_abc");
        let value: serde_json::Value = serde_json::from_str(&json).expect("json");
        let server = &value["mcpServers"]["bitacora"];
        assert_eq!(server["type"], "http");
        assert_eq!(server["url"], "http://127.0.0.1:12316/mcp");
        assert_eq!(server["headers"]["Authorization"], "Bearer bit_abc");
        let cmd = client_config_command("http://127.0.0.1:12316/mcp", "bit_abc");
        assert!(cmd.contains("--transport http") && cmd.contains("Bearer bit_abc"));
    }
}
