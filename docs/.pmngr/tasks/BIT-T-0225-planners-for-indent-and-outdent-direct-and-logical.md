---
id: BIT-T-0225
type: task
title: Planners for Indent and Outdent (direct and logical)
status: backlog
priority: critical
parent: BIT-US-0034
milestone: BIT-M-0003
author: mcp
labels: [bitacora-core, outliner]
estimate: 3
created: 2026-10-06T14:31:33Z
updated: 2026-10-06T14:31:33Z
---

## Description
In `crates/bitacora-core/src/commands/indent.rs`: `plan_indent(ids)` — reduce to top-level consecutive siblings (else `Refusal`), `Move` each as last child of the previous sibling of the first; if that sibling has `collapsed:: true`, add an `EditText` removing it. `plan_outdent(ids, logical)` — `Move` after the parent; in direct mode also `Move` the following siblings to become trailing children of the last moved block. No-op (empty plan) for first child indent / top-level outdent.

## Acceptance Criteria
- Op tests for scenarios of BIT-SP-0004.R9 including collapsed sibling and logical mode.
- Serializer output shows only leading indentation changes for clean moved blocks.

## Notes
Story BIT-US-0034. Implements BIT-SP-0004.R9. Logseq `core.cljs:802-854`.
