---
id: BIT-T-0479
type: task
title: Persist :graph/settings and :graph/forcesettings
status: done
priority: medium
parent: BIT-US-0158
milestone: BIT-M-0009
author: mcp
labels: [v2, graph, bitacora-config]
estimate: 2
created: 2026-10-07T09:20:34Z
updated: 2026-10-07T11:30:51Z
closed: 2026-10-07T11:30:51Z
---

## Description
Read/write the Logseq keys via `bitacora-config` comment-preserving edits through the core Op path; debounce slider writes.

## Acceptance Criteria
- Golden test: only the target keys change in config.edn.
