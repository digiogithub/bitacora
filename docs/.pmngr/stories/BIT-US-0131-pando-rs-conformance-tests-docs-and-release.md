---
id: BIT-US-0131
type: story
title: pando-rs conformance tests, docs and release
status: in_review
priority: medium
parent: BIT-EP-0018
milestone: BIT-M-0007
author: mcp
labels: [v2, pando-repo, sdk, release]
estimate: 3
created: 2026-10-07T09:14:29Z
updated: 2026-10-07T10:45:40Z
started: 2026-10-07T10:41:12Z
---

## Description
As the Pando maintainer, I want `pando-rs` tested against the real server and released with the other SDKs, so that Bitacora can depend on a versioned crate.

## Acceptance Criteria
- Shared conformance fixtures (recorded SSE streams) replayed by TS, Python and Rust SDKs.
- Integration test in Pando CI against `pando agui-serve` + REST server.
- README/docs, semver policy tied to the Pando API version, crates.io publish (Bitacora pins a git rev until then).

## Notes
Depends on the REST and AG-UI client stories.
