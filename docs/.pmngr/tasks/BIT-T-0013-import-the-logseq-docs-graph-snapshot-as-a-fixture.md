---
id: BIT-T-0013
type: task
title: Import the Logseq docs graph snapshot as a fixture
status: in_progress
priority: critical
parent: BIT-US-0011
milestone: BIT-M-0001
author: mcp
labels: [infra, fixtures]
estimate: 2
created: 2026-10-06T14:26:00Z
updated: 2026-10-06T16:46:30Z
started: 2026-10-06T16:46:30Z
---

## Description
The Logseq documentation graph (`github.com/logseq/docs`) is MIT-licensed and may be used as a fixture (ADR-021).
1. **Pin the source.** Choose a file-graph-era commit of `logseq/docs` (contemporary with Logseq 0.10.15) and record its SHA.
2. **Copy only** `pages/`, `journals/` and `logseq/config.edn` into `fixtures/graphs/logseq-docs/`, plus the upstream `LICENSE.md` next to them. Do not copy media or other large paths (`assets/`, `gifs/`, `screenshots/`, etc.); asset links in pages stay unresolved.
3. Write `fixtures/graphs/logseq-docs/PROVENANCE.md` with: source repo URL, commit SHA, date, license (MIT), the list of copied and excluded paths, and the size of the snapshot.

## Acceptance Criteria
- Snapshot is < 10 MB and contains > 100 pages.
- `PROVENANCE.md` and the upstream `LICENSE.md` are present.
- No file was modified compared to upstream (verify with `diff -r` against a fresh clone at the recorded SHA, limited to the copied paths).

## Notes
- [[01-file-graph-layout]], [[02-markdown-block-syntax]]. ADR-014 (license hygiene). ADR-015 (Logseq-produced graphs are fixtures only if their license allows redistribution). ADR-021 (`logseq/docs` is MIT: usable as fixture with only `pages/`, `journals/`, `logseq/config.edn` from a pinned commit, plus LICENSE.md and provenance; no media).
