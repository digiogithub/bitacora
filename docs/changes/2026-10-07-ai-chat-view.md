---
created_at: 2026-10-07T15:00:00Z
updated_at: 2026-10-07T15:00:00Z
tags:
    - change
    - ai
    - pando
    - app
---
# AI chat view: streaming, tool cards, approvals, threads, context chips

Implements BIT-US-0147, BIT-US-0148, BIT-US-0149 (UI part) and BIT-US-0150 (tasks BIT-T-0452 remainder, 0453, 0454, 0455, 0456, 0458, 0460) of [[bitacora-v2-plan]]. Backend and design: [[ai-agents]] (section 8 describes the view).

## What changed
- `bitacora-app` `views/chat/` (new): `ChatView` (`mod.rs`: session lifecycle, event pump, cards, threads, chips, `close_session(DenyReason)`), `render.rs` (header, transcript, tool-call and approval cards, thread list, composer), `host.rs` (`AppHost` as `FrontendHost`, `AppResolver` as `PageResolver`, `PageBlocks`/`ContextItem`/`ChatContext`), `markdown.rs`, `diff.rs`, `tools.rs`, `tests.rs`. Mounted with `RightPanel::set_agent_slot` in `Workspace::new`; `Workspace` wires session handle, link and graph, closes the chat on dock close (`on_dock_event`), graph close/restart and quit (`take_session(reason, cx)` now takes a `DenyReason`), adds `ask_about_selection` and `chat_context`; palette command `AskAboutSelection`; locale keys `chat.*` (en, es); `ui::ScrollHandle` re-export; dependency `similar` on the app.
- Known gap fixed in `bitacora-pando` `agents/edits.rs`: `PageResolver` trait + `QueueEditApplier::with_resolver`. A proposal now loads the page on demand and blocks without a persisted `id::` are addressed by the index uuid (copy of the snapshot, in memory only, matched by position and identical text). No `id::` is ever written. `Session::start_chat_with(config, host, resolver)` in `bitacora-runtime` (`start_chat` delegates).
- `bitacora-pando` `agents/chat.rs`: `ChatEvent::State` and `ChatEvent::Activity`, `AgentState`/`SubAgent`, tool-call `started`/`duration`, `ChatModel::{state, activity}`. `ContentGuard::page_blocks` (tags and privacy from page properties). `pando-rs`: `agui::apply_patch` re-export. `bitacora-runtime`: `ai` re-export module so the app does not depend on `bitacora-pando` or `pando-rs`.

## Verification
`cargo clippy -p bitacora-pando -p pando-rs -p bitacora-runtime -p bitacora-app --all-targets --locked -- -D warnings` clean; `cargo test -p bitacora-pando -p pando-rs -p bitacora-runtime -p bitacora-app --locked` all green (app lib 523 tests including the new chat tests: resolver against a real indexed graph, host channel and guard, scripted event rendering, deny-on-close for each reason, chips, unavailable session). Not verified by eye: pixel look against a live Pando (no server in CI), IME in the composer.
