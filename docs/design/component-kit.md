# Design system component kit

Reusable UI building blocks for the v2 screens (BIT-US-0117, plan [[bitacora-v2-plan]]). Source of
truth for looks is the design system (`bitacora-design-system/docs/componentes.md`); source of
values is the `BitacoraTheme` global (`cx.bitacora()`: `colors`, `type_scale`, `metrics`, see
`ui/theme/`). Code lives in `crates/bitacora-app/src/views/kit/`.

## Rules

- Import from `crate::views::kit`; never hardcode a colour or a token-sized dimension in a view.
  The few design-doc literals that have no token (chip padding 7px, kbd padding 6px, tab `-1px`
  overlap) are named constants in `inline.rs` / `tab.rs`.
- Every component is a `RenderOnce` builder with `Into<ElementId>` ids where it is interactive.
  Interactive components are focusable (tab stop, accent border only for keyboard focus,
  Enter/Space activate through GPUI's keyboard click) and expose
  `.debug_selector(id)` so tests can find their bounds.
- Looks that can be decided without a window are pure functions returning a spec
  (`button_spec`, `icon_button_spec`, `marker_spec`, `chip_colors`, `pill_colors`, `tab_colors`,
  `segment_colors`, `fade_duration`); unit tests assert on them for both modes.
- GPUI/GPUI Kit types come from the `crate::ui` facade. This story added `ui::assets::icon_svg`
  and re-exports `Animation`, `AnimationExt`, `Div`, `ElementId`, `KeyUpEvent`, `Keystroke`.
- Keyboard click needs the element to know it is focused, which happens on the next paint.

## Components

| Component | Builder | Notes |
|---|---|---|
| `Button` | `Button::new(id).label("Save").icon(Glyph::Plus).primary()` / `.ai()` / `.secondary()` (default) / `.ghost()`, `.compact()`, `.disabled(b)`, `.on_click(f)` | 36px (`.compact()` 32px, radius one pixel tighter). Primary `accent`/`on_accent`, AI `ai`/`on_ai`, secondary `line_2` border, ghost `muted`. Disabled: half strength, no hover, handler never runs. |
| `IconButton` | `IconButton::new(id, Glyph::Search).small().active(b).disabled(b).on_click(f)` | 36px with 18px icon; `.small()` 34px with 15px icon. `active` fills with `hover` and brightens the icon. |
| `Glyph`, `glyph(g, size, colour, cx)` | `Glyph::ALL` | Lucide subset from the kit's embedded catalogue repainted at `metrics.icon_stroke` (1.7), plus the custom filled four-point `Glyph::Sparkle` for AI. `glyph()` returns a kit `Icon` (a `Styled` element). |
| `Kbd` | `Kbd::new("Tab")` | Mono 11.5, `line_2` border, `text_2`. |
| `Chip` | `Chip::new("#tag").tone(ChipTone::Accent).icon(g).mono(true)` | Tones: `Accent`, `Neutral`, `Ai`, `Outline`, `Dashed` ("+ context"). |
| `TaskMarker` | `TaskMarker::new(Marker::Doing)` | `bitacora_markdown::tasks::head::Marker`; all 11 Logseq markers. `marker_spec(m, &theme)` gives `MarkerSpec` (`look`, colours, `strike`, `mutes_block()`); the outline block renderer should mute the whole block when `mutes_block()`. |
| `Pill` | `Pill::new(id, "All").count(12).active(b).on_click(f)` | 34px, `radius_pill`; active inverts (`text` fill, `bg` text). |
| `Overline` | `Overline::new("Favorites").count(3).warn(true)` | Uppercase `overline` style, optional mono count. No letter spacing (GPUI divs have none). |
| `Card` | `Card::new().surface(Surface::Panel).radius(r).padding(v, h).child(..)` | `raised` (default) or `panel` fill, 1px `line`, `radius_card`, 10 x 12 padding. |
| `Tab` | `Tab::new(id, "Title").icon(g).active(b).on_click(f).on_close(f)` | 38px, truncated title, close "x" that does not activate the tab. Active tab: `bg` fill, `line` border on three sides, weight 600, overlaps the bar border by 1px (design doc; the backlog text mentions an underline, the design doc wins). |
| `Segmented` | `Segmented::new(id).option(key, label).selected(key).on_change(\|key, w, cx\| ..)` | `hover` track, selected segment `raised` with `line` border. Not specified in the design doc: derived from the tokens. |
| `PopoverShell` | `PopoverShell::new(id).width(px(280.)).on_dismiss(\|w, cx\| ..).child(..)` | `raised`, 1px `line`, `radius_popover`, `shadow_lg` (the only shadowed surface). Takes focus when rendered; `on_dismiss` fires on Escape inside it and on a mouse press outside. The owner removes it. Position it with `deferred(anchored()..)`. Fades in for 120ms unless `cx.reduce_motion()`. |
| `Gallery` | `cx.new(\|_\| Gallery::new())` | One page with every component and variant; the view-test fixture. Not wired into the app chrome. |

## Tests

`views/kit/tests.rs` drives the gallery in a `#[gpui::test]` window in both modes: token heights
(36/32/34/38), enabled vs disabled click, Enter activation, pill/tab/segment selection, popover
Escape and outside press. Pure spec tests live next to each component.

## Open questions

- Tooltips for `IconButton` (needs a kit tooltip view); screens can attach one with
  `.tooltip(..)` on a wrapping element until a kit helper exists.
- Segmented has no mockup; revisit when a screen story needs it.
