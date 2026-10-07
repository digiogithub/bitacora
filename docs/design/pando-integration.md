---
created_at: 2026-10-07T12:00:00Z
updated_at: 2026-10-07T12:00:00Z
tags:
    - design
    - pando
---
# Pando integration

Design of how Bitacora talks to [Pando](https://github.com/digiogithub/pando) (semantic search over the graph, agent chat, agent memory). Plan: [[bitacora-v2-plan]] (D1-D3). Decisions: ADR-027 (generic `pando-rs` SDK), ADR-028 (crate placement), ADR-029 (opt-in, managed by default, machine-local settings), ADR-031 (agents read through a least-privilege MCP token; supervised managed Pando) in [[architecture]]. Pando is never changed for Bitacora: only its generic APIs are used. The AI agents backend (chat, approvals, journal review, recommender) is in [[ai-agents]].

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

### 2.1 Settings page and consent (BIT-US-0137, BIT-US-0138)

Settings window > Pando (`views/settings/pando.rs`) edits this file: status chip, mode, URLs, keychain tokens (never shown, only their source), Test connection (`bitacora_pando::test_connection`: version and minimum check), the managed instance (state, Restart, Open log, and whether it shares the user's KB, from `kb_sharing`), feature switches, and per graph the consent dialog, the `agent_writes` switch and the exclusions editor (page, namespace, `folder/` or `#tag`). Saving writes only `pando.json`. Changes that alter how the session starts reopen the graph; exclusions and revoked consent go to the running session through `Session::apply_pando_consent` (semantic `ContentPolicy` plus the `pando` token's `ReadExclusions`; revoke uses `ContentPolicy::denying_all()` and may purge). `SessionOptions::pando_settings_path` makes the app build `PandoOptions` with `pando_options_from_file` at every open, and the Agent tab is marked configured when Pando is active, chat is on and the graph is consented.

## 3. Credentials

`bitacora_pando::PandoCredentials`: tokens are never in the settings file. Per kind (`Rest`, `Agui`) the order is environment (`BITACORA_PANDO_REST_TOKEN`, `BITACORA_PANDO_AGUI_TOKEN`) then OS keychain (service `bitacora`, accounts `pando/rest`, `pando/agui`; feature `keyring-store`, on by default). The environment value is never written back. `pando::Token` and all option structs have redacted `Debug`; a test captures TRACE logs and asserts the secret never appears.

## 4. Service lifecycle

`PandoService::start(PandoOptions)` (called by `Session::open` when `RuntimeConfig::pando` is set; never fails the open):

1. not `enabled`, or mode `off` -> `PandoStatus::Off`, nothing started;
2. no consent for this graph -> `ConsentRequired`, nothing contacted;
3. invalid URLs -> `Unavailable { reason }`;
4. otherwise a 1-worker tokio runtime resolves the endpoints (external: settings + keychain; managed: `Supervisor::ensure_running`), builds a `PandoClient`, and probes `GET /health` every `probe_interval` (15 s), publishing `Connected { version }` / `Unavailable { reason }` on change.

Events (`PandoEvent`: `Status`, `SyncProgress`, `Run`) go to `std::sync::mpsc` receivers from `Session::pando_events()` (first message is the current status). `PandoService::sink()` lets the sync and chat stories publish progress and run events; `client()`, `endpoints()` and `handle()` give them the REST client, AG-UI URL/token and the runtime to spawn on.

### Status machine, degradation and activity log (BIT-US-0140)

`PandoStatus` also has `Unauthorized` (the server rejected the token) and `TooOld { version, min }` (below `DEFAULT_MIN_VERSION`). The probe loop backs off while the server is down (`probe_delay`: the interval doubles per consecutive failure up to 8x, i.e. two minutes by default) and returns to the base interval after one success. The app derives the user-facing `PandoState` (`views::pando_status`: Disabled / NotConfigured / Connecting / Ok / Unauthorized / Unreachable / TooOld) from the settings, the graph consent and the polled live status (every 3 s through `SessionHandle::run`); it feeds the sidebar footer row, the top-bar "Pando" control with its popover (status chip, what is degraded, per-graph feature switches, link to the settings, activity log) and the settings chip. Anything but `Ok` degrades gracefully: semantic search returns lexical results only, the assistant is paused, and editing, sync and MCP never depend on Pando (`pando_status.degraded.*` strings say so).

The **activity log** is a machine-local, bounded (500 entries, compacted at 600), clearable JSONL file `pando-activity.jsonl` next to `pando.json` (`bitacora_pando::ActivityLog`; never inside the graph). `EventSink::set_log` attaches it: status changes and `PandoEvent::Activity` entries are written when emitted. Writers: the semantic worker (one `Sync` entry per acknowledged batch: count plus up to 20 block uuids, `upserted` / `deleted`) and chat sessions (`ChatDeps::activity`: run started / finished / failed / cancelled, approvals, applied edits). It stores counts, uuids and run ids only, never block text or titles. The viewer (`views::pando_activity`) merges the file with the MCP audit entries made with the `pando` token and filters by sync / agent / MCP / status; "Agent writes..." opens the existing agent activity dialog (undo).


Shutdown: `Session::shutdown` stops Pando first (it reads core and the index), then sync, core, watcher, index, MCP. `PandoService::stop(budget)` signals the probe, calls `Supervisor::stop` when managed mode started one, and shuts the runtime down within the budget.

## 5. Managed mode

`Supervisor` (`ensure_running(graph) -> ManagedEndpoint`, `stop(graph)`, plus `set_mcp_access`, `managed_status`, `restart`, `log_path`) is the seam; `bitacora_pando::ManagedSupervisor` (module `managed`) is the real implementation and `Session::open` installs it when the mode is `managed` and the caller passed none. Without any supervisor managed mode reports `Unavailable("managed Pando is not available in this build; use external mode")`. The design follows git-in-track's `internal/pando/supervisor` (ideas only, written for this crate's blocking seam).

