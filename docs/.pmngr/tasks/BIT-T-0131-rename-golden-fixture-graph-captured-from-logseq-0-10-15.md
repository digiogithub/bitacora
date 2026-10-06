---
id: BIT-T-0131
type: task
title: Rename golden fixture graph captured from Logseq 0.10.15
status: backlog
parent: BIT-US-0082
milestone: BIT-M-0003
author: mcp
labels: [bitacora-core, test, rename, fixtures]
estimate: 3
created: 2026-10-06T14:29:56Z
updated: 2026-10-06T14:29:56Z
---

## Description
Create `fixtures/graphs/rename/` (before) with pages covering: `[[Old]]`, `#Old`, `#[[Old]]`, `[[old]]` (case variant), `old::` key, `tags:: Old, x`, namespace `Old/child`, nested-title page `Notes on [[Old]]`, refs in code spans, legacy and triple-lowbar variants. Run the rename `Old` → `New Idea` in Logseq 0.10.15 and commit the resulting files as `fixtures/graphs/rename.expected/`. Test `crates/bitacora-core/tests/rename_golden.rs` runs Bitacora's rename and compares per-block text (normalising Logseq's whole-file re-serialization: compare parsed block contents and file paths; untouched blocks must be byte-identical to the before state).

## Acceptance Criteria
- Test passes; differences intentionally chosen by Bitacora (e.g. case-insensitive `[[old]]`) are listed in an allowlist with justification.
- README in fixture dir describes how the expected output was produced.

## Notes
Epic AC: rename cascade matches Logseq on the fixture graph. BIT-SP-0002.R13.
