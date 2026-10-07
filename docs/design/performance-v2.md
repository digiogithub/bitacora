---
created_at: 2026-10-07T15:00:00Z
updated_at: 2026-10-07T15:00:00Z
tags:
    - design
    - performance
    - v2
---
# Performance report for 2.0

Measurements and fixes of the v2 performance pass (BIT-US-0161, task BIT-T-0485), taken on the Linux development host. It extends [[performance-report-1.0]] (same tools, same host, same caveats) and follows ADR-026 in [[architecture]]. Related: [[graph-view]], [[semantic-search]], [[pando-integration]], [[sqlite-index-schema]], [[owner-manual-validation-checklist]] (per-OS runs).

Environment: Linux x86_64, 24 cores, release build, Xvfb 1280x900 with Mesa lavapipe (software Vulkan), SQLite 3.45 bundled. **No real GPU, no macOS or Windows host**: frame numbers are CPU-side only and the per-OS runs are tasks for the owner (checklist section V8).

## 1. How to measure

| Tool | Measures |
|---|---|
| `bitacora --graph <dir> --perf-bench` | Real workspace script: startup, journals scroll, 5,000-block page, typing, structural commands, search (see 1.0 report). |
| `cargo test -p bitacora-index --release --test bench_v2 -- --ignored --nocapture --test-threads=1` | Graph-view data load (`graph_data`, `local_graph_data`) and Tasks view queries (`task_groups`, `overdue_count`, calendar) on the `large` (540k blocks, 10,000 pages) and `fifty_k` presets, with ~2,000 / ~1,000 dated tasks added. |
| `cargo test -p bitacora-graph --release --test bench_scale -- --ignored --nocapture --test-threads=1` | Layout construction, per-tick cost and ticks/time to settle at 3k, 5k, 10k, 20k nodes; idle CPU of the simulation worker once settled (asserted below 100 ms in 5 s). |
| `BENCH_PAGES=<n> cargo test -p bitacora-pando --release --test bench_v2 -- --ignored --nocapture --test-threads=1` | Hybrid search latency and semantic sync backlog throughput against an in-process mock Pando KB (50 blocks per page; default 1,000 pages, `BENCH_PAGES=10000` is the large preset). |
| `/proc/<pid>/stat` sampling | Idle CPU of the running app (script in the change record). |

## 2. Results

### 2.1 Startup (reopen last graph, warm index, 5.2k files, 56k blocks)

| Metric | Before (measured on this branch) | After | 1.0 (ADR-026) | Target |
|---|---|---|---|---|
| Window opened | 167-176 ms | 176-194 ms | 160-170 ms | |
| First frame | 206-216 ms | 216-236 ms | 295-320 ms | |
| Index open + reconcile done | 553-602 ms | **365-401 ms** | 360-375 ms | |
| First journal rendered | 585-644 ms | **402-452 ms** | 386-410 ms | < 1 s warm |
| Cold index (first run) first journal | 2,240 ms | not re-measured | 1,990 ms | |
| RSS once settled | 271 MiB | 243-269 MiB | 251 MiB | reported |

Startup via `open_startup_graph` costs nothing extra over `--graph` (same `open_graph` path; the reopen only reads the recents list).

**Fix: a busy MCP port no longer doubles the session open.** `Session::open` binds the MCP port after the index has been opened; on `PortInUse` the runtime tears the session down and `session::open` retried it without MCP, so a second Bitacora (or any process on port 12316) made startup about 200 ms slower on a warm 5k-page graph, and much more on a graph that needs reconciling. `session::open` now probes the loopback port first (`mcp_port_in_use`) and opens directly without MCP, with the same `McpUnavailable` notice; the retry loop stays as a safety net. Test: `a_busy_mcp_port_is_detected_before_the_session_opens`. The "before" column above was measured with that port busy (another Bitacora was running), so it is the worst case; with a free port the cost is the MCP start itself (keychain lookup plus bind, ~25 ms in this run).

