---
created_at: 2026-10-09T07:11:07.785127169Z
updated_at: 2026-10-09T07:11:16.083016239Z
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
| BIT-US-0181 | Print view + system print dialog | new `crates/bitacora-app/src/print.rs` (pure HTML renderer), `views/workspace/top_bar.rs`, actions/keymap, locales |
| BIT-US-0182 | Tabs collapse into a dropdown when they do not fit | `views/workspace/top_bar.rs`, locales |
| BIT-US-0183 | FR + ZH locales, system detection, CJK fallback | `src/i18n.rs`, `assets/locales/*.{fr,zh}.yml`, fonts, `typos.toml` |

## Decisions
- **Printing**: GPUI exposes no print API. The app renders the active route to a self-contained HTML print view (print palette, `@page` CSS), writes it to the app cache dir (never the graph), and opens it in the default browser with `window.print()` on load, which shows the OS/browser print dialog (also "Save as PDF"). Native print dialogs per OS are out of scope.
- **Tab overflow**: pure fit function over estimated tab widths and the left-slot budget; collapsed state = one button at the first tab position (active tab title + chevron) opening a dropdown of all tabs; `+` stays.
- **Languages**: `fr` and `zh` (Simplified); Traditional Chinese locales fall back to `zh`. Own translations (ADR-015).

## Phases
1. Wave 1 (parallel worktrees, sonnet subagents): US-0181, US-0182. Coordinator merges to main, resolves `top_bar.rs`/`en.yml`/`es.yml` conflicts, removes worktrees.
2. Wave 2: US-0183 on top of merged main so fr/zh cover the new keys.
3. Close: workspace fmt/clippy/test, KB change records, stories done/in_review (visual/OS checks), push to origin.

## Progress
- [ ] Wave 1: US-0181
- [ ] Wave 1: US-0182
- [ ] Wave 2: US-0183
- [ ] Close and push
