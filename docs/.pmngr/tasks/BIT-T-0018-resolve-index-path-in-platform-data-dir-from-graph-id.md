---
id: BIT-T-0018
type: task
title: Resolve index path in platform data dir from graph id
status: backlog
priority: critical
parent: BIT-US-0004
milestone: BIT-M-0002
author: mcp
labels: [bitacora-index, index]
estimate: 2
created: 2026-10-06T14:26:33Z
updated: 2026-10-06T14:26:33Z
---

## Description
Add `crates/bitacora-index/src/location.rs` with `fn index_path(graph_root: &Path, data_dir_override: Option<&Path>) -> Result<PathBuf, IndexError>`:
- Canonicalize the graph root (resolve symlinks), NFC-normalize, `graph_id = hex(blake3(path_bytes))[..16]`.
- Base dir via `directories::ProjectDirs` (`data_dir()`), i.e. `~/.local/share/bitacora`, `~/Library/Application Support/bitacora`, `%APPDATA%\bitacora`; result `<base>/graphs/<graph_id>/index.sqlite`; create parent dirs.
- Write `graph_root` into `<base>/graphs/<graph_id>/graph.json` for `doctor` and for humans.

## Acceptance Criteria
- Unit tests: same graph via a symlink and via the real path give the same id; two different graphs give different ids.
- No path inside the graph root is ever returned (assert in test).
- Override parameter used by tests to point into a tempdir.

## Notes
BIT-SP-0003.R1, ADR-005. [[sqlite-index-schema]] §1.1.
