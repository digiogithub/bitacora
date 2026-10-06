---
id: BIT-T-0273
type: task
title: Define GitBackend trait, domain types and error classification
status: done
priority: critical
parent: BIT-US-0041
milestone: BIT-M-0004
author: mcp
labels: [bitacora-sync, git, backend]
estimate: 2
created: 2026-10-06T14:32:56Z
updated: 2026-10-06T17:30:13Z
closed: 2026-10-06T17:30:13Z
---

## Description
`crates/bitacora-sync/src/backend/mod.rs`: `trait GitBackend: Send + Sync { fn fetch(&self, remote, branch) -> Result<FetchOutcome>; fn push(&self, remote, branch) -> Result<PushOutcome>; fn merge_base(&self, a: Oid, b: Oid) -> Result<Option<Oid>>; fn read_blob(&self, commit: Oid, path: &RepoPath) -> Result<Option<Vec<u8>>>; fn diff_trees(&self, a: Oid, b: Oid) -> Result<Vec<TreeChange>>; fn write_tree(&self, base: Oid, edits: &[TreeEdit]) -> Result<Oid>; fn commit(&self, msg: &CommitMessage, opts: CommitOpts) -> Result<Oid>; fn commit_tree(&self, tree, parents, msg) -> Result<Oid>; fn update_ref(&self, name, new, expected_old) -> Result<()>; fn status(&self) -> Result<RepoStatus>; fn kind(&self) -> ActiveBackend; }` plus `HybridBackend { cli: CliBackend, gix: GixBackend }` delegating per ADR-007 (used when system git is found), and `GixBackend` used alone when it is not (ADR-020); `select_backend(detection) -> Box<dyn GitBackend>`. `GitError` (`thiserror`): `Network`, `Auth { hint: Option<String> }`, `NonFastForward`, `GitTooOld{found}`, `NotARepo`, `ExternalOperationInProgress`, `Other{stderr}`. `TreeChange { Added, Deleted, Modified, Renamed{from,to,similarity} }`.

## Acceptance Criteria
- Compiles with docs; a `FakeBackend` for unit tests of the sync loop.
- Unit test: `select_backend` returns Hybrid for a found git ≥ 2.38 and GixOnly for none / too old.

## Notes
Story BIT-US-0041. Implements BIT-SP-0006.R6. ADR-007, ADR-020.
