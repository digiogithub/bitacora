---
id: BIT-US-0181
type: story
title: Print button builds a print view of the current page and opens the system print dialog
status: in_progress
priority: high
parent: BIT-EP-0027
milestone: BIT-M-0011
author: mcp
labels: [bitacora-app, ui, print]
estimate: 5
created: 2026-10-09T07:10:42Z
updated: 2026-10-09T07:12:04Z
started: 2026-10-09T07:12:04Z
---

## Description
The title-bar Printer button (`top_bar.rs`, `IconButton::new("top-pdf", Glyph::Printer).disabled(true)`) does nothing. GPUI has no print API, so printing goes through a generated print view:

1. Render the active route (page, journal day, or zoomed block) to a self-contained HTML document: title, outline with bullets/indentation, all blocks expanded, rendered inline markup (bold/italic/code/links/page refs as text, tasks markers, properties hidden like the editor does), images from `assets/` embedded or linked with absolute `file://` URLs, print stylesheet based on `BitacoraTheme::print()` / light palette, `@page` margins, no app chrome.
2. Write it atomically to the app cache dir (never inside the graph folder — user files are sacred), e.g. `<cache>/print/<slug>.html`.
3. Open it in the system default browser (`cx.open_url("file://...")`), with an inline `window.onload = () => window.print()` so the OS/browser print dialog appears immediately (users can also "Save as PDF").
4. Enable the button, add a `Print` action with `Cmd/Ctrl+P` keybinding, command palette entry and gear/app menu entry. Disabled (with tooltip) when the route has nothing printable (e.g. settings).
5. All new strings via `t!` in `en.yml`/`es.yml`.

Native print dialogs (NSPrintOperation, GTK PrintOperation, Win32 PrintDlg) are out of scope; record that in docs.

## Acceptance Criteria
- Clicking Printer or pressing Ctrl/Cmd+P on a page opens the print view with the system print dialog.
- HTML renderer is a pure, unit-tested function (golden test over a fixture page: nesting, tasks, refs, code blocks, HTML escaping of user text).
- Nothing is written into the graph folder.
- `cargo test -p bitacora-app` and clippy green.
