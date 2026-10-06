---
id: BIT-US-0088
type: story
title: Page identity, duplicate-title resolution and read-only .org pages
status: backlog
priority: high
parent: BIT-EP-0004
milestone: BIT-M-0002
author: mcp
labels: [core, compat]
estimate: 3
created: 2026-10-06T14:30:18Z
updated: 2026-10-06T14:30:18Z
---

## Description
As a user, I want `[[Foo]]`, `[[foo]]` and NFD/NFC variants to resolve to the same page, duplicate files to be reported (not merged), and `.org` pages to be visible read-only, so that Bitacora's page set matches Logseq's.

## Acceptance Criteria
- Page key = `page_name_sanity_lc(title)` (lower-case, one boundary `/` stripped, NFC).
- `pages/Foo.md` + `pages/sub/foo.md`: first in `filter-files` order wins; a `DuplicateTitle { kept, skipped }` diagnostic names both paths.
- `pages/Meeting.org` with `#+TITLE: Weekly Meeting` is a read-only page; `[[Weekly Meeting]]` resolves to it; it is never written.
- `pages/spec.adoc` produces no page.

## Notes
Implements: BIT-SP-0002.R8, BIT-SP-0002.R20
[[01-file-graph-layout]] §3.1, §3.6, §7; Logseq `handler/repo.cljs:216-238`. [[architecture]]
