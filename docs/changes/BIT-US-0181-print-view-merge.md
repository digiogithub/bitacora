---
created_at: 2026-10-09T07:22:39.545269716Z
updated_at: 2026-10-09T07:22:39.545269716Z
tags:
    - change
    - app
    - print
---
# BIT-US-0181 merged: print view + system print dialog

Merged into main (feature commit 4eef3f3). Details in [[BIT-US-0181-print-view]]; plan [[bit-m-0011-owner-improvements-plan]].

- Files: `crates/bitacora-app/src/print.rs` (new, pure renderer + `write_print_file`), `views/workspace/print_ui.rs` (`Workspace::print_current`, `can_print`, `print_tooltip`), `views/workspace/top_bar.rs` (Printer button enabled), `actions.rs` (`Print`, `secondary-p`), palette `PaletteCommand::Print`, `menus.rs`, `views/kit/button.rs` (`IconButton::tooltip`), `en.yml`/`es.yml` `print.*`, `docs/design/frameless-window.md` Printing section.
- Why: owner request; GPUI has no print API so HTML in cache dir + browser `window.print()`.
- Verified: merged main bitacora-app 660 tests passed, clippy clean, fmt clean. Browser/print dialog not exercised headless; story in_review.