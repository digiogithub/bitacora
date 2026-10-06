---
created_at: 2026-10-06T19:30:29.80479026Z
updated_at: 2026-10-06T19:30:29.80479026Z
tags:
    - change
    - core
---
# BIT-US-0061 / 0082 / 0087: page rename, reference cascade, merge

Commit 17ebe83 (see [[bitacora-full-development-plan]], [[01-file-graph-layout]] 10.2, [[04-editor-outliner-operations]], [[bit-us-0028-0057-0089-0099-page-lifecycle]]).

## What
- `crates/bitacora-core/src/rename.rs`: `rewrite_refs` span-based rewriter ([[old]], #old, #[[old]], nested, link targets, embed, namespace prefix, property keys/values, comma-separated plain fragments; skips code/fences/quoted/unparsed keys). Case-insensitive (decision on open question 2).
- `editor/rename.rs`: `Workspace::plan_rename`, `RenameRequest`, `MergeMode`, `RefLookup` trait, `RenameReport`/`RenameError`.
- New ops `Op::RenamePage` (re-key + title) and `Op::EditFile` (config.edn via ConfigEditor, flushed with expected-content check); `Workspace::pending_edits`.
- flush: config edit writes; case-only file rename via temp bridge (safe on case-insensitive FS).
- queue: `Request::RenamePage`, `Response::Renamed`, `QueueError::Rename`, `CommandQueue::rename_page`.
- markdown: `block::inline_text_ranges`. runtime: `IndexRefLookup`, `Session::ref_lookup`.

## Decisions
- Journals refused (RenameError::Journal). Title:: always rewritten when present (open question 7). Merge needs MergeMode::Merge opt-in; source aliases dropped with warning unless keep_aliases. Merge does not rename namespace children or rewrite their refs. rename-nested-pages not implemented.

## Verification
cargo fmt, clippy -D warnings (core, markdown, runtime), cargo test core/markdown/runtime all green (23 new integration tests, 9 rewriter unit tests, 1 runtime test), typos, xtask check-deps.

## Open
BIT-T-0131 (golden outputs from Logseq 0.10.15) and BIT-T-0157 (UI dialog).
