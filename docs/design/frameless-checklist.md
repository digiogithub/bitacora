---
created_at: 2026-10-07T12:00:00Z
updated_at: 2026-10-07T12:00:00Z
tags:
    - design
    - ui
    - bitacora-app
    - frameless
---
# Frameless window manual checklist

Run on every OS before a release; record the result in the table at the bottom. Design and decision:
[[frameless-window]], ADR-033 in [[architecture]]. Implementation: `crates/bitacora-app/src/views/title_bar.rs`.
Automated coverage: `title_bar` unit tests (control selection, options, inset) and the slot render test.

## Checks

1. Window opens with no OS title bar; the 52px bar shows and the window title still appears in the taskbar / alt-tab.
2. Drag the bar: the window moves (Windows: OS caption drag; Linux/macOS: `start_window_move`).
3. Double-click the bar: maximize / restore (macOS honours the system setting).
4. Min / max (restore icon when maximized) / close buttons work and show hover colours. Close quits through the
   normal shutdown flow (no data loss, index idle). Not drawn on macOS.
5. Resize from all 8 edges and corners; resize cursors appear (Linux CSD; native on Windows/macOS).
6. The window cannot shrink below 640x400.
7. Tile left/right/corner with the OS shortcut: the shadow ring and the 1px border disappear on tiled sides and
   there is no gap or jump on the next resize. Maximized: no shadow ring.
8. Linux: right click on the bar opens the compositor window menu. Compositor without minimize/maximize
   (tiling WM): only close is drawn.
9. Linux server decorations (X11 without compositor): system frame is shown, our bar draws no controls (no
   duplicate close buttons).
10. macOS: traffic lights visible, vertically centred, not overlapped by bar content (78px inset). Full screen
    removes the inset.
11. Windows: hovering the maximize button shows snap layouts; Win+arrows snap works.
12. Light and dark theme: bar colours follow the theme.

## Results

| Date | OS / session | Result | Notes |
|---|---|---|---|
| 2026-10-07 | Linux COSMIC Wayland | Not run interactively (headless agent); granted `Client`, all controls available per the spike | Needs a human pass |
| | Linux GNOME Wayland | Pending | |
| | Linux KDE | Pending | |
| | Linux X11 no compositor | Pending | |
| | macOS | Pending | no host |
| | Windows 11 | Pending | no host |
