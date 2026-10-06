---
id: BIT-US-0093
type: story
title: "Surgical block text edits: properties, id::, collapsed::, SCHEDULED, LOGBOOK"
status: done
priority: high
parent: BIT-EP-0003
milestone: BIT-M-0002
author: mcp
labels: [markdown, compat, serializer, properties]
estimate: 5
created: 2026-10-06T14:30:45Z
updated: 2026-10-06T17:08:48Z
started: 2026-10-06T17:02:01Z
closed: 2026-10-06T17:08:48Z
---

## Description
As a developer of the editor, MCP write tools and rename cascade, I want pure text-transform functions that set/remove a property, insert `id::`, toggle `collapsed::`, set SCHEDULED/DEADLINE and append CLOCK entries by changing only the affected lines, so that every higher-level operation produces Logseq-identical, minimal diffs.

Re-implements, from the documented behaviour in [[02-markdown-block-syntax]], the effects of Logseq's `insert-property` / `remove-property` (`src/main/frontend/util/property.cljs:226-351`), `set-blocks-id!` (`editor.cljs:955-972`), collapsed handling (`core.cljs:20-32`) and drawer placement (`drawer.cljs:31-86`) — cited for behaviour only — but never hoists or reorders untouched lines.

## Acceptance Criteria
- Replacing an existing key changes only that line; a new key is appended to the end of the first property group, or inserted after the title line if there is none.
- `ensure_block_id` inserts `id:: <uuid>` after the title / appended to the property group, never into a pre-block, and is a no-op when an id exists.
- `set_collapsed(true)` writes `collapsed:: true`; `set_collapsed(false)` removes the line, restoring original bytes.
- New SCHEDULED/DEADLINE goes after the title; new properties after title/SCHEDULED; new `:LOGBOOK:` after properties.
- Our own property-edit vectors covering the documented insert/remove/update cases, verified black-box against Logseq (no Logseq test file copied or translated).

## Notes
Implements: BIT-SP-0001.R7, BIT-SP-0001.R13, BIT-SP-0001.R15.
See [[02-markdown-block-syntax]] §3.3, §4, §5.3, §7; [[04-editor-outliner-operations]]; ADR-006 (id written only when referenced). ADR-015. The "store collapse state in files" setting is exposed by bitacora-config/app (BIT-EP-0007/0013); this story provides the switchable behaviour.
