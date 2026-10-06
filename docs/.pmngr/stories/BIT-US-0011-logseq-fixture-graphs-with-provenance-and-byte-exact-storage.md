---
id: BIT-US-0011
type: story
title: Logseq fixture graphs with provenance and byte-exact storage
status: backlog
priority: critical
parent: BIT-EP-0001
milestone: BIT-M-0001
author: mcp
labels: [infra, fixtures, testing]
estimate: 5
created: 2026-10-06T14:25:33Z
updated: 2026-10-06T15:16:54Z
---

## Description
As a developer of the parser, index and sync crates, I want real and edge-case Logseq graphs committed under `fixtures/graphs/` with recorded provenance and protection against byte changes, so that round-trip (`serialize(parse(bytes)) == bytes`) and compatibility tests run against realistic data from day one.

## Acceptance Criteria
- `fixtures/graphs/logseq-docs/` (a snapshot of the MIT-licensed Logseq docs graph: `pages/`, `journals/`, `logseq/config.edn` from a pinned commit, its `LICENSE.md`, no media; ADR-021) and `fixtures/graphs/edge-cases/` exist, each with a `PROVENANCE.md` stating source URL, commit/tag, date, license and any modifications.
- `.gitattributes` marks `fixtures/**` as `-text` (no EOL normalisation) so CRLF/BOM/tab fixtures stay byte-exact on every OS checkout, including Windows CI.
- A manifest with SHA-256 per file is verified by a test/xtask command in CI.
- A shared test helper lets any crate's tests iterate fixture files without hard-coding paths.

## Notes
- Epic acceptance: fixture graphs with provenance notes.
- Edge cases from [[02-markdown-block-syntax]] (§2.1 indentation, §2.3 continuation lines, §2.5 pre-block, edge-case table: CRLF, mixed tabs/spaces, BOM, unclosed fences, escaping) and [[01-file-graph-layout]] (triple-lowbar vs legacy file names, journals, namespaces, assets, `logseq/bak`, `.recycle`).
- AGENTS.md §3 rule 1 and §6 (round-trip property tests over `fixtures/graphs/**`). ADR-003, ADR-013, ADR-021.
