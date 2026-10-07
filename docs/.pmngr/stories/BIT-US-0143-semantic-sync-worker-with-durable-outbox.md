---
id: BIT-US-0143
type: story
title: Semantic sync worker with durable outbox
status: done
priority: high
parent: BIT-EP-0021
milestone: BIT-M-0007
author: mcp
labels: [v2, search, bitacora-pando, bitacora-index]
estimate: 8
created: 2026-10-07T09:16:57Z
updated: 2026-10-07T10:51:10Z
started: 2026-10-07T10:47:32Z
closed: 2026-10-07T10:51:10Z
---

## Description
As a user, I want my semantic index to follow my edits automatically and survive restarts, index rebuilds and Pando outages.

## Acceptance Criteria
- A machine-local sync ledger per graph, kept outside the rebuildable index DB (e.g. `<data_dir>/pando/<graph-key>/semantic.sqlite`), records what was sent:
  - `semantic_state(doc_id, block_uuid, content_hash, synced_at)` and `semantic_outbox(op, doc_id, hash, attempts, next_at)`;
  - it is the source of truth for what exists in Pando, so purge and orphan cleanup never need a remote listing.
- The worker subscribes to `IndexWriter::subscribe`, diffs by `content_hash` and enqueues upserts and deletes.
- The sender makes bounded concurrent per-document `POST/DELETE /api/v1/remembrances/kb/documents` calls, debounced, with backoff.
- Cold-start reconcile compares eligible blocks with the ledger. Progress events are emitted.
- The scenarios of BIT-SP-0010.R3 are covered by tests with a mock Pando.

## Notes
Implements BIT-SP-0010.R3. Index writer: `crates/bitacora-index/src/writer.rs:142`. Uses only the current generic Pando REST API (owner decision 2026-10-07).
