---
id: BIT-US-0031
type: story
title: "Keyboard and IME input layer: key contexts, Logseq default keymap and IME composition"
status: in_review
priority: high
parent: BIT-EP-0007
milestone: BIT-M-0003
author: mcp
labels: [editor, ui, bitacora-app]
estimate: 5
created: 2026-10-06T14:28:07Z
updated: 2026-10-06T21:12:41Z
started: 2026-10-06T21:12:41Z
---

## Description
As a multilingual Logseq user, I want my muscle-memory shortcuts to work and my IME (Japanese, Chinese, dead keys, emoji) to compose text correctly, so that I can switch to Bitacora without relearning or fighting the editor.

Defines GPUI actions and the four key contexts (`Outliner`, `BlockSelection`, `BlockEditor`, `Autocomplete`) with Logseq defaults, a user keymap override file, and full `EntityInputHandler` IME support in `BlockEditor`.

## Acceptance Criteria
- Actions for every MVP command are registered with Logseq default bindings per platform (`Mod` = Cmd on macOS, Ctrl elsewhere).
- Context precedence `Autocomplete` > `BlockEditor` > `BlockSelection` > `Outliner` is verified by `#[gpui::test]`.
- A keymap file in the app config dir overrides bindings; invalid entries are reported and ignored.
- IME composition (marked text) works on Linux (IBus/Fcitx), macOS and Windows; Enter/Backspace/Tab do not trigger outliner commands during composition; no commit mid-composition.
- Manual IME checklist documented and executed on the three OSes.

## Notes
Implements: BIT-SP-0004.R20, BIT-SP-0004.R21.
See [[block-editor]] §7.3, [[04-editor-outliner-operations]] §7, [[gpui-and-gpui-kit]]. ADR-001, ADR-002. Builds on the IME spike from BIT-EP-0002.
