---
created_at: 2026-10-07T09:56:09.241278994Z
updated_at: 2026-10-07T09:56:09.241278994Z
tags:
    - changes
    - backlog
    - v2
    - pando
---
# 2026-10-07 — v2 backlog adjusted to owner decisions

Continues [[2026-10-07-v2-backlog-generated]] and [[bitacora-v2-plan]] (section "Owner decisions"). No code changed.

## Decisions applied
1. **Managed Pando mode** (like git-in-track `internal/pando/supervisor`, ADR-039 there) is the default. BIT-US-0141 is renamed and now high priority. BIT-T-0437 now supervises `pando serve`, which serves the REST API (`cmd/serve.go:162`). New tasks:
   - BIT-T-0488: generated `.pando.toml` with Bitacora profiles, personas and `MCPServers.bitacora`.
   - BIT-T-0489: spike on running the managed instance against the user's shared KB.
2. **Shared KB** in the agent memory. The BIT-US-0138 consent dialog must say that indexed blocks are visible to any Pando agent.
3. **Semantic search with client-side solutions on the current Pando API. No Bitacora-specific work in Pando.**
   - Cancelled: BIT-EP-0019, BIT-US-0132, 0133, 0134 and BIT-T-0415..0422. The reason is commented on the epic.
   - Rewritten: BIT-EP-0021, BIT-US-0143, BIT-T-0441 (ledger is machine-local, outside the index), BIT-T-0443 (per-document calls with bounded concurrency), BIT-T-0444, BIT-T-0449 (purge goes through the ledger), BIT-US-0129 and BIT-T-0408 (the SDK covers the current routes only), BIT-T-0434 (MCP registration through the generated config), and requirement BIT-SP-0010.R1.
   - Verified in Pando:
     - REST upsert writes no mirror file (`internal/api/handlers_remembrances_kb.go`).
     - `SyncDirectoryWithStats` only deletes documents that carry `source_path` metadata (`internal/rag/kb/sync.go:288-313`).
4. **Journal reviews only in a local cache.** BIT-US-0151 updated.
5. **Paper theme removed.** BIT-T-0384 cancelled; BIT-US-0116 and BIT-US-0162 updated. New post-2.0 story BIT-US-0164 for colour schemes from a config file.
6. **Reopen the last graph at startup, plus a graph menu.** New story BIT-US-0165 with tasks BIT-T-0490 and BIT-T-0491. Code references: `app.rs` `open_workspace`, `recent.rs`, `paths.rs:100`, `ui/mod.rs:275`.

## Verification
All gintrack writes were accepted, and every status change to `cancelled` was validated by the workflow.
