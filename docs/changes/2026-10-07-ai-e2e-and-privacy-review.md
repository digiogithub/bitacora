---
created_at: 2026-10-07T16:30:00Z
updated_at: 2026-10-07T16:30:00Z
tags:
    - change
    - pando
    - ai
    - privacy
    - tests
---
# AI end-to-end tests against a real Pando and the privacy review

Continues [[bitacora-v2-plan]], [[2026-10-07-ai-agents-backend]], [[2026-10-07-ai-ghost-text-and-compose]]. Story BIT-US-0154, tasks BIT-T-0468 and BIT-T-0469 (specs BIT-SP-0011.R1-R6, BIT-SP-0009.R1). Findings and rules: [[ai-privacy-review]].

## What changed

- **Opt-in E2E tests** `crates/bitacora-runtime/tests/ai_e2e.rs` (7 `#[ignore]` tests): managed real Pando from a temp cache dir with isolated `HOME`/`XDG_CONFIG_HOME` and a generated global config (local Ollama embeddings; optional chat model through `BITACORA_E2E_CHAT_MODEL`); semantic sync + hybrid search, MCP access with the read-only `pando` token, consent revoke purge, chat run, `propose_edit` approval -> core Op -> undo, prompt injection, journal review + recommendations. LLM tests skip with a `SKIP` line without a model. `.github/workflows/ai-e2e.yml`: manual (`workflow_dispatch`) self-hosted job, not enabled by default.
- **Privacy fixes found by the review and the real-Pando run** (details in [[ai-privacy-review]] section 2):
  - `bitacora-pando/src/semantic/doc.rs`: `#tag` exclusions and `private::` on ancestors (`ContentPolicy::is_excluded_with_tags`, `is_private`); `bitacora-index/src/read/semantic.rs`: `SemanticBlock::ancestor_properties`.
  - `bitacora-mcp/src/exclusion.rs`: `FilteredReader::block_hidden` (private blocks and subtrees in every read path, raw page file withheld), `ReadExclusions::deny_all`.
  - `bitacora-runtime/src/live.rs`: `apply_pando_consent` closes the `pando` token on revocation and follows `agent_writes`; `provision_pando_mcp` revokes a stale token; `Session::agent_guard_source`; chat and applier use the live guard.
  - `bitacora-pando/src/agents/guard.rs` (`GuardSource`, `under_private_block`), `chat.rs` (`ChatDeps::live_guard`), `edits.rs` (`QueueEditApplier::with_guard`, `check_guard`); app: `views/chat/host.rs` (`PageBlocks::from_snapshot`) and `editor/view/ai.rs` (`edit_is_private`).
- **Real-Pando protocol bugs**: `chat.rs::answer_next` answered Pando's own parked tool calls (409 on resume); `runs.rs` (`fail_closed_answer`, `is_read_only_tool`) had the same bug and denied every read prompt so reviews and recommendations could not read the graph; compose uses the same helper with every prompt denied.

## Tests added

`agents_chat.rs` (server-side calls beside a prompt, live guard, injection), `agents_runs.rs` (read-tool approval), `edits.rs` (guarded applier), `guard.rs`, `doc.rs`, `read_semantic.rs`, `pando_token.rs` (private blocks), `bitacora-runtime/tests/agents.rs` (stale token, consent revoke and scopes), `exclusion.rs` (deny all).

## Verification

`cargo clippy -p bitacora-pando -p bitacora-mcp -p bitacora-index -p bitacora-runtime -p bitacora-app --all-targets --locked -- -D warnings` clean; `cargo test` of the same crates: all pass. `cargo test -p bitacora-runtime --test ai_e2e -- --ignored --test-threads=1` against `pando` v1.2.11 with local Ollama: embedding-dependent tests and the MCP test pass, LLM tests skip without `BITACORA_E2E_CHAT_MODEL`; with the local model `nova4b:latest` the chat, approval/undo and injection tests passed and review/recommendations were inconclusive (the small model produced no valid JSON in time).
