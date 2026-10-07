---
id: BIT-T-0430
type: task
title: Consent records and consent dialog
status: backlog
priority: high
parent: BIT-US-0138
milestone: BIT-M-0007
author: mcp
labels: [v2, privacy]
estimate: 2
created: 2026-10-07T09:16:34Z
updated: 2026-10-07T09:16:34Z
---

## Description
Consent per graph+feature with timestamp and Pando URL; dialog explains what is sent, destination and model; features check consent before any request.

## Acceptance Criteria
- Test: without consent no request is issued (mock server sees zero calls).
