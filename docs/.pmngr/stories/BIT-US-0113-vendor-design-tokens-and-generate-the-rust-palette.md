---
id: BIT-US-0113
type: story
title: Vendor design tokens and generate the Rust palette
status: done
priority: high
parent: BIT-EP-0015
milestone: BIT-M-0006
author: mcp
labels: [v2, design-system, bitacora-app, xtask]
estimate: 3
created: 2026-10-07T09:11:26Z
updated: 2026-10-07T10:22:28Z
closed: 2026-10-07T10:22:28Z
---

## Description
As a maintainer, I want the design system tokens vendored and turned into Rust code by a reproducible generator, so that colours never drift from the design.

## Acceptance Criteria
- `tokens.json`, `gen_tokens.py` (or a Rust port in `xtask`) and `check_contrast.py` live in the repo (e.g. `design/`), with provenance noted.
- `cargo xtask tokens` regenerates `crates/bitacora-app/src/ui/theme/palette.rs`; CI fails if the generated file is stale.
- A test asserts `Palette::dark()`/`light()` equal the JSON values; the contrast check runs in CI.

## Notes
Implements BIT-SP-0008.R1, BIT-SP-0008.R2. Source `/www/Bitacora/bitacora-design-system/tokens/`, `tools/`.
