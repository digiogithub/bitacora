---
id: BIT-T-0463
type: task
title: Review cache and optional schedule
status: done
priority: low
parent: BIT-US-0151
milestone: BIT-M-0008
author: mcp
labels: [v2, ai, journal]
estimate: 2
created: 2026-10-07T09:19:06Z
updated: 2026-10-07T11:51:01Z
closed: 2026-10-07T11:51:01Z
---

## Description
Machine-local review cache keyed by graph + range + content hash; optional daily schedule while the app runs (respecting consent and status).

## Acceptance Criteria
- Cached review shown without new run when content unchanged; schedule test with fake clock.
