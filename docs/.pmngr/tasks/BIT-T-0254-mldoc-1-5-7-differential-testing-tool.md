---
id: BIT-T-0254
type: task
title: mldoc 1.5.7 differential testing tool
status: backlog
parent: BIT-US-0095
milestone: BIT-M-0002
author: mcp
labels: [bitacora-markdown, test, tooling]
estimate: 3
created: 2026-10-06T14:32:01Z
updated: 2026-10-06T14:32:01Z
---

## Description
Create `tools/mldoc-diff/` (Node, `package.json` pinning `mldoc@1.5.7`): `dump.mjs <file>` runs `parseJson` with the `default-config` JSON from `deps/graph-parser/src/logseq/graph_parser/mldoc.cljc:57-75` and emits a normalized JSON summary — block start offsets (`start_pos`), levels, marker, priority, heading size, properties (`Property_Drawer`/`Properties`), page refs and block refs (walk `Link`, `Nested_link`, `Tag`, `Macro embed`). Add a Rust binary `crates/bitacora-markdown/examples/dump_summary.rs` emitting the same JSON shape, and `tools/mldoc-diff/compare.sh <dir>` that diffs both over a corpus (fixtures, `src/resources/tutorials/tutorial-*.md`, `dummy-notes-*.md`). Add an opt-in CI job (`workflow_dispatch` + nightly) that runs it and uploads the diff.

## Acceptance Criteria
- Running on `fixtures/markdown/roundtrip` reports zero differences (or each difference is listed in an allowlist with a justification, e.g. documented quirks).
- README in the tool dir explains usage; no Node dependency for normal `cargo test`.

## Notes
Part of BIT-US-0095. Verifies BIT-SP-0001.R1–R10. See [[02-markdown-block-syntax]] §11 "Differential testing"; [[03-parsing-indexing-search]].
