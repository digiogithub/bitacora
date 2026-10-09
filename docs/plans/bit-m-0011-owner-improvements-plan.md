---
created_at: 2026-10-09T07:11:16.083647886Z
updated_at: 2026-10-09T07:37:35.617593927Z
tags:
    - plan
    - app
    - print
    - i18n
    - title-bar
---
# BIT-M-0011 — 2.1 owner improvements plan

Owner request (2026-10-09): working print button, title-bar tab overflow dropdown, French and Chinese UI with system language detection. Epic BIT-EP-0027. Continues [[2-0-x-owner-feedback-plan]] and the orchestration rules in [[bitacora-v2-plan]].

## Stories
| Id | Title | Main files |
|----|-------|-----------|
| BIT-US-0181 | Print view + system print dialog | `crates/bitacora-app/src/print.rs` (pure HTML renderer), `views/workspace/print_ui.rs`, `views/workspace/top_bar.rs`, `actions.rs`, `menus.rs`, locales |
| BIT-US-0182 | Tabs collapse into a dropdown when they do not fit | `views/workspace/top_bar.rs`, `views/kit/tab.rs` |
| BIT-US-0183 | FR + ZH locales, system detection, CJK fallback | `src/i18n.rs`, `assets/locales/*.{fr,zh}.yml`, `fonts.rs`, `typos.toml` |

## Decisions
- **Printing**: GPUI exposes no print API. The app renders the active route to a self-contained HTML print view (print palette, `@page` CSS), writes it to the app cache dir (never the graph), and opens it in the default browser with `window.print()` on load, which shows the OS/browser print dialog (also "Save as PDF"). Native print dialogs per OS are out of scope.
- **Tab overflow**: pure fit function over estimated tab widths and the left-slot budget; collapsed state = one button at the first tab position (active tab title + chevron) opening a dropdown of all tabs; `+` stays.
- **Languages**: `fr` and `zh` (Simplified); Traditional Chinese locales fall back to `zh`. Own translations (ADR-015). CJK glyphs fall back to system fonts automatically (GPUI cosmic-text on Linux); no fonts bundled.

## Phases
1. Wave 1 (parallel worktrees, sonnet subagents): US-0181, US-0182.
2. Wave 2: US-0183 split into French and Chinese agents.
3. Close: workspace fmt/clippy/test, KB change records, backlog, push.

## Progress (2026-10-09)
- [x] Wave 1: US-0181 merged — [[BIT-US-0181-print-view]], [[BIT-US-0181-print-view-merge]]
- [x] Wave 1: US-0182 merged — [[BIT-US-0182-tab-overflow]], [[BIT-US-0182-tab-overflow-merge]]
- [x] Wave 2: US-0183 merged — [[BIT-US-0183-french-locale]], [[BIT-US-0183-chinese-locale]]. Coordinator added `print.*` keys to fr/zh (worktrees branched before US-0181 merged).
- [x] Close: workspace 2089 tests passed, 0 failed; clippy -D warnings and fmt clean; pushed to origin.

All three stories `in_review` awaiting owner validation in a real build.

## Owner validation
- Printer button / Ctrl+P (Cmd+P) on a page and on a single journal day: browser opens, print dialog appears, output readable (nesting, tasks, code, images).
- Many tabs or narrow window: tabs collapse into one button; dropdown activates/closes tabs. Budget is conservative (with client decorations ~2 tabs collapse at 900px) — say if too eager.
- Settings → language: Français, 中文; system locale fr_FR / zh_CN starts in that language; check label widths and CJK rendering on macOS/Windows.

## Known gaps
- Journals feed (multi-day) is not printable; live queries/embeds are skipped in print.
- Tab overflow trigger does not shrink at very narrow widths (slot clips it).
