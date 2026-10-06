---
id: BIT-US-0061
type: story
title: "Page rename: file, title::, namespace children and config.edn"
status: backlog
priority: high
parent: BIT-EP-0009
milestone: BIT-M-0003
author: mcp
labels: [core, compat, rename]
estimate: 8
created: 2026-10-06T14:29:01Z
updated: 2026-10-06T14:29:01Z
---

## Description
As a user, I want renaming a page to rename its file, its namespace children and the config entries pointing at it exactly as Logseq does, so that the graph stays consistent for both apps and git shows a clean rename.

Follows `rename!` → `rename-page-aux` (`handler/page.cljs:450-645`): file renamed in place (same dir and extension) with `compute-new-file-path` (`page.cljs:191-199`); `title::`/front-matter `title:` rewritten when the first block mentions the old name (`page.cljs:460-471`); namespace children renamed with the prefix replaced once (`rename-namespace-pages!`, `:550-566`); pages whose titles contain `[[old]]` / `[[old/` renamed (`rename-nested-pages`, `:516-548`); case-only rename updates title + file; `:favorites` and `:default-home :page` updated (`:487-500`); journals never renamed on disk (`:486`). Reference rewriting across blocks is a separate story.

## Acceptance Criteria
- `Old` → `New Idea`: `pages/Old.md` becomes `pages/New Idea.md` (atomic rename), no other file name changes.
- `a` → `b` renames `pages/a___x.md`, `pages/a___y.md` to `pages/b___x.md`, `pages/b___y.md`.
- `foo` → `Foo` (case-only) renames the file on case-insensitive and case-sensitive file systems.
- `pages/foo.md` with `title:: foo` is rewritten to `title:: bar`; front matter `title: foo` → `title: bar`.
- `:favorites ["Old"]` and `:default-home {:page "Old"}` become `"New"`, rest of `config.edn` byte-identical.
- Renaming a journal page does not move its file.
- The whole rename is one undoable transaction.

## Notes
Implements: BIT-SP-0002.R13, BIT-SP-0002.R4, BIT-SP-0002.R11
See [[01-file-graph-layout]] §3.4, §10.2, open questions 2 and 7; [[04-editor-outliner-operations]]; [[block-editor]]. ADR-011, ADR-013.
