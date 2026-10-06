---
id: BIT-T-0252
type: task
title: Round-trip fixture corpus and fixture-driven test runner
status: done
parent: BIT-US-0095
milestone: BIT-M-0002
author: mcp
labels: [bitacora-markdown, test, fixtures]
estimate: 3
created: 2026-10-06T14:32:01Z
updated: 2026-10-06T17:36:00Z
closed: 2026-10-06T17:36:00Z
---

## Description
Create `fixtures/markdown/roundtrip/NN-name/` for the 19 categories in [[02-markdown-block-syntax]] §11 ("Round-trip fixtures to write ourselves"), each with `input.md`, `expected.json` (blocks: offsets, levels, parent, head, properties, refs) and optional `edit.json` + `expected-after-edit.md`. Write `crates/bitacora-markdown/tests/fixtures.rs` that discovers all fixture dirs (plus every `.md` under `fixtures/graphs/**`) and asserts: (1) byte round-trip, (2) parsed structure equals `expected.json`, (3) applying `edit.json` yields `expected-after-edit.md` exactly. Use `insta` or a small JSON diff for readable failures; add `UPDATE_EXPECT=1` to regenerate.

## Acceptance Criteria
- All 19 fixture categories present; runner passes in `cargo test -p bitacora-markdown`.
- Adding a new fixture dir requires no code change.
- Fixture 2 (one edited block) shows only that block's lines changed in the diff.

## Notes
Part of BIT-US-0095. Verifies BIT-SP-0001.R1–R19. AGENTS.md §6.
