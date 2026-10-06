---
id: BIT-US-0060
type: story
title: "Spike: 1,000-block virtualized page performance"
status: in_review
priority: high
parent: BIT-EP-0002
milestone: BIT-M-0001
author: mcp
labels: [ui, spike, performance, bitacora-app]
estimate: 5
created: 2026-10-06T14:28:56Z
updated: 2026-10-06T17:59:55Z
started: 2026-10-06T17:33:08Z
---

## Description
As a developer, I want to measure a page of 1,000+ blocks rendered with `gpui::list` (variable heights) and the spike editor, so that we know scrolling, editing latency, memory, startup time and binary size are acceptable before building the real page view.

## Acceptance Criteria
- Spike page loads 1,000 blocks (generated, plus one real long page from `fixtures/graphs/logseq-docs`) via `gpui::list` + `ListState` with lazily measured variable row heights, indentation guides and bullets.
- Collapse/expand of a subtree updates the list with `ListState::splice` on the affected range only.
- Measurements recorded on the 3 OSes: scroll FPS, keystroke-to-paint latency in a block mid-page, RSS memory, cold start to first frame, release binary size (with and without tree-sitter features of GPUI Kit).
- Targets stated up front (e.g. 60 fps scroll, < 16 ms keystroke latency, < 1 s cold start) and pass/fail reported.

## Notes
- [[block-editor]] §7.1 (PageView row list, `ListState::splice`); [[gpui-and-gpui-kit]] §1.1 (Lists), Risk R8, open question on binary size and cold start.
- Feeds the spike report (go/no-go for ADR-002).