### 2.2 Interaction on the 5,000-block page (UI thread CPU, ms)

| Metric | 2.0 (4 runs) | 1.0 | Target |
|---|---|---|---|
| Scroll, CPU per frame | p50 6.3-6.5, p95 6.7-7.0, **p99 7.0-7.4** | p50 4.0-4.6, p95 5.3-5.5, p99 5.4-5.8 | p99 < 16.7 |
| Journals feed scroll, CPU per frame | p99 7.5-11.7 | p99 7.5-7.8 | p99 < 16.7 |
| Keystroke to end of paint | p50 5.9-6.2 (one run 8.2), p95 6.2-6.4 | p50 3.0-3.3, p95 4.4-4.6 | < 16 |
| Structural command to end of paint | p50 11.3-12.1, p95 12.0-13.7 | p50 7.7-8.4, p95 10.0-11.8 | < 16 |
| Search on the 5.2k-page graph, UI thread | p95 8.9-11.1 | p95 7.7 | < 50 |
| Navigate to the loaded 5,000-block page | 54-136 ms (two runs at 103-136) | 37-54 ms | < 100 ms |

Every frame costs about 2 ms more CPU than in 1.0: the redesign adds the title bar, tab strip, left sidebar, status footer and the larger block chrome to each frame, and the cost is flat (it does not grow with the page). All metrics stay inside their budgets with at least 2x headroom on the p99, so this is **accepted without an ADR**: the regression is the price of the design-system chrome, there is no single hot spot (spans such as `page_view.render` are ~0.016 ms), and a fix would mean caching element trees the toolkit rebuilds each frame. Re-check on a real GPU before tagging (checklist V8); if the p99 there exceeds 16.7 ms, profile the chrome first.

### 2.3 Tasks view and sidebar queries (index, release)

`fifty_k` is a typical big graph (4,800 open tasks, 334 overdue); `large` is the worst case (46,000 open tasks, 667 overdue, nearly all undated).

| Query | fifty_k before | fifty_k after | large before | large after | Target |
|---|---|---|---|---|---|
| `task_groups` all open tasks, p95 | 24.7 ms | **9.7 ms** | 265 ms | **109 ms** | < 50 ms |
| `task_groups` with a marker or page filter, p95 | 0.05 ms | 0.05 ms | 0.05 ms | 0.05 ms | < 50 ms |
| `overdue_count` (sidebar badge), p95 | 1.0 ms | 1.0 ms | 22.5 ms | 22.1 ms | < 50 ms |
| `journal_days_with_notes` (calendar month), p95 | 0.02 ms | 0.02 ms | 0.04 ms | 0.04 ms | < 50 ms |

Tasks view p95 is under the 50 ms target for any realistic graph. Only the synthetic 46k-open-task case misses it (109 ms, background thread; the view shows a spinner for a moment, the UI never blocks).

**Fix: task properties are loaded in batches.** `IndexReader::task_items` ran one `block_properties` query per task and cloned every `BlockRow` first; it now fills properties with a few `IN (...)` queries of 500 ids (`load_task_properties`) and no clone. Same results (existing `read_misc` and `read_graph` tests unchanged and green); 2.4x faster at 46k tasks and 2.5x at 4.8k.

Possible further step if graphs with tens of thousands of open tasks matter: `LIMIT` the "no date" group in SQL and add a count (changes the `TaskGroups` contract; not done).

### 2.4 Graph view data and layout

Data load (`graph_data`, background thread; the 1.x journals-off default):

| Preset | Nodes / edges | Before p95 | After p95 | Target |
|---|---|---|---|---|
| fifty_k | 1,005 / 20,141 | 16.6 ms | 10.8 ms | < 500 ms |
| large | 9,025 / 237,097 (journals on: 11,025 / 267,730) | 175 ms | 161 ms | < 500 ms |

