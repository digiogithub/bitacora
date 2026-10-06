---
id: BIT-T-0036
type: task
title: Writer thread, job channel and IndexEvent emission
status: done
priority: critical
parent: BIT-US-0006
milestone: BIT-M-0002
author: mcp
labels: [bitacora-index, index]
estimate: 3
created: 2026-10-06T14:27:35Z
updated: 2026-10-06T17:53:44Z
closed: 2026-10-06T17:53:44Z
---

## Description
`crates/bitacora-index/src/writer/mod.rs`: spawn one `std::thread` owning the write connection; it receives `WriterJob::{Replace(ParsedFile, FileStat, Bytes), Delete(RelPath), Rename{from,to}, Batch(Vec<..>), Flush(oneshot)}` over a `crossbeam_channel`. After each commit, publish `IndexEvent::FileReplaced { file_id, page_ids_touched, block_uuids_added, block_uuids_removed }` / `FileDeleted` to subscribers (`crossbeam` broadcast list; the app bridges to GPUI). Graceful shutdown drains the queue.

## Acceptance Criteria
- Test: 1,000 jobs from 8 producer threads are applied in order per path; no `SQLITE_BUSY`.
- Test: subscriber receives events only after the transaction is committed (a reader sees the new rows when the event arrives).
- `Flush` returns after all prior jobs committed (used by tests and CLI).

## Notes
BIT-SP-0003.R7. ADR-004. [[sqlite-index-schema]] §1 principle 5, §4.4.
