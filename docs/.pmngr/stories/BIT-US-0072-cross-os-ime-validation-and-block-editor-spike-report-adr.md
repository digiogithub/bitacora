---
id: BIT-US-0072
type: story
title: Cross-OS IME validation and block-editor spike report (ADR-002 go/no-go)
status: in_progress
priority: critical
parent: BIT-EP-0002
milestone: BIT-M-0001
author: mcp
labels: [ui, spike, ime, docs]
estimate: 5
created: 2026-10-06T14:29:22Z
updated: 2026-10-06T17:33:08Z
started: 2026-10-06T17:33:08Z
---

## Description
As the project lead, I want the spike validated with real input methods on every supported platform and its findings written up with a clear go/no-go, so that ADR-002 (custom block editor on `EntityInputHandler`) is either confirmed or replaced by the GPUI Kit Textarea-per-block fallback before M2 work begins.

## Acceptance Criteria
- An IME/text test checklist exists and has been executed on: macOS (Japanese Kana/Romaji, Pinyin, dead keys, emoji palette, dictation optional), Windows 11 (Microsoft Japanese and Pinyin IME), Linux X11 and Wayland (GNOME + KDE) with fcitx5 and ibus.
- Each case recorded as pass/fail with notes/screenshots.
- `docs/design/block-editor-spike.md` covers IME, wrapping, caret, selection, clipboard, performance numbers, Textarea fallback comparison, risks, and a go/no-go for ADR-002; linked from [[block-editor]] and [[architecture]].
- If the decision changes, a new ADR row is added to [[architecture]] §5 (never silently edited).

## Notes
- Epic acceptance criterion (spike report in `docs/design/`).
- [[gpui-and-gpui-kit]] §1.2 (IME per platform; Linux most fragile), §3.1 (Option A fallback), §3.2 item 7, Risks R3/R4; [[block-editor]] §7.
- ADR-002. AGENTS.md §6 (manual IME checklist per OS before releases — this checklist becomes that artifact).
