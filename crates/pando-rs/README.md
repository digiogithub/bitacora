# pando-rs

A generic async Rust SDK for [Pando](https://github.com/digiogithub/pando). MIT licensed, built on
`reqwest` (rustls) on `tokio`. It contains no application-specific code and depends on no other crate
of this workspace, so it can move to Pando's `sdk/rust/` unchanged. The library name is `pando`.

```rust
use pando::{PandoClient, PandoConfig};
use pando::kb::{SearchRequest, UpsertDocument};

async fn demo() -> pando::Result<()> {
    let client = PandoClient::new(PandoConfig::new("http://127.0.0.1:8765").with_token("..."))?;
    client.kb().upsert(&UpsertDocument::new("notes/a.md", "# A")).await?;
    let hits = client.kb().search(&SearchRequest::new("alpha", 5)).await?;
    Ok(())
}
```

## Scope

Today: the REST knowledge-base routes (`/api/v1/remembrances/...`): upsert, delete, search,
reindex (409 -> `Error::ReindexRunning`), embedding-model list and embedder test, plus `/health`
version info. Auth is the `X-Pando-Token` header; the token is redacted in `Debug`. Responses
tolerate unknown and missing fields.

AG-UI (`client.agui()`, module `pando::agui`): `info`, `run` / `run_text` (SSE stream of tolerant
`Event`s; unknown or malformed events become `Event::Unknown`), `attach` (reattach to a live run),
`cancel_run`, `list_threads`, `thread_messages`, `delete_thread`; a `Thread` helper that resends the
transcript every turn, folds `STATE_SNAPSHOT`/`STATE_DELTA` (RFC 6902) into a state document and
tracks interrupts; and `agui::hitl` payload helpers for permission prompts and `AskUserQuestion`.
Auth differs from REST: the adapter reads `Authorization: Bearer`, not `X-Pando-Token`
(`AguiOptions` can point at a dedicated `--agui-port` listener and carry its own token). Streams use
no overall timeout, only `stream_idle_timeout` (default 90 s, server heartbeat is 15 s).

```rust
use pando::agui::{Interrupt, RunOutcome, Thread, hitl};

async fn chat(client: &pando::PandoClient) -> pando::Result<()> {
    let mut thread = Thread::new(client.agui());
    let mut run = thread.send("Refactor the parser").await?;
    while let Some(event) = run.next().await {
        let _event = event?; // already reduced into the thread
    }
    for interrupt in thread.interrupts() {
        if let Interrupt::Permission { tool_call_id, .. } = interrupt {
            thread.resume(&tool_call_id, hitl::deny()).await?.drain().await?;
        }
    }
    Ok(())
}
```

Errors: `NotConfigured` (503), `Unauthorized` (401/403), `Unreachable`, `Timeout`,
`ReindexRunning`, `Server { status, message }`, `Decode`, `Config`.

## Decision: own AG-UI types vs `ag-ui-core` (evaluated 2026-10-07)

Facts from crates.io on that date:

- `ag-ui-core` 0.1.0 (MIT): a single release from 2025-08-12, no repository link, no updates since.
- `ag-ui` 0.5.0-alpha.3 (MIT, `KimSoungRyoul/ag-ui-rust`): alpha, third-party, actively released but
  pre-1.0 with a server runtime we do not need.
- Neither is published by the AG-UI protocol authors, and the event set Pando emits (see its
  `internal/agui`) evolves with the protocol.

Decision (implemented in `agui`): keep our own small, tolerant event types in this crate
(unknown event types and fields are preserved, never fatal), and revisit if an official Rust SDK
appears. Licences are not a blocker (all MIT); maintenance and event coverage are.

## Request shapes

Verified against Pando `internal/api/handlers_remembrances_kb.go`,
`handlers_remembrances_search.go`, `handlers_embedding_models.go`, `handlers_remembrances.go`
and `server.go` (auth middleware).

Errors added by AG-UI: `Protocol` (200 that is not an SSE stream), `Run { code, message }` (`RUN_ERROR`).

AG-UI shapes were verified against Pando `internal/agui/{events,input,threads,hitl,server,sse}.go`
and the TypeScript SDK `sdk/typescript/src/agui/{client,thread,hitl}.ts`.
