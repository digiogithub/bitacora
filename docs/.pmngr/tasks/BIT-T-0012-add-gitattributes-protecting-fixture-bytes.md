---
id: BIT-T-0012
type: task
title: Add .gitattributes protecting fixture bytes
status: in_progress
priority: critical
parent: BIT-US-0011
milestone: BIT-M-0001
author: mcp
labels: [infra, fixtures]
estimate: 1
created: 2026-10-06T14:26:00Z
updated: 2026-10-06T16:46:30Z
started: 2026-10-06T16:46:30Z
---

## Description
Create the root `.gitattributes`:
- `* text=auto eol=lf` for source files;
- `fixtures/** -text -diff linguist-vendored` (binary semantics: no EOL conversion, no BOM stripping) — use `-text` but keep `diff` if readable diffs are preferred; decide and document in the file comment;
- `*.png *.jpg *.pdf binary`.
Must be committed **before** any fixture file so the first commit already stores exact bytes.

## Acceptance Criteria
- A CRLF fixture committed on Linux checks out with CRLF intact on `windows-latest` (`core.autocrlf=true`) — verified by the fixture checksum job.
- `git check-attr -a fixtures/graphs/edge-cases/pages/crlf.md` shows `text: unset`.

## Notes
- AGENTS.md §3 rule 1 and 4 (preserve line endings). [[02-markdown-block-syntax]] edge-case table (CRLF, BOM).
