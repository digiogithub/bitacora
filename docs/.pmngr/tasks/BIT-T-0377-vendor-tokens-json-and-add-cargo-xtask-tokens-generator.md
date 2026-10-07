---
id: BIT-T-0377
type: task
title: Vendor tokens.json and add `cargo xtask tokens` generator
status: backlog
priority: high
parent: BIT-US-0113
milestone: BIT-M-0006
author: mcp
labels: [v2, design-system, xtask]
estimate: 2
created: 2026-10-07T09:12:05Z
updated: 2026-10-07T09:12:05Z
---

## Description
Copy `tokens/tokens.json` into `design/tokens.json` with a provenance note; port `gen_tokens.py` to an `xtask tokens` subcommand (Rust, no Python dependency in CI) that writes `crates/bitacora-app/src/ui/theme/palette.rs`; add `--check` mode for CI staleness.

## Acceptance Criteria
- Running the generator twice yields identical output; CI `xtask tokens --check` fails on stale file.
