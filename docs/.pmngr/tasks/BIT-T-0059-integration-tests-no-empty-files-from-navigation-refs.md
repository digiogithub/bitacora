---
id: BIT-T-0059
type: task
title: "Integration tests: no empty files from navigation, refs, aliases"
status: backlog
parent: BIT-US-0028
milestone: BIT-M-0003
author: mcp
labels: [bitacora-core, test, lifecycle]
estimate: 2
created: 2026-10-06T14:28:27Z
updated: 2026-10-06T14:28:27Z
---

## Description
Add `crates/bitacora-core/tests/lazy_creation.rs` using a temp copy of `fixtures/graphs/basic`. Drive the command queue: open a page via ref, create a ref `[[New Page]]`, set `alias:: Mine` on a page, open namespace page `a/b/c`, type a blank block and delete it. Snapshot the directory tree (paths + hashes) before/after.

## Acceptance Criteria
- Directory snapshot unchanged except for the files whose blocks received non-blank content.
- No `pages/Mine.md`, `pages/a.md`, `pages/a___b.md` or `pages/New Page.md` created.
- New files are UTF-8 without BOM, LF only (assert bytes).

## Notes
BIT-SP-0002.R12, R16. Epic AC: "No empty page files are created by navigation or references".
