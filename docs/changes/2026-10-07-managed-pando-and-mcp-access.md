---
created_at: 2026-10-07T13:30:00Z
updated_at: 2026-10-07T13:30:00Z
tags:
    - change
    - pando
    - mcp
    - managed
---
# Managed Pando mode and MCP access for Pando agents

Stories BIT-US-0141 (BIT-T-0437, BIT-T-0488, BIT-T-0489) and BIT-US-0139 (BIT-T-0433, BIT-T-0434). Plan [[bitacora-v2-plan]] D3/D5, design [[pando-integration]], decision ADR-031 in [[architecture]]. Continues [[changes/2026-10-07-bitacora-pando-crate-and-settings.md]].

## What changed
- `bitacora-pando::managed` (new): `ManagedSupervisor` (the real `Supervisor`), `instance` (per-graph cache dir `<cache>/pando/<key>/` 0700, atomic writes, `state.json` `ManagedStatus`, redacted rotated `LogSink`), `config` (generated `.pando.toml` + persona files: AG-UI listener, profiles `bitacora-chat`, `-journal-reviewer`, `-recommender`, `-writer` with read-only tool allow-lists, `[MCPServers.bitacora]`; never `[Data]`/`[Remembrances]`; `external_config_snippet`), `supervise` (spawn `pando serve --agui-port`, process group, `PR_SET_PDEATHSIG`, readiness over the Pando CA, health, crash backoff, `Failed` until `restart`, orphan reaping, adoption of an instance held by another process, version check), `sys` (the only `unsafe`: libc calls, lifeline watchdog `run_watchdog`).
- `Supervisor` trait gained `set_mcp_access`, `managed_status`, `restart`, `log_path` (defaults); `ManagedEndpoint` gained `ca_pem`; `PandoOptions` gained `mcp`; `PandoService` gained `managed_status/restart_managed/managed_log_path` and re-resolves the endpoint when a managed probe fails.
- `pando-rs`: `PandoConfig::with_root_certificate_pem` (trust the private CA of `pando serve`), `PandoClient::fetch_api_token` (`GET /api/v1/token`).
- `bitacora-config`: `GraphConsent::agent_writes`, `PandoSettings::binary`.
- `bitacora-mcp`: `PANDO_TOKEN_NAME`, `TokenStore::ensure_token`, `ReadExclusions`/`FilteredReader` (`exclusion.rs`), `McpServer::set_read_exclusions`; `handler.rs`/`compat.rs` read through `Services::reader_for(token)`.
- `bitacora-runtime` (`live.rs`): `provision_pando_mcp` (token, exclusions, endpoint) and `default_supervisor`.
- Docs: `docs/design/pando-integration.md` sections 5.1-5.5 (incl. the shared-KB spike), ADR-031 row in `docs/architecture.md`.

## Why
Managed mode gives a zero-setup Pando without any Pando change; the dedicated Read-only `pando` token with consent exclusions keeps agent access least-privilege, audited and consented (ADR-031).

## Findings (real Pando v1.2.11)
- `pando serve` is HTTPS-only, CA at `<config>/pando/tls/ca.crt`, API token from loopback `GET /api/v1/token`; the AG-UI listener (`--agui-port`) uses the same cert/token. The generated config is accepted (the four profiles are listed by `/api/v1/agui/info`) and Pando connected to Bitacora's MCP and listed only the read tools.
- Shared KB: `pando.db` lives in `Data.Directory` (default relative `.pando`); a document upserted by one instance was found by a second instance in another directory when the global config sets an absolute `Data.Directory` (details in the design doc).

## Verification
- `cargo clippy -p pando-rs -p bitacora-pando -p bitacora-config -p bitacora-mcp -p bitacora-runtime -p bitacora-cli --all-targets --locked -- -D warnings` clean; `cargo test` of the same crates green; `cargo check -p bitacora-app --all-targets`, `cargo xtask check-deps`, `cargo deny check`, `cargo fmt --all --check` clean.
- Tests: `crates/bitacora-pando/tests/managed.rs` (fake `pando` script: readiness, generated config and private files, redacted log, crash and restart, failed state and restart, no orphan after stop/drop, version/binary errors, adoption, orphan reaping, watchdog), `src/managed/{instance,config}.rs` unit tests, `service.rs` (follows a restarted instance, registers MCP), `bitacora-mcp/tests/pando_token.rs` + `tokens.rs` (exclusions across search/page/backlinks/block/list/resources, writes rejected, per-token), `bitacora-runtime/tests/pando.rs` (token minted Read-only, Write on grant, nothing when inactive), `pando-rs/tests/local_server.rs`.
- Opt-in with a real `pando` (ignored): `cargo test -p bitacora-pando --test managed -- --ignored` and `cargo test -p bitacora-runtime --test pando_real -- --ignored`; both passed locally.
