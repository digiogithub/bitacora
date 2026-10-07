---
id: BIT-T-0444
type: task
title: Cold-start reconcile and initial full index
status: done
priority: high
parent: BIT-US-0143
milestone: BIT-M-0007
author: mcp
labels: [v2, bitacora-pando]
estimate: 2
created: 2026-10-07T09:17:36Z
updated: 2026-10-07T10:50:56Z
closed: 2026-10-07T10:50:56Z
---

## Description
On enable or start, diff the local eligible blocks against the ledger:
- enqueue upserts for missing or changed blocks;
- enqueue deletes for ledger entries whose block is gone or now excluded.

Initial indexing of large graphs is throttled. A journals policy setting chooses all journals or the last N months.

## Acceptance Criteria
- A restart with no changes sends nothing.
- The time for the initial index of the large preset is measured and recorded.
