---
id: BIT-US-0092
type: story
title: Byte-preserving serializer with Logseq-canonical edited blocks
status: in_progress
priority: critical
parent: BIT-EP-0003
milestone: BIT-M-0002
author: mcp
labels: [markdown, compat, serializer]
estimate: 8
created: 2026-10-06T14:30:45Z
updated: 2026-10-06T17:02:01Z
started: 2026-10-06T17:02:01Z
---

## Description
As a Bitacora user who shares a graph with Logseq and git, I want Bitacora to write back untouched blocks byte-for-byte and to write edited or new blocks in exactly the form Logseq writes, so that git diffs stay minimal and Logseq re-parses every file identically.

Implements the "Writer" of [[02-markdown-block-syntax]] §9 and the canonical form of §7 (`transform-content`, `src/main/frontend/modules/file/core.cljs:34-110`), without Logseq's full-page normalisation and without its quirks (bullet-less non-first blocks, bullet-less `heading:: true` first block, accidental pre-blocks).

## Acceptance Criteria
- `serialize(parse(bytes)) == bytes` for every fixture.
- Editing one block changes only that block's byte range; untouched CRLF/tabs/blank lines stay.
- Edited/new blocks: `unit*(depth-1) + "- " + first line`, continuation prefix `unit*(depth-1) + "  "`, empty block `-`, no blank lines between blocks, pre-block + one blank line, LF; unit from `:export/bullet-indentation`.
- A Markdown `:PROPERTIES:` drawer is converted to `key:: value` lines only when its block is edited.
- `heading:: true` first block keeps its bullet; non-first blocks always have a bullet.
- Image metadata `{:height N, :width M}` written in EDN `pr-str` style on resize.
- Output of the canonical-form example in §7 is byte-identical to the doc.

## Notes
Implements: BIT-SP-0001.R11, BIT-SP-0001.R12, BIT-SP-0001.R14, BIT-SP-0001.R17, BIT-SP-0001.R18.
See [[02-markdown-block-syntax]] §2.2, §2.3, §2.7, §7; [[block-editor]] (byte-preserving serializer); ADR-003, ADR-011 (atomic writes happen in bitacora-core).
