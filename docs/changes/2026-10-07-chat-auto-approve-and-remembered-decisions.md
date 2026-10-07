---
created_at: 2026-10-07T16:00:00Z
updated_at: 2026-10-07T16:00:00Z
tags:
    - change
    - pando
    - ai
---
# Chat: auto-approve read tools, remember write decisions, one data-removal button

Continues [[bitacora-v2-plan]] (owner decision 2026-10-07), story BIT-US-0166 (epic BIT-EP-0022), [[design/ai-agents]], [[design/ai-privacy-review]].

## What changed
- Chat auto-approves Pando permission prompts for the allow-listed read tools (`MCP_READ_TOOLS`, `KB_READ_TOOLS`, via `runs::is_read_only_tool`) without a card (`ChatEvent::AutoAnswered`, logged `auto-approved`).
- Write cards (`propose_edit`, other permission prompts) get "Remember my decision": `ApprovalCard::remember_tool`, `ChatHandle::decide_and_remember`, `ChatDeps::tool_memory: Option<Arc<dyn ToolMemory>>` (`agents/memory.rs`, `InMemoryToolMemory`). Remembered deny always applies; remembered allow applies only with consent and never to prompts demanding explicit approval; `propose_edit` still goes through preview/apply (ContentGuard, core queue, audit, undo). Unknown tools always ask.
- Persistence: `GraphConsent::tool_decisions` in machine-local `pando.json` (`PandoSettings::{remember,forget}_tool_decision`; `revoke_consent` clears). Runtime: `PandoOptions::settings_file`, `agents::SessionToolMemory` over the live consent record, wired in `Session::start_chat_with`.
- App: chat card toggle (`ChatView::toggle_remember`), Settings > Pando "Remembered tool decisions" list with Forget (`forget_pando_tool_decision`).
- Coordinator extra: a single data-removal button "Revoke and remove my data" (`Pending::RevokePandoConsent`, unit variant). Removed `Pending::PurgeSemantic`, `purge_semantic`, the plain revoke button and their locale keys; confirmation text states revoke, deletion from Pando KB, stop of sync/chat/agent access, clearing remembered decisions, files and local index untouched. Resync kept (disabled unless sync runs).
- Docs: `design/ai-agents.md`, `design/ai-privacy-review.md` section 5, `user/ai-features.md`, `user/pando-setup.md`.

## Verification
Tests: `bitacora-config` (decisions + revoke), `bitacora-pando` `agents_chat` (read auto-approve, write/unknown ask, remembered allow/deny, propose_edit remembered allow through queue + undo, remembered deny writes nothing), `bitacora-runtime` `SessionToolMemory`, `bitacora-app` (toggle, settings forget + revoke clears). See the commit for exact counts.
