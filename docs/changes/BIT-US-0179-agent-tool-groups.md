# BIT-US-0179: group tool calls in the agent transcript

Continues [[2-0-x-owner-feedback-plan]].

## What
Each run of consecutive tool calls (plus their resolved approval cards) of an assistant turn is one collapsed row ("> 3 tool calls"), with a running dot while any call runs and a warn tint plus "N failed" if any failed. Clicking expands to the existing individual cards (own expand kept). Approval cards still `Pending` stay visible outside the group. Assistant text is untouched; the reasoning row is already a single collapsed row and was left as is.

## Where
- `crates/bitacora-app/src/views/chat/tools.rs`: `ToolSegment`, `group_tool_calls`, `group_counts` (pure, unit-tested).
- `crates/bitacora-app/src/views/chat/render.rs`: `message_el` uses segments; new `tool_group`. Expand key `group:<first call id>` in `ChatView::expanded`.
- Locales `en.yml` / `es.yml`: `chat.tool_group`, `chat.tool_group_failed`.

## Verification
`cargo test -p bitacora-app --lib chat` (21 pass, incl. `tool_calls_group_around_pending_approvals`), `cargo clippy -p bitacora-app --all-targets --locked -- -D warnings` clean. Visuals not checked.
