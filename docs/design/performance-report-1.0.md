# Performance report for 1.0

Measurements and fixes of the 1.0 performance pass (BIT-US-0109, tasks BIT-T-0336 and BIT-T-0337), taken on the Linux development host. Related: [[sqlite-index-schema]] (index design), [[block-editor]], [[block-editor-spike-report]] (the earlier spike harness), [[gpui-and-gpui-kit]] (section 1.10), [[accessibility-1.0]] and ADR-026 in [[architecture]].

## 1. How to measure

| Tool | What it does |
|---|---|
| `bitacora --graph <dir> --perf-bench` | Drives the **real workspace** through a script (startup, journals scroll, open a 5,000-block page, scroll it, type, indent/outdent, search, query widgets) and prints one `PERF_BENCH {json}` line. Module `crates/bitacora-app/src/perf/`. |
| `BITACORA_PERF=1` | Records marks and spans in a normal session; `RUST_LOG=bitacora::perf=debug` logs each one. Disabled, a span costs one relaxed atomic load. |
| `cargo test -p bitacora-index --release --test bench_large -- --ignored --nocapture` | The 540k-block `large` preset: cold build, search, page open, linked references and DSL queries (BIT-T-0336). |
| `BITACORA_BENCH_OUT=<dir> cargo test -p bitacora-index --release --test bench_cold_build -- --ignored generate_app_bench_graph` | Writes the app benchmark graph (5,000 pages, ~56k blocks, `Big page` with 5,000 blocks, `Query bench` with three query widgets, 14 recent journals). |
| `bench_search`, `bench_cold_build` | The 5,000-page / 50k-block benchmarks (`--ignored`) plus non-ignored smoke tests with relaxed bounds that run in CI (BIT-SP-0003.R14 and R16). |

Environment: Linux x86_64, 24 cores, 93 GB RAM, Xvfb 1280x900 with Mesa lavapipe (software Vulkan; `VK_ICD_FILENAMES=.../lvp_icd.json`, `WAYLAND_DISPLAY` unset), release build, rustc 1.98.1, SQLite 3.45 bundled with rusqlite. **No real GPU and no macOS or Windows host**: frame numbers are CPU-side only and the per-OS reference-machine runs are still to do.

Caveat on frame times: on Xvfb the display paces frames at ~48 ms regardless of load, so the interval between frames is meaningless. The report uses the CPU side of a frame: from the start of the frame (the `on_next_frame` callback) to the end of paint (a zero-size marker element placed last in the window, `perf::FrameEnd`). Keystroke and command latencies are measured from the call to the end of the paint that shows the result.

## 2. Results

App benchmark graph: 5,217 files, ~56k blocks. Medians of single runs (the numbers vary by a few percent between runs).

### 2.1 Startup

| Metric | Cold index (first run) | Warm index | Target |
|---|---|---|---|
| Window opened (since process start) | 160-170 ms | 160-170 ms | |
| First frame | 310-320 ms | 295-320 ms | |
| Index open + reconcile done | 1,960-1,990 ms (5.2k files parsed) | 360-375 ms | |
| First journal rendered | 1,990 ms | **386-410 ms** | < 1 s warm |
| RSS once settled (anonymous part) | 341 MiB (148) | 251 MiB (84) | reported |

The journals feed waits for the index reader, so a cold build delays the first journal; the status bar shows the build and the UI stays responsive meanwhile.

### 2.2 Hot spots found and fixed (UI thread, ms)

| Metric | Before | After |
|---|---|---|
| Indent/outdent on the 5,000-block page, command to end of paint | p50 18.3, p95 21.5 | p50 7.7-8.4, **p95 10.0-11.8** |
| `rebuild_rows` after a command | p50 10.3-10.9, p95 11.8-12.9 | p50 0.04, p95 1.2-1.7 |
| Blocking `queue.run` on the UI thread (includes the rebuild) | p50 12.9-13.8, p95 16.3 | p50 2.8-4.0, p95 5.4 |
| `PageView::finish_load` (UI thread when a page opens or reloads) | p50 12.8-13.9, p95 14.3-16.3 | p50 1.7-2.2, p95 4.2-5.8 |
| Page load in the background (5,000-block page) | p95 ~40 | p95 18-22 |
| Search on the 5.2k-page graph, UI thread, 14 queries | p50 3.6-3.8, p95 7.7-8.1 | p50 3.6, p95 7.7 (title cache) |

1. **Row models were rebuilt from scratch after every local command** (`OutlineEditor::rebuild_rows`: parse every block and run one SQLite `block_ref_count` query per block with an id). `Outline::rows_reusing` now keeps the parsed model and reference count of every block whose text and uuid did not change; index-driven rebuilds still start from scratch because link resolution may have changed. A test (`rows_after_a_command_equal_a_full_rebuild`) checks that the reusing path equals a full rebuild.
2. **Opening or reloading a live page re-read all its rows from SQLite and rebuilt them on the UI thread.** A live page shows core's rows, so a reload (triggered by every index event of the page, that is, by each autosave while typing) now only fetches the header and the first chunk, and the rows are built in the background thread (`Prebuilt`) and adopted by the UI thread when the core snapshot is still current. While the user is typing a stale snapshot falls back to the row-reusing refresh.
3. **Fuzzy search read every page title from SQLite per query.** The pool of read connections now keeps one shared title list, invalidated by a generation counter that the writer bumps after each job (`search::TitleCache`, test `cached_fuzzy_titles_follow_writes`). The saving is ~0.1-0.2 ms per query at 5k pages and grows with the page count.
4. **Linked references** ran one breadcrumb query per hit; top-level blocks have no ancestors, so the query is skipped (~10% of the 41k-hit worst case).

