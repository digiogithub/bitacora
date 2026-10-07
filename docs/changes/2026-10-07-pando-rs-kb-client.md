---
created_at: 2026-10-07T10:30:00Z
updated_at: 2026-10-07T10:30:00Z
tags:
    - change
    - pando
    - sdk
---
# pando-rs crate scaffolding and REST KB client (BIT-US-0129)

Implements BIT-T-0407 and BIT-T-0408 of [[bitacora-v2-plan]] (D1), story BIT-US-0129. Owner override: the SDK lives in this workspace at `crates/pando-rs` until it moves to Pando `sdk/rust/` (ADR-027).

## What changed
- New crate `crates/pando-rs` (package `pando-rs`, lib `pando`, no bitacora deps). Modules: `config` (`PandoConfig`, redacted `Token`), `error` (`Error`: NotConfigured, Unauthorized, Unreachable, Timeout, ReindexRunning, Server, Decode, Config), `client` (`PandoClient`, `ServerInfo`, `info()` via `/health`), `kb` (`KbClient`: `upsert`, `delete`, `search`, `reindex`, `embedding_models`, `test_embedding`). The AG-UI client (BIT-US-0130) is intended as a sibling `agui` module reusing `PandoClient`/`Error`.
- Root `Cargo.toml`: workspace deps `pando-rs` and `reqwest 0.13` (json, query, rustls; already in the lock via gix). `docs/architecture.md`: crate-map row and ADR-027.
- README holds the own-types vs `ag-ui-core` decision note.

## Verified against Pando (read only)
`internal/api/handlers_remembrances_kb.go`, `handlers_remembrances_search.go`, `handlers_embedding_models.go`, `handlers_remembrances.go`, `server.go` (`X-Pando-Token`, 401), `handlers_base.go` (`/health` carries `version`; there is no `/info` route). 503 = store or mirror not configured, 409 = reindex running.

## Verification
`cargo clippy -p pando-rs --all-targets --locked -- -D warnings` clean; `cargo test -p pando-rs --locked`: 14 passed (axum mock server: each route, 401, 503, 409, 500, timeout, refused connection, unknown-field tolerance, token redaction); `cargo deny check` ok; `cargo machete` ok; xtask tests 19 passed.
