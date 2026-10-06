---
id: BIT-T-0243
type: task
title: Publish Velopack packages and update feeds from the release workflow
status: in_review
priority: medium
parent: BIT-US-0100
milestone: BIT-M-0005
author: mcp
labels: [auto-update, release, ci]
estimate: 2
created: 2026-10-06T14:31:45Z
updated: 2026-10-06T20:18:19Z
started: 2026-10-06T20:18:19Z
---

## Description
Extend `release.yml`: install the `vpk` CLI (pinned version matching the `velopack` crate), `vpk download github` previous release for delta generation, `vpk pack` per OS/arch with the signing settings from the bundles story, and `vpk upload github` (or attach `RELEASES`/feed files to the draft release) for the matching channel (`stable` or `beta`). Skip for formats the ADR marks notice-only.

## Acceptance Criteria
- A test release pair (N and N+1) on a fork yields full + delta packages and a working update from N to N+1 on each Velopack-enabled OS.

## Notes
- [[crate-stack]] §5.2 (`bundle` job: "Velopack pack for updates").
