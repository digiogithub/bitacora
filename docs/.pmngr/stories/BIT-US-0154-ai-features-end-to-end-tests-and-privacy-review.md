---
id: BIT-US-0154
type: story
title: AI features end-to-end tests and privacy review
status: backlog
priority: medium
parent: BIT-EP-0023
milestone: BIT-M-0008
author: mcp
labels: [v2, ai, tests, privacy]
estimate: 3
created: 2026-10-07T09:18:16Z
updated: 2026-10-07T09:18:16Z
---

## Description
As the maintainer, I want the AI flows tested end to end against a real Pando and reviewed for privacy and prompt-injection risks before release.

## Acceptance Criteria
- Opt-in CI job / scripted manual run: chat with tool calls, approval accept/reject, journal review, recommendations, semantic search, Pando stopped mid-run.
- Privacy review doc: data sent per feature, injection test pages ("ignore previous instructions…") cannot trigger unapproved writes.

## Notes
Covers BIT-SP-0011.R1-R6 verification, BIT-SP-0009.R1.