**Fix:** edges are collected in a `Vec`, sorted and deduplicated once instead of inserted into a `BTreeSet` (same sorted, deduplicated output). The remaining time is the SQLite scan joining `block_page_refs` with `blocks`. `local_graph_data` currently loads the whole graph before cutting the neighbourhood: 11 ms (fifty_k) and 150 ms (large); acceptable because it runs in the background, but a targeted 1-hop query is the obvious next step for graphs above 5k pages (not done: it would duplicate the filter semantics of `finish`).

Layout (`bitacora-graph`, one core, release; graph with a heavy-tailed degree distribution, 1.5 links per node):

| Nodes | Links | Build | Tick p50 / p95 | Ticks to settle | Time to settle |
|---|---|---|---|---|---|
| 3,000 | 4,491 | 0.08 ms | 2.0 / 2.4 ms | 301 | 0.6 s |
| 5,000 | 7,473 | 0.04 ms | 3.4 / 4.2 ms | 301 | 1.0 s |
| 10,000 | 15,017 | 0.1 ms | 6.9 / 9.1 ms | 301 | 2.1 s |
| 20,000 | 30,031 | 0.2 ms | 13.6 / 18.9 ms | 301 | 4.1 s |

Tick cost is linear in the node count (Barnes-Hut). The worker is paced at 16 ms (`TICK`), so a 5k graph settles in about 5 s of wall time while using about 20% of one core, and 20k nodes would settle in 6-9 s but each tick alone fills a 60 Hz frame budget (p95 above it). `GraphModel::from_data`, `restrict` and the model clone on each refresh are O(n) and negligible against these numbers.

**Idle CPU:** a settled worker blocks on its channel (`recv`), measured 0 ns CPU in 5 s (`settled_worker_is_idle`, asserted), and `GraphView::render` only calls `request_animation_frame` while `!settled || drag || wake_generation` (`wants_frames`), so a settled graph does not repaint. Not measurable without a display automation: the paint cost of the canvas itself (needs a real GPU, checklist V5).

