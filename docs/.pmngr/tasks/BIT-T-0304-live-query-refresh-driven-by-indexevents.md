---
id: BIT-T-0304
type: task
title: Live query refresh driven by IndexEvents
status: done
priority: medium
parent: BIT-US-0102
milestone: BIT-M-0005
author: mcp
labels: [bitacora-app, bitacora-index, query]
estimate: 2
created: 2026-10-06T14:33:07Z
updated: 2026-10-06T22:20:30Z
closed: 2026-10-06T22:20:30Z
---

## Description
`crates/bitacora-app/src/query_registry.rs`: registry of mounted queries with their dependency summary (referenced page ids, markers, property keys, journal-day range, or "any" for text/unknown). On `IndexEvent::FileReplaced` decide which queries may be affected (page ids touched ∩ deps, or any block changes for text queries) and re-run them debounced 300 ms; unmounted queries are dropped.

## Acceptance Criteria
- `#[gpui::test]`: adding `TODO x [[project-x]]` on another page refreshes a `(and [[project-x]] (task TODO))` query; editing an unrelated page does not re-run it (counter).

## Notes
BIT-SP-0003.R7, BIT-SP-0003.R18. Logseq affected-keys refresh (`src/main/frontend/db/react.cljs:237-361`).
