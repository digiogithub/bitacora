---
id: BIT-T-0082
type: task
title: Ensure-today-journal service with template expansion
status: done
parent: BIT-US-0057
milestone: BIT-M-0003
author: mcp
labels: [bitacora-core, journals]
estimate: 3
created: 2026-10-06T14:28:51Z
updated: 2026-10-06T19:13:24Z
closed: 2026-10-06T19:13:24Z
---

## Description
Add `crates/bitacora-core/src/lifecycle/journal_today.rs`: `fn ensure_today(graph, clock: &dyn Clock) -> PageId`. If `:feature/enable-journals?` is not false and no page exists for today's journal-day, create a `Virtual` journal page with one block: empty, or the blocks of the template block named by `:default-templates {:journals "<name>"}` (find the block with `template:: <name>`, copy its children without the `template::`/`template-including-parent::` property, like `handler/repo.cljs:89-100`). Mark the page `pristine` with a hash of the template content. `Clock` trait injected for tests; schedule a re-check at local midnight (app layer calls `ensure_today` on day change).

## Acceptance Criteria
- Unit tests with a fixed clock (2025-11-14): page key `nov 14th, 2025`, journal-day `20251114`, backing `Virtual`.
- Template `daily` with two child blocks expands into two blocks without the `template::` line.
- `:feature/enable-journals? false` → no page created.
- Existing `journals/2025_11_14.md` is reused, not recreated.

## Notes
[[01-file-graph-layout]] §4 (`handler/page.cljs:820-852`). BIT-SP-0002.R11.
