---
id: BIT-EP-0003
type: epic
title: Lossless Logseq Markdown parser and serializer
status: backlog
priority: critical
milestone: BIT-M-0002
author: mcp
labels: [markdown, compat]
created: 2026-10-06T14:21:13Z
updated: 2026-10-06T14:21:13Z
---

## Description
`bitacora-markdown`: line-based outline splitter that keeps raw bytes per block, property scanner (`key:: value`, page pre-block, front matter), inline scanners for page refs, tags, block refs, embeds, macros, task markers, priorities, SCHEDULED/DEADLINE, drawers; code-fence and `#+BEGIN` awareness; byte-preserving serializer plus Logseq-canonical serialization for edited blocks; content-vs-metadata property classification used by the merge engine.

## Acceptance Criteria
- `serialize(parse(bytes)) == bytes` for every file in the fixtures.
- Block tree and refs match Logseq's mldoc output for the fixture corpus listed in [[02-markdown-block-syntax]].
- Canonical form of an edited block matches what Logseq 0.10.15 writes.

## Notes
ADR-003. Spec: Logseq Markdown compatibility.
