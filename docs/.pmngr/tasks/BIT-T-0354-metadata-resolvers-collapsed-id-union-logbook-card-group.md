---
id: BIT-T-0354
type: task
title: "Metadata resolvers: collapsed, id union, LOGBOOK, card-* group, LWW keys"
status: backlog
priority: critical
parent: BIT-US-0050
milestone: BIT-M-0003
author: mcp
labels: [bitacora-merge, merge, metadata]
estimate: 3
created: 2026-10-06T14:34:49Z
updated: 2026-10-06T15:17:52Z
---

## Description
`crates/bitacora-merge/src/meta.rs`: `collapsed` (ours if changed, else theirs; policy from `sync.collapsed_policy`); `id` union — both added different ids → keep id referenced in the index (`IndexLookup::is_referenced(uuid)`), else lexicographically smaller; record `IdRewrite{from,to}` applied as a post-pass rewriting `((from))` in all merged outputs; LOGBOOK — set-union of CLOCK lines sorted by start, at most one open clock (latest kept); `card-*` — whole group from side with later `card-last-reviewed` (parse ISO; tie → ours); other Metadata-class keys (`query-*`, `filters`, `hl-*`, marker timestamps, `created-at`…) last-writer-wins by side commit time. Never produce conflicts.

## Acceptance Criteria
- Table-driven tests for every rule incl. the BIT-SP-0006.R11 scenarios.

## Notes
Story BIT-US-0050. Implements BIT-SP-0006.R11. ADR-009. See [[02-markdown-block-syntax]] §5.3–5.4.
ADR-016: lives in the `bitacora-merge` crate (depends only on `bitacora-markdown`), shared by `bitacora-core` (external edits, BIT-US-0069) and `bitacora-sync`.
Needed by BIT-US-0069 (ADR-016).
