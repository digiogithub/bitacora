---
id: BIT-T-0407
type: task
title: Scaffold sdk/rust crate, CI job and ag-ui-core evaluation
status: done
priority: high
parent: BIT-US-0129
milestone: BIT-M-0007
author: mcp
labels: [v2, pando-repo, sdk]
estimate: 2
created: 2026-10-07T09:15:16Z
updated: 2026-10-07T10:11:31Z
closed: 2026-10-07T10:11:31Z
---

## Description
Create `sdk/rust/` (crate `pando-rs`, MIT, edition 2024), add Pando CI job (fmt, clippy -D warnings, test, cargo deny), common config/auth/error types; write a short decision note comparing own types vs `ag-ui-core` (event coverage, maintenance, licence).

## Acceptance Criteria
- CI green in the Pando repo; decision note committed in `sdk/rust/README.md` or Pando docs.
