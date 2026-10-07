---
created_at: 2026-10-07T12:30:00Z
updated_at: 2026-10-07T12:30:00Z
tags:
    - change
    - pando
    - app
---
# Pando connection status, degradation and activity log

Implements BIT-US-0140 (BIT-T-0435, BIT-T-0436) and BIT-T-0429 of [[bitacora-v2-plan]]; builds on [[changes/2026-10-07-pando-settings-panel-and-consent.md]] and [[pando-integration]].

## What changed
- `bitacora-pando`: `PandoStatus::{Unauthorized, TooOld}` (+ `word()`), health probe backs off while failing (`service::probe_delay`), minimum-version check on every probe; new `activity` module (`ActivityLog`, `ActivityEntry`, `ActivityKind`: bounded JSONL, newest 500, ids capped at 20); `EventSink::{set_log, record}` and `PandoEvent::Activity`; `PandoOptions::activity`; the semantic worker logs each acknowledged batch (`SemanticWorker::batch_done`); `ChatDeps::activity` makes chat sessions log runs, approvals and applied edits.
- `bitacora-runtime`: re-exports `ActivityLog` types; `pando_options_from_file` sets the log next to `pando.json`; `Session::open_chat` passes the sink to chat sessions.
- `bitacora-cli`: `semantic` command describes the two new statuses.
- `bitacora-app`: `views::pando_status` (`PandoState::derive`, labels, degradation text), `views::pando_activity` (`PandoActivityView`, `merge_rows`), `views::workspace::pando_ui` (status poll every 3 s, `Workspace::pando_control`, popover, `open_pando_activity`), sidebar footer row (`LeftSidebar::set_pando_state`, `SidebarEvent::OpenPando`), top-bar "Pando" control next to the sparkle (which still opens the Agent tab), settings chip handles the new statuses, locales `pando_status.*`.

## Why
Users must always see whether Pando is available, what stops working when it is not (search falls back to keywords; editing, sync and MCP are unaffected) and what was sent to it, without anything leaving the machine.

## Verification
`cargo test -p bitacora-pando -p bitacora-runtime -p bitacora-app -p bitacora-cli --locked`, clippy `-D warnings` for the same crates (see the commit for counts). Unit tests: state derivation, probe backoff, log bounds and clearing, row merge; `#[gpui::test]`: popover opens from the top-bar control and the sidebar, degraded text, Escape closes it, activity view filter and clear.
