---
id: BIT-T-0248
type: task
title: ensure_block_id, duplicate-id detection and self-ref stripping
status: in_progress
parent: BIT-US-0093
milestone: BIT-M-0002
author: mcp
labels: [bitacora-markdown, serializer, properties]
estimate: 2
created: 2026-10-06T14:32:01Z
updated: 2026-10-06T17:02:01Z
started: 2026-10-06T17:02:01Z
---

## Description
Add `crates/bitacora-markdown/src/edit/identity.rs`:
- `ensure_block_id(content, uuid) -> (String, Uuid)`: if a valid `id::`/`custom-id::`/`custom_id::` exists return it unchanged; else append `id:: <uuid>` via `set_property` (after the title, end of property group). Refuse on pre-blocks (`editor.cljs:962`).
- `parse_block_id(props) -> Option<Uuid>` (`block.cljs:424-432`); invalid uuid → None.
- `find_duplicate_ids(pages: impl Iterator<(PageKey, Vec<(BlockIdx, Uuid)>)>) -> Vec<Duplicate>`: first occurrence in load order wins (`block.cljs:610-644`); Bitacora reports duplicates instead of silently removing the line (removal only when the block is edited).
- `strip_self_ref(content, own_uuid)` removes `((own-uuid))` (`editor.cljs:324`).

## Acceptance Criteria
- `"- Parent block\n  tags:: demo"` + uuid → `"- Parent block\n  tags:: demo\n  id:: 6500c1a4-0000-4000-8000-000000000001"`.
- Fixture 18 of §11 (duplicate `id::` across pages) reports one duplicate.
- Block with existing id → no change.

## Notes
Part of BIT-US-0093. Implements BIT-SP-0001.R7. ADR-006.
