---
id: BIT-T-0054
type: task
title: Virtual page state and lazy path assignment in the page model
status: done
parent: BIT-US-0028
milestone: BIT-M-0003
author: mcp
labels: [bitacora-core, lifecycle]
estimate: 3
created: 2026-10-06T14:28:27Z
updated: 2026-10-06T19:13:16Z
closed: 2026-10-06T19:13:16Z
---

## Description
In `crates/bitacora-core/src/model/page.rs` add a `PageBacking` enum: `Virtual` (referenced/alias/namespace parent/property page; no file), `File { path, format }`, `ReadOnly { path }` (org). Add `crates/bitacora-core/src/lifecycle/create.rs` with `fn assign_path(page, config) -> Result<RelPath>` mirroring `transact-file-tx-if-not-exists!` (`src/main/frontend/modules/file/core.cljs:115-142`):
- journal → `<:journals-directory>/<format(:journal/file-name-format)>.md` (delegate to journal naming from BIT-EP-0004);
- otherwise → `<:pages-directory>/<file_name_sanity(original_name)>.md` using the active codec (triple-lowbar or legacy) from `bitacora-core::naming`.
- If a page with the same key is already backed by `.org`, return `Err(ReadOnlyBacking)`.
The writer only materialises a `Virtual` page when the serialized block tree is non-blank (`core.cljs:146-166`): a single empty block (`-`) is blank. The transition happens inside the `Op` transaction applied by the command queue (single writer); no direct fs access.

## Acceptance Criteria
- Unit tests: referenced page stays `Virtual` after open/close; first non-blank save assigns `pages/Projects___Bitacora.md` for `Projects/Bitacora`; namespace parent `Projects` stays `Virtual`.
- Legacy graph assigns `pages/Projects%2FBitacora.md`.
- Org-backed page returns `ReadOnlyBacking` and writes nothing.
- File name keeps original case (`My Page` → `pages/My Page.md`).

## Notes
Refs [[01-file-graph-layout]] §10.1, §7. BIT-SP-0002.R12. ADR-013.
