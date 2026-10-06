---
id: BIT-US-0071
type: story
title: Comment-preserving config.edn editor
status: done
priority: high
parent: BIT-EP-0004
milestone: BIT-M-0002
author: mcp
labels: [config, compat]
estimate: 5
created: 2026-10-06T14:29:02Z
updated: 2026-10-06T16:52:52Z
started: 2026-10-06T16:46:26Z
closed: 2026-10-06T16:52:52Z
---

## Description
As a user who hand-edits `logseq/config.edn`, I want Bitacora to change only the key it needs (favorites, default home, UI toggles) so that my comments, formatting and commented-out options survive and git diffs stay one line.

Implement a rewrite-edn-style concrete syntax tree in `bitacora-config` that keeps every byte (whitespace, comments, commas, discards) and supports surgical `assoc`/`update`/`dissoc` on top-level keys and nested map paths.

## Acceptance Criteria
- Round-trip: `print(parse(bytes)) == bytes` for our own large, heavily commented fixture config (`crates/bitacora-config/tests/data/commented-config.edn`, self-authored), Bitacora's default config and every `config.edn` under `fixtures/graphs/**`.
- Adding `"Projects"` to `:favorites []` in the commented fixture config changes exactly one line.
- Setting a missing `:default-home {:page "Home"}` inserts before the top-level closing `}` and changes no other line.
- Writes go through the core writer (atomic) — the editor itself returns bytes only.

## Notes
Implements: BIT-SP-0002.R4
Logseq uses `borkdude.rewrite-edn` (`handler/config.cljs:9-31`). [[01-file-graph-layout]] §2.1. ADR-011 (atomic writes). ADR-015 (no copy of Logseq's config template in fixtures). [[architecture]]
