---
id: BIT-T-0279
type: task
title: "Cross-check test: GixBackend vs git CLI on generated repo histories"
status: done
priority: medium
parent: BIT-US-0042
milestone: BIT-M-0004
author: mcp
labels: [bitacora-sync, gix, testing]
estimate: 2
created: 2026-10-06T14:32:57Z
updated: 2026-10-06T17:30:22Z
closed: 2026-10-06T17:30:22Z
---

## Description
`crates/bitacora-sync/tests/gix_vs_cli.rs`: proptest-generated small histories (add/modify/delete/rename of up to 20 files across 2 branches) built with the CLI; assert `GixBackend` `merge_base`, `diff_trees` (path sets), `read_blob` and `status` agree with CLI equivalents. Pin gix exact version in `[workspace.dependencies]`.

## Acceptance Criteria
- 64 cases by default; seed reproducible; runs on 3-OS CI.

## Notes
Story BIT-US-0042. Verifies BIT-SP-0006.R6.
