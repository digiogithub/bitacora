---
id: BIT-T-0178
type: task
title: Install/launch/uninstall smoke tests for every bundle format
status: in_review
priority: medium
parent: BIT-US-0090
milestone: BIT-M-0005
author: mcp
labels: [packaging, ci, testing]
estimate: 2
created: 2026-10-06T14:30:55Z
updated: 2026-10-06T19:59:48Z
started: 2026-10-06T19:59:48Z
---

## Description
Add a `bundle-smoke` job per OS that installs the freshly built bundle (dmg mount + copy, `msiexec /qn`, `dpkg -i`, AppImage `--appimage-extract-and-run`), runs `bitacora --smoke-test` (from the app shell story) and `bitacora-cli --version`, then uninstalls and asserts that data dirs created by the run remain (user data is never removed by uninstallers).

## Acceptance Criteria
- Job green on the 3 OSes for a release-candidate build.
- Failure output includes installer logs.

## Notes
- AGENTS.md §3 rule 1 (user files are sacred). [[crate-stack]] §5.2.
