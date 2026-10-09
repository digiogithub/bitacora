//! Connection details of the MCP server (BIT-US-0186): what the sidebar footer popover offers
//! and the exact text each copy action puts on the clipboard.
//!
//! The pure functions here are the whole decision logic; the sidebar only renders the result.
//! Config snippets are built by the same [`client_snippet`] the Settings > Agents token table
//! uses, so both surfaces copy identical text for the same token. A config is never offered
//! without a token (it would not authenticate), and secrets are never logged.

use bitacora_mcp::{PANDO_TOKEN_NAME, TokenStore, TokenSummary};

use crate::views::settings::{SnippetKind, client_snippet};
use crate::views::status_bar::SlotState;

/// What a copy action puts on the clipboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpCopy {
    /// The endpoint URL alone (no token).
    Url,
    /// `mcpServers` JSON with the token inline.
    Json,
    /// The `claude mcp add` command with the token inline.
    Command,
}

/// What the popover shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McpPopover {
    /// Server running and a usable token exists: all copy actions.
    Ready {
        /// Endpoint URL.
        endpoint: String,
        /// Name of the token the config snippets use.
        token: String,
    },
    /// Server running but no usable token: only the URL can be copied; creating a token is a
    /// link to Settings > Agents.
    NoToken {
        /// Endpoint URL.
        endpoint: String,
    },
    /// The server is disabled.
    Off,
    /// The server failed to start (for example the port is in use).
    Failed,
}

/// The token the snippets use: the first one whose secret is available, skipping the internal
/// Pando bridge token.
pub fn pick_token(tokens: &[TokenSummary]) -> Option<&TokenSummary> {
    tokens
        .iter()
        .find(|t| t.available && t.name != PANDO_TOKEN_NAME)
}

/// Popover content for the footer state.
pub fn popover_state(
    mcp: SlotState,
    endpoint: Option<&str>,
    tokens: &[TokenSummary],
) -> McpPopover {
    match (mcp, endpoint) {
        (SlotState::Idle | SlotState::Busy, Some(endpoint)) => match pick_token(tokens) {
            Some(token) => McpPopover::Ready {
                endpoint: endpoint.to_owned(),
                token: token.name.clone(),
            },
            None => McpPopover::NoToken {
                endpoint: endpoint.to_owned(),
            },
        },
        (SlotState::Error, _) => McpPopover::Failed,
        _ => McpPopover::Off,
    }
}

/// The text of a copy action; `None` when it cannot be produced (no token or secret lost).
pub fn copy_text(
    kind: McpCopy,
    endpoint: &str,
    token_name: &str,
    store: Option<&TokenStore>,
) -> Option<String> {
    match kind {
        McpCopy::Url => Some(endpoint.to_owned()),
        McpCopy::Json | McpCopy::Command => {
            let secret = store?.secret_of(token_name)?;
            let kind = if kind == McpCopy::Json {
                SnippetKind::Json
            } else {
                SnippetKind::Command
            };
            Some(client_snippet(kind, endpoint, &secret))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bitacora_mcp::{Scope, TokenStorage};

    const EP: &str = "http://127.0.0.1:12316/mcp";

    fn summary(name: &str, available: bool) -> TokenSummary {
        TokenSummary {
            name: name.into(),
            scopes: vec![Scope::Read],
            created_at: None,
            storage: TokenStorage::File,
            available,
        }
    }

    #[test]
    fn running_with_a_token_is_ready_and_skips_unusable_and_internal_tokens() {
        let tokens = [
            summary(PANDO_TOKEN_NAME, true),
            summary("lost", false),
            summary("claude", true),
        ];
        assert_eq!(
            popover_state(SlotState::Idle, Some(EP), &tokens),
            McpPopover::Ready {
                endpoint: EP.into(),
                token: "claude".into()
            }
        );
    }

    #[test]
    fn running_without_a_usable_token_only_offers_the_url() {
        for tokens in [vec![], vec![summary("lost", false)]] {
            assert_eq!(
                popover_state(SlotState::Busy, Some(EP), &tokens),
                McpPopover::NoToken {
                    endpoint: EP.into()
                }
            );
        }
    }

    #[test]
    fn off_and_failed_servers_say_so() {
        let tokens = [summary("claude", true)];
        assert_eq!(
            popover_state(SlotState::Off, None, &tokens),
            McpPopover::Off
        );
        assert_eq!(
            popover_state(SlotState::Error, None, &tokens),
            McpPopover::Failed
        );
        // Running without a known endpoint is treated as off, never as ready.
        assert_eq!(
            popover_state(SlotState::Idle, None, &tokens),
            McpPopover::Off
        );
    }

    #[test]
    fn url_copy_has_no_token_and_config_copies_match_the_settings_snippets() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = TokenStore::load_or_init(dir.path().join("tokens.json")).expect("store");
        let secret = store.create("claude", &[Scope::Read]).expect("create");
        assert_eq!(
            copy_text(McpCopy::Url, EP, "claude", Some(&store)).as_deref(),
            Some(EP)
        );
        let json = copy_text(McpCopy::Json, EP, "claude", Some(&store)).expect("json");
        assert_eq!(json, client_snippet(SnippetKind::Json, EP, &secret));
        assert!(json.contains(&secret));
        let cmd = copy_text(McpCopy::Command, EP, "claude", Some(&store)).expect("cmd");
        assert_eq!(cmd, client_snippet(SnippetKind::Command, EP, &secret));
        // No store, or an unknown token: no config at all.
        assert_eq!(copy_text(McpCopy::Json, EP, "claude", None), None);
        assert_eq!(copy_text(McpCopy::Command, EP, "nope", Some(&store)), None);
    }
}
