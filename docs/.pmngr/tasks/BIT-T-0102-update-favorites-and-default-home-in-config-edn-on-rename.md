---
id: BIT-T-0102
type: task
title: Update :favorites and :default-home in config.edn on rename and delete
status: done
parent: BIT-US-0061
milestone: BIT-M-0003
author: mcp
labels: [bitacora-config, bitacora-core, rename]
estimate: 2
created: 2026-10-06T14:29:22Z
updated: 2026-10-06T19:30:10Z
closed: 2026-10-06T19:30:10Z
---

## Description
Using the comment-preserving editor in `bitacora-config` (BIT-EP-0004), add `fn rename_page_refs(cfg, old, new)` and `fn remove_favorite(cfg, name)` in `crates/bitacora-config/src/page_refs.rs`: replace matching strings (case-insensitive by page key) in the `:favorites` vector and `:default-home {:page …}` (`page.cljs:487-500`, `313-340`). Emitted as an `Op::EditConfig` in the same transaction as the rename/delete.

## Acceptance Criteria
- Our self-authored commented fixture `config.edn` with `:favorites ["Old" "Other"]` → `["New" "Other"]`, diff is one line.
- `:default-home {:page "old"}` matched case-insensitively.
- Delete removes the entry; no other byte changes.

## Notes
BIT-SP-0002.R4, R13, R14. [[01-file-graph-layout]] §2.1. ADR-015 (no Logseq template in fixtures).
