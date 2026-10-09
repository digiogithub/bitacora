---
tags:
    - changes
    - print
    - bitacora-app
---
# BIT-US-0181: print view of the current page

**What:** The title-bar Printer button, `Ctrl/Cmd+P` (`bitacora::Print`), the command palette entry "Print page..." and the app (Bitacora) menu entry render the page or zoomed block on screen to a self-contained HTML file in the app cache and open it in the system browser, whose print dialog appears on load ("Save as PDF" works the same way). Plan: [[bit-m-0011-owner-improvements-plan]].

**Why:** GPUI has no print API. A generated print view reuses the already parsed row model and needs no native print code.

**Files and symbols**
- `crates/bitacora-app/src/print.rs` (new, pure): `render_html(&PrintDoc)`, `layout_html`, `stylesheet` (colours from `BitacoraTheme::print()`), `file_url`, `print_path`, `write_print_file` (temp file + fsync + rename under `<cache>/print/`).
- `crates/bitacora-app/src/views/workspace/print_ui.rs` (new): `Workspace::can_print`, `print_tooltip`, `print_current`, `print_action`. Errors surface as translated error notifications.
- `top_bar.rs`: the Printer `IconButton` is enabled and has a tooltip; disabled (with tooltip "nothing to print") on routes other than a page or zoomed block.
- `actions.rs` (`Print`), `assets/keymaps/default.json` (`secondary-p` in the `Workspace` context; `ctrl-p` elsewhere only exists in the `Autocomplete` context, so no conflict), `palette.rs` (`PaletteCommand::Print`), `menus.rs`.
- `views/kit/button.rs`: `IconButton::tooltip`; `ui/mod.rs`: re-export of `Tooltip`.
- Locale keys `print.*` in `en.yml` and `es.yml`.

**Behaviour:** all blocks expanded; properties hidden like the editor (`is_hidden_property`); task markers as checkboxes or labels; `[[refs]]` and `#tags` as text; only http, https and mailto links become anchors; images under `assets/` use absolute `file://` URLs; all user text is HTML-escaped. Nothing is written into the graph folder.

**Known gaps:** the journals feed is not printable (open a single journal day); unsaved edits that core has not flushed yet are included only when the page view already shows them; native print dialogs are out of scope (see `docs/design/frameless-window.md`, "Printing").

**Verified:** `cargo test -p bitacora-app --locked` (653 lib tests green, including 12 `print::tests` and the menu/keymap test), `cargo clippy -p bitacora-app --all-targets --locked -- -D warnings`. The browser hand-off and the visual output were not exercised headless.
