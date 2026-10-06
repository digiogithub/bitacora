---
id: BIT-T-0058
type: task
title: "SetPageProperty op: pre-block and front-matter writing"
status: backlog
parent: BIT-US-0028
milestone: BIT-M-0003
author: mcp
labels: [bitacora-core, bitacora-markdown, lifecycle]
estimate: 3
created: 2026-10-06T14:28:27Z
updated: 2026-10-06T14:28:27Z
---

## Description
Add `Op::SetPageProperty { page, key, value }` / `Op::RemovePageProperty` in `crates/bitacora-core/src/ops/page_props.rs`.
- No pre-block: insert `key:: value` lines as an un-bulleted pre-block followed by exactly one blank line before the first bullet (`core.cljs:41-48`).
- Existing `k:: v` pre-block: replace in place or append to the group (Logseq `insert-property`, `util/property.cljs:226-316`), touching only that line.
- Front matter at byte 0: write `key: value` inside the `---` fence (`util/property.cljs:222-224,282-285`), never create front matter.
- Keys lower-cased; values trimmed. Remaining bytes of the file untouched (byte-preserving serializer from BIT-EP-0003).

## Acceptance Criteria
- `- a` + `tags:: demo` → `tags:: demo\n\n- a`.
- `---\ntitle: Front\n---\n- a` + `icon=🚀` → `---\ntitle: Front\nicon: 🚀\n---\n- a`.
- Changing `tags` in `title:: X\ntags:: a\n\n- b` changes only line 2.
- Undo restores the original bytes.

## Notes
[[01-file-graph-layout]] §8, §9; [[02-markdown-block-syntax]] §3.3. BIT-SP-0002.R18.
