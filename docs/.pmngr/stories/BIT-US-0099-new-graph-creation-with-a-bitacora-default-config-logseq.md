---
id: BIT-US-0099
type: story
title: New graph creation with a Bitacora default config Logseq accepts
status: done
priority: medium
parent: BIT-EP-0009
milestone: BIT-M-0003
author: mcp
labels: [core, compat, config]
estimate: 3
created: 2026-10-06T14:31:16Z
updated: 2026-10-06T19:13:32Z
closed: 2026-10-06T19:13:32Z
---

## Description
As a new user, I want Bitacora to create a fresh graph that Logseq opens as if it had created it, so that I can switch between both apps from day one.

Logseq creates `pages/`, `journals/`, `logseq/config.edn`, empty `logseq/custom.css`, `logseq/.recycle/`, and `pages/contents.md` with `-` (`handler/repo.cljs:40-124`). Bitacora creates the same layout but writes its own default `config.edn`, authored by us, containing only the keys/values Logseq needs (facts such as `:meta/version 1` and `:file/name-format :triple-lowbar`), with no text or comments copied from Logseq's template. Logseq's MD5 "untouched default config" detection (`config.cljs:349-353`) will therefore not match; that is an accepted non-goal.

## Acceptance Criteria
- "New graph" in an empty folder creates `pages/contents.md` (`-`), `journals/`, `assets/` (on first use only is acceptable), Bitacora's default `logseq/config.edn`, empty `logseq/custom.css`, `logseq/.recycle/`.
- The config contains `:file/name-format :triple-lowbar`.
- Refuses a non-empty folder unless it already is a graph (then just opens it).
- Manual check: Logseq 0.10.15 opens the graph without conversion prompt.

## Notes
Implements: BIT-SP-0002.R19, BIT-SP-0002.R1
See [[01-file-graph-layout]] §1, §2.3. ADR-013, ADR-014. ADR-015: Logseq's config template is AGPL and is not embedded; we write our own default config and drop the MD5 byte-identity goal.
