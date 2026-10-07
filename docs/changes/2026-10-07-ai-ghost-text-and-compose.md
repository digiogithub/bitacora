---
created_at: 2026-10-07T16:00:00Z
updated_at: 2026-10-07T16:00:00Z
---
# Inline AI ghost text and Compose with AI box

Implements BIT-US-0153 (tasks BIT-T-0466, BIT-T-0467), spec BIT-SP-0011.R6. Continues [[bitacora-v2-plan]] (D5) and [[ai-agents]].

## What changed
- `crates/bitacora-pando/src/agents/compose.rs` (new): `run_compose`, `ComposeRequest/Mode`, `ComposeDeps`, `PageLookup` (impl for `IndexReader`), `check_allowed`, `prompt_for`, `clean_answer`. Streaming one-shot of the existing `bitacora-writer` profile without frontend tools; the guard refuses (no request) for no consent, excluded or private pages/blocks; dropping the future cancels the run. `runs::final_text` made `pub(super)`.
- `crates/bitacora-runtime`: `Session::compose_deps()`; re-exports `ComposeDeps`, `ComposeMode`, `ComposeRequest`, `run_agent_compose`, `AgentError`.
- `crates/bitacora-app`: `editor/ai.rs` (decisions, `AiBackend` global, `SessionAiBackend`), `editor/view/ai.rs` (ghost + compose state on `OutlineEditor`), `editor/ai_view.rs` (hint bar, compose box with `PopoverShell`), hooks in `editor/view.rs` (`edit_buffer`, `motion`, `exit_edit`, key context `GhostText`, `attach`), `editor/element.rs` + `editor/style.rs` (ghost shaped after the text in amber), actions `AiCompose/AcceptGhost/DismissGhost/DismissCompose` and keymap (`secondary-j`, `GhostText`, `ComposeBox` sections), `settings.rs` (`AiAssistSettings`, both off by default), Settings, Editor switches, locales, workspace installs/removes the backend on graph open/close.
- Docs: `docs/design/ai-agents.md` section 8, IME checklist cases C21-C23.

## Why
The design system's inline AI and "Compose with AI" surfaces. Accepted text must be a normal undoable edit and must never be sent for excluded or private content or interrupt IME composition.

## Verification
- `cargo test -p bitacora-pando -p bitacora-runtime -p bitacora-app --locked`: pando 71 lib + 4 `agents_compose` (mock AG-UI server: streaming, cleaning, nothing sent for excluded/private/unconsented), app 539 (12 new `editor::tests::ai_tests` plus unit tests: debounce, cancel on keystroke, IME guard, Tab/Esc, accept and Insert/Replace are one undo step, discard leaves the block, off by default).
- `cargo clippy -p bitacora-pando -p bitacora-runtime -p bitacora-app --all-targets --locked -- -D warnings` clean; `cargo fmt --all --check` clean.
- Manual IME checks (C21-C23) and a run against a real Pando are still pending.
