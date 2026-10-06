---
id: BIT-T-0161
type: task
title: "Read-only .org page support: format detection, #+TITLE and refs"
status: backlog
priority: medium
parent: BIT-US-0088
milestone: BIT-M-0002
author: mcp
labels: [bitacora-core, compat]
estimate: 2
created: 2026-10-06T14:30:36Z
updated: 2026-10-06T14:30:36Z
---

## Description
- `crates/bitacora-core/src/graph/format.rs`: `FileFormat::from_ext` — `md`/`markdown` → Markdown, `org` → Org, others → none (`graph_parser/util.cljs:188-219`).
- `crates/bitacora-core/src/org/mod.rs`: minimal Org scanner extracting `#+TITLE:` (case-insensitive), `#+ALIAS:`, `#+TAGS:`, headline lines (`*`, `**`…) as blocks with raw text, and `[[…]]` refs (reuse the inline ref scanner from `bitacora-markdown` on headline/body text).
- Pages backed by `.org` get `read_only = true`; the command queue rejects edit `Op`s on them with a typed error.

## Acceptance Criteria
- `pages/Meeting.org` with `#+TITLE: Weekly Meeting` → page `Weekly Meeting`, `[[Weekly Meeting]]` resolves.
- Attempted edit returns `CoreError::ReadOnlyPage`; file bytes unchanged.
- `.adoc` ignored.

## Notes
Refs BIT-SP-0002.R20. Non-goal: org editing ([[architecture]] §1).
