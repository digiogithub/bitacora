---
created_at: 2026-10-07T19:40:00Z
updated_at: 2026-10-07T19:40:00Z
tags:
    - change
    - editor
    - ui
---
# Special blocks: code, quote and admonitions (BIT-US-0169)

Part of [[bitacora-v2-plan]] (epic BIT-EP-0017). Tasks BIT-T-0503..0506.

## What changed
- Slash/angle commands (`editor/commands.rs`, `begin_block`): mirrors Logseq 0.10.15 `commands.cljs` `->block` (`:155-185`) and the block list (`:189-210`): `<src` writes a Markdown fence (caret after the opening fence), new `<latex` (`#+BEGIN_EXPORT latex`) and `<ascii` (`#+BEGIN_EXPORT ascii`); every other `<name` writes `#+BEGIN_NAME` / `#+END_NAME` with the caret on the empty body line. `/Code block` is unchanged.
- Render model (`render/model.rs`): new `CalloutKind`, `BodyItem::{Callout, Center, Verse}`; consecutive `> ` lines join `BodyItem::Quote`; `#+BEGIN_EXAMPLE` is verbatim code (no label, no tokens); `COMMENT` stays hidden.
- Views (`views/block_view.rs`): `callout_element` (tinted card, left rule, icon + localized title per kind, palette `accent`/`warn` only, `dims::CALLOUT_TINT`), quote rule uses `line_2`, code blocks get a "Copy code" button (stops mouse-down so the row does not enter edit mode). No new dependency: the existing lexical highlighter (`render/highlight.rs`) is kept.
- Editing (`editor/regions.rs`, `editor/view.rs`): inside an open fence or `#+BEGIN_X` region Enter inserts a newline (Logseq does this for src/admonition blocks), Tab inserts two spaces, Shift+Tab removes up to two. After the closing line Enter splits as usual. Clicking a block still shows the raw markers.
- Locale keys `page.copy_code`, `page.callout_*` (en, es).

## Verification
- `cargo test -p bitacora-app -p bitacora-markdown --locked`: 821 passed. New: `roundtrip_suite` cases "20 ..." (lossless), model tests, `regions` unit tests, slash command text tests, gpui tests for Enter/Tab/Shift+Tab in a fence and Enter after the closing fence, callout style test in light and dark.
- `cargo clippy -p bitacora-app -p bitacora-markdown --all-targets --locked -- -D warnings` clean.
