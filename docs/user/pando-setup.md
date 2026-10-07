# Setting up Pando

Pando is an optional local AI agent server. Bitacora uses it for semantic search and the assistant. Without Pando, Bitacora works exactly as before and search falls back to keywords.

Everything is in Settings > Pando. Settings are machine-local (`pando.json`), never stored in your graph. Tokens live in the OS keychain.

## Modes
- **Managed:** Bitacora starts and supervises a private `pando serve` for you. It needs the `pando` binary, version 1.2.0 or newer, on your PATH (or its path set in the page). It restarts after crashes and stops when you quit. The page shows its state with Restart and Open log. On Windows managed mode is not available yet; use external.
- **External:** Bitacora connects to a Pando you run yourself (REST and AG-UI URLs, tokens). Remote servers need "Allow remote".
- **Off:** no connection.

"Test connection" reports the version and warns when it is older than the minimum.

### Shared knowledge base note
A managed instance shares **your own** Pando knowledge base only when your global Pando config (`~/.config/pando/.pando.toml`) sets an **absolute** `Data.Directory`. With the default relative `.pando`, the managed instance keeps a private database in Bitacora's cache directory: semantic search works, but your own Pando does not see Bitacora's indexed blocks. The settings page tells you which case applies. If you prefer a single Pando process, use external mode.

## Consent (per graph)
Enabling Pando sends nothing by itself. For each graph you grant consent in a dialog that explains: indexed blocks go into Pando's knowledge base (agent memory), readable by any agent on that Pando; `private:: true` pages and your exclusions are never sent; agent writes are a separate opt-in. Revoking consent stops sending at once and can remove what was already sent.

## Exclusions
Per graph, list page names, namespaces, path prefixes or `#tags` to keep away from Pando. `private:: true` blocks and pages are always excluded. Exclusions apply immediately to semantic sync, chat context, compose, and to what Pando's agents can read back from Bitacora over MCP.

## Features and agent writes
Switches for semantic search, chat, the MCP bridge, journal review and recommendations (plus the optional daily review and auto-recommend). "Agent writes" is off by default; even when on, every edit still needs your explicit approval (see [[ai-features]]).

## Status and activity log
The top bar and sidebar footer show Pando's state (connected, offline, unauthorized, too old...) and what degrades: search falls back to keywords; editing, sync and MCP are unaffected. "Pando activity" lists what was sent (batches of blocks, chat runs, approvals, applied edits) and can be cleared. It is stored locally.

## Privacy summary
- Off by default; nothing leaves your machine until you enable Pando and grant consent for a graph.
- The managed Pando listens on 127.0.0.1 only. Bitacora's MCP server stays loopback-only with a bearer token; Pando gets a dedicated read-only token, filtered by your exclusions.
- Your model provider keys stay in your own Pando config; Bitacora never copies them.
- Reviews and recommendations are cached in a machine-local file, never written into your graph.
- Pando itself may send text to the model provider you configured in Pando; check that provider's terms.
