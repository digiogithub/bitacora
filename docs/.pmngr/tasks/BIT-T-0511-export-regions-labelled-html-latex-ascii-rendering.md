---
id: BIT-T-0511
type: task
title: "EXPORT regions: labelled html/latex/ascii rendering"
status: backlog
priority: medium
parent: BIT-US-0188
milestone: BIT-M-0006
author: mcp
estimate: 2
created: 2026-10-13T07:57:55Z
updated: 2026-10-13T07:57:55Z
---

## Description
Logseq renders `#+BEGIN_EXPORT html` as sanitized HTML and `#+BEGIN_EXPORT latex` with KaTeX (`components/block.cljs:3262-3280`). Bitacora has no KaTeX/HTML engine. Decide and record in `docs/design/block-editor.md`: default = code view with an explicit kind label ("HTML export", "LaTeX export", "ASCII export"); optional = render html through the safe subset in `editor/html.rs`.

## Acceptance Criteria
- Decision recorded (ADR row in `docs/architecture.md` if it diverges from Logseq).
- Model test per export type; locale keys en/es.
