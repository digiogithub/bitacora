---
id: BIT-T-0063
type: task
title: "Outline fixtures: CRLF, BOM, blank lines, unclosed fences"
status: backlog
parent: BIT-US-0026
milestone: BIT-M-0002
author: mcp
labels: [bitacora-markdown, test, fixtures]
estimate: 2
created: 2026-10-06T14:28:29Z
updated: 2026-10-06T14:28:29Z
---

## Description
Add fixture files under `fixtures/markdown/outline/` for round-trip fixtures 1, 3, 4, 6, 7 of [[02-markdown-block-syntax]] §11: 2-space indented page, CRLF page, BOM page, tabs+spaces mixture, blank lines between and inside blocks, fenced code containing `- x`, `foo:: bar`, `[[x]]`, `((uuid))`, and unclosed fence / unclosed `#+BEGIN_QUOTE` followed by bullets. For each, record expected segmentation (`*.expected.json`: block start offsets, raw levels, parent indices). Generate the expectations for the unclosed-region cases by running `mldoc@1.5.7` `parseJson` with the `default-config` JSON (`mldoc.cljc:57-75`) in a scratch Node project (script in `tools/mldoc-diff/`), and set the `unclosed_region` default accordingly; record the finding in [[02-markdown-block-syntax]] §12 (open question 1) and BOM behaviour (open question 2).

## Acceptance Criteria
- Fixtures and expected JSON committed; `cargo test -p bitacora-markdown outline_fixtures` passes.
- Open questions 1 and 2 in [[02-markdown-block-syntax]] are answered with evidence.

## Notes
Part of BIT-US-0026. Implements BIT-SP-0001.R1, R19. Feeds the round-trip harness story.
