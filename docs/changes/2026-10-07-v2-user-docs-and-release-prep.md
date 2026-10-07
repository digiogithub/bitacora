---
created_at: 2026-10-07T12:00:00Z
updated_at: 2026-10-07T12:00:00Z
tags:
    - change
    - docs
    - release
---
# v2 user documentation and release preparation (BIT-US-0163, BIT-T-0487)

Part of [[bitacora-v2-plan]]. Documentation half only: nothing was tagged, pushed or published.

## What changed
- New `docs/user/` guide: `README.md`, `whats-new-2.0.md`, `upgrade-from-1x.md`, `ui-tour.md`, `graph-view-guide.md`, `pando-setup.md` (modes, consent, exclusions, shared-KB note, privacy), `ai-features.md` (semantic search, CLI, chat and approvals, review, recommendations, ghost text, compose).
- New `CHANGELOG.md` with the 2.0.0 entry (unreleased).
- `README.md` and `docs/README.md` link the guide and changelog.
- `docs/design/release-checklist-2.0.md`: owner checklist for bump, tag, signing, publishing.

## Why
No user docs existed. Content was written from the v2 design docs and `docs/changes/2026-10-07-*.md`.

## Open points
- Journal review and recommendation UIs (BIT-US-0151/0152) are not in the app yet; the guide says so.
- Workspace version is `0.1.0`; the bump to 2.0.0 is left to the owner.

## Verification
Docs only; facts cross-checked against the design docs, settings defaults and CLI source (`cmd/semantic.rs`, keymap).
