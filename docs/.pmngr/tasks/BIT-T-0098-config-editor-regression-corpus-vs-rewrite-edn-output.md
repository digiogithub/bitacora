---
id: BIT-T-0098
type: task
title: Config editor regression corpus vs rewrite-edn output
status: backlog
priority: medium
parent: BIT-US-0071
milestone: BIT-M-0002
author: mcp
labels: [bitacora-config, test, compat]
estimate: 2
created: 2026-10-06T14:29:19Z
updated: 2026-10-06T14:39:30Z
---

## Description
Build `crates/bitacora-config/tests/edit_golden.rs` with a corpus of (input config, edit op, expected output) cases. Generate expected outputs once with `borkdude/rewrite-edn` (Babashka script in `tools/rewrite-edn-oracle/`, documented, dev-only, not run in CI) applying `r/assoc`/`r/assoc-in`/`r/update` like Logseq's `handler/config.cljs:9-31`, and commit the outputs. All input configs are self-authored (no Logseq template text).

Cases: favorites add/remove on our commented fixture config, default-home set/rename, nested map key add, key at end of file with trailing comment, CRLF config, config with `#_` discards.

## Acceptance Criteria
- ≥ 12 golden cases committed and passing.
- Any intentional deviation from rewrite-edn whitespace is documented in the test file.

## Notes
Refs BIT-SP-0002.R4. ADR-015 (own input configs; oracle tooling dev-only).
