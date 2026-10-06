---
id: BIT-T-0340
type: task
title: Echo filter for own writes and no-op event dropping
status: done
priority: critical
parent: BIT-US-0067
milestone: BIT-M-0003
author: mcp
labels: [bitacora-core, bitacora-watch, io]
estimate: 2
created: 2026-10-06T14:34:13Z
updated: 2026-10-06T18:47:26Z
started: 2026-10-06T18:47:24Z
closed: 2026-10-06T18:47:26Z
---

## Description
`crates/bitacora-core/src/external/echo.rs`: `EchoFilter` (map `(rel_path, hash) -> expires_at`, TTL 5 s, injectable clock) filled by the writer right before `rename`. The queue consumer drops `FileEvent`s whose hash is in the filter or equals the page's `DiskSnapshot.hash`; events whose content differs only by surrounding whitespace from the snapshot are treated as no-op for reload (Logseq trim semantics) but the snapshot bytes are updated so the pre-write check stays exact.

## Acceptance Criteria
- Unit tests: own write dropped; external write with different hash 100 ms later processed; TTL expiry.
- Whitespace-only change does not trigger a reload but updates the snapshot.

## Notes
Story BIT-US-0067. Implements BIT-SP-0005.R12. Logseq `watcher_handler.cljs:91-94`.
