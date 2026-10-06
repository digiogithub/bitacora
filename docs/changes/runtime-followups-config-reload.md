---
created_at: 2026-10-06T21:05:32.235918711Z
updated_at: 2026-10-06T21:05:32.235918711Z
tags:
    - change
    - runtime
    - index
---
# Runtime follow-ups: config reload, R17 template journals, flaky tests, reindex()

Continues [[changes/bit-us-0067-runtime-crate-wiring.md]], [[changes/ci-3os-core-tests-and-r17.md]], [[changes/quality-flaky-tests-1.md]], [[changes/bit-us-0068-0069-0070-external-reload-merge-conflict.md]].

## config.edn hot reload
- `bitacora-runtime` (session.rs `Pump::reload_config`, live.rs): on a `logseq/config.edn` watcher event the effective config is reloaded and, if changed, swapped into a shared handle (`Session::current_config()`), core gets `EditorSettings::from_config` via `SetSettings`, the watcher's `:hidden` predicate reads the shared handle (no watch crate change), the index gets `Indexer::set_config`, and a full reconcile runs when the config hash changed. Events: `RuntimeEvent::ConfigChanged` (unchanged) then `ConfigReloaded { config, reindexed }` or `ConfigReloadFailed { message }` (invalid EDN keeps the previous config, so a half-saved file never resets to defaults). `Session::config()` still returns the startup config.
- `bitacora-index`: `Indexer::set_config` (hash change -> pending `FullReparse`, new hash recorded by the next reconcile only, so a crash re-parses), `Indexer::pending_rebuild`, `IndexWriter::set_config_hash`, `WriteConnection::set_config_hash`. No config key triggers FtsOnly (none feeds the normalizer).
- Not applied live (follow-ups): MCP `QueueBridge` keeps its startup config copy; the sync engine keeps its journal-template text; app code must read `current_config()`/the event to refresh view settings, journal formats and file name format (core takes those per request).
- Tests: `crates/bitacora-runtime/tests/config_reload.rs`, `crates/bitacora-index/tests/config_reload.rs`.

## R17 remainder
`Pump::is_template_only_journal`: an upserted, not loaded journal whose trimmed content equals the configured default template is ignored (no index, no core reload). Test `journal_equal_to_the_default_template_is_ignored` (fails when the rule is disabled). BIT-SP-0002.R17 trace updated with `session.rs` and the test; `verify_requirement` refused: no ingested test results (needs `gintrack spec ingest`).

## Flaky tests
- `clean_page_reloads_with_stable_ids`: waited on the first `ExternalChange` event, which can be a stale event for the file as created, and wrote with truncating `fs::write` so the watcher could report an empty intermediate file. Now waits for the final block state and all tests replace the page atomically (temp + rename). Original failed 0 of 160 runs under 30 CPU burners, so the cause is by analysis, not reproduction.
- `incremental_equals_rebuild_on_a_small_graph`: not reproduced in about 8,400 more cases (12 parallel processes under 24 CPU burners, including 3,600 with ops up to 40). No seed, no code change; failures persist their seed (see [[changes/quality-flaky-tests-1.md]]).

## reindex()
`Session::reindex()`: flushes core, then the pump thread runs `Indexer::reindex` (full reparse, cold build) while queued index updates wait and resume in order; emits `Reconciled`. The app no longer needs to delete the DB and reopen (app change not done here).

## Verification
fmt, clippy workspace -D warnings clean, `cargo test --workspace --locked --no-fail-fast`: 1297 passed, 0 failed, after merging main.