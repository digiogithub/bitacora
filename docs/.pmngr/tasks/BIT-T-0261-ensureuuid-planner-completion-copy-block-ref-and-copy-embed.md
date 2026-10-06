---
id: BIT-T-0261
type: task
title: EnsureUuid planner, (( completion, copy block ref and copy embed
status: done
priority: high
parent: BIT-US-0038
milestone: BIT-M-0003
author: mcp
labels: [bitacora-core, bitacora-app, autocomplete]
estimate: 3
created: 2026-10-06T14:32:23Z
updated: 2026-10-06T21:12:28Z
closed: 2026-10-06T21:12:28Z
---

## Description
`crates/bitacora-core/src/commands/uuid.rs`: `plan_ensure_uuid(id)` → if the block has no `id::`, `EditText` inserting `id:: <uuid v4>` after the title line (before other properties, Logseq order) using the file's continuation prefix; uniqueness checked against the graph index. Block completion commits a transaction containing the buffer `EditText` (`((uuid))`) and `EnsureUuid(target)` (possibly on another page). `Mod+C` in edit mode with empty selection → EnsureUuid + clipboard `((uuid))`; `Mod+E` → `{{embed ((uuid))}}`.

## Acceptance Criteria
- Golden test: target page diff is exactly one added `  id:: …` line.
- One undo removes both the reference and the `id::`.
- Existing `id::` → target page untouched.

## Notes
Story BIT-US-0038. Implements BIT-SP-0004.R16. ADR-006. Logseq `editor.cljs:955-973`, `:1944-1966`.
