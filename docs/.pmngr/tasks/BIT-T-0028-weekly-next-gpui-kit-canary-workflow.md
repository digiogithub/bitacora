---
id: BIT-T-0028
type: task
title: Weekly "next GPUI Kit" canary workflow
status: backlog
priority: medium
parent: BIT-US-0013
milestone: BIT-M-0001
author: mcp
labels: [ci, dependencies, bitacora-app]
estimate: 2
created: 2026-10-06T14:26:40Z
updated: 2026-10-06T14:26:40Z
---

## Description
Add `.github/workflows/gpui-kit-canary.yml` (`schedule: weekly`, `workflow_dispatch`):
1. Query `https://crates.io/api/v1/crates/gpui-kit` for `max_stable_version`; stop if equal to the current pin.
2. Rewrite the pin in `Cargo.toml` (`=X.Y.Z`), run `cargo update -p gpui-kit`, then `cargo build -p bitacora-app` and `cargo test -p bitacora-app` on macos-latest and ubuntu-24.04.
3. Use `gh issue` to create or update a single issue titled "GPUI Kit canary: X.Y.Z" with the result and the first compiler errors.
Never pushes code or opens PRs automatically.

## Acceptance Criteria
- Manual dispatch with a forced version input runs end to end and updates the issue.
- Workflow has least-privilege `permissions` (`contents: read`, `issues: write`).

## Notes
- [[gpui-and-gpui-kit]] Risk R1/R2 mitigations; ADR-001.