### 5.1 Instance directory

`<machine-local cache>/pando/<graph-key>/` (`directories::ProjectDirs("es", "Digio", "Bitacora")`, mode `0700`; `<graph-key>` = sanitised folder name + 8 hex of blake3 of the full path). **Nothing is ever written inside the graph folder.**

| File | Content |
|---|---|
| `.pando.toml` | generated on every child start (atomic, `0600`) |
| `agents/personas/<profile>.md` | persona files of the four profiles |
| `token` | the instance's API token, once the child answers (`0600`) |
| `state.json` | `ManagedStatus`, rewritten atomically on every change |
| `supervisor.lock` | exclusive `flock` while a process supervises the directory |
| `pando.log` (+ `.1`, `.2`) | child stdout/stderr, tokens redacted, rotated at 10 MB |
| `.pando/` | Pando's own data when the user's config does not select another store (see 5.4) |

### 5.2 Generated `.pando.toml`

Pando merges it over the user's global `~/.config/pando/.pando.toml` (with `PANDO_CONFIG_PARENT_SEARCH=false` it is the only local file read), so model providers and API keys are never copied. Key names come from Pando's `internal/config/config.go`. It contains:

- `[AGUI]`: `Enabled`, `Host = "127.0.0.1"`, a free `Port`, `RequireToken = true`, `AllowedOrigins = []`, `FrontendTools`, `HumanInTheLoop = true`, `AutoApprove = false`, `Mesnada = false`, and a read-only `Tools` allow-list;
- `[AGUI.Profiles.<name>]` for `bitacora-chat`, `bitacora-journal-reviewer`, `bitacora-recommender`, `bitacora-writer` (Base `coder`, persona, prompt, per-profile `Tools`). Every profile gets the `bitacora_*` read tools and Pando's read-only KB tools (`kb_search_documents`, `kb_get_document`, `kb_related_documents`, `hybrid_search_remembrances`, `recall`); only `bitacora-writer` also gets the frontend tool `propose_edit`. Pando's KB write tools (`kb_add_document`, `kb_delete_document`, `remember`, `forget`) and every Bitacora write tool are never listed;
- `[ToolDiscovery]`/`[MCPGateway]` off, so MCP tools keep their `<server>_<tool>` names and the allow-lists can see them;
- `[MCPServers.bitacora]` (`streamable-http`, loopback URL, `[...Auth] Type = 'bearer'` with the `pando` token), only when the MCP server runs and the consent allows the bridge;
- **no** `[Data]` and **no** `[Remembrances]` (shared KB, 5.4).

