---
id: BIT-T-0373
type: task
title: "GixBackend network and ref writes for the gix-only fallback: fetch, push, clone, commit, update-ref"
status: in_review
priority: high
parent: BIT-US-0042
milestone: BIT-M-0004
author: mcp
labels: [bitacora-sync, gix, backend]
estimate: 5
created: 2026-10-06T15:15:10Z
updated: 2026-10-06T17:30:22Z
started: 2026-10-06T17:30:22Z
---

## Description
`crates/bitacora-sync/src/backend/gix_net.rs` (gix API isolated here): implement the network and ref-writing half of `GitBackend` on `gix` so `GixBackend` can run alone when no system git is installed (ADR-020):
- `fetch(remote, branch)` (prune, no tags) and `ls-remote` via gix's blocking network client over HTTPS and SSH (gix transport).
- `push(remote, HEAD:<branch>)`, never forced; map rejected / non-fast-forward to `GitError::NonFastForward`.
- `clone(url, dir)` for onboarding.
- `commit(msg, opts)` (tree from index, repo-local identity, `--amend` equivalent for squash), `commit_tree(tree, parents, msg)` and `update_ref(name, new, expected_old)` with compare-and-swap semantics.
- Credentials come from the credential provider of BIT-T-0374 (keyring + in-app prompt); timeouts per op as in `CliBackend` (fetch/push 120 s).
- Classify errors into the same `GitError` variants as `CliBackend`.
Enable the needed gix network features in `[workspace.dependencies]` (exact pin kept).

## Acceptance Criteria
- Integration tests against temp bare repos (`file://` and a local HTTP test server) for fetch/push round-trip, rejected push, commit-tree with two parents + CAS update-ref failure, clone.
- Never writes global git config; never force-pushes.

## Notes
Story BIT-US-0042. Implements BIT-SP-0006.R6, BIT-SP-0006.R4. ADR-007, ADR-020.
