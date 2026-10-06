---
id: BIT-T-0056
type: task
title: "Legacy-mode title:: auto-write on page creation"
status: backlog
parent: BIT-US-0028
milestone: BIT-M-0003
author: mcp
labels: [bitacora-core, lifecycle, compat]
estimate: 2
created: 2026-10-06T14:28:27Z
updated: 2026-10-06T14:38:56Z
---

## Description
Re-implement, from the documented behaviour in [[01-file-graph-layout]] §3.3 (Logseq reference for behaviour only: `fs-util/create-title-property?`, `src/main/frontend/util/fs.cljs:198-206`, called from `handler/page.cljs:95-99`), `crates/bitacora-core/src/naming/legacy.rs::needs_title_property(title) -> bool`: true when `legacy_decode(legacy_encode(title)) != title` or the encoded name contains reserved chars. When materialising a page in a legacy graph and this is true, prepend a pre-block `title:: <Original Title>` (canonical pre-block form: trimmed + one blank line) to the first write. Never do this in triple-lowbar graphs at creation time.

## Acceptance Criteria
- `Version 1.0` (legacy) → file `pages/Version 1.0.md` content `title:: Version 1.0\n\n- hello`.
- `My Page` (legacy) → no `title::`.
- `Projects/Bitacora` (legacy) → `title:: Projects/Bitacora` written (round-trip via `%2F` decodes but check per Logseq rule; add a test vector whose expected value is confirmed black-box against Logseq).
- Triple-lowbar graph never auto-writes `title::` on create.

## Notes
[[01-file-graph-layout]] §3.3. BIT-SP-0002.R7. ADR-013. ADR-015 (no Logseq code copied/translated; Logseq only as black-box oracle).
