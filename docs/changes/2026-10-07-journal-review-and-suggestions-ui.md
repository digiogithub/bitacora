---
created_at: 2026-10-07T12:00:00.000000000Z
updated_at: 2026-10-07T12:00:00.000000000Z
tags:
    - change
    - app
    - ai
---
# Journal review card and AI suggestion chips (BIT-US-0151, BIT-US-0152)

Continues [[bitacora-v2-plan]], [[ai-agents]] (sections 6 and 7 are the backend, section 8 the app side) and the stories BIT-US-0151 (task BIT-T-0462) and BIT-US-0152 (task BIT-T-0465).

## What changed

- `bitacora-config` (`src/pando.rs`): `PandoFeature::{JournalReview, Recommendations}` (on by default), `AiAuto { review_daily, review_at_minute, recommend_auto }` in `PandoSettings::ai` (off by default, old files load unchanged), `DEFAULT_REVIEW_AT_MINUTE`.
- `bitacora-app/src/views/ai_assist/`: `mod.rs` (`AiContext`, `AiGate`, pure `apply_link`, `link_markup`, `merge_tags`), `review_card.rs` (`ReviewCard`, `ReviewEvent`, `week_range`), `suggestions.rs` (`SuggestionsView`, `Item`, `perform` for accepted edits), `dismiss.rs` (`DismissStore`).
- `views/kit/card.rs`: `Card::ai()` amber treatment.
- `views/journals.rs`: `set_review_card`, the card renders above the feed. `views/right_panel.rs`: `set_suggestions`, chips in the Context tab.
- `views/palette.rs`: commands `ReviewToday`, `ReviewWeek`. `views/workspace/ai_ui.rs` (new) and hooks in `workspace.rs`: gates, context, 20 s timer for `DailySchedule` / `AutoRecommender`, event routing.
- `views/settings/pando.rs`: feature rows for review and recommendations, rows for daily review, review time and auto recommendations (`set_pando_ai_auto`); these apply live (no graph reopen). The quick-settings popover used locale keys that did not exist (`feature_semantic_search`...); it now uses the same keys as the settings page.
- Locales `ai.{en,es}.yml`, palette and settings strings. `docs/design/ai-agents.md` section 8, `docs/design/component-kit.md`.

## Why

Backend of both stories existed (`run_review`, `run_recommend`, schedule, auto mode); the user-facing review card, the chips, the entry points, the accept-as-edit path and the dismiss memory were missing.

## Verification

- Unit tests: link/tag edit helpers, gate, dismiss store (persistence per graph, bound, corrupt file), review card and chips state (gate off clears, events, dismissal persistence, accept result), settings switches apply live.
- Integration test `accepted_link_and_tag_edit_the_page_through_the_queue_and_undo`: real session, accepted link and tag edit the page, a stale link is refused, two undos restore the file bytes exactly.
- Not exercised: a real Pando run (needs a live agent); the rendered look was not inspected by eye.
