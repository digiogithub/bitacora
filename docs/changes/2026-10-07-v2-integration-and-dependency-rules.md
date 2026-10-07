---
created_at: 2026-10-07T14:21:28.900540417Z
updated_at: 2026-10-07T14:21:28.900540417Z
tags:
    - change
    - v2
    - xtask
    - integration
---
# v2 integration: final merge and dependency rules

Coordinator change after merging all v2 story branches (continues [[bitacora-v2-plan]]).

- `xtask/src/deps.rs`: allowed `bitacora-pando -> bitacora-markdown` (privacy review reads block syntax, BIT-US-0154) and `bitacora-runtime -> pando-rs` (runtime re-exports AI types from the generic SDK, ADR-027). Both follow the documented direction (markdown at the bottom, pando-rs a leaf).
- Merge-conflict resolutions done by the coordinator: `Session::apply_pando_consent` also updates the agents' consent snapshot (`AgentContext` behind `parking_lot::RwLock`) so a live revoke blocks chat and agent runs; `Route::Graph`/`Route::Tasks` covered in the top-bar tab labels; locale and ADR table merges.
- `docs/plans/bitacora-v2-plan.md`: execution status section.

Verification (Linux): `cargo fmt --check`, `cargo clippy --workspace --all-targets -D warnings`, `cargo test --workspace` 1982 passed / 0 failed, `cargo xtask check-deps` OK, `cargo xtask tokens --check` up to date, `cargo test -p xtask` 24 passed.

Open owner items: [[owner-manual-validation-checklist]] section V, [[release-checklist-2.0]], [[ai-privacy-review]] §5.
