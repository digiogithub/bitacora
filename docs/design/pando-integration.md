---
created_at: 2026-10-07T12:00:00Z
updated_at: 2026-10-07T12:00:00Z
tags:
    - design
    - pando
---
# Pando integration

Design of how Bitacora talks to [Pando](https://github.com/digiogithub/pando) (semantic search over the graph, agent chat, agent memory). Plan: [[bitacora-v2-plan]] (D1-D3). Decisions: ADR-027 (generic `pando-rs` SDK), ADR-028 (crate placement), ADR-029 (opt-in, managed by default, machine-local settings) in [[architecture]]. Pando is never changed for Bitacora: only its generic APIs are used.

## 1. Crates and dependency direction

```
bitacora-config (PandoSettings)  \
pando-rs (lib `pando`, no bitacora deps) --> bitacora-pando --> bitacora-runtime --> {app, cli}
bitacora-core / bitacora-index (types and read API, as sync stories need them) /
```

`bitacora-core` never depends on `pando-rs`, `bitacora-pando` or tokio; `cargo xtask check-deps` enforces it. `bitacora-pando` holds the async code; the rest of the session stays synchronous.

## 2. Settings (machine-local)

`bitacora_config::PandoSettings`, stored as JSON in the platform config directory (`PandoSettings::load` / `save`, atomic write), never in a graph or git (cf. ADR-019). Fields: `enabled` (default `false`), `mode` (`managed` default, `external`, `off`), `rest_url`, `agui_url`, `allow_remote`, `profiles` and `features` per `PandoFeature` (`semantic_search`, `agent_chat`, `mcp_bridge`; a missing switch means on), `graphs[<canonical path>]` = `GraphConsent { granted, granted_at, exclusions }`.

URL policy (`validate_pando_url`): `http`/`https` only, no credentials in the URL, loopback (`localhost`, `127.0.0.0/8`, `::1`) always allowed; anything else needs `allow_remote` **and** `https`. Managed endpoints are always loopback regardless of `allow_remote`.

## 3. Credentials

`bitacora_pando::PandoCredentials`: tokens are never in the settings file. Per kind (`Rest`, `Agui`) the order is environment (`BITACORA_PANDO_REST_TOKEN`, `BITACORA_PANDO_AGUI_TOKEN`) then OS keychain (service `bitacora`, accounts `pando/rest`, `pando/agui`; feature `keyring-store`, on by default). The environment value is never written back. `pando::Token` and all option structs have redacted `Debug`; a test captures TRACE logs and asserts the secret never appears.

## 4. Service lifecycle

`PandoService::start(PandoOptions)` (called by `Session::open` when `RuntimeConfig::pando` is set; never fails the open):

1. not `enabled`, or mode `off` -> `PandoStatus::Off`, nothing started;
2. no consent for this graph -> `ConsentRequired`, nothing contacted;
3. invalid URLs -> `Unavailable { reason }`;
4. otherwise a 1-worker tokio runtime resolves the endpoints (external: settings + keychain; managed: `Supervisor::ensure_running`), builds a `PandoClient`, and probes `GET /health` every `probe_interval` (15 s), publishing `Connected { version }` / `Unavailable { reason }` on change.

Events (`PandoEvent`: `Status`, `SyncProgress`, `Run`) go to `std::sync::mpsc` receivers from `Session::pando_events()` (first message is the current status). `PandoService::sink()` lets the sync and chat stories publish progress and run events; `client()`, `endpoints()` and `handle()` give them the REST client, AG-UI URL/token and the runtime to spawn on.

Shutdown: `Session::shutdown` stops Pando first (it reads core and the index), then sync, core, watcher, index, MCP. `PandoService::stop(budget)` signals the probe, calls `Supervisor::stop` when managed mode started one, and shuts the runtime down within the budget.

## 5. Managed mode seam

`Supervisor` (`ensure_running(graph) -> ManagedEndpoint`, `stop(graph)`) is the only thing BIT-US-0141 has to implement: per-graph cache dir, generated `.pando.toml`, process spawn and readiness. Without a supervisor managed mode reports `Unavailable("managed Pando is not available in this build; use external mode")`.

## Requirements

- MUST keep Pando settings and secrets out of graphs and git.
- MUST NOT contact Pando before the user consents for the graph.
- MUST NOT let a Pando failure prevent a graph from opening.
- SHOULD keep every Pando type out of `bitacora-core`.

## Open questions

- Settings file location and the settings page wiring (`<config_dir>/pando.json` is proposed) belong to the app/UI stories.
- Whether consent should be re-asked when the Pando server identity changes (remote servers).
