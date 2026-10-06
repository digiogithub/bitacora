---
id: BIT-T-0263
type: task
title: Typing coalescing of EditText into word/pause-sized undo steps
status: backlog
priority: high
parent: BIT-US-0039
milestone: BIT-M-0003
author: mcp
labels: [bitacora-core, undo]
estimate: 2
created: 2026-10-06T14:32:23Z
updated: 2026-10-06T14:32:23Z
---

## Description
In `History::push`, merge a transaction with `coalesce: Some(CoalesceKey { block, run })` into the top entry when: same block, top is a typing entry, Δt < 1.5 s since the last keystroke, no structural op in between, and not (word boundary after a pause ≥ 700 ms). Merging concatenates adjacent `EditText` ranges into one op when contiguous, otherwise appends. Uses an injectable clock.

## Acceptance Criteria
- Scenarios of BIT-SP-0004.R18 pass with a fake clock.
- Coalesced entry's inverse restores the exact pre-typing text.

## Notes
Story BIT-US-0039. Implements BIT-SP-0004.R18.
