---
id: BIT-US-0056
type: story
title: Load config.edn and compute the effective graph config
status: backlog
priority: critical
parent: BIT-EP-0004
milestone: BIT-M-0002
author: mcp
labels: [config, compat]
estimate: 5
created: 2026-10-06T14:28:35Z
updated: 2026-10-06T14:37:04Z
---

## Description
As a user with an existing Logseq graph, I want Bitacora to read `logseq/config.edn` (merged over the optional global `~/.logseq/config/config.edn`) exactly as Logseq does, so that file naming, journals, hidden paths and indentation behave the same in both apps.

`bitacora-config` parses EDN (including `(fn …)` lists, sets, keywords, tagged literals), merges `defaults ← global ← graph` with shallow map merge, and exposes typed accessors for the keys that affect layout and parsing.

## Acceptance Criteria
- Missing `:file/name-format` ⇒ `NameFormat::Legacy`; `:file/name-format :triple-lowbar` (as in new-graph configs) ⇒ `TripleLowbar`.
- Global `{:macros {"a" "x"} :preferred-workflow :todo}` + graph `{:macros {"b" "y"} :preferred-workflow :now}` ⇒ `:now` and macros `{a, b}`.
- Duplicate keys or invalid EDN produce a `ConfigError` diagnostic; the graph still loads with defaults + global.
- Typed accessors exist for every key in [[01-file-graph-layout]] §2.2 with Logseq defaults.
- Bitacora's default `config.edn` and our own large, heavily commented fixture config (covering every key of §2.2, `(fn …)` lists, sets, tagged literals, `#_` discards) parse without error. Optionally, a dev-only ignored test parses the config template from the local `../logseq` checkout as a black-box check (never copied into the repo).

## Notes
Implements: BIT-SP-0002.R3
[[01-file-graph-layout]] §2.1–2.3, `state.cljs:335-383`. ADR-013. ADR-015 (no copy of Logseq's config template; own fixtures). [[architecture]]
