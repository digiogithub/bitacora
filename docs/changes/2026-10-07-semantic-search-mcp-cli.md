---
created_at: 2026-10-07T14:00:00Z
updated_at: 2026-10-07T14:00:00Z
tags:
    - change
    - mcp
    - cli
    - pando
---
# Semantic search over MCP and CLI (BIT-US-0146)

Plan: [[bitacora-v2-plan]]. Design: [[mcp-server]], [[semantic-search]], [[pando-integration]]. Story BIT-US-0146 (tasks BIT-T-0450, BIT-T-0451), requirements BIT-SP-0010.R5 and R6.

## What changed
- `bitacora-mcp`: new `semantic.rs` (`SemanticProvider`, `SemanticMatch`, `SemanticFailure`, tools `semantic_search` and `related_blocks`), `McpServer::set_semantic_provider`, `Services::semantic`, error codes `SEMANTIC_DISABLED` / `SEMANTIC_UNAVAILABLE`. Candidates are resolved through `Services::reader_for(token)`, so the `pando` token (and any token with `ReadExclusions`) never receives excluded blocks.
- `bitacora-runtime`: `mcp_semantic.rs` (`HybridProvider` over `HybridSearch`, installed in `Session::open`), `pando_settings.rs` (`default_pando_settings_path`, `load_pando_settings`, `pando_options_from_file`, file `<config_dir>/pando.json`).
- `bitacora-cli`: `semantic status|resync|purge|search` (`cmd/semantic.rs`, exit 3 when not enabled or not connected), `serve --pando-settings` builds `PandoOptions` from `pando.json` so semantic sync runs headless, `doctor` gains a Pando/semantic section (settings only; text and JSON).
- Docs: `docs/design/mcp-server.md` (tools table + implementation notes), `docs/design/pando-integration.md` (settings file location).

## Why
External agents and headless users need semantic search; read exclusions of the dedicated Pando token must hold for semantic results too.

## Verification
`cargo clippy -p bitacora-mcp -p bitacora-runtime -p bitacora-cli --all-targets --locked -- -D warnings` clean; `cargo test` for the same crates passes, including `crates/bitacora-mcp/tests/semantic_tools.rs` (disabled and unavailable errors, exclusion filtering for the `pando` token vs an unrestricted token), CLI parse and eligibility tests, and the runtime settings-file test. `semantic resync/purge` against a live Pando were not exercised (needs a real or mock server end to end).
