---
id: BIT-T-0099
type: task
title: "RenamePage op planner: target resolution and rename set"
status: backlog
parent: BIT-US-0061
milestone: BIT-M-0003
author: mcp
labels: [bitacora-core, rename]
estimate: 3
created: 2026-10-06T14:29:22Z
updated: 2026-10-06T14:29:22Z
---

## Description
`crates/bitacora-core/src/lifecycle/rename.rs`: `fn plan_rename(graph, old: PageId, new_title: &str) -> RenamePlan`. Sanitise `new_title` like `create!` (trim, strip `[[ ]]`, leading `#`, boundary `/`). Classify:
1. same lower-cased key → `CaseOnly`;
2. target exists → `Merge` (handled by merge story);
3. else `Rename` with the list of pages to rename: the page itself + namespace children (`a/x` → `b/x`, replace prefix once like `string/replace-first`) + pages whose titles contain `[[old]]` or `[[old/` (`page.cljs:516-566`).
For each, compute `new_path = same dir + file_name_sanity(new_title) + same ext` (`compute-new-file-path`, `page.cljs:191-199`); journals keep their path. Detect collisions among children (report error, abort plan).

## Acceptance Criteria
- Tests: `a` → `b` with children `a/x`, `a/y/z` yields 3 renames with correct paths; `foo` → `Foo` is `CaseOnly`; journal rename keeps path; child collision returns an error and no ops.
- Legacy graph uses the legacy codec for new paths.

## Notes
[[01-file-graph-layout]] §10.2. BIT-SP-0002.R13. ADR-013.
