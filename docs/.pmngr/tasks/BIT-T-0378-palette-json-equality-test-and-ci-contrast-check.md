---
id: BIT-T-0378
type: task
title: Palette/JSON equality test and CI contrast check
status: done
priority: high
parent: BIT-US-0113
milestone: BIT-M-0006
author: mcp
labels: [v2, design-system, ci]
estimate: 1
created: 2026-10-07T09:12:05Z
updated: 2026-10-07T10:22:24Z
closed: 2026-10-07T10:22:24Z
---

## Description
Test that every token in `design/tokens.json` matches `Palette::dark()`/`light()`; port `check_contrast.py` rules into a Rust test (reuse `crates/bitacora-app/src/contrast.rs`) covering all text/background pairs in both modes.

## Acceptance Criteria
- Lowering a text token below 4.5:1 makes `cargo test` fail with the pair and mode.
