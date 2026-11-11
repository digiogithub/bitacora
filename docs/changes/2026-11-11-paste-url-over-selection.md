---
created_at: 2026-11-11T20:57:30.8899531Z
updated_at: 2026-11-11T20:59:04.70026431Z
tags:
    - change
    - feature
    - editor
---
# 2026-11-11 — Paste a URL over selected text makes a Markdown link

Owner request, backlog item BIT-T-0516 (part of BIT-SP-0004.R13, paste). Continues [[block-editor]]. Released in [[2026-11-11-release-2.4.0]].

## What changed
- `crates/bitacora-core/src/editor/clipboard.rs`: new pure `link_paste(before, selected, after, pasted)` and private `is_link_url`; re-exported from `crates/bitacora-core/src/editor/mod.rs`.
- `crates/bitacora-app/src/editor/view.rs` `OutlineEditor::paste`: when editing with a non-empty selection and the paste is not raw, the selection is replaced by the link in the buffer (one buffer edit, flushed like typing).
- Rules, matching Logseq 0.10.15 behaviour (`src/main/frontend/handler/paste.cljs:137-146`, `deps/graph-parser/src/logseq/graph_parser/util.cljs:82`; read for behaviour only, ADR-015): URL over text gives `[text](url)`; text over a selected URL gives `[text](url)`; URL over URL, or a selection that is the label or target of an existing link, is a plain replacement. Only `http`, `https`, `ftp`, `ws`, `wss` URLs with a host count. Our deviations: the pasted text is trimmed and multi-line pastes or selections never convert.
- `docs/design/block-editor.md` clipboard paragraph and `CHANGELOG.md` updated.

## Verification
- `cargo test -p bitacora-core --test editor_commands pasting` (new `pasting_a_url_over_a_selection_makes_a_markdown_link`): pass.
- `cargo test -p bitacora-app pasting_a_url` (new `editor::tests::pasting_a_url_over_selected_text_makes_a_link`): pass.
- `cargo clippy -p bitacora-app -p bitacora-core --all-targets --locked -- -D warnings`: clean.
- Full `bitacora-app` suite: 699–700 of 701; `views::workspace::tests::opening_a_graph_hides_the_picker_indexes_and_shows_a_page` and `deleting_the_open_page_recycles_it_and_updates_favorites_and_recent` failed intermittently on their polling timeouts while the host load average was above 200 from unrelated jobs; both pass when run alone (32/32 in `views::workspace::tests`).