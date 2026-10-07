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
- On-demand pages and blocks without `id::` (BIT-US-0149): `QueueEditApplier::with_resolver(Arc<dyn PageResolver>)`. When the page is not loaded in the writer, `PageResolver::ensure_loaded` loads it (the app's `AppResolver` only loads pages the index knows, it never creates one, so `PageNotLoaded` remains for unknown pages). Blocks that have no persisted `id::` are addressed by the uuid the index (and so MCP reads) gave them: `with_index_uuids` copies the snapshot and gives a block that uuid **in memory only** when it sits at the same position and its text is identical to the index row (a lagging index never aliases another block). Nothing is ever written into the file for it (rule 1); a stale or unmatched block stays `UnknownBlock`. `Session::start_chat_with(config, host, resolver)` passes the resolver to the applier.

## 4. Approvals fail closed (BIT-T-0459)

`approvals::PendingApprovals` is the book of cards. Only an explicit `Decision::Approve` for a pending id of kind permission/edit approves; a mismatched decision, a repeat or an unknown id changes nothing, and every resolution is final. Everything else resolves as denied (a question as cancelled), with a `DenyReason`: `Timeout` (default 120 s, `Clock` injectable), `PanelClosed` (also: handle dropped or event receiver dropped), `ThreadSwitch`, `GraphSwitch`, `Quit`, `Cancelled`, `ServerExpired` (Pando gave up first: a failing resume denies what is left). The app calls `ChatHandle::close(reason)` for panel close, thread switch, graph switch and quit. A close or cancel wins over anything already resolved: nothing is delivered after it, the run is cancelled on the server (which also releases a parked prompt) and no edit is applied. The agent is told the outcome: `{"approved":false,"applied":false,"reason":"timeout"}`.

## 5. Consent and exclusions

`ContentGuard` (`guard.rs`) is the gate for attached context and `get_selection` results: nothing passes without `GraphConsent::granted`; pages matching the consent exclusions (names, namespace children, path prefixes, `#tags`) and `private:: true` blocks or pages are dropped, using the same `ContentPolicy` as the semantic indexer; blocks are capped (50 blocks, 8000 chars). `Session::agent_guard()` follows the semantic worker's live exclusions. Review and recommendation runs refuse to start without consent and drop results that name hidden pages.

## 6. Journal review (BIT-T-0461, BIT-T-0463)

`review::run_review(&ReviewDeps, ReviewRange, force)` runs `bitacora-journal-reviewer` as a one-shot (`runs::run_once`: fresh thread, every interrupt resolved fail-closed, budget 5 min then cancel). The answer must be one JSON object; `parse_review` is tolerant (prose and fences around it, string-or-object items, camelCase aliases) but a missing `summary` is `AgentError::InvalidOutput`. Schema: `summary`, `themes[]`, `mood`, `pending_tasks[block_uuid]`, `next_actions[]`. `cross_check` drops tasks that are unknown to the index, no longer open (`TODO/DOING/NOW/LATER/WAITING`) or hidden by the guard; task text comes from the index, never from the agent. Nothing is written to the graph.

Cache (`cache::ReviewCache`): machine-local JSON file `agent-reviews.json` in the index data dir (`Session::review_deps` passes `index.location().dir()`), atomic write, 200 entries. Key = graph id + day range + hash of the allowed journal blocks (uuid, marker, text) in the range, so an edit makes the cached review stale and an untouched journal is served with no run (`ReviewReport::from_cache`). `DailySchedule::due(clock, gate)` decides, with a `WallClock` (fake in tests), when the app should review today on its own: enabled (off by default), local time past `at_minute`, once per day, only when Pando is connected, the graph consented and no run is in flight; `retry()` re-arms the day after a failure.

## 7. Recommender (BIT-T-0464)

`recommend::run_recommend(&RecommendDeps, &RecommendRequest { page })` runs `bitacora-recommender` the same way. Schema: `related_pages`, `link_suggestions { block_uuid, text, target }`, `tag_suggestions`, `next_actions`. Validation (`parse_suggestions`): link suggestions must name an existing block of the requested page that the guard allows, quote text that is really in the block now (on word boundaries, outside `[[links]]`, code spans, URLs, `](..)` and property lines; stated `start/end` must match) and target a page that is not excluded; the result carries the byte range to replace. Invalid items are dropped and counted (`dropped_links`, `dropped_other`). The page itself must not be excluded. `AutoRecommender` is the optional auto mode: off by default, `touch(page, now)` restarts a debounce, `poll(now, gate)` returns the page to recommend for once the quiet period passed, no run is in flight, the minimum gap since the last start elapsed and Pando is connected with consent. Dismissal memory ("not re-offered for the same content") belongs to the UI story.

## 8. Chat view (BIT-US-0147..0150)

Code: `crates/bitacora-app/src/views/chat/` (`mod.rs` state and behaviour, `render.rs` elements, `host.rs` seams to the backend, `markdown.rs`, `diff.rs`, `tools.rs` pure helpers). The workspace owns the `ChatView` and mounts it with `RightPanel::set_agent_slot`.

- **Run loop**: `ChatView::start` calls `Session::start_chat_with` on the session thread (`SessionHandle::run`), keeps the `ChatHandle`, and moves events out of the blocking std channel with a forwarding thread into an `async_channel`; one foreground task folds batches of events into the `ChatModel`, so a burst of deltas costs one render and the UI thread never waits on the network. Every session has a generation: events and start results of an older one are ignored (a late start result closes its handle).
- **Answers**: `markdown.rs` parses the subset (headings, paragraphs, bullets, quotes, fenced code; an unclosed fence streams as code). Inline text goes through the outline's inline renderer, so `[[Page]]` and `((block))` are links (`ChatViewEvent::Navigate`), resolved through the index. Layouts are cached per message and recomputed only when its text changed.
- **Tool calls and header**: tool calls are collapsible mono cards (status, name, argument summary, duration, JSON input and output); reasoning is collapsed by default; the header shows model, token usage and running sub-agents from `STATE_SNAPSHOT/DELTA`, and the latest `ACTIVITY_SNAPSHOT` as an amber progress line.
- **Approvals**: `propose_edit`, permission prompts and questions render as inline cards (`ai_bg`/`ai_line`). An edit card shows a line diff per op (`diff.rs`, added lines in `ai`, removed in `warn`); Apply and Deny are channel sends, the backend applies through the core queue (undoable, audited). After approval the card shows the `EditApplied` note (Ctrl+Z undoes it). A card that is not answered is denied by the backend after its timeout.
- **Closing**: `ChatView::close_session(reason)` is called with `PanelClosed` when the right dock closes (workspace `on_dock_event`), `ThreadSwitch` for new/resume/profile switch, `GraphSwitch` when the graph closes or restarts and `Quit` on app quit (`Workspace::take_session`); pending cards are marked denied at once and the thread id is kept, so the next message resumes it.
- **Profiles**: read-only `bitacora-chat` by default; the composer toggle "Can propose edits" switches to `bitacora-writer` (restarts the session on the same thread).
- **Threads**: header "Threads" lists `AguiClient::list_threads` (only the chat and writer profiles), titled with the first user message of the 12 newest; click resumes (`resume_thread` -> history), delete calls `delete_thread`. Network calls run on the tokio bridge.
- **Context (BIT-US-0150)**: "+ Page" and "+ Selection" chips attach the page on screen (a journal page counts as "journal day") or the selected blocks; the chip is exactly what is sent and is consumed by the message. `attached_blocks` builds `AttachedBlock`s with the live `ContentGuard` (tags and privacy from the page properties) and `ChatHandle::send` filters them again; nothing else from the graph is attached. The palette command "Ask Pando about the selection" attaches the selection (or the page) and focuses the composer. There is no block context menu in the app yet, so the command is the entry point.
- **Frontend tools**: `AppHost` answers `open_page` (only pages the index knows) and `get_selection` (asks the UI thread through a channel, 3 s timeout, result filtered by the guard).

## 8. Review card and suggestion chips (BIT-US-0151, BIT-US-0152)

`bitacora-app/src/views/ai_assist/` is the UI of sections 6 and 7; it reaches the backend only through `bitacora_runtime::ai` and `Session::{review_deps, recommend_deps, agent_edit_applier}` (called on the session thread through `SessionHandle::run`). Runs are awaited on the tokio bridge, never on the UI thread; the consent guard, the exclusions and the machine-local review cache are the backend's.

- **Gate** (`AiGate`): a surface shows when its feature (`PandoFeature::JournalReview` / `Recommendations`, on by default once Pando is active) is on and the graph consented; runs start only when Pando is connected. Turning the feature off clears the card or the chips at once.
- **Review card** (`ReviewCard`, above the journals feed, `Card::ai()` amber treatment): entry points "Review today" / "Review this week" (also palette commands `ReviewToday` / `ReviewWeek`, which switch to the journals and start a run), summary, mood, themes, still-open tasks (click opens the block) and next actions, a "from the saved review" note for cache hits and "Review again" (`force`). Nothing is written to the graph.
- **Suggestion chips** (`SuggestionsView`, Context tab under the related blocks): related pages (click opens), missing links, tags and next actions for the page on screen. Results belong to one page; another page drops them.
- **Accept** = an ordinary undoable edit through the command queue. A link goes through `QueueEditApplier` as one `UpdateBlock` (`id::`/`collapsed::` stripped from the proposal, restored by the applier; stale text is refused and audited like chat edits): the quoted span becomes `[[Target]]`, or `[text]([[Target]])` when the text differs from the page title. A tag runs `Cmd::SetPageProperty { key: "tags" }` (source `Ui`) with the merged value; pages whose properties are YAML front matter are not edited automatically. A page or block that changed since the suggestion refuses the edit (`ai.suggest.stale`).
- **Dismiss memory** (`DismissStore`, `ai-dismissed.json` in the app state directory, per graph, 1000 entries): a suggestion is keyed by a hash of kind, page and its own content, so the same suggestion is not offered again while another one for the page still is. Only hashes are stored.
- **Automation** (Settings > Pando, `PandoSettings::ai`, all off by default): `review_daily` + `review_at_minute` drive `DailySchedule::due`, `recommend_auto` drives `AutoRecommender::touch/poll/finished`; one 20 s workspace timer (`workspace/ai_ui.rs`) checks both. A failed scheduled review is retried after 10 minutes. These switches and the two feature switches apply without reopening the graph.

## Requirements

- MUST NOT apply an agent edit except through `QueueEditApplier` after an explicit approval of the exact card.
- MUST deny every card that is not answered (timeout, cancel, close, drop) and never deliver an answer after a close.
- MUST send graph content to an agent only through `ContentGuard` (consent, exclusions, privacy).
- MUST keep reviews outside the graph (machine-local cache).
- SHOULD keep the app thin: it implements `FrontendHost`, renders `ChatModel` and calls the runtime accessors.

## Open questions

- Pando's own approval timeout is not known to the client; the default card timeout (120 s) is a guess below Pando's. A resume that fails because Pando already denied is reported as `Error` and the remaining cards are denied (`ServerExpired`).
- The app side of review and recommendations is described in section 8.
- `ChatEvent::State` carries the whole shared-state document; `AgentState::from_value` reads only `model`, `tokenUsage` and `subAgents` leniently. Todos and touched files of that document are not shown yet.
