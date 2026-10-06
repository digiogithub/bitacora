---
id: BIT-US-0095
type: story
title: Round-trip, golden and mldoc differential test harness
status: done
priority: critical
parent: BIT-EP-0003
milestone: BIT-M-0002
author: mcp
labels: [markdown, compat, test]
estimate: 8
created: 2026-10-06T14:30:45Z
updated: 2026-10-06T17:36:00Z
closed: 2026-10-06T17:36:00Z
---

## Description
As a developer, I want an automated test harness that proves Bitacora's parser and serializer are lossless and agree with Logseq (mldoc 1.5.7 and `tree->file-content`), so that compatibility regressions are caught in CI before they can damage a user's graph.

Covers the test corpus of [[02-markdown-block-syntax]] §11: round-trip fixtures we write ourselves, our own vectors covering the cases Logseq's unit tests exercise (written by us, verified black-box against Logseq/mldoc — never copied or translated), real graphs whose license allows redistribution (the MIT-licensed logseq/docs graph committed as a fixture by BIT-T-0013, ADR-021) and differential testing against mldoc under Node (dev-only tooling, not distributed).

## Acceptance Criteria
- `fixtures/markdown/roundtrip/` contains the 19 fixture categories of §11 with expected trees/properties/refs and, for edits, expected bytes.
- A proptest generates random outlines (indent units, CRLF, blank lines, fences, properties) and asserts `serialize(parse(x)) == x` and tree stability after a single-block edit.
- Golden tests assert canonical bytes for edited blocks (incl. the §7 canonical example).
- A differential job (`tools/mldoc-diff`, Node + `mldoc@1.5.7`, dev-only, not distributed) compares block segmentation, properties and refs over the corpus; CI runs it nightly or on demand.
- The large real-world graph (`fixtures/graphs/logseq-docs/`, pinned logseq/docs commit) parses with block/page counts matching those produced by the mldoc oracle.

## Notes
Implements: BIT-SP-0001.R1–R19 (verification). AGENTS.md §6 testing expectations; [[crate-stack]] CI matrix; ADR-003. ADR-015 (own vectors; Logseq/mldoc only as black-box oracle; fixture graphs only if redistributable). ADR-021 (logseq/docs is MIT and usable as fixture).
