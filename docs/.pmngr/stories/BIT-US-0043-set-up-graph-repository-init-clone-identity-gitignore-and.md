---
id: BIT-US-0043
type: story
title: "Set up graph repository: init, clone, identity, .gitignore and attributes"
status: done
priority: high
parent: BIT-EP-0011
milestone: BIT-M-0004
author: mcp
labels: [git, sync, onboarding]
estimate: 5
created: 2026-10-06T14:28:30Z
updated: 2026-10-06T20:37:24Z
started: 2026-10-06T17:30:22Z
closed: 2026-10-06T20:37:24Z
---

## Description
As a non-technical user, I want to connect my graph to a remote (or clone an existing graph) from settings, so that sync works without me touching git.

## Acceptance Criteria
- "Enable sync" on a folder without `.git` runs `git init` (standard in-folder `.git`), sets remote, initial commit, first push; "Open graph from remote" clones into a chosen folder.
- Repo-local `user.name`/`user.email` from settings (fallback `Bitacora <device@hostname>`); `~/.gitconfig` untouched.
- Default `.gitignore` written/merged idempotently without reordering user lines.
- `.git/info/attributes` contains `*.md merge=binary`, `logseq/config.edn merge=binary`, `* -text`.
- Graphs using Logseq's separate gitdir pointer (`.git` file into `~/.logseq/git/…`) are detected and the user is offered migration.

## Notes
Implements: BIT-SP-0006.R3, BIT-SP-0006.R20, BIT-SP-0006.R8. See [[git-sync-merge]] §5.2, §7, [[05-git-and-apis]] §1.1, §1.6.
