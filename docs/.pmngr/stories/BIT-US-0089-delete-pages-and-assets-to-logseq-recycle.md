---
id: BIT-US-0089
type: story
title: Delete pages and assets to logseq/.recycle
status: backlog
priority: high
parent: BIT-EP-0009
milestone: BIT-M-0003
author: mcp
labels: [core, compat, delete]
estimate: 3
created: 2026-10-06T14:30:30Z
updated: 2026-10-06T14:30:30Z
---

## Description
As a user, I want deleted pages and assets to be moved to `logseq/.recycle/` instead of being erased, so that I can recover them and Logseq sees the same behaviour it implements.

`delete!` (`handler/page.cljs:352-383`) → `fs/unlink!` (`frontend/fs.cljs:77-81`) → Electron `:unlink` (`electron/handler.cljs:51-66`): the file is moved to `logseq/.recycle/<rel path with "/" and "\" → "_">`, overwriting an existing entry. References are not rewritten. The page is removed from `:favorites`; if another page aliases it, the entity is kept without attributes (`:369-376`). Deleting an asset from a block also recycles the file (`editor.cljs:1495-1512`).

## Acceptance Criteria
- Deleting `foo` moves `pages/foo.md` → `logseq/.recycle/pages_foo.md`; `pages/sub/bar.md` → `logseq/.recycle/pages_sub_bar.md`.
- An existing `logseq/.recycle/pages_foo.md` is overwritten.
- `see [[foo]]` elsewhere is unchanged and resolves to a virtual page.
- `:favorites` no longer contains `foo`; rest of config unchanged.
- Page aliased by another page stays as a virtual entity.
- Deleting an image block's asset (with confirmation) moves `assets/x.png` to `logseq/.recycle/assets_x.png`.
- No code path calls `remove_file` on user graph files (enforced by a test/grep lint).
- Delete is undoable (file moved back).

## Notes
Implements: BIT-SP-0002.R14, BIT-SP-0002.R4
See [[01-file-graph-layout]] §5, §10.3; [[04-editor-outliner-operations]]. ADR-011.
