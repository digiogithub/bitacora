---
id: BIT-T-0153
type: task
title: Forward launch arguments to the running instance over local IPC
status: backlog
priority: high
parent: BIT-US-0086
milestone: BIT-M-0005
author: mcp
labels: [release, bitacora-app]
estimate: 3
created: 2026-10-06T14:30:21Z
updated: 2026-10-06T14:30:21Z
---

## Description
In `crates/bitacora-app/src/single_instance.rs`: on startup try `InstanceLock::acquire(Desktop)`. If held by a desktop instance, connect to its IPC endpoint (local socket via the `interprocess` crate: Unix domain socket in the runtime/data dir with 0600 perms, Windows named pipe scoped to the user), send `{"cmd":"open","graph":<abs path or null>}` as a JSON line, wait for an ack (2 s timeout) and exit 0. The primary instance runs an IPC listener on the tokio runtime and dispatches `open` to the UI via the event channel (focus window, open/switch graph). Also handle macOS "open file/URL" Apple events and Linux `.desktop` `%U` the same way.

## Acceptance Criteria
- Integration test: start primary (test mode, no window), run a second process with `--graph X`, assert the primary received `open X` and the second exited 0.
- IPC endpoint is not accessible to other users (permissions checked in test on Unix).

## Notes
- [[mcp-server]] §2; [[gpui-and-gpui-kit]] §1.4 (tokio bridge, event channel). Add `interprocess` to `[workspace.dependencies]` and pass `cargo deny`.
