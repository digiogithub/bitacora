---
id: BIT-EP-0002
type: epic
title: GPUI Kit app shell and block-editor spike
status: backlog
priority: critical
milestone: BIT-M-0001
author: mcp
labels: [ui, spike]
created: 2026-10-06T14:21:13Z
updated: 2026-10-06T14:21:13Z
---

## Description
De-risk the UI stack: a GPUI Kit window with sidebar/dock layout, theme switching, tokio bridge, and a throwaway prototype of the custom block editor (one `EntityInputHandler` text buffer per block, `gpui::list` virtualization, 1,000 blocks) validated with IME on Linux (X11 + Wayland), macOS and Windows.

## Acceptance Criteria
- App window opens on the 3 OSes from CI artifacts.
- Spike report in `docs/design/` with IME, wrapping, caret, selection and performance findings and a go/no-go for ADR-002.

## Notes
See [[gpui-and-gpui-kit]] §3, [[block-editor]] §7.
