---
created_at: 2026-10-07T15:30:00Z
updated_at: 2026-10-07T15:30:00Z
tags:
    - change
    - performance
    - qa
    - v2
---
# v2 performance pass and consolidated per-OS checklist (BIT-US-0161)

Continues [[bitacora-v2-plan]]; story BIT-US-0161, tasks BIT-T-0484 and BIT-T-0485. Report: [[performance-v2]]. Checklist: [[owner-manual-validation-checklist]] (section V1 to V9).

## What changed

- **Startup:** `session::open` probes the MCP loopback port before opening the session (`mcp_port_in_use`); previously a busy port made `Session::open` fail after the index was open and the whole session was opened a second time (+~200 ms warm, more when reconciling). Warm start to first journal went from 585-644 ms to 402-452 ms in the same environment.
- **Tasks view query:** `IndexReader::task_items` (used by `tasks`, `task_groups`) loads properties with batched `IN (...)` queries (`load_task_properties`) and no longer clones every row: `task_groups` over 46k open tasks 265 ms to 109 ms, over 4.8k open tasks 24.7 ms to 9.7 ms.
- **Graph data:** `load_graph` / `finish` keep edges in a sorted, deduplicated `Vec` instead of a `BTreeSet` (`graph_data` large preset 175 ms to 161 ms).
- **Benchmarks (ignored, release):** `crates/bitacora-index/tests/bench_v2.rs`, `crates/bitacora-graph/tests/bench_scale.rs` (also asserts that a settled worker uses no CPU), `crates/bitacora-pando/tests/bench_v2.rs` (hybrid search and semantic backlog against a mock KB).
- **Docs:** new `docs/design/performance-v2.md` (numbers, node-ceiling recommendation: keep 5k for 2.0, no 20k LOD now), a note in `docs/design/graph-view.md`, and a v2 section (V1 to V9) in `docs/plans/owner-manual-validation-checklist.md` merging the frameless checklist, IME C21-C23, macOS menu, Windows snap and managed-mode-unavailable, the light/dark visual review, the live Pando run and the list of in_review items.

## Why

The story asks for v2 to be inside the performance budgets before 2.0 and for every manual check to be in one place per OS. Hot spots were found by measuring (not guessing): the MCP retry showed up as a gap in the `--perf-bench` marks and the debug log; the tasks query was dominated by a per-task SQL round trip.

## Verification

- `cargo clippy -p bitacora-index -p bitacora-graph -p bitacora-pando -p bitacora-app --all-targets --locked -- -D warnings` clean.
- `cargo test -p bitacora-index -p bitacora-graph -p bitacora-pando -p bitacora-app --locked`: 910 passed, 0 failed, 12 ignored (new test `a_busy_mcp_port_is_detected_before_the_session_opens`).
- Benchmarks run as documented in [[performance-v2]] section 1 (before/after numbers recorded there); `--perf-bench` on the release app under Xvfb + lavapipe, 4 runs before and 3 after.

## Not done / follow-ups

Per-OS and real-GPU runs; graph canvas paint cost; targeted `local_graph_data` query; top-N cut above the node ceiling; per-frame chrome cost (+2 ms CPU vs 1.0, accepted, within budget).
