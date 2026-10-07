---
created_at: 2026-10-07T12:00:00Z
updated_at: 2026-10-07T12:00:00Z
tags:
    - change
    - app
    - search
    - pando
---
# Semantic search UI: palette, related blocks, index status (BIT-US-0145)

Continues [[bitacora-v2-plan]] and [[design/semantic-search]] (BIT-US-0144 hybrid search). Implements BIT-SP-0010.R5 and R6 UI.

## What changed
- `bitacora-runtime`: `Session::hybrid()` (clone of the hybrid searcher) and `HybridSearch` re-export.
- `bitacora-pando`: `Ledger::last_error()`; `SemanticStatus` gains `last_error` (no longer `Copy`); CLI status adapted.
- `bitacora-app`:
  - `session.rs`: `SessionLink::hybrid`.
  - `views/palette.rs`: hybrid mode toggle (`set_hybrid`, `set_semantic`), lexical-then-hybrid staged search, `Hit::Block { semantic, stale }` badge, `hits_from_hybrid`, `run_hybrid_search`, "semantic unavailable: ..." hint.
  - `views/related.rs` (new) + `views/right_panel.rs` Context tab: Related blocks section (`RightPanel::set_hybrid`, `Related`).
  - `views/settings/pando.rs` + `mod.rs`: Resync / Remove semantic data buttons (`Pending::PurgeSemantic`), last error in the Activity row.
  - `views/workspace.rs`: passes the searcher to palette and panel.
  - Locales en/es (`palette.semantic_*`, `right.related*`, `settings.pando.resync` ...).
- Docs: section 4 of `docs/design/semantic-search.md`.

## Verification
`cargo clippy -p bitacora-pando -p bitacora-runtime -p bitacora-cli -p bitacora-app --all-targets --locked -- -D warnings` clean; tests for those crates pass (app lib 524, new palette/related/ledger tests). Rendered visuals were not checked by hand (no Pando server in the test host); badge/section layout needs a manual look.
