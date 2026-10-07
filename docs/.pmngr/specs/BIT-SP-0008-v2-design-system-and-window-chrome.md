---
id: BIT-SP-0008
type: spec
title: v2 design system and window chrome
status: backlog
author: mcp
labels: [ui, design-system, v2]
created: 2026-10-07T09:08:04Z
updated: 2026-10-07T13:08:02Z
requirements:
  R1:
    status: backlog
    trace:
      code:
        - crates/bitacora-app/src/ui/theme/palette.rs
        - crates/bitacora-app/src/ui/theme/scale.rs
        - crates/bitacora-app/src/ui/theme/bitacora.rs
        - crates/bitacora-app/assets/themes/bitacora.json
        - crates/bitacora-app/src/theme.rs
        - xtask/src/tokens.rs
        - crates/bitacora-app/src/views/dims.rs
        - docs/design/design-tokens.md
      tests:
        - crates/bitacora-app/src/ui/theme/palette_tests.rs
        - crates/bitacora-app/src/theme.rs#tests
        - crates/bitacora-app/src/lib.rs#views_have_no_magic_ui_values
  R2:
    status: backlog
    trace:
      code:
        - xtask/src/tokens.rs
        - design/tokens/tokens.json
        - .github/workflows/ci.yml
        - crates/bitacora-app/src/views/chat/render.rs
      tests:
        - xtask/src/tokens.rs#vendored_tokens_pass_contrast
        - crates/bitacora-app/src/lib.rs#ok_colour_is_only_used_for_dots_and_icons
  R3:
    status: backlog
    trace:
      code:
        - crates/bitacora-app/src/fonts.rs#register
        - crates/bitacora-app/assets/fonts
      tests: [crates/bitacora-app/src/fonts.rs#embedded_families_resolve]
  R4:
    status: backlog
    trace:
      code:
        - crates/bitacora-app/src/views/title_bar.rs#AppTitleBar
        - crates/bitacora-app/src/views/title_bar.rs#main_window_options
        - crates/bitacora-app/src/views/workspace.rs#Workspace
      tests:
        - crates/bitacora-app/src/views/title_bar.rs#client_draws_all_supported_controls
        - crates/bitacora-app/src/views/title_bar.rs#server_decorations_and_macos_draw_no_controls
        - crates/bitacora-app/src/views/title_bar.rs#main_window_options_are_frameless_but_titled
  R5:
    status: backlog
    trace:
      code:
        - crates/bitacora-app/src/views/title_bar.rs#AppTitleBar
        - crates/bitacora-app/src/views/workspace.rs#Workspace
      tests:
        - crates/bitacora-app/src/views/title_bar.rs#title_bar_renders_with_all_slots
        - docs/design/frameless-checklist.md
  R6:
    status: backlog
---

## Purpose
Define how the v2 UI takes its visual values from the Bitacora design system and how the app draws its own window chrome.

## Scope
Tokens pipeline, fonts, theme, component kit, frameless window with app-drawn controls, accessibility constraints. Source: `/www/Bitacora/bitacora-design-system`. Plan: [[bitacora-v2-plan]].

## Requirements

### BIT-SP-0008.R1 — UI values come only from the generated token theme

The UI SHALL take every colour, type style, spacing, radius and size from the theme generated from the design system's `tokens/tokens.json` (palette, `TypeScale`, `Metrics`). Views SHALL NOT contain raw hex colours or ad-hoc pixel literals outside an explicit allowlist; a test SHALL fail when one appears.

#### Scenario: Raw colour in a view
- GIVEN a view file under `crates/bitacora-app/src/views/` contains `rgb(0x1a2b3c)`
- WHEN `cargo test -p bitacora-app` runs
- THEN the no-magic-values test fails naming the file and line

#### Scenario: Token regeneration
- GIVEN `tokens.json` changes one colour
- WHEN the token generator runs
- THEN the generated palette changes only that colour and a test comparing palette and JSON passes

### BIT-SP-0008.R2 — Text contrast meets WCAG 4.5:1 in dark and light

Every text/background token pair used for text SHALL meet a WCAG contrast ratio of at least 4.5:1 in both dark and light modes. CI SHALL run the contrast check and fail on any violation. The `ok` status colour SHALL only be used for dots and icons.

#### Scenario: Contrast regression
- GIVEN a token change lowers `muted` on `bg` to 4.2:1 in light mode
- WHEN CI runs
- THEN the contrast job fails naming the pair and mode

### BIT-SP-0008.R3 — Embedded fonts registered before first render

The app SHALL embed Atkinson Hyperlegible Next, Atkinson Hyperlegible Mono and Literata (SIL OFL 1.1, licence shipped with the app) and register them before the first window renders, so the UI does not depend on system-installed fonts. User font-family and font-size overrides SHALL keep working.

#### Scenario: Clean machine
- GIVEN none of the fonts are installed on the OS
- WHEN the app starts
- THEN the text system resolves all three families and the UI renders in Atkinson Hyperlegible Next

### BIT-SP-0008.R4 — App draws its own window chrome with window controls

The main window SHALL open with client-side decorations (no OS title bar) and an app-drawn top bar that contains the window controls: minimize, maximize/restore and close on Linux and Windows, and the native traffic lights inset into the bar on macOS. Controls SHALL honour `window.window_controls()` (e.g. hide minimize when the compositor does not support it). Close SHALL go through the existing close/quit flow (tray mode, unsaved state). When the platform forces server decorations (e.g. X11 without compositor), the app SHALL NOT draw duplicate controls.

#### Scenario: Linux Wayland GNOME
- WHEN the app starts on GNOME Wayland
- THEN there is no system title bar and the top bar shows minimize, maximize and close buttons that work

#### Scenario: Server decorations forced
- GIVEN `window.window_decorations()` returns `Decorations::Server`
- THEN the top bar shows no window control buttons

### BIT-SP-0008.R5 — Window move, maximize and resize work on every OS

Dragging the empty area of the top bar SHALL move the window; double-clicking it SHALL toggle maximize (macOS: follow the system titlebar double-click preference); the window SHALL be resizable from all edges and corners (native borders on Windows; client resize zones on Linux CSD). Interactive elements inside the bar (tabs, search field, buttons) SHALL NOT start a window drag. On Windows the drag and control areas SHALL map to native hit-test regions so snap layouts work.

#### Scenario: Drag on tab
- WHEN the user drags a tab in the top bar
- THEN the tab moves (reorder) and the window does not

#### Scenario: Resize on Linux
- GIVEN client-side decorations on Wayland
- WHEN the user drags the bottom-right corner
- THEN the window resizes and the cursor shows the diagonal resize shape

### BIT-SP-0008.R6 — Amber reserved for AI; accessible targets and focus

The `ai` (amber) token family SHALL be used only for unaccepted AI content and primary AI actions; `accent` SHALL be used for selection, links and primary non-AI actions. Interactive targets SHALL be at least 28px (30-36px in bars), every focusable control SHALL show a visible focus ring, and animations SHALL respect the OS reduce-motion preference.

#### Scenario: Keyboard focus
- WHEN the user tabs through the top bar
- THEN each control shows the focus ring in turn

#### Scenario: Reduce motion
- GIVEN the OS reduce-motion setting is on
- THEN panel and popover transitions are instant
