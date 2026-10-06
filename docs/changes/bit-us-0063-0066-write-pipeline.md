---
created_at: 2026-10-06T18:52:55.534015475Z
updated_at: 2026-10-06T18:52:55.534015475Z
tags:
    - change
    - core
    - io
---
# BIT-US-0063..0066: debounced writes, atomic writer, pre-write check, logseq/bak

Continues [[bitacora-full-development-plan]] and [[bit-us-0029-0062-core-editor-model-ops-and-command-queue]]; designs [[block-editor]], [[01-file-graph-layout]], [[04-editor-outliner-operations]].

## What changed (crates/bitacora-core, commit 217b40a)
- `src/write_queue.rs`: `WriteQueue`/`DebounceConfig` (400 ms debounce, 2 s max delay, per-transaction batches, 1 s..60 s backoff), pure with a Duration clock (fake-clock unit tests).
- `src/queue.rs`: `QueueConfig::debounce` (default on); worker uses `recv_timeout` to flush due pages; `Request::Resolve{Keep::Mine|Disk}`, `Request::CheckMissing`; events `Conflict`, `WriteFailed`, `Recreated`, `PagesRemoved`; `QueueJoin::shutdown` flushes and `shutdown_with_report` returns the final `FlushReport`; stale temp cleanup at start.
- `src/editor/fsio.rs` (new, allowlisted in the single-writer guard): `atomic_write` (temp + fsync + perms + rename + dir fsync, in-place fallback), `cleanup_stale_tmp`, `FsStore`.
- `src/editor/flush.rs`: `FileStore` gains `stat`/`list`/`cleanup_stale`/`take_warnings`; pre-write stat then blake3 check against `DiskSnapshot` (now carries len/mtime); self-check refusal; recreate deleted files; backups; `resolve_keep_mine`, `take_disk`, `drop_missing`; richer `FlushReport`.
- `src/editor/backup.rs`: Logseq layout `logseq/bak/<dir>/<stem>/<ts>.Desktop.<ext>`, keep newest 6, `removes_text` trigger.
- Unrepresentable text: `OpError::Unrepresentable` / `Refusal::Unrepresentable` for text with a line that re-parses as a bullet (mirrors Logseq's parser, fences are fine); `Page::serialize_checked`.
- Tests: tests/write_pipeline.rs (30), tests/crash_safety.rs (kill child 40x), unit tests in write_queue/backup; guard allowlist updated (fsio.rs, cli reindex.rs deleting SQLite files).

## Verification
`cargo fmt`, `cargo clippy --workspace --all-targets --locked -D warnings`, `typos`, `cargo xtask check-deps` clean; `cargo test -p bitacora-core --locked`: 122 tests pass (3 repeated runs, no flakes).

## Known limits / follow-ups
- bitacora-app/cli must call `shutdown_with_report()` on quit and render notices from the new `QueueEvent`s (T-0317, T-0323/0326 UI part).
- bitacora-markdown: a file starting with a BOM treats its first `- ` bullet as pre-block text (BOM defeats line-start detection); round-trip is intact but the first block is not editable as a block.
- Preamble text containing list-like lines is only caught by the flush self-check, not at op time.
