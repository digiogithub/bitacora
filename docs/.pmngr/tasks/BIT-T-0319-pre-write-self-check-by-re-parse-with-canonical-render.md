---
id: BIT-T-0319
type: task
title: Pre-write self-check by re-parse with canonical-render fallback
status: done
priority: critical
parent: BIT-US-0064
milestone: BIT-M-0003
author: mcp
labels: [bitacora-core, io]
estimate: 2
created: 2026-10-06T14:33:18Z
updated: 2026-10-06T18:52:31Z
closed: 2026-10-06T18:52:31Z
---

## Description
`crates/bitacora-core/src/writer/self_check.rs`: `verify(page, bytes) -> Result<(), Mismatch>` re-parses with `bitacora-markdown` and compares DFS `(depth, text)` with the model. On mismatch: `render_canonical(page)`, verify again, log a structured bug report (page path, first mismatching block, both outputs hashed) and write canonical; if still mismatched return `WriteError::SelfCheckFailed` (page stays dirty, error notice).

## Acceptance Criteria
- Tests with an injected faulty serializer trigger the fallback and the log.
- Normal path adds < 2 ms for a 1,000-block page (benchmark).

## Notes
Story BIT-US-0064. Implements BIT-SP-0005.R3. [[block-editor]] §5.1.
