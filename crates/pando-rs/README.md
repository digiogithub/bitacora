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

Planned: an AG-UI client in a sibling `agui` module reusing `PandoClient` and `Error`.

Errors: `NotConfigured` (503), `Unauthorized` (401/403), `Unreachable`, `Timeout`,
`ReindexRunning`, `Server { status, message }`, `Decode`, `Config`.

## Decision: own AG-UI types vs `ag-ui-core` (evaluated 2026-10-07)

Facts from crates.io on that date:

- `ag-ui-core` 0.1.0 (MIT): a single release from 2025-08-12, no repository link, no updates since.
- `ag-ui` 0.5.0-alpha.3 (MIT, `KimSoungRyoul/ag-ui-rust`): alpha, third-party, actively released but
  pre-1.0 with a server runtime we do not need.
- Neither is published by the AG-UI protocol authors, and the event set Pando emits (see its
  `internal/agui`) evolves with the protocol.

Decision: keep our own small, tolerant event types in this crate when the AG-UI client lands
(unknown event types and fields are preserved, never fatal), and revisit if an official Rust SDK
appears. Licences are not a blocker (all MIT); maintenance and event coverage are.

## Request shapes

Verified against Pando `internal/api/handlers_remembrances_kb.go`,
`handlers_remembrances_search.go`, `handlers_embedding_models.go`, `handlers_remembrances.go`
and `server.go` (auth middleware).
