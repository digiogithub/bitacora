---
id: BIT-T-0257
type: task
title: Implement bitacora-cli self-update
status: done
priority: low
parent: BIT-US-0110
milestone: BIT-M-0005
author: mcp
labels: [release, auto-update, bitacora-cli]
estimate: 2
created: 2026-10-06T14:32:06Z
updated: 2026-10-06T19:59:49Z
closed: 2026-10-06T19:59:49Z
---

## Description
Add `bitacora-cli self-update [--channel stable|beta] [--check]` using `self_update = "1.3"` (GitHub backend, rustls) restricted to the CLI archive asset for the current target; verify against `SHA256SUMS` before replacing; detect installs not owned by the user's install script (inside an app bundle, `/usr/bin`, Homebrew/Scoop paths) and print the right upgrade instruction instead. Refuse while a `serve` instance holding the instance lock is running from the same binary.

## Acceptance Criteria
- Integration test against a mocked GitHub API serving a newer archive replaces a temp-copied binary and the new one reports the new version.
- `--check` performs no writes.

## Notes
- [[crate-stack]] §4.1 (`self_update 1.3.0`: OK for the CLI, wrong for GUI bundles). Add `self_update` to `[workspace.dependencies]` and pass `cargo deny`.
