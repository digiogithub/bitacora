---
id: BIT-US-0094
type: story
title: Content vs metadata vs identity classification for merges
status: backlog
priority: medium
parent: BIT-EP-0003
milestone: BIT-M-0002
author: mcp
labels: [markdown, compat, merge]
estimate: 3
created: 2026-10-06T14:30:45Z
updated: 2026-10-06T14:30:45Z
---

## Description
As the git sync engine, I want `bitacora-markdown` to classify every property line and drawer of a block as Content, Metadata or Identity and to diff two versions of a block by class, so that metadata-only differences (`collapsed::`, `card-*`, LOGBOOK, property order, whitespace) can be auto-resolved and only real content conflicts reach the visual resolver.

## Acceptance Criteria
- Classification table of [[02-markdown-block-syntax]] §5.4 implemented as data (one place), covering underscore variants and `:block-hidden-properties` (still Content).
- `classify_diff(base, ours, theirs)` reports `MetadataOnly`, `Content` or `IdentityConflict` per block.
- Helper merges metadata deterministically: last-writer-wins per key, union of CLOCK lines, de-dup by uuid for `id`.
- Unit tests cover each row of the §5.4 table.

## Notes
Implements: BIT-SP-0001.R16.
See [[02-markdown-block-syntax]] §5.4; [[git-sync-merge]]; ADR-008, ADR-009. Consumed by BIT-EP-0012.
