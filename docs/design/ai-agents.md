---
created_at: 2026-10-07T14:00:00Z
updated_at: 2026-10-07T14:00:00Z
tags:
    - design
    - pando
    - ai
---
# AI agents backend

GPUI-free backend of the AI features (spec BIT-SP-0011, milestone BIT-M-0008): chat, approved edits, journal review and recommendations over Pando's AG-UI API. Plan: [[bitacora-v2-plan]] (D5). Integration overview: [[pando-integration]]. The UI (chat panel, cards, review and recommendation views) consumes the channels and traits described here and lives in `bitacora-app`.

Code: `crates/bitacora-pando/src/agents/` (module map in `mod.rs`), thin accessors on `bitacora_runtime::Session` (`crates/bitacora-runtime/src/live.rs`, glue in `agents.rs`).

## 1. Profiles

The profiles come from the managed `.pando.toml` (`managed/config.rs::PROFILES`); the agents module only names them: `bitacora-chat` (read-only, `CHAT_PROFILE`), `bitacora-writer` (the only one that may call `propose_edit`, `WRITER_PROFILE`), `bitacora-journal-reviewer`, `bitacora-recommender`. Agents read the graph through Bitacora's MCP server (Read token, exclusions applied server-side); prompts of review and recommendation runs carry no graph content.

## 2. Chat session (BIT-T-0452)

`ChatSession::spawn(&tokio::runtime::Handle, ChatDeps) -> (ChatHandle, std::sync::mpsc::Receiver<ChatEvent>)`. One task owns a `pando_rs::agui::Thread` (declared tools, transcript resent every turn).

- `ChatHandle` (cloneable, usable from any thread): `send(text, attached_blocks)`, `approve/deny/decide(call_id, ..)`, `cancel()` (`POST /runs/{thread}/cancel`, the session stays usable), `close(reason)`. Dropping every handle closes the session.
- `ChatEvent`: `History`, `RunStarted`, `MessageStart/TextDelta/MessageEnd`, `ReasoningDelta`, `ToolCallStart/Args/End/Result`, `Custom`, `ApprovalRequested(ApprovalCard)`, `ApprovalResolved`, `EditRejected/EditApplied/EditFailed`, `Error{fatal}`, `RunFinished(Finished|Failed|Cancelled)`, `Closed(reason)`.
- `ChatModel::apply(&ChatEvent)` folds events into the transcript a view renders: messages (text, reasoning, tool calls with args and result, `cancelled` flag), approval cards with their state. An event touches only its own message, so the view re-renders one message per delta. The GPUI `Chat` entity (BIT-T-0452 remaining part) wraps a `ChatModel` and polls the receiver.
- An interrupted run (`RUN_FINISHED{outcome:"interrupt"}`) is answered one pending call at a time and resumed. Frontend tools `open_page` and `get_selection` run on `FrontendHost` (trait the app implements; `NoHost` refuses); Pando permission prompts and `AskUserQuestion` become cards; `propose_edit` becomes an edit card (section 3).
- `ChatConfig { profile, propose_edit, approval_timeout, resume_thread }`; `ChatConfig::writer()` enables `propose_edit`. `Session::start_chat(config, host)` wires the AG-UI client, guard and applier; it fails with `RuntimeError::Agent` unless Pando is connected, the chat feature is on and the graph consented.
- Attached context: `send(text, blocks)` passes the blocks through `ContentGuard::context_entries` into `RunAgentInput.context`; exactly the allowed blocks, nothing else from the graph (R1). `pando_rs::agui::Thread::set_context` was added for this (generic SDK change).

## 3. Frontend tools and `propose_edit` (BIT-T-0457)

`tools::tool_set(with_propose_edit)` declares the tools with JSON schemas (`propose_edit_schema()`). A proposal (`edits::Proposal`) targets one page and carries at most 50 ops: `insert_block`, `update_block` (with `expected_text`), `move_block`, `delete_block`, `set_property`. Blocks are addressed by persisted `id::` uuid.

- `Proposal::parse` validates shape (sizes, one-block text, no `id::`/`collapsed::`, property key syntax). `validate(&Proposal, &PageSnapshot)` checks against the live page: unknown block, `expected_text` no longer current (`EditError::Stale`), same block deleted and edited, move into own subtree. The result carries a `Preview` (title, per-op before/after) for the card.
- `QueueEditApplier` (`EditApplier`) re-validates against the current page and only then commits through the core command queue with `Source::Agent` (new `Source` variant; MCP-style conflicted-page refusal applies to it too). `update_block` is an optimistic `Op::SetText { before, after }`; the others are `Cmd`s. The group is all or nothing (inverse ops roll it back) and each transaction lands in the normal undo history, so Ctrl+Z undoes it step by step.
- Audit and undo: `AuditSink` records the applied edit. In the runtime it is the MCP server's audit log (`AuditLog::record_agent_write`, tool `propose_edit`, client `bitacora-chat`), so it shows in `Session::agent_activity` and undoes as one step with `Session::undo_agent_entry` (refused with `Changed` when the page moved on).
- Limitation: the page must be loaded in the writer (`EditError::PageNotLoaded`, code `page_not_loaded`); the app opens the page the user is looking at, other pages are answered to the agent as an error. A block without a persisted `id::` cannot be addressed.

