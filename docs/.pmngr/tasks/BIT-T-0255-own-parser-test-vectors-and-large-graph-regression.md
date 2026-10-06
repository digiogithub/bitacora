---
id: BIT-T-0255
type: task
title: Own parser test vectors and large-graph regression
status: backlog
parent: BIT-US-0095
milestone: BIT-M-0002
author: mcp
labels: [bitacora-markdown, test, fixtures]
estimate: 3
created: 2026-10-06T14:32:01Z
updated: 2026-10-06T14:38:19Z
---

## Description
Write our own input→expected vectors as Rust tests in `crates/bitacora-markdown/tests/compat/` covering the documented cases of [[02-markdown-block-syntax]] (outline/headings vs bullets and irregular indent, block content extraction, property extraction and value interpretation, refs via content/properties/page properties, markers, priorities, clock/logbook, diff-merge content), with expected results verified black-box against Logseq 0.10.15 / mldoc 1.5.7. Do not copy or translate Logseq or mldoc test files (behaviour areas may cite `path:line` in the reference checkout `../logseq` tag `0.10.15` for context only).

Add an ignored-by-default test `logseq_docs_graph` that loads an external graph from env `LOGSEQ_DOCS_GRAPH` (e.g. a local clone of logseq/docs, never committed unless BIT-T-0013's license check allows it) and checks block/page counts against values produced by the dev-only mldoc oracle (`tools/mldoc-diff`), plus byte round-trip of every file. For tutorial-style content, use our own generated fixture graph (`fixtures/graphs/tutorial-like/`) instead of copying Logseq's `src/resources/tutorials/` files.

## Acceptance Criteria
- All our vectors pass.
- Large-graph test passes when the env var is set; CI nightly runs it.

## Notes
Part of BIT-US-0095. Verifies BIT-SP-0001.R1–R15. ADR-015: Logseq/mldoc are AGPL — no code, test files or bundled content copied/translated; Logseq/mldoc only as black-box oracle in dev-only tooling (AGENTS.md rule 8).
