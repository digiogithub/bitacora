---
id: BIT-T-0097
type: task
title: "Surgical edit API: assoc, update-in and dissoc on the CST"
status: backlog
priority: high
parent: BIT-US-0071
milestone: BIT-M-0002
author: mcp
labels: [bitacora-config, compat]
estimate: 3
created: 2026-10-06T14:29:19Z
updated: 2026-10-06T14:39:30Z
---

## Description
`crates/bitacora-config/src/edit.rs`:
- `assoc(cst, &[Keyword], EdnValue)`: if the key exists, replace only the value node's byte range with the printed new value (EDN printed `pr-str` style: `["a" "b"]`, `{:page "X"}`); else insert ` :key value` before the closing `}` of the target map, using the indentation of the previous entry (newline + same column) when entries are one per line.
- `update_in` for nested map paths (e.g. `[:default-home :page]`).
- `dissoc` removes the key, the value and the whitespace between them, leaving neighbouring comments.
- Helpers used by rename/delete: `favorites_rename(old,new)`, `favorites_remove(name)`, `default_home_rename(old,new)`.

## Acceptance Criteria
- Golden tests: our self-authored commented fixture config (`tests/data/commented-config.edn`) + `:favorites ["Projects"]` → single-line diff; insert `:default-home` → only an inserted line; `dissoc :macros` leaves surrounding comments.
- Edit then `EdnValue` re-read equals expected semantic value.

## Notes
Refs BIT-SP-0002.R4, used by page rename (BIT-SP-0002.R13). ADR-015 (no copy of Logseq's config template in tests).
