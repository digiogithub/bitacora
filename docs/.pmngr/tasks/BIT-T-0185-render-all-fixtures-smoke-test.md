---
id: BIT-T-0185
type: task
title: Render-all-fixtures smoke test
status: done
priority: high
parent: BIT-US-0074
milestone: BIT-M-0002
author: mcp
labels: [bitacora-app, testing]
estimate: 2
created: 2026-10-06T14:30:56Z
updated: 2026-10-06T18:44:23Z
started: 2026-10-06T18:28:35Z
closed: 2026-10-06T18:44:23Z
---

## Description
`crates/bitacora-app/tests/render_fixtures.rs`: `#[gpui::test]` that indexes each graph in `fixtures/graphs/`, opens every page in the page view (headless test window), renders all blocks (no lazy limit) and asserts no panic and no `unwrap` error log. Records render time per page and fails if any page > 500 ms.

## Acceptance Criteria
- Runs in CI on Linux; covered by `cargo test -p bitacora-app`.

## Notes
Epic acceptance criterion of BIT-EP-0006.
