---
id: BIT-M-0001
type: milestone
title: M0 — Foundations & spikes
status: backlog
author: mcp
created: 2026-10-06T14:20:19Z
updated: 2026-10-06T14:20:19Z
due: 2026-11-15
---

## Description
Cargo workspace, 3-OS CI, GPUI Kit hello window, block-editor IME spike (1,000 blocks on Linux/macOS/Windows), and the Markdown round-trip test harness with Logseq fixture graphs.

## Acceptance Criteria
- CI green on Linux, macOS and Windows (fmt, clippy, tests, cargo deny).
- Spike report documents IME/text-input behaviour per OS and a go/no-go for the custom block editor (ADR-002).
- Round-trip harness runs over `fixtures/graphs/**`.

## Notes
See [[architecture]] §6.
