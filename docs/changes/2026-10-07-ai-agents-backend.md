---
created_at: 2026-10-07T14:00:00Z
updated_at: 2026-10-07T14:00:00Z
tags:
    - change
    - pando
    - ai
---
# AI agents backend: chat session, propose_edit, approvals, journal review, recommender

Continues [[bitacora-v2-plan]] (D5). Tasks BIT-T-0452 (backend part), BIT-T-0457, BIT-T-0459, BIT-T-0461, BIT-T-0463, BIT-T-0464 of spec BIT-SP-0011. Design: [[ai-agents]].

## What changed
- `bitacora-pando::agents` (new, `crates/bitacora-pando/src/agents/`): `chat` (`ChatSession`, `ChatHandle`, `ChatEvent`, `ChatModel`), `tools` (`tool_set`, `FrontendHost`, `NoHost`), `edits` (`Proposal`, `validate`, `EditApplier`, `QueueEditApplier`, `AuditSink`), `approvals` (`PendingApprovals`, `Decision`, `DenyReason`, `Clock`), `guard` (`ContentGuard`, `AttachedBlock`), `lookup` (`BlockLookup` over `IndexReader`), `runs` (`run_once`, `extract_json`), `review` (`run_review`, `parse_review`, `cross_check`), `cache` (`ReviewCache`, `DailySchedule`, `WallClock`), `recommend` (`run_recommend`, `parse_suggestions`, `locate_span`, `AutoRecommender`).
- `bitacora-core`: `Source::Agent` (audit attribution; conflicted pages are refused for it like for MCP).
- `bitacora-mcp`: `AuditLog::record_agent_write` + `AgentWrite` (audit and undo of approved edits made outside MCP tool calls), `bridge::fingerprint_of`.
- `bitacora-runtime`: `Session::{agent_guard, agui_client, agent_edit_applier, review_deps, recommend_deps, start_chat}`, `RuntimeError::Agent`, glue in `src/agents.rs`.
- `pando-rs`: `Thread::set_context` (ambient context for following runs).
- Dependencies: `bitacora-pando` gains `bitacora-core`, `bitacora-markdown`, `jiff` (all workspace members/pins); `bitacora-runtime` gains `pando-rs`.

## Why
Spec BIT-SP-0011 R1-R5 need a tested backend before the UI: agent writes only after explicit approval, as audited undoable Op transactions; unanswered approvals deny; content sent to agents obeys consent and exclusions; reviews stay out of the graph; structured outputs are validated against the index.

## Verification
- `crates/bitacora-pando/tests/agents_chat.rs` (axum SSE mock): streaming/message model, attached context guard, approved edit applied + audited + undone, denied, timeout, close for every reason (no resume, cancel POST), dropped handle, stale/invalid proposals without card, permission prompts, frontend tools and guarded selection, cancel (`POST /runs/{id}/cancel`), run error.
- `crates/bitacora-pando/tests/agents_runs.rs`: valid review parsed, unknown tasks dropped, invalid output error, no consent sends nothing, cache hit/miss/force/content change, fail-closed permission in one-shot runs, timeout + cancel, recommender span validation.
- Unit tests in each module (approvals triggers, proposal validation/ops/rollback, JSON extraction, schedule with fake clock, debounce).
- `crates/bitacora-runtime/tests/agents.rs`: edit audited in the MCP log and undone with `undo_agent_entry`; agents unavailable without connection/consent.
- `cargo fmt`, `cargo clippy -p pando-rs -p bitacora-core -p bitacora-mcp -p bitacora-pando -p bitacora-runtime --all-targets --locked -- -D warnings`, `cargo test` for the same crates.