## 4. Approvals fail closed (BIT-T-0459)

`approvals::PendingApprovals` is the book of cards. Only an explicit `Decision::Approve` for a pending id of kind permission/edit approves; a mismatched decision, a repeat or an unknown id changes nothing, and every resolution is final. Everything else resolves as denied (a question as cancelled), with a `DenyReason`: `Timeout` (default 120 s, `Clock` injectable), `PanelClosed` (also: handle dropped or event receiver dropped), `ThreadSwitch`, `GraphSwitch`, `Quit`, `Cancelled`, `ServerExpired` (Pando gave up first: a failing resume denies what is left). The app calls `ChatHandle::close(reason)` for panel close, thread switch, graph switch and quit. A close or cancel wins over anything already resolved: nothing is delivered after it, the run is cancelled on the server (which also releases a parked prompt) and no edit is applied. The agent is told the outcome: `{"approved":false,"applied":false,"reason":"timeout"}`.

## 5. Consent and exclusions

`ContentGuard` (`guard.rs`) is the gate for attached context and `get_selection` results: nothing passes without `GraphConsent::granted`; pages matching the consent exclusions (names, namespace children, path prefixes, `#tags`) and `private:: true` blocks or pages are dropped, using the same `ContentPolicy` as the semantic indexer; blocks are capped (50 blocks, 8000 chars). `Session::agent_guard()` follows the semantic worker's live exclusions. Review and recommendation runs refuse to start without consent and drop results that name hidden pages.

## 6. Journal review (BIT-T-0461, BIT-T-0463)

`review::run_review(&ReviewDeps, ReviewRange, force)` runs `bitacora-journal-reviewer` as a one-shot (`runs::run_once`: fresh thread, every interrupt resolved fail-closed, budget 5 min then cancel). The answer must be one JSON object; `parse_review` is tolerant (prose and fences around it, string-or-object items, camelCase aliases) but a missing `summary` is `AgentError::InvalidOutput`. Schema: `summary`, `themes[]`, `mood`, `pending_tasks[block_uuid]`, `next_actions[]`. `cross_check` drops tasks that are unknown to the index, no longer open (`TODO/DOING/NOW/LATER/WAITING`) or hidden by the guard; task text comes from the index, never from the agent. Nothing is written to the graph.

Cache (`cache::ReviewCache`): machine-local JSON file `agent-reviews.json` in the index data dir (`Session::review_deps` passes `index.location().dir()`), atomic write, 200 entries. Key = graph id + day range + hash of the allowed journal blocks (uuid, marker, text) in the range, so an edit makes the cached review stale and an untouched journal is served with no run (`ReviewReport::from_cache`). `DailySchedule::due(clock, gate)` decides, with a `WallClock` (fake in tests), when the app should review today on its own: enabled (off by default), local time past `at_minute`, once per day, only when Pando is connected, the graph consented and no run is in flight; `retry()` re-arms the day after a failure.

## 7. Recommender (BIT-T-0464)

`recommend::run_recommend(&RecommendDeps, &RecommendRequest { page })` runs `bitacora-recommender` the same way. Schema: `related_pages`, `link_suggestions { block_uuid, text, target }`, `tag_suggestions`, `next_actions`. Validation (`parse_suggestions`): link suggestions must name an existing block of the requested page that the guard allows, quote text that is really in the block now (on word boundaries, outside `[[links]]`, code spans, URLs, `](..)` and property lines; stated `start/end` must match) and target a page that is not excluded; the result carries the byte range to replace. Invalid items are dropped and counted (`dropped_links`, `dropped_other`). The page itself must not be excluded. `AutoRecommender` is the optional auto mode: off by default, `touch(page, now)` restarts a debounce, `poll(now, gate)` returns the page to recommend for once the quiet period passed, no run is in flight, the minimum gap since the last start elapsed and Pando is connected with consent. Dismissal memory ("not re-offered for the same content") belongs to the UI story.

## Requirements

- MUST NOT apply an agent edit except through `QueueEditApplier` after an explicit approval of the exact card.
- MUST deny every card that is not answered (timeout, cancel, close, drop) and never deliver an answer after a close.
- MUST send graph content to an agent only through `ContentGuard` (consent, exclusions, privacy).
- MUST keep reviews outside the graph (machine-local cache).
- SHOULD keep the app thin: it implements `FrontendHost`, renders `ChatModel` and calls the runtime accessors.

## Open questions

- Pages that are not loaded in the writer cannot receive `propose_edit`; whether the app should load the page on demand (through `Session::open_page`) when a proposal names it.
- Pando's own approval timeout is not known to the client; the default card timeout (120 s) is a guess below Pando's. A resume that fails because Pando already denied is reported as `Error` and the remaining cards are denied (`ServerExpired`).
- The Chat entity, cards, review and recommendation views, the settings switches for the schedule and auto mode, and dismissal memory are UI stories (BIT-US-0147, 0149, 0151, 0152).