The MCP token is in clear in this `0600` file inside the `0700` cache directory (git-in-track's `age1:` encryption needs the `pando secret` CLI and age keys; revisit if Pando exposes a keychain reference).

### 5.3 Process supervision

`pando serve --host 127.0.0.1 --port P --agui-port Q` with the instance directory as working directory, `PANDO_CONFIG_PARENT_SEARCH=false`, in its own process group. `pando serve` is HTTPS-only (its private CA lives in `<pando config dir>/tls/ca.crt`) and generates its own API token, so the supervisor trusts that CA and reads the token from the loopback-only `GET /api/v1/token`; both are handed to the REST and AG-UI clients (the AG-UI listener uses the same certificate and token). Lifecycle:

1. lock the directory (`File::try_lock`); if another Bitacora process holds it, **adopt** its running instance through `state.json` + `token` instead of spawning a second one;
2. end an orphan: a live pid in `state.json` whose working directory is the instance directory (Linux `/proc/<pid>/cwd`; elsewhere `ps` names the binary) and whose supervisor is gone;
3. `pando --version` against the minimum (`DEFAULT_MIN_VERSION = 1.2.0`, an assumption: the release that has AG-UI profiles and `/api/v1/token`); a missing binary or old version is a `Failed` state with a message for the user (inside Flatpak the message says the host binary must be reachable);
4. per run: prefer the previous ports when free (agents that cached them keep working), write config, spawn; Linux `PR_SET_PDEATHSIG` (the supervisor thread that spawned the child outlives it), other unix hosts run the child under the lifeline watchdog `run_watchdog_fd3` when `ManagedOptions::watchdog` names the binary's hidden subcommand (not wired in app/cli yet, macOS unverified);
5. ready when `GET /health` answers over the CA within `ready_timeout` (30 s); then health every 30 s, 3 consecutive failures count as a crash;
6. a crash restarts with backoff 1 s doubling to 60 s; 5 crashes in 10 minutes mark the instance `Failed` until `restart`; `restart` also bounces a healthy child without counting a crash;
7. `stop`: SIGTERM to the process group, SIGKILL after 10 s; the runtime calls it from `Session::shutdown`.

`PandoService` re-asks the supervisor when a probe fails in managed mode, so a restarted child on a new port or with a new token replaces the client. `PandoService::managed_status()`, `restart_managed()` and `managed_log_path()` back the settings page (mode, status, Restart, Open log; the page itself belongs to the app stories). Windows has no Job Object yet: managed mode answers `Unavailable` there.

### 5.4 Shared KB (spike BIT-T-0489)

Findings, with Pando v1.2.11 source and an experiment on the real binary:

- The KB and remembrances live in `<Data.Directory>/pando.db` (`internal/db/connect.go:27`); `Data.Directory` defaults to `.pando` **relative to the working directory** (`internal/config/config.go:2513`). The KB path (`[Remembrances] KBPath`, `internal/app/remembrances.go:123`) only selects a folder mirrored into that database.
- Pando's IPC primary/secondary election is per working directory (`<cwd>/.pando/ipc.lock`, `internal/ipc/lock_common.go:35`), so an instance started in the cache directory is the primary of its own directory; it does **not** coordinate with the user's own Pando.
- Experiment (two `pando serve` processes in different directories, one global config with `[Data] Directory = '<abs>'`): a document upserted through instance A (`POST /api/v1/remembrances/kb/documents`, `file_path = bitacora/test/page.md`) was returned by instance B's `POST /api/v1/remembrances/kb/search` with `path_prefix = "bitacora/"`. Both processes wrote the same SQLite file in WAL mode without errors.
- **Decision:** the generated config never sets `[Data]` or `[Remembrances]`. The managed instance therefore shares the user's KB exactly when the user's global Pando configuration selects an **absolute** `Data.Directory`; with the default relative `.pando` it gets a private database under the cache directory (semantic search still works, but agent memory is not shared with the user's own Pando). The settings page should say which of the two applies (read the global config; semantic-sync stories). Two Pando primaries writing one WAL database rely on SQLite busy handling rather than Pando's IPC write proxy: acceptable for the low write rate of KB sync, to be re-checked under load.
- When the user's own Pando already runs and the user prefers one process, `external` mode connects to it (token from the keychain) instead of spawning.

### 5.5 MCP access for agents (BIT-US-0139, ADR-031)

- `Session::open` (after the MCP server started, before `PandoService::start`) calls `provision_pando_mcp` when the integration is active, the graph has consent and the `McpBridge` feature is on: `TokenStore::ensure_token("pando", [Read])` (`[Read, Write]` when `GraphConsent::agent_writes`), `McpServer::set_read_exclusions("pando", ReadExclusions::new(consent.exclusions))` and `PandoOptions::mcp = McpAccess { url, token }`.
- `FilteredReader` (`bitacora-mcp/src/exclusion.rs`) wraps the shared reader per call for tokens with exclusions: pages that are `private:: true`, match an excluded name (and its namespace children), graph-relative path prefix or `#tag` disappear from search, page/list/journal tools, blocks, backlinks, tasks, query results, resources and prompts; subscriptions get no change events and aggregate/tuple queries are refused for that token. The same entries feed semantic sync (the `ContentPolicy` of BIT-US-0138 should expose them through `ReadExclusions::new`).
- Managed mode registers the endpoint in the generated config (5.2); external mode shows `external_config_snippet(&McpAccess)` for the user's own `.pando.toml` (a UI "copy" action, since it contains the token).
- Opt-in end-to-end check against a real Pando: `cargo test -p bitacora-runtime --test pando_real -- --ignored` starts the real `pando serve` from the generated config and asserts that it discovered the `bitacora_*` read tools and none of the write tools. A model-driven call test needs a stub model provider and stays a follow-up.

## Requirements

- MUST keep Pando settings and secrets out of graphs and git.
- MUST NOT contact Pando before the user consents for the graph.
- MUST NOT let a Pando failure prevent a graph from opening.
- SHOULD keep every Pando type out of `bitacora-core`.

## Open questions

- Settings page wiring of managed mode (status, Restart, Open log, shared-KB hint) and the app/cli hidden watchdog subcommand for macOS belong to the app stories.
- Settings file: `<platform config dir>/pando.json` (`bitacora_runtime::default_pando_settings_path`, `pando_options_from_file`), read by `bitacora-cli serve`, `semantic` and `doctor` (BIT-US-0146); the settings page wiring belongs to the app/UI stories.
- Whether consent should be re-asked when the Pando server identity changes (remote servers).
