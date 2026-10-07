---
id: BIT-T-0464
type: task
title: Recommender run and suggestion schema
status: done
priority: medium
parent: BIT-US-0152
milestone: BIT-M-0008
author: mcp
labels: [v2, ai, bitacora-pando]
estimate: 3
created: 2026-10-07T09:19:06Z
updated: 2026-10-07T11:51:01Z
closed: 2026-10-07T11:51:01Z
---

## Description
Run `bitacora-recommender` for the current page (manual + optional debounced auto); schema for related_pages, link_suggestions(block_uuid, text span, target), tag_suggestions, next_actions; validate spans against current block text.

## Acceptance Criteria
- Tests: invalid spans dropped; auto mode respects debounce and feature switch.
