---
id: BIT-T-0413
type: task
title: Shared conformance fixtures and live-server integration test
status: in_progress
priority: medium
parent: BIT-US-0131
milestone: BIT-M-0007
author: mcp
labels: [v2, pando-repo, tests]
estimate: 2
created: 2026-10-07T09:15:16Z
updated: 2026-10-07T10:41:12Z
started: 2026-10-07T10:41:12Z
---

## Description
Record SSE streams (text, tool call, interrupt, permission, error) into a shared fixtures folder used by TS, Python and Rust SDK tests; add a Pando CI job running `pando-rs` against `pando agui-serve` with a stub model.

## Acceptance Criteria
- All three SDKs pass the same fixtures; live job green.
