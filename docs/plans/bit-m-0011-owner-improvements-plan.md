---
created_at: 2026-10-09T07:10:56.457326732Z
updated_at: 2026-10-09T07:11:07.774442487Z
tags:
    - plan
    - app
    - print
    - i18n
    - title-bar
---
# BIT-M-0011 — 2.1 owner improvements plan

Mirror of docs/plans/bit-m-0011-owner-improvements-plan.md. Owner request (2026-10-09): working print button (BIT-US-0181), title-bar tab overflow dropdown (BIT-US-0182), French + Simplified Chinese UI with system detection (BIT-US-0183). Epic BIT-EP-0027. Continues [[2-0-x-owner-feedback-plan]] and [[bitacora-v2-plan]].

Decisions: printing via self-contained HTML print view in the app cache dir opened in the default browser with window.print() (GPUI has no print API; native dialogs out of scope). Tab overflow via a pure fit function; collapsed = single button at first tab position with dropdown. fr + zh (Traditional falls back to zh), own translations (ADR-015).

Phases: wave 1 parallel (US-0181, US-0182) in worktrees with sonnet subagents; wave 2 US-0183 on merged main; close: fmt/clippy/tests, KB change records, push.