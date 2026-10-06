---
id: BIT-T-0280
type: task
title: "Repo preparation: .gitignore merge, info/attributes and repo-local identity"
status: backlog
priority: high
parent: BIT-US-0043
milestone: BIT-M-0004
author: mcp
labels: [bitacora-sync, git, onboarding]
estimate: 2
created: 2026-10-06T14:32:57Z
updated: 2026-10-06T14:32:57Z
---

## Description
`crates/bitacora-sync/src/repo_setup.rs`: `ensure_gitignore(graph)` appends missing default entries under `# Bitacora / Logseq volatile files` (idempotent, preserves user lines/order, keeps file's line ending); `ensure_attributes(git_dir)` writes `*.md merge=binary`, `logseq/config.edn merge=binary`, `* -text` into `.git/info/attributes` (merge with existing lines); `ensure_identity(settings)` sets `user.name`/`user.email` with `git config --local` (fallback `Bitacora` / `device@<hostname>`).

## Acceptance Criteria
- Tests: idempotency, existing-file merge, attributes present, global config untouched (temp HOME).

## Notes
Story BIT-US-0043. Implements BIT-SP-0006.R3, BIT-SP-0006.R8, BIT-SP-0006.R20.
