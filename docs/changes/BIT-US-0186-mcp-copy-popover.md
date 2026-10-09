# BIT-US-0186: MCP connection popover in the sidebar footer

The "MCP server" row of the sidebar footer is now clickable (pointer cursor, hover, tooltip "Connection details") and opens a popover above it.

- Running with a usable token: endpoint (monospace) plus "Copy URL" (no token), "Copy JSON config", "Copy claude mcp add command" and "Manage tokens..." (Settings > Agents).
- Running without a usable token: URL copy only, a hint and "Create token..." linking to Settings > Agents. A config is never copied without auth.
- Server off or failed (port in use): says so and links to Settings > Agents.
- After a copy: "Copied" toast (`crate::ui::notify`) and the popover closes. Secrets are only written to the clipboard, never logged.

## Code
- `crates/bitacora-app/src/views/mcp_connect.rs` (new): pure `popover_state`, `pick_token` (first available token, skipping the Pando bridge token) and `copy_text`, with tests.
- `views/settings/agents.rs`: new `client_snippet(kind, endpoint, secret)` shared with `SettingsView::snippet`; re-exported with `SnippetKind` from `views/settings/mod.rs`. Settings and sidebar copy identical text.
- `views/sidebar.rs`: row click, `mcp_popover`, `copy_mcp`, `set_mcp_tokens`, `SidebarEvent::OpenMcpSettings`, gpui test.
- `views/workspace.rs`: handles `OpenMcpSettings` (`open_settings(Some(Section::Agents))`) and feeds the token store to the sidebar once per MCP endpoint (`sidebar_tokens_for`).
- i18n `sidebar.mcp_*` in en, es, fr, zh.

## Verification
`cargo test -p bitacora-app --locked` (674 passed), `cargo clippy -p bitacora-app --all-targets --locked -- -D warnings`, `cargo fmt --all --check`. Visual placement of the popover was not inspected on screen.
