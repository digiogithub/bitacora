---
created_at: 2026-10-07T16:00:00Z
updated_at: 2026-10-07T16:00:00Z
tags:
    - change
    - graph
    - fix
---
# Graph view: no more perpetual jitter, static off-screen layout

Continues [[design/graph-view.md]] and [[bitacora-v2-plan]]; story BIT-US-0157, task BIT-T-0493.

## Root cause
`bitacora-graph::worker::handle` counted `Pin` messages (one per pointer move) and decremented on the single `Unpin`, so after any drag `alpha_target` stayed 0.3 and the layout never cooled. Secondary: collide is not alpha-scaled, leaving residual velocity at settle; `Pause` / `Reheat`-less sends left `wake_generation` set, and a paused unsettled layout kept requesting frames.

## Change
- `crates/bitacora-graph/src/sim.rs`: `Simulation::settle`, `freeze`, `cool_quickly`, `layout_offscreen` (survivors pinned exactly); `tick` freezes on settle.
- `crates/bitacora-graph/src/worker.rs`: held-node `HashSet`, `RELEASE_COOL_TICKS = 40`; initial snapshot reports `settled`.
- `crates/bitacora-app/src/views/graph_view.rs` (+ `graph_view/panel.rs`): `rebuild(Relayout)` computes layouts on the background executor and `install_layout`s them static; `is_laying_out`; viewport refit on canvas size change; frames only for drag / live sim.
- `docs/design/graph-view.md` updated.

## Verification
`cargo test -p bitacora-graph` (new `tests/settle.rs`: multi-Pin drag regression, exact freeze, pinned survivors, quick cooling), `cargo test -p bitacora-app graph_view` (static initial layout, refresh leaves existing nodes untouched, drag live then frozen), clippy `-D warnings` on both crates.
