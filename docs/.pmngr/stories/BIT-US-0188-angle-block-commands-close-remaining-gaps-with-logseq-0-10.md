---
id: BIT-US-0188
type: story
title: "Angle (<) block commands: close remaining gaps with Logseq 0.10.15"
status: in_review
priority: medium
parent: BIT-EP-0017
milestone: BIT-M-0006
author: mcp
labels: [v2, editor, ui, bitacora-app, logseq-compat]
created: 2026-10-13T07:57:36Z
updated: 2026-10-13T08:29:22Z
started: 2026-10-13T08:09:22Z
---

## Description
Follow-up to BIT-US-0169 (in_review), which already ships the `<` menu (16 commands, `ANGLE` in `crates/bitacora-app/src/editor/commands.rs`), `begin_block` insertion mirroring Logseq `->block`, and read-mode rendering for QUOTE / NOTE / TIP / IMPORTANT / CAUTION / WARNING / PINNED / CENTER / VERSE / EXAMPLE / SRC / QUERY / COMMENT.

A source review against Logseq 0.10.15 (`../logseq`) found these remaining gaps:

1. **Rich content inside special regions.** Logseq renders the body of every `#+BEGIN_X` region with `markup-elements-cp` (`src/main/frontend/components/block.cljs:3286-3315`): paragraphs, lists, headings, nested code fences. Bitacora flattens the body into one inline `TextLayout` (`render/model.rs` `Special::item`), so a list or a fence inside `#+BEGIN_NOTE` is shown as plain lines.
2. **Unknown region names.** Logseq renders `#+BEGIN_<any>` not in its list as `<div class=name>` with rendered markup (`block.cljs:3308-3312`). Bitacora falls back to a code block for any unknown name.
3. **EXPORT regions.** Logseq renders `#+BEGIN_EXPORT html` as sanitized HTML and `#+BEGIN_EXPORT latex` with KaTeX (`block.cljs:3262-3280`). Bitacora shows both as highlighted code. Decide: keep code view (no HTML/KaTeX engine yet) but add a clear label (`HTML export`, `LaTeX export`), or render via the existing safe HTML subset (`editor/html.rs`).
4. **Custom commands from `config.edn` `:commands`.** Logseq appends `(state/get-commands)` (`:commands` in config.edn, `state.cljs:522`) to both the `/` and `<` menus (`commands.cljs:214`, `:310`); string values are inserted verbatim (`handler/editor.cljs:1369-1387`). Bitacora ignores `:commands`.
5. **Full-width trigger.** Logseq opens the `<` menu when the user types `《〈` (CJK IME), replacing it with `<` (`handler/editor.cljs:3010-3018`). Bitacora only reacts to ASCII `<`.
6. **Manual visual check** of BIT-T-0505 (callouts light/dark) is still pending; close BIT-US-0169 once done.

Intentional divergence kept: Bitacora only opens the menu when `<` starts a word (`editor/completion.rs` `command_trigger`), Logseq opens it on any `<`. This avoids popups while typing `a<b`; document it in `docs/design/block-editor.md`.

## Acceptance Criteria
- A `#+BEGIN_NOTE` (and every other special kind) containing a list, a heading and a code fence renders them structured in read mode; `serialize(parse(bytes)) == bytes` still holds (fixtures unchanged or new fixture added + MANIFEST updated).
- `#+BEGIN_FOO` renders its body as normal markup, not as code.
- EXPORT html/latex render with the agreed treatment (decision recorded in `docs/design/block-editor.md`).
- `:commands [["js" "Javascript"]]` in `logseq/config.edn` shows `js` in both `/` and `<` menus and inserts `Javascript`; config parse stays comment-preserving.
- Typing `《〈` opens the `<` menu and the text becomes `<`.
- Tests: model unit tests, command text tests, completion trigger tests, `#[gpui::test]` for menu with custom commands.

## Notes
Logseq is AGPL: behaviour read from source, never copied (ADR-015). Plan: KB `plans/angle-block-commands-gaps-plan.md`.
