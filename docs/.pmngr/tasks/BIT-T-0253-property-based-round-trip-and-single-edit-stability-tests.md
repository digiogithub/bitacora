---
id: BIT-T-0253
type: task
title: Property-based round-trip and single-edit stability tests
status: backlog
parent: BIT-US-0095
milestone: BIT-M-0002
author: mcp
labels: [bitacora-markdown, test, proptest]
estimate: 2
created: 2026-10-06T14:32:01Z
updated: 2026-10-06T14:32:01Z
---

## Description
Add `crates/bitacora-markdown/tests/proptest_roundtrip.rs` using `proptest`. Strategy generates outlines: random depth sequence (including irregular outdents), indent unit (tab / 2 / 4 spaces / mixed), LF or CRLF, optional BOM, optional front matter or property pre-block, blank lines between/inside blocks, fenced code with bullet-like and property-like lines, property groups (valid/invalid keys), refs, markers, SCHEDULED, LOGBOOK. Properties checked:
1. `serialize(parse(x)) == x`;
2. after editing one random block's title, re-parsing yields the same tree shape and all other blocks' raw spans are byte-identical;
3. canonical output of an edited page re-parses to the same model (idempotence: `canon(parse(canon(m))) == canon(m)`).
Keep shrinking-friendly generators; run 512 cases by default, 10k in nightly CI (`PROPTEST_CASES`).

## Acceptance Criteria
- Test passes locally and in CI on Linux/macOS/Windows.
- A deliberately injected bug (e.g. dropping trailing whitespace) is caught (documented in the PR).

## Notes
Part of BIT-US-0095. Verifies BIT-SP-0001.R1, R11, R12, R19.
