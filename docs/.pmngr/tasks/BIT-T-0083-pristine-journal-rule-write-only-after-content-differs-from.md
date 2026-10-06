---
id: BIT-T-0083
type: task
title: "Pristine-journal rule: write only after content differs from template"
status: done
parent: BIT-US-0057
milestone: BIT-M-0003
author: mcp
labels: [bitacora-core, journals]
estimate: 2
created: 2026-10-06T14:28:51Z
updated: 2026-10-06T19:13:24Z
closed: 2026-10-06T19:13:24Z
---

## Description
In the writer path (`crates/bitacora-core/src/writer/mod.rs`), skip materialising a `Virtual` journal while its serialized content, trimmed, equals `-` or the expanded template (mirror `fs/watcher_handler.cljs:91-102`). Also treat an external journal file whose trimmed content is `-`/template as no-op for conflict purposes. File path from `assign_path` (journal branch): `<:journals-directory>/<format(:journal/file-name-format|"yyyy_MM_dd")>.md`.

## Acceptance Criteria
- Typing then deleting text leaves no file.
- Typing `hello` writes `journals/2025_11_14.md` = `- hello`.
- Custom dir/format writes `daily/2025-11-14.md`.
- An external `journals/2025_11_14.md` containing `-` is not reported as a conflict.

## Notes
BIT-SP-0002.R11, R12. Open question 1 in [[01-file-graph-layout]]: Bitacora avoids empty journal files by design.
