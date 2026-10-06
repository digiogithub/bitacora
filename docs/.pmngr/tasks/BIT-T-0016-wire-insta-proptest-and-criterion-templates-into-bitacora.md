---
id: BIT-T-0016
type: task
title: Wire insta, proptest and criterion templates into bitacora-markdown
status: backlog
priority: high
parent: BIT-US-0012
milestone: BIT-M-0001
author: mcp
labels: [testing, bitacora-markdown]
estimate: 2
created: 2026-10-06T14:26:22Z
updated: 2026-10-06T14:26:22Z
---

## Description
In `crates/bitacora-markdown`:
- `[dev-dependencies]` `insta` (feature `yaml`), `proptest`, `criterion` (workspace pins), plus `bitacora-testkit`.
- `tests/snapshot_template.rs`: an `insta::assert_snapshot!` over a placeholder `split_lines()` function iterating `bitacora_testkit::markdown_files("edge-cases")` (one snapshot per file via `insta::glob!` or named snapshots).
- `tests/proptest_template.rs`: identity property on the placeholder (e.g. `join(split(s)) == s` for arbitrary `String` including `\r\n`, tabs and BOM).
- `benches/split.rs` with `criterion_group!` and `[[bench]] harness = false`.
- CI: set `INSTA_UPDATE=no`, `CI=true`; add `cargo bench --no-run -p bitacora-markdown` to `test-core`.

## Acceptance Criteria
- `cargo test -p bitacora-markdown` runs all three templates green.
- A changed snapshot fails CI instead of being auto-accepted.
- `proptest-regressions/` is committed (not git-ignored).

## Notes
- [[crate-stack]] §4.1 Testing row; AGENTS.md §6. The real parser lands in BIT-EP-0003; these are templates it replaces.
