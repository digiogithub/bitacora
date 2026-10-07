---
id: BIT-US-0138
type: story
title: Per-graph consent and content exclusions
status: backlog
priority: high
parent: BIT-EP-0020
milestone: BIT-M-0007
author: mcp
labels: [v2, pando, privacy, bitacora-pando]
estimate: 5
created: 2026-10-07T09:15:49Z
updated: 2026-10-07T09:55:09Z
---

## Description
As a user, I want to decide per graph and per feature what Pando may see, so that private notes never leave my machine.

## Acceptance Criteria
- The consent dialog states what is sent, where (URL, and the model from `/info`) and for which feature.
  - It explicitly says that semantic documents go into Pando's **shared KB**, the agent memory, so any Pando agent or client searching that KB can see indexed blocks.
  - Consent is recorded with a timestamp per graph and feature.
- One `ContentPolicy` is used by semantic sync, by the MCP reader for the `pando` token and by context attachment. It covers `private::` pages and excluded pages, namespaces and tags.
- Revoking consent stops transmission immediately and offers purge.

## Notes
Implements BIT-SP-0009.R1, BIT-SP-0010.R2. Owner decision 2026-10-07: use the shared KB in the agent memory.
