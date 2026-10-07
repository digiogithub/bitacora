---
created_at: 2026-10-07T12:00:00Z
updated_at: 2026-10-07T12:00:00Z
tags:
    - change
    - graph
    - index
---
# Graph data read API and force layout engine

Continues [[bitacora-v2-plan]] (D8, ADR-034); stories BIT-US-0155 and BIT-US-0156 (tasks BIT-T-0470..0474).

## What changed
- `bitacora-index`: new `read/graph.rs` with `IndexReader::graph_data` and `local_graph_data`, types `GraphFilter`, `GraphData`, `GraphDataNode`, `GraphDataEdge` (re-exported from the crate root). Edges from `block_page_refs` kinds 1/2/3/8, `page_tags`, namespace parents; placeholders included; self-links, UUID-like and `assets/` names, `exclude-from-graph-view::` pages and configured excluded pages removed; journals / orphans filters; degree and tag / namespace-parent flags. Tests: `crates/bitacora-index/tests/read_graph.rs` (incl. incremental == rebuild).
- New leaf crate `bitacora-graph` (no bitacora or GPUI deps): `Simulation` (quadtree Barnes-Hut many-body theta 0.5 with distance max = charge range, link springs, grid-accelerated collide r 26 x2, x/y gravity 0.02, alpha cooling, velocity decay 0.5; Logseq defaults in `ForceParams::default`), deterministic seed (phyllotaxis start + LCG jiggle), `SimulationHandle` worker thread with `Control` messages (`SetParams`, `Pause`, `Reheat`, `Pin`, `Unpin`) publishing `Snapshot { positions: Arc<[[f32; 2]]>, .. }`. Tests in `crates/bitacora-graph/tests/sim.rs` (determinism, finite positions and cooling proptests, energy decrease, collide, pinning, worker, 5k-node >= 60 ticks/s gate in release, about 140 ticks/s measured).
- Docs: ADR-034 row and crate table in `docs/architecture.md`, dependency-direction line in `AGENTS.md`, graph notes in `docs/design/sqlite-index-schema.md` section 5.1.
- Tooling: `xtask check-deps` allows `bitacora-graph` (leaf; used by runtime and app); CI `test-core` job runs `-p bitacora-graph`; workspace member and dependency entry.

## Why
Graph view (BIT-SP-0012) needs filtered graph data from the index and an off-UI-thread layout engine that does not pull GPUI into core crates.

## Verification
`cargo clippy -p bitacora-graph -p bitacora-index --all-targets --locked -- -D warnings`, `cargo test -p bitacora-graph -p bitacora-index --locked`, `cargo test -p bitacora-graph --release`, `cargo test -p xtask`, `cargo xtask check-deps`.
