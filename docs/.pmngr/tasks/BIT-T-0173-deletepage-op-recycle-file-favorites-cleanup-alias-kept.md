---
id: BIT-T-0173
type: task
title: "DeletePage op: recycle file, favorites cleanup, alias-kept entity"
status: backlog
parent: BIT-US-0089
milestone: BIT-M-0003
author: mcp
labels: [bitacora-core, bitacora-config, delete]
estimate: 3
created: 2026-10-06T14:30:53Z
updated: 2026-10-06T14:30:53Z
---

## Description
`crates/bitacora-core/src/lifecycle/delete.rs`: `Op::DeletePage(page)` transaction = `RecycleFile` (if the page has a file) + `EditConfig` removing it from `:favorites` (task in rename story provides helper) + model update: if another page lists it in `alias::`, keep a virtual entity with attributes stripped (`page.cljs:369-376`); else drop it (it reappears as virtual if still referenced). Do not rewrite references. Journals can be deleted the same way. External deletes detected by the watcher only update the model (`watcher_handler.cljs:104-110`), never touch disk.

## Acceptance Criteria
- Tests: delete `foo` with `:favorites ["foo"]` → file recycled, favorites `[]`, `see [[foo]]` unchanged and resolves virtually.
- Aliased page stays as a virtual entity.
- External delete event removes the page from the model without writes.

## Notes
BIT-SP-0002.R14, R4.
