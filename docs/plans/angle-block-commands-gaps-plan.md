---
created_at: 2026-10-13T07:58:04.768493011Z
updated_at: 2026-10-13T07:58:04.768493011Z
tags:
    - plan
    - editor
    - logseq-compat
---
# Plan: angle (<) block commands — remaining Logseq gaps (BIT-US-0188)

Continues [[changes/2026-10-07-special-blocks.md]] (BIT-US-0169) under [[bitacora-v2-plan]] (epic BIT-EP-0017). Design: [[block-editor]].

## Logseq 0.10.15 behaviour (read, not copied — ADR-015)
- Trigger: typing `<` opens block-commands menu (`handler/editor.cljs:1901`); `《〈` is converted to `<` (`:3010-3018`); Backspace on `<` closes it (`:2774`).
- Commands: `commands.cljs:189-214` — Quote, Src, Query, Latex export, Note, Tip, Important, Caution, Pinned, Warning, Example, Export, Verse, Ascii, Center, Comment, plus config.edn `:commands`.
- Insertion `->block` (`:155-185`): `#+BEGIN_X\n\n#+END_X`, caret on body line; Markdown `src` is a ``` fence; prefixed with `\n` when not at line start (`:502-513`).
- Render (`components/block.cljs:3262-3315`): admonitions via `ui/admonition` (icon per kind), CENTER centered, unknown names -> div with class; body rendered as full markup; EXPORT html/latex rendered; QUERY = advanced query.

## Already in Bitacora (BIT-US-0169)
Menu (`editor/commands.rs` `ANGLE`, `begin_block`), trigger (`editor/completion.rs` `command_trigger`, word-start only — intentional divergence), render model (`render/model.rs` `Special`, `CalloutKind`), callout view, code-region editing.

## Phases (each independently verifiable)
1. BIT-T-0510 — nested markup inside special regions + unknown names as containers. Verify: model tests, round-trip fixture.
2. BIT-T-0511 — EXPORT html/latex/ascii labelled rendering, decision in design doc.
3. BIT-T-0512 — config.edn `:commands` in `/` and `<` menus.
4. BIT-T-0513 — full-width `《〈` trigger; document word-start divergence.
5. BIT-T-0514 — manual visual check; close BIT-US-0169.

## Progress
- 2026-10-13: plan + backlog created; no code yet.
