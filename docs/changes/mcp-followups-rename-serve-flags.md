---
created_at: 2026-10-06T19:57:25.017419388Z
updated_at: 2026-10-06T19:57:25.017419388Z
tags:
    - change
    - mcp
    - cli
---
# MCP follow-ups: core rename and serve flags

Continues [[changes/bit-us-0020-0023-mcp-write-tools-audit-compat-api.md]] and [[changes/bit-us-0061-0082-0087-page-rename-cascade-merge.md]]. Plan: [[bitacora-full-development-plan]].

## What changed
- `rename_page` (MCP tool and Logseq-compatible `Editor.renamePage`) now calls `CommandQueue::rename_page` (core `RenameRequest`) instead of delete + create. One undoable transaction with namespace children, references graph-wide and config.edn; the report's `Transaction` goes into the audit group, so audit undo still reverts everything in one step.
- New args: `merge` (default false -> CONFLICT with `target_exists` extra, retry with `merge: true`) and `keep_aliases`. `update_links: false` is now refused with NOT_SUPPORTED (core always cascades). Details now report `renamed`, `merged`, `rewritten_pages`, `rewritten_blocks`, `skipped_pages`, `dropped_aliases`, `config_updated`, `warnings`.
- `QueueBridge::with_ref_lookup(Arc<dyn RefLookup>)`; `bitacora-runtime/src/live.rs` passes `session.ref_lookup()` (one line; mcp cannot depend on runtime).
- Error mapping: `map_rename` covers every `RenameError` variant (Blank/Unchanged/Journal/BadPath -> INVALID_ARGUMENT, ReadOnly -> READ_ONLY, TargetExists/PathTaken/ChildCollision -> CONFLICT, Lookup/Read -> INTERNAL, Commit -> map_commit); `map_queue` lists every `QueueError` variant without a catch-all. Removed the now-unused `rewrite_links`.
- `bitacora-cli serve`: `--allow-writes`, `--allow-deletes`, `--api` -> `McpConfig.allow_writes/allow_deletes/api_enabled`, default off, documented in `--help`.

## Files
crates/bitacora-mcp/src/{bridge,write_tools,handler}.rs, crates/bitacora-runtime/src/live.rs, crates/bitacora-cli/src/{main.rs,cmd/serve.rs,cmd/serve_tests.rs}, crates/bitacora-runtime/tests/mcp_write.rs.

## Verification
fmt, clippy workspace -D warnings, typos, xtask check-deps clean; `cargo test --workspace --locked --no-fail-fast`: 85 suites ok, 0 failed. New tests: collision/merge/journal/undo in mcp_write, `rename_errors_map_to_distinct_codes`, CLI flag parse/help tests, end-to-end flag test (tools/list and POST /api 404 vs enabled).

## Notes
The old file is moved by core (no logseq/.recycle copy on rename). Merge drops source aliases unless `keep_aliases`.