Not changed, by decision: structural commands still wait for the core queue on the UI thread (`queue.run`, 2.8-4.0 ms p50 after the row fix) because the caret must land on the result before the next key; typing itself submits `EditText` without waiting (`flush_in_background`).

### 2.3 Interaction on the 5,000-block page

| Metric | Result | Target |
|---|---|---|
| Navigate to the loaded page (UI adopted) | 37-54 ms | < 100 ms |
| Navigate to the end of the paint that shows it | 51 ms (quantized by the 48 ms display pacing; 100 ms when a frame is missed) | < 100 ms |
| Scroll, CPU per frame | p50 4.0-4.6, **p95 5.3-5.5, p99 5.4-5.8, max 6.5** | p99 < 16.7 |
| Journals feed scroll, CPU per frame | p50 4.8, p95 6.3-6.9, p99 7.5-7.8 | p99 < 16.7 |
| Keystroke to end of paint (block in the middle of the page) | p50 3.0-3.3, p95 4.4-4.6 | < 16 |
| Search on the UI thread (palette runs it in the background) | p95 7.7 | < 50 (5k pages) |
| Query widget load (3 widgets, background) | p50 3-5.6, p95 ~14 | |

The spike (ADR-002) measured scroll CPU p95 ~3 ms and edit-to-paint p95 ~2 ms on 1,000 blocks; the 5,000-block live page with real rows, bullets, refs and the editor is within 2x of that.

### 2.4 Memory

RSS 251 MiB with the 5.2k-page graph open (anonymous 84 MiB; the rest is mapped SQLite pages, GPU/font libraries and shared mappings). Opening the 5,000-block page adds 29 MiB (the editor's rows plus the page view's copy). After the whole script the process is at 462-490 MiB RSS (anonymous ~215 MiB); the growth comes from the SQLite page caches of the writer and reader connections filling up (`cache_size = -65536` is 64 MiB per connection) and does not scale with the number of edits: 100 structural commands instead of 20 add only ~30 MiB. Lowering `cache_size` for readers is the knob if memory matters more than speed.

### 2.5 Index and queries, 540k blocks (`large` preset, 10,000 files)

| Metric | Result | Target |
|---|---|---|
| Cold build | **9.7 s**, index 392 MiB | < 60 s |
| Search, 14 queries x 10 | p50 39 ms, **p95 93 ms**, max 100 ms | p95 < 100 ms |
| Page open (first 100 outline rows) | p95 0.25 ms | < 100 ms |
| Linked references, typical page (a few dozen refs) | p95 0.22 ms | < 100 ms |
| Linked references, the most referenced page (41k blocks in 9.8k groups) | p50 330 ms | < 100 ms (not met, see below) |
| `(and [[budget]] [[design]] [[release]])` through the path-refs view | p95 17.5 ms | < 200 ms |
| `(and [[Page 1]] [[Page 2]] [[Page 3]])` | p95 0.3 ms | < 200 ms |
| `(task TODO DOING)`, app cap of 501 rows | p95 2.5 ms | < 200 ms |
| `(between -30d today)`, app cap of 501 rows | p95 1.9 ms | < 200 ms |

Without the row cap `(task TODO DOING)` returns 45k blocks in 215 ms; the app always applies `RESULT_CAP + 1`.

**Decision (ADR-026): `block_path_refs` stays a view.** The three-reference AND query is 11x under budget on 540k blocks, so no materialized `block_path_refs_mat` table is built.

**Not met: linked references of a page with tens of thousands of referencing blocks (330 ms).** The time is the volume (41k block rows with properties), not the join. The right fix is to paginate the linked references section in the UI (the page already loads its outline in chunks); that is recorded as a follow-up rather than changed in the index.

Smoke tests that run in CI: `search_latency_smoke_on_a_1000_page_graph` (p95 < 500 ms in debug, 50 ms in release) and `cold_build_smoke_is_fast_and_consistent` (5k blocks, < 30 s debug / 2 s release, empty `foreign_key_check`, no-op warm reconcile).

## 3. Open items

- Numbers on macOS and Windows reference machines and on a real GPU (this report is Linux software-rendered only); BIT-T-0337 asks for p99 frame time < 16.7 ms on those.
- Paginate linked references for extreme tag pages (above).
- `JournalsView::render_entry` clones the rows of each visible day editor every frame (`views/journals.rs`); journals scroll CPU is 6-7 ms p95 so it is not a hot spot yet.
- A frame-time overlay (`gpui-fps`) was not added: the `perf` module reports the same data without a dependency; revisit if the kit exposes it cheaply.
