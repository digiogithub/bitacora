---
id: BIT-T-0107
type: task
title: Implement tri-lb-file-name-sanity encoder
status: backlog
priority: critical
parent: BIT-US-0081
milestone: BIT-M-0002
author: mcp
labels: [bitacora-core, compat]
estimate: 3
created: 2026-10-06T14:29:47Z
updated: 2026-10-06T14:29:47Z
---

## Description
`crates/bitacora-core/src/naming/triple_lowbar.rs`, `pub fn encode(title: &str) -> String`, steps in order (each on the previous output):
1. `page_name_sanity`: strip one leading and one trailing `/`, NFC (case preserved) — `graph_parser/util.cljs:134-142`.
2. Pre-escape `%[0-9a-fA-F]{2}` → `%25XX`; bare `%` untouched — `fs.cljs:90-92,131`.
3. Each run of ``[: * ? " < > | # \\]`` → `encodeURIComponent(run)` with `*`→`%2A` (uppercase hex) — `fs.cljs:76-79,118-123,132`.
4. Leading `.` → `%2E` — `fs.cljs:133`.
5. If body is exactly a Windows reserved name (`CON PRN AUX NUL COM1-9 LPT1-9`, case-sensitive) or ends with `.`, append `/` — `fs.cljs:106-116,134`.
6. `___`→`%5F%5F%5F`, `_/`→`%5F/`, `/_`→`/%5F` (JS `replace` with global regex semantics, left-to-right, non-overlapping), then `/`→`___` — `fs.cljs:94-104,135`.

Put `page_name_sanity` / `page_name_sanity_lc` in `crates/bitacora-core/src/naming/sanity.rs` (shared with identity).

## Acceptance Criteria
- Unit tests for every row of [[01-file-graph-layout]] §3.2 table.
- No length limit, no lowercasing; spaces, Unicode, `,'!&+[](){}=;@$~` and backtick untouched.

## Notes
Refs BIT-SP-0002.R6. Check JS regex replace semantics carefully for overlapping `_/_` cases (e.g. `a_/_b`).
