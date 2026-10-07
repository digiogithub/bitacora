# Accessibility audit for 1.0

Audit of BIT-T-0338 (BIT-US-0109). Facts about what GPUI can expose are in [[gpui-and-gpui-kit]] section 1.10; the measurements of the same pass are in [[performance-report-1.0]].

## 1. What is automated

| Check | Where |
|---|---|
| Contrast of the four bundled themes (WCAG 2.x): 18 text pairs at 4.5:1, focus ring and caret at 3:1, text over the selection colour at 4.5:1 | `crates/bitacora-app/src/contrast.rs` (`every_bundled_theme_meets_wcag_aa`, `selection_keeps_text_readable`) |
| Every `bitacora::` and `outliner::` action has a default key binding or a documented keyboard route (no action is mouse-only) | `keymap.rs` (`every_action_is_reachable_from_the_keyboard`) |
| Reduced motion follows the setting | `theme.rs` (`reduce_motion_follows_the_setting`) |
| Translated slash menu and Spanish filter | `tests/language.rs` |

The audit changed colours: **Paper** `muted.foreground`; **Solarized Light** foregrounds, muted text, link, primary and selection (the canonical Solarized accent `#268bd2` is 3.4:1 on the cream background); **Solarized Dark** foregrounds, muted text, link and primary. Midnight already passed. User themes are not checked; `contrast::ratio` is public if the settings page ever wants to warn about them.

## 2. Reduced motion

`AppSettings.reduce_motion` (Settings > Appearance, off by default) calls `App::set_reduce_motion`, which GPUI's animations and spinners honour, and keeps the editor caret steady (no blink). GPUI cannot read the operating system preference, hence the in-app switch.

## 3. Keyboard checklist

"Focus" is how the user sees where the keyboard is. Components of GPUI Kit (buttons, inputs, menus, switches, tables, dropdowns) take Tab focus and draw the theme `ring` colour (contrast-checked above).

| View | Keyboard route | Focus indication | Status |
|---|---|---|---|
| Workspace shell | `Ctrl/Cmd+K` search, `Ctrl/Cmd+Shift+P` actions palette, `Ctrl/Cmd+B` / `+Shift+B` sidebars, `Ctrl/Cmd+[ ]` history, `Ctrl/Cmd+,` settings, `g j` / `g a`, `Ctrl/Cmd+Q` | window focus | ok |
| Outliner (page, journals, sidebar pages) | Arrows, Tab/Shift+Tab indent, Enter, Esc, `Ctrl/Cmd+Up/Down` fold, `Ctrl/Cmd+.` zoom, block selection with Shift+arrows, undo/redo; fold arrows and bullets are mouse targets with keyboard equivalents | caret and selection fill | ok |
| Search palette | type, Up/Down, Enter, Shift+Enter (sidebar), Tab (scope), Esc | highlighted row | ok |
| Actions palette | type, Up/Down, Enter, Esc; lists 19 commands including **Rename current page** (added by this audit: the title rename was click-only) | highlighted row | ok |
| All pages | `Ctrl/Cmd+K` then Enter, or the table: arrows and Enter | selected row | ok |
| Right sidebar | `Ctrl/Cmd+Shift+R` focus, Up/Down, Enter/Space, Backspace/Delete, `o` | selected item | ok |
| Left sidebar | Tab (kit sidebar menu) | kit ring | ok (not yet exercised with a screen reader) |
| Settings | `Ctrl/Cmd+,`, Tab through kit controls, Esc | kit ring | ok |
| Sync panel, sync dialog, history, conflicts, agent activity, disk diff | open from the actions palette; Esc closes the topmost overlay | kit ring on buttons | ok |
| Credential prompt | Tab, Enter; **Esc now declines** (it was only reachable with the Cancel button) | kit ring | fixed |
| Graph picker | Tab through the kit list and buttons | kit ring | ok |
| Status bar | indicators are display-only; the sync indicator is reachable through the actions palette (Sync now, Sync settings) | n/a | ok |
| Completion popup, calendar | Up/Down/Enter/Esc; Left/Right in the calendar | highlighted row | ok |

"ok" means the route exists in code and the keymap test above passes; Tab behaviour inside GPUI Kit components was not re-tested here.

Known gaps, none blocking 1.0 but worth tracking:

- Overlays are not focus traps: Tab can leave a modal for the page behind it. GPUI Kit has a `focus_trap` example; adopting it for `views::modal` is a follow-up.
- Fold arrows, bullets, the reference bubble and journal titles are click targets only; their keyboard equivalents are the outliner shortcuts above.
- Blocks being edited are drawn by a custom element, so a text caret is not a focus ring; the active block has the caret and the selection fill.

## 4. Programmatic accessibility (screen readers)

GPUI builds an AccessKit tree only while assistive technology is connected. Annotated in this pass: outline rows (`TreeItem` named with the block title, expanded and selected state), fold toggles and bullets (`Button` with a label), modal cards (`Dialog`) and their titles (`Heading`). Kit components carry their own roles. Not exposed: the text and caret of a block being edited (custom text element; see the gap list in [[gpui-and-gpui-kit]] section 1.10), toasts (no live-region API in GPUI).

**Still to do before 1.0 (manual, needs the three operating systems): validation with VoiceOver, Narrator or NVDA, and Orca.** None can run on the development host, so BIT-US-0109 stays `in_review` for this item only. Checklist for the tester: open a graph, read the journal feed, move through blocks with the arrows (each row should read its text, level and expanded state), fold and unfold, open the search palette, open Settings and change a switch, trigger a dialog and dismiss it with Esc.

> v2 note: the Paper, Solarized and Midnight themes audited above were removed in v2 (BIT-US-0116); `contrast.rs` now checks the generated "Bitacora Dark/Light" themes.
