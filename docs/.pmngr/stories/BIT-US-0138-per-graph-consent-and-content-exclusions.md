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
updated: 2026-10-07T09:15:49Z
---

## Description
As a user, I want to decide per graph and per feature what Pando may see, so that private notes never leave my machine.

## Acceptance Criteria
- Consent dialog states what is sent, where (URL, model from `/info`) and for which feature; consent recorded with timestamp per graph+feature.
- One `ContentPolicy` used by semantic sync, MCP reader for the `pando` token and context attachment: `private::` pages, excluded pages/namespaces/tags.
- Revoking consent stops transmission immediately and offers purge.

## Notes
Implements BIT-SP-0009.R1, BIT-SP-0010.R2.
