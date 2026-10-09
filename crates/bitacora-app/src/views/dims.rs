//! Named layout literals that have no design token yet.
//!
//! Views must not contain raw `px(<number>)` literals (BIT-SP-0008.R1, enforced by
//! `views_have_no_magic_ui_values`). Sizes that exist as tokens live in
//! `cx.bitacora().metrics` / `type_scale`; the values below come from the design mockups
//! without a token. When the design adds a token, migrate the use site and drop the constant.

use crate::ui::text_edit::hsla;
use crate::ui::{Hsla, Pixels, px};

// Colour literals without a palette token (the only raw colours allowed in `views/`).

/// Black scrim behind dialogs (the design system has no scrim token).
pub const SCRIM: Hsla = hsla(0., 0., 0., 0.4);
/// Background of `==highlighted==` text.
pub const HIGHLIGHT_BG: Hsla = hsla(0.14, 0.9, 0.55, 0.35);
/// Keyword colour of code-block syntax highlighting (other classes use theme colours).
pub const SYNTAX_KEYWORD: Hsla = hsla(0.75, 0.65, 0.7, 1.);

/// Opacity of the admonition background (the palette tint tokens are accent/ai only).
pub const CALLOUT_TINT: f32 = 0.13;

/// Share of the window height the journal review card may take before its body scrolls.
pub const REVIEW_CARD_MAX_VIEWPORT_FRACTION: f32 = 0.4;

// Size literals without a metrics token.

pub const PX_NEG_1: Pixels = px(-1.0);
pub const PX_1: Pixels = px(1.0);
pub const PX_2: Pixels = px(2.0);
pub const PX_3: Pixels = px(3.0);
pub const PX_4: Pixels = px(4.0);
pub const PX_6: Pixels = px(6.0);
pub const PX_7: Pixels = px(7.0);
pub const PX_8: Pixels = px(8.0);
pub const PX_10: Pixels = px(10.0);
pub const PX_12: Pixels = px(12.0);
pub const PX_14: Pixels = px(14.0);
pub const PX_16: Pixels = px(16.0);
pub const PX_18: Pixels = px(18.0);
pub const PX_22: Pixels = px(22.0);
pub const PX_24: Pixels = px(24.0);
pub const PX_26: Pixels = px(26.0);
pub const PX_40: Pixels = px(40.0);
pub const PX_44: Pixels = px(44.0);
pub const PX_60: Pixels = px(60.0);
pub const PX_64: Pixels = px(64.0);
pub const PX_72: Pixels = px(72.0);
pub const PX_80: Pixels = px(80.0);
pub const PX_90: Pixels = px(90.0);
pub const PX_100: Pixels = px(100.0);
pub const PX_110: Pixels = px(110.0);
pub const PX_150: Pixels = px(150.0);
pub const PX_160: Pixels = px(160.0);
pub const PX_200: Pixels = px(200.0);
pub const PX_220: Pixels = px(220.0);
pub const PX_240: Pixels = px(240.0);
pub const PX_260: Pixels = px(260.0);
pub const PX_280: Pixels = px(280.0);
pub const PX_300: Pixels = px(300.0);
pub const PX_320: Pixels = px(320.0);
pub const PX_340: Pixels = px(340.0);
pub const PX_380: Pixels = px(380.0);
pub const PX_400: Pixels = px(400.0);
pub const PX_420: Pixels = px(420.0);
pub const PX_440: Pixels = px(440.0);
pub const PX_520: Pixels = px(520.0);
pub const PX_560: Pixels = px(560.0);
pub const PX_600: Pixels = px(600.0);
pub const PX_640: Pixels = px(640.0);
pub const PX_680: Pixels = px(680.0);
