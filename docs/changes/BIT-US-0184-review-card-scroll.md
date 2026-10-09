# BIT-US-0184: journal review card scrolls its body

Part of [[bit-m-0011-owner-improvements-plan]].

## What changed
- `crates/bitacora-app/src/views/ai_assist/review_card.rs`: `ReviewCard::render` bounds the card
  (`max_h` = window height x `dims::REVIEW_CARD_MAX_VIEWPORT_FRACTION`) and puts only the body
  (summary, chips, tasks, next actions) in a vertical scroll container (`journal-review-body`).
  The header and the footer buttons ("Revisar de nuevo" / "Cerrar") stay outside the scroll area.
  `ready()` now returns `(body, footer)`.
- `crates/bitacora-app/src/views/dims.rs`: new `REVIEW_CARD_MAX_VIEWPORT_FRACTION` (0.4).

## Why
Previously the whole card (header and buttons included) was one scroll area capped at 340 px, so a
long review scrolled its header and buttons out of view and looked clipped under the page header.
The card was already in normal flow in `JournalsView`, so placement needed no change.

## Verification
- New gpui test `a_very_long_review_is_bounded_and_keeps_its_footer_visible` (60 actions, long
  summary): card height <= fraction of viewport, footer inside card bounds, body above footer.
- fmt, clippy and `cargo test -p bitacora-app` (see the final report for counts).
