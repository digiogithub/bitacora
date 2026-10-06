---
id: BIT-T-0108
type: task
title: Implement tri-lb-title-parsing decoder
status: done
priority: critical
parent: BIT-US-0081
milestone: BIT-M-0002
author: mcp
labels: [bitacora-core, compat]
estimate: 2
created: 2026-10-06T14:29:47Z
updated: 2026-10-06T16:58:59Z
closed: 2026-10-06T16:58:59Z
---

## Description
`crates/bitacora-core/src/naming/triple_lowbar.rs`, `pub fn decode(file_body: &str) -> String` (`graph_parser/util.cljs:153-160`):
1. Replace `___` with `/` left-to-right (so `____` → `/_`).
2. Replace each `%XX` token (case-insensitive hex) individually via `decodeURIComponent(token)`; tokens that are not decodable alone (e.g. `%E4`, part of a multi-byte sequence) stay as-is (`safe-decode-uri-component`, `util.cljs:10-22`). Single pass: `%252F` → `%2F`.
3. `make-valid-namespaces`: split on `/`, drop empty segments, join with `/` (`util.cljs:144-149`).

## Acceptance Criteria
- Decoding every §3.2 file body gives the expected title.
- `%E4%B8%AD` stays `%E4%B8%AD` (per-token decode), `%252F` → `%2F`, `a______b` behaves as JS reference.
- Never panics on arbitrary input (proptest).

## Notes
Refs BIT-SP-0002.R6.