**Node ceiling recommendation (owner question, [[bitacora-v2-plan]] open question 2): keep 5,000 nodes as the 2.0 ceiling, do not build 20k LOD now.**
- 5k is comfortably inside the budget on the CPU side: 3.4 ms per tick (60 ticks/s needs under 16 ms; ADR-034's "5k nodes at 60 ticks/s" holds with 4x headroom), 1 s of compute to settle, data load far under 100 ms.
- 20k is at the limit of the simulation alone (13.6 ms p50, 18.9 ms p95 per tick), before paint, edge batching and hit-testing; it would need a coarser simulation (fewer collide passes, a larger theta, or layout only on a degree-filtered subset) and LOD rendering (no labels, batched edges, aggregate far nodes), which is a feature, not a tuning pass.
- Above the ceiling the cheap, honest behaviour is: show the top N pages by degree (journals off, orphans off already reduce a large graph a lot: 9k nodes at 10k pages) with a notice and let focus mode (1-6 hops) reach the rest. That is a small follow-up if the owner wants it for 2.0; otherwise 2.x.
- Revisit with the real GPU numbers (checklist V5/V8): if painting 5k nodes takes more than ~8 ms of CPU per frame on the reference machines, lower the ceiling instead.

### 2.5 Hybrid search (mock Pando, 50k-block graph, local FTS5)

| Mode | p50 | p95 | Notes |
|---|---|---|---|
| Lexical only (`HybridSearch` without remote) | 11.2 ms | 16.4 ms | includes `spawn_blocking` |
| Hybrid, mock answers instantly | 11.6 ms | 16.5 ms | fusion plus re-resolving 20 hits adds < 1 ms |
| Hybrid, mock answers in 50 ms | 55.9 ms | 58.9 ms | = Pando latency + ~6 ms |
| Hybrid, mock answers in 300 ms | 307.6 ms | 309.2 ms | = Pando latency + ~7 ms |

The remote request overlaps with the local FTS pass, so the hybrid cost is `max(local, Pando)` plus under 1 ms of fusion; the palette runs it off the UI thread and the 2.5 s timeout (`HybridOptions::timeout`) bounds the worst case, after which the result is lexical only with an "unavailable" chip. Nothing to fix; the budget is dominated by the real Pando round trip (embedding the query), which needs the live run in the checklist (V7).

### 2.6 Semantic sync backlog (initial sync, 50,000 documents, in-process mock KB)

| Step | Result |
|---|---|
| Cold index build of the graph | 0.68 s |
| `DocSource` scan (what the cold-start reconcile does: every file, every block mapped to a document) | 50,000 docs in 0.26 s (194,000 docs/s) |
| First outbox rows available | 26 ms after the worker starts |
| Send, mock 0 ms, concurrency 4 (default) | 14.2 s = **3,520 docs/s** |
| Send, mock 20 ms, concurrency 4 | 283 s = 177 docs/s |
| Send, mock 20 ms, concurrency 16 | 76.6 s = 652 docs/s |

Throughput is `concurrency / request latency` (4 / 20 ms = 200 docs/s, 16 / 20 ms = 800 docs/s measured 177 and 652), so the worker adds about 0.3 ms of overhead per document (HTTP client, ledger transaction) and is never the bottleneck. On the `large` preset (about 540k documents) that is 2.6 minutes of pure Bitacora overhead at 0 ms latency; with a real embedding backend at, say, 100 ms per document and the default concurrency of 4 it would be about 3.7 hours, running in the background without blocking the UI, resumable after a restart (ledger), and restarts send nothing when nothing changed. The scan and the outbox are not a concern.

Recommendation, no change made: concurrency stays at 4 by default (the Pando server's capacity is unknown and Pando must not be overloaded); the owner can decide whether to expose it or raise it after the live run (checklist V7). A bigger lever than concurrency would be a batch upsert endpoint in Pando, which is out of scope (Pando stays generic and unchanged).

### 2.7 Idle CPU

| State | Result |
|---|---|
| App open on the 5.2k-page graph, journals visible, Pando off (15 s settle, 20 s sampled) | **0.20 % of one core** (4 ticks in 20 s); 171 threads, of which the software-GPU workers dominate |
| Settled graph layout worker | 0 CPU (blocks on its channel) |
| Settled graph canvas | no animation frames requested |
| Chat idle | no timers: streaming runs only while a turn is active |
| Background wakeups that remain | semantic diff thread polls every 100 ms, session pump every 50 ms, semantic sender every 5 s (500 ms while Pando is disconnected), Pando probe every 15 s |

The periodic wakeups are cheap (they only look at a channel or the ledger). If idle power draw on laptops becomes a concern, the 100 ms and 50 ms polls can move to blocking `recv` with a timeout of seconds; not done because the measured cost is below the noise floor of the measurement (0.2 %).

## 3. Summary of changes

| Area | Change | File |
|---|---|---|
| Startup | Probe the MCP port before the session opens instead of opening twice | `crates/bitacora-app/src/session.rs` |
| Tasks view | Batch property loading, no row clone | `crates/bitacora-index/src/read/misc.rs` |
| Graph data | Sorted `Vec` instead of `BTreeSet` for edges | `crates/bitacora-index/src/read/graph.rs` |
| Benchmarks | New ignored benchmarks | `crates/bitacora-index/tests/bench_v2.rs`, `crates/bitacora-graph/tests/bench_scale.rs`, `crates/bitacora-pando/tests/bench_v2.rs` |

## 4. Open items

- Per-OS and real-GPU runs of the same tools: [[owner-manual-validation-checklist]] section V8 (the 1.0 per-OS runs are still pending as well).
- Graph canvas paint cost and fps at 3k and 5k nodes (needs a GPU).
- Targeted `local_graph_data` query and a top-N-by-degree cut above the node ceiling (optional follow-ups).
- Per-frame chrome cost (+2 ms vs 1.0): re-check on a real GPU.
