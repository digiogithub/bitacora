---
id: BIT-US-0143
type: story
title: Semantic sync worker with durable outbox
status: backlog
priority: high
parent: BIT-EP-0021
milestone: BIT-M-0007
author: mcp
labels: [v2, search, bitacora-pando, bitacora-index]
estimate: 8
created: 2026-10-07T09:16:57Z
updated: 2026-10-07T09:16:57Z
---

## Description
As a user, I want my semantic index to follow my edits automatically and survive restarts and Pando outages.

## Acceptance Criteria
- Index schema: `semantic_state(block_uuid, content_hash, synced_at)` and `semantic_outbox(op, doc_id, hash, attempts, next_at)` with migration + version bump; rebuildable from the graph.
- Worker subscribes to `IndexWriter::subscribe`, diffs by `content_hash`, enqueues upserts/deletes; debounced batch sender with backoff; uses batch/hash-skip endpoints when available, per-document calls otherwise.
- Cold start reconcile (remote list when available, else local state); progress events; scenarios of BIT-SP-0010.R3 covered by tests with a mock Pando.

## Notes
Implements BIT-SP-0010.R3. Index writer: `crates/bitacora-index/src/writer.rs:142`.
