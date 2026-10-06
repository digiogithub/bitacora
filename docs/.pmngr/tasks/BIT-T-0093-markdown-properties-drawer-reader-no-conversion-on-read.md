---
id: BIT-T-0093
type: task
title: "Markdown :PROPERTIES: drawer reader (no conversion on read)"
status: done
parent: BIT-US-0058
milestone: BIT-M-0002
author: mcp
labels: [bitacora-markdown, parser, properties]
estimate: 2
created: 2026-10-06T14:29:17Z
updated: 2026-10-06T16:59:58Z
started: 2026-10-06T16:46:41Z
closed: 2026-10-06T16:59:58Z
---

## Description
In `crates/bitacora-markdown/src/properties/drawer.rs`, parse an org-style `:PROPERTIES:` … `:END:` drawer inside a Markdown block body into the same `PropertyGroup` model, with key mapping from `->new-properties` (`deps/graph-parser/src/logseq/graph_parser/property.cljs:135-158`): `id`/`custom_id`/`custom-id` → `id`, `last-modified-at` → `updated-at`, `_` → `-`. Mark the group `origin = Drawer` so the serializer can convert it to `key:: value` lines **only** when the block is edited (canonical serialization story). No bytes change on read.

## Acceptance Criteria
- Fixture 9 of §11: `"- a\n  :PROPERTIES:\n  :custom_id: 6500c1a4-0000-4000-8000-000000000001\n  :END:"` → id property; round-trip unchanged.
- `:END:` matched case-insensitively.
- Drawer inside a fence is ignored.

## Notes
Part of BIT-US-0058. Implements BIT-SP-0001.R14.
