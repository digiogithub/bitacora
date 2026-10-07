---
created_at: 2026-10-07T12:00:00.000000000Z
updated_at: 2026-10-07T12:00:00.000000000Z
tags:
    - change
    - graph
    - config
    - app
---
# Graph settings panel persisted in config.edn, SVG and PNG export

Implements BIT-US-0158 (BIT-T-0478, BIT-T-0479) and BIT-US-0160 (BIT-T-0483) of [[bitacora-v2-plan]]; continues the graph view of BIT-US-0157/0159 ([[graph-view]]).

## What changed
- `bitacora-config` (`src/graph_view.rs`): `GraphViewSettings`, `GraphToggle`, `GraphForce`, `EffectiveConfig::graph_view_settings()`, `ConfigEditor::{set_graph_toggle, set_graph_force, reset_graph_forces}`. Logseq 0.10.x keys `:graph/settings {:journal? :orphan-pages? :builtin-pages? :excluded-pages?}` and `:graph/forcesettings {:link-dist :charge-strength :charge-range}`; tolerant reads, comment-preserving writes.
- `bitacora-index` (`read/graph.rs`): `GraphFilter::show_excluded` (Logseq's `:excluded-pages?` shows the pages with `exclude-from-graph-view:: true`).
- `bitacora-app`:
  - `views/settings/graph_config.rs`: `GraphEdit::{GraphToggle, GraphForce, GraphForcesReset}` (live apply mode) through the existing `edit_config` -> command queue path.
  - `views/graph_view.rs` + `views/graph_view/panel.rs`: `GraphSettings::from_prefs`, settings state (`prefs`, `queue`, serialised `pending`/`saving` writes), the floating panel (Nodes, Search, Forces, Export) built from the component kit, label search (`search_mask`), pause, `toggle_pref`, `set_force`, `step_force`, `reset_forces`, `scene`, `export`, `write_export`.
  - `graph_view/export.rs`: GPUI-free `Scene` with `to_svg` (own writer) and `to_png` (`tiny-skia`, no labels), `Format`.
  - `graph_view/prefs.rs`: Logseq slider ranges and steps, query helpers.
  - `views/main_view.rs`: `set_session_link` hands the command queue to the graph view.
  - Locale keys `graph_view.*` (en, es).
- Workspace: `tiny-skia = "=0.11.4"` (already in the lockfile through gpui -> resvg; BSD-3-Clause; `cargo deny check` passes).
- `docs/design/graph-view.md`: panel, persistence and export sections.

## Why
Users want Logseq's graph settings with their choices saved in the graph's config (BIT-SP-0012.R5) and a picture of the graph (R6). Edits go through the single-writer queue with comment-preserving splices so unrelated bytes of `config.edn` never change.

## Decisions
- PNG via `tiny-skia` (open question of the plan resolved: no new dependency, size neutral). Labels only in the SVG, the PNG has none (no text shaping in `tiny-skia`).
- Settings use stepper buttons (the kit has no slider).
- None of the graph keys feed the index `config_hash`, so saving never reindexes.

## Verification
`cargo test -p bitacora-config -p bitacora-index -p bitacora-app --locked` (config: 6 new unit tests; index: excluded-pages case; app: export, prefs and panel unit tests plus six gpui tests incl. persistence through a real session queue), `cargo clippy -p bitacora-config -p bitacora-index -p bitacora-app --all-targets --locked -- -D warnings`, `cargo deny check`. Manual check of the save dialog and the panel look on real hardware not done.
