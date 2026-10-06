---
id: BIT-T-0268
type: task
title: "Enforce single writer: disallowed fs methods outside core writer and concurrency stress test"
status: done
priority: high
parent: BIT-US-0062
milestone: BIT-M-0003
author: mcp
labels: [bitacora-core, test, ci]
estimate: 2
created: 2026-10-06T14:32:23Z
updated: 2026-10-06T18:39:25Z
started: 2026-10-06T18:28:39Z
closed: 2026-10-06T18:39:25Z
---

## Description
Add `clippy.toml` `disallowed-methods` for `std::fs::write`, `File::create`, `OpenOptions::open` (write) in all crates except `bitacora-core::writer` (allowed via `#[allow]` at the single call site) and test-only code. Add `crates/bitacora-core/tests/queue_stress.rs`: two producer threads (UI-like typing, MCP-like appends) × 1,000 commands on one page; assert all applied and the final file contains every change.

## Acceptance Criteria
- CI clippy fails if a new direct write appears.
- Stress test passes 100 iterations without flakiness.

## Notes
Story BIT-US-0062. Implements BIT-SP-0005.R1. AGENTS.md rule 3.
