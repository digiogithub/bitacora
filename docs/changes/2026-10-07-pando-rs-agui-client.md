---
created_at: 2026-10-07T12:00:00Z
updated_at: 2026-10-07T12:00:00Z
tags:
    - change
    - pando
    - sdk
    - agui
---
# pando-rs AG-UI client: events, runs, threads and interrupts (BIT-US-0130)

Implements BIT-T-0409..0412 of [[bitacora-v2-plan]] (D1), story BIT-US-0130, continuing [[2026-10-07-pando-rs-kb-client]]. Own tolerant types, not `ag-ui-core` (decision in the crate README).

## What changed
- New module `crates/pando-rs/src/agui/`:
  - `types.rs`: `Event` (tagged enum, 21 modelled types + `Event::Unknown{event_type,data}` for unmodelled or misshapen events), `RunInput`, `Message`/`MessageContent` (string or parts), `Tool`, `ContextEntry`, `PatchOp`, `Info`, `Health`, `ThreadsPage`, `new_id`.
  - `sse.rs`: `SseParser` (chunk-boundary safe, comments, multi-line data, `[DONE]`).
  - `client.rs`: `AguiClient` (`info`, `healthz`, `run`/`run_agent`/`run_text`, `attach`, `cancel_run`, `list_threads`, `thread_messages`, `delete_thread`), `AguiOptions` (dedicated listener URL, path, agent, own bearer token), `RunStream::next()`.
  - `thread.rs`: `Thread`/`ThreadRun` reducer (transcript resend, text/tool-call folding, state snapshot+delta, pending tool calls, interrupts), `RunOutcome`.
  - `patch.rs`: RFC 6902 applier for `STATE_DELTA`. `hitl.rs`: `Interrupt::classify`, `approve`/`deny`/`answer_question`/`cancel_question`, `PermissionRequest`, `QuestionRequest`.
- `PandoClient::agui()` / `agui_with(AguiOptions)`; `PandoConfig::stream_idle_timeout` (default 90 s); streams use a second reqwest client with only a read timeout (the REST client's overall timeout would kill long runs). New `Error::Protocol` and `Error::Run{code,message}`.
- README: AG-UI scope, auth difference, example.

## Decisions and gaps
- Auth is `Authorization: Bearer` for AG-UI (Pando `authorize`), unlike REST `X-Pando-Token`; no Origin header is sent.
- `RunStream` exposes `async next()` instead of `futures::Stream` to avoid a new dependency.
- `MESSAGES_SNAPSHOT` is stored in `Thread::last_snapshot`, not merged (client and server message ids differ); `Thread::load_history()` adopts the server transcript explicitly.
- No automatic reconnect (use `attach`). No Pando server change needed.

## Verified against Pando (read only)
`internal/agui/{events,input,threads,hitl,server,sse,doc}.go`; TS SDK `sdk/typescript/src/agui/{client,thread,hitl}.ts`.

## Verification
`cargo clippy -p pando-rs --all-targets --locked -- -D warnings` clean; `cargo test -p pando-rs --locked`: 8 unit + 11 `agui_client` (axum SSE mock plus raw-TCP chunking/idle-timeout server) + 14 `kb_client` + 1 doctest pass; `cargo deny check` ok. No new dependencies.
