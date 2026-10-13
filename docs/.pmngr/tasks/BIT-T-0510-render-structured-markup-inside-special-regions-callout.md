---
id: BIT-T-0510
type: task
title: Render structured markup inside special regions (callout, quote, center, verse, unknown)
status: in_review
priority: high
parent: BIT-US-0188
milestone: BIT-M-0006
author: mcp
estimate: 5
created: 2026-10-13T07:57:55Z
updated: 2026-10-13T08:29:22Z
started: 2026-10-13T08:09:22Z
---

## Description
Replace the single `TextLayout` body of `BodyItem::{Quote, Callout, Center, Verse}` with a nested `Vec<BodyItem>` built by the same body builder used for block content, so lists, headings, fences and nested special regions inside `#+BEGIN_X` render structured (Logseq `markup-elements-cp`, `components/block.cljs:3286-3315`). VERSE keeps line breaks. Unknown names (`#+BEGIN_FOO`) render as a plain container with that body instead of a code block (`block.cljs:3308-3312`).

## Acceptance Criteria
- Model tests: NOTE with list + fence + heading; QUOTE with nested NOTE; `#+BEGIN_FOO` -> container, not `Code`.
- `views/block_view.rs` renders nested items inside the callout card / quote rule.
- Round-trip fixture for a nested region added (`cargo xtask fixtures update`), `cargo test -p bitacora-markdown -p bitacora-app` green.
