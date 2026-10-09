---
created_at: 2026-10-09T11:35:41.664588615Z
updated_at: 2026-10-09T11:35:41.664588615Z
tags:
    - change
    - fix
    - app
---
# Owner rendering fixes: AI review card scroll, nested code block (BIT-US-0184/0185)

Plan [[bit-m-0011-owner-improvements-plan]]. Details: [[BIT-US-0184-review-card-scroll]], [[BIT-US-0185-code-block-render]].

- BIT-US-0184 (`crates/bitacora-app/src/views/ai_assist/review_card.rs`, `views/dims.rs` `REVIEW_CARD_MAX_VIEWPORT_FRACTION`): card capped at 40% of window height; only the body scrolls; header and actions stay visible. Cause: whole card was a single scroll area capped at 340px.
- BIT-US-0185 (`crates/bitacora-app/src/render/model.rs` `BlockModel::from_content`, `views/block_view.rs` `code_element`/`gutter_numbers`, `crates/bitacora-markdown/tests/roundtrip_suite.rs`): a block whose first line is a fence is a code block with no title; code box gets a line-number gutter. Cause: first line always treated as title.
- Verified: workspace 2093 tests passed, 0 failed; clippy -D warnings, fmt clean. Visual check pending (both in_review).