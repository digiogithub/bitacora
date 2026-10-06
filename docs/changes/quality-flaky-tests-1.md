---
created_at: 2026-10-06T19:30:17.564478231Z
updated_at: 2026-10-06T19:30:17.564478231Z
---
# Quality: flaky tests round 1

Continues [[bitacora-full-development-plan]]; see [[bit-t-0123-mcp-query-dsl]] and [[sqlite-index-schema]].

## Index property test (`crates/bitacora-index/tests/property.rs`)
- Could not reproduce `incremental_equals_rebuild_on_a_small_graph` failing: about 8,000 cases passed (6 processes x 400, then 10 x 300 under 48 CPU burners on a 24-core host, plus 6 ms sleeps between ops to force distinct mtimes). The one-off failure under load has no recorded seed.
- Hypotheses checked and ruled out: explicit `id::` clash across files (new test `explicit_id_clash_between_files_is_reported_deterministically` shows incremental == rebuild), mtime/ms granularity (page `updated_at` follows touch), `indexed_at` (not in the canonical dump).
- Hardening: `PROPTEST_CASES` now overrides the hard-coded case counts (`cases_from_env`); failures persist their seed in `proptest-regressions/property.txt` (commit it if it ever appears); the generated `id::` counter starts at 0x10000 so it cannot collide with fixture ids such as `...00aa`.
- No index-code change: no rebuild != incremental bug was found.

## MCP notification test (`crates/bitacora-mcp/tests/read_tools.rs`)
`page_changes_notify_subscribed_resources_within_two_seconds`: the old version slept 500 ms, touched once and asserted <2 s measured across the whole join. Now it re-triggers (edit + reconcile) every 300 ms until the SSE listener sees the `bitacora://page/Notes` URI (30 s cap), and asserts the 2 s bound from the trigger that caused it. Waiting on the Notes URI also fixed a second flake found while stress-testing (the first notification could be for the other subscribed page). 25 consecutive runs green.

## Spec text
BIT-SP-0007.R11 and BIT-US-0018 updated: advanced Datalog subset executes; unsupported construct -> `NOT_SUPPORTED` with `unsupported: <construct>`; malformed -> `INVALID_QUERY`.

## Full-suite runs
See the final report of the task for the three `cargo test --workspace --locked --no-fail-fast` results.