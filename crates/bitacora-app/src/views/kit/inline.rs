//! Small inline components (`docs/componentes.md`): [`Kbd`], [`Chip`], [`TaskMarker`], [`Pill`]
//! and [`Overline`].

use std::rc::Rc;

use bitacora_markdown::tasks::head::Marker;

use super::icons::{Glyph, glyph};
use super::keyed_focus;
use crate::ui::text_edit::FontWeight;
use crate::ui::theme::{ActiveBitacoraTheme as _, BitacoraTheme, TypeStyle, TypeStyleExt as _};
use crate::ui::{
    App, ClickEvent, ElementId, FluentBuilder as _, Hsla, InteractiveElement as _, IntoElement,
    ParentElement as _, RenderOnce, SharedString, StatefulInteractiveElement as _, Styled as _,
    Window, div, h_flex, px,
};

/// Horizontal padding of a task / tag chip (`componentes.md`: "padding 1 x 7").
const CHIP_PAD_X: f32 = 7.0;
/// Horizontal padding of a `kbd` (`componentes.md`: "0 x 5-6").
const KBD_PAD_X: f32 = 6.0;

/// The mono style at the chip / kbd size (11.5, the size of the overline scale step).
fn small_mono(theme: &BitacoraTheme) -> TypeStyle {
    TypeStyle {
        size: theme.type_scale.overline.size,
        ..theme.type_scale.mono.clone()
    }
}

/// A keyboard hint: `Kbd::new("Tab")`.
#[derive(IntoElement)]
pub struct Kbd {
    keys: SharedString,
}

impl Kbd {
    pub fn new(keys: impl Into<SharedString>) -> Self {
        Self { keys: keys.into() }
    }
}

impl RenderOnce for Kbd {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.bitacora();
        div()
            .flex_shrink_0()
            .px(px(KBD_PAD_X))
            .rounded(theme.metrics.radius_chip)
            .border_1()
            .border_color(theme.colors.line_2)
            .text_color(theme.colors.text_2)
            .type_style(&small_mono(theme))
            .child(self.keys)
    }
}

/// Colour family of a [`Chip`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChipTone {
    /// `accent` on `accent_bg`: tags, context chips.
    Accent,
    /// `text_2` on `hover`: neutral context chips.
    Neutral,
    /// Amber on `ai_bg` with an `ai_line` border.
    Ai,
    /// `text_2` with a `line_2` border and no fill.
    Outline,
    /// `muted` with a dashed `line_2` border: the "+ context" affordance.
    Dashed,
}

/// Resolved colours of a chip: `(fill, text, border, dashed)`.
#[must_use]
pub fn chip_colors(
    tone: ChipTone,
    theme: &BitacoraTheme,
) -> (Option<Hsla>, Hsla, Option<Hsla>, bool) {
    let c = &theme.colors;
    match tone {
        ChipTone::Accent => (Some(c.accent_bg), c.accent, None, false),
        ChipTone::Neutral => (Some(c.hover), c.text_2, None, false),
        ChipTone::Ai => (Some(c.ai_bg), c.ai, Some(c.ai_line), false),
        ChipTone::Outline => (None, c.text_2, Some(c.line_2), false),
        ChipTone::Dashed => (None, c.muted, Some(c.line_2), true),
    }
}

/// A compact label pill-shaped as a chip: `Chip::new("#project").tone(ChipTone::Accent)`.
#[derive(IntoElement)]
pub struct Chip {
    label: SharedString,
    tone: ChipTone,
    icon: Option<Glyph>,
    mono: bool,
}

impl Chip {
    pub fn new(label: impl Into<SharedString>) -> Self {
        Self {
            label: label.into(),
            tone: ChipTone::Neutral,
            icon: None,
            mono: false,
        }
    }

    pub fn tone(mut self, tone: ChipTone) -> Self {
        self.tone = tone;
        self
    }

    /// Leading icon at the small icon size.
    pub fn icon(mut self, icon: Glyph) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Set the label in the mono face (dates, models, servers).
    pub fn mono(mut self, mono: bool) -> Self {
        self.mono = mono;
        self
    }
}

impl RenderOnce for Chip {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.bitacora().clone();
        let (bg, fg, border, dashed) = chip_colors(self.tone, &theme);
        let style = if self.mono {
            small_mono(&theme)
        } else {
            theme.type_scale.caption.clone()
        };
        h_flex()
            .flex_shrink_0()
            .items_center()
            .gap(theme.metrics.space[2])
            .px(px(CHIP_PAD_X))
            .rounded(theme.metrics.radius_chip)
            .border_1()
            .border_color(border.unwrap_or_else(Hsla::transparent_black))
            .when(dashed, |d| d.border_dashed())
            .when_some(bg, |d, bg| d.bg(bg))
            .text_color(fg)
            .type_style(&style)
            .when_some(self.icon, |d, g| {
                d.child(glyph(g, theme.metrics.icon_sm, fg, cx))
            })
            .child(self.label)
    }
}

/// How a task marker is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkerLook {
    /// Filled accent chip (`DOING`, `NOW`, `STARTED`, `IN-PROGRESS`).
    Active,
    /// Chip with an inner `line_2` border (`TODO`).
    Outline,
    /// `hover` chip with muted text (`LATER`, `WAITING`, `WAIT`).
    Subtle,
    /// No chip, struck-through muted text; the whole block turns muted
    /// (`DONE`, `CANCELED`, `CANCELLED`).
    Done,
}

/// Resolved look of a task marker.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MarkerSpec {
    /// The marker word as written in the file.
    pub label: &'static str,
    pub look: MarkerLook,
    pub bg: Option<Hsla>,
    pub fg: Hsla,
    pub border: Option<Hsla>,
    pub strike: bool,
}

impl MarkerSpec {
    /// Whether the rest of the block is drawn muted (finished tasks).
    #[must_use]
    pub fn mutes_block(&self) -> bool {
        self.look == MarkerLook::Done
    }
}

/// The look of `marker` for `theme`. Every Logseq marker is mapped.
#[must_use]
pub fn marker_spec(marker: Marker, theme: &BitacoraTheme) -> MarkerSpec {
    let c = &theme.colors;
    let look = match marker {
        Marker::Doing | Marker::Now | Marker::Started | Marker::InProgress => MarkerLook::Active,
        Marker::Todo => MarkerLook::Outline,
        Marker::Later | Marker::Waiting | Marker::Wait => MarkerLook::Subtle,
        Marker::Done | Marker::Canceled | Marker::Cancelled => MarkerLook::Done,
    };
    let (bg, fg, border) = match look {
        MarkerLook::Active => (Some(c.accent_bg), c.accent, None),
        MarkerLook::Outline => (None, c.text_2, Some(c.line_2)),
        MarkerLook::Subtle => (Some(c.hover), c.muted, None),
        MarkerLook::Done => (None, c.muted, None),
    };
    MarkerSpec {
        label: marker.as_str(),
        look,
        bg,
        fg,
        border,
        strike: look == MarkerLook::Done,
    }
}

/// The marker word of a task block: `TaskMarker::new(Marker::Doing)`.
#[derive(IntoElement)]
pub struct TaskMarker {
    marker: Marker,
}

impl TaskMarker {
    pub fn new(marker: Marker) -> Self {
        Self { marker }
    }
}

impl RenderOnce for TaskMarker {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.bitacora().clone();
        let spec = marker_spec(self.marker, &theme);
        let style = TypeStyle {
            weight: FontWeight(500.0),
            ..small_mono(&theme)
        };
        let chip = spec.look != MarkerLook::Done;
        div()
            .flex_shrink_0()
            .when(chip, |d| {
                d.px(px(CHIP_PAD_X))
                    .rounded(theme.metrics.radius_chip)
                    .border_1()
                    .border_color(spec.border.unwrap_or_else(Hsla::transparent_black))
            })
            .when_some(spec.bg, |d, bg| d.bg(bg))
            .text_color(spec.fg)
            .type_style(&style)
            .when(spec.strike, |d| d.line_through())
            .child(spec.label)
    }
}

/// Resolved colours of a filter pill: `(fill, text, border, count text)`.
#[must_use]
pub fn pill_colors(active: bool, theme: &BitacoraTheme) -> (Option<Hsla>, Hsla, Hsla, Hsla) {
    let c = &theme.colors;
    if active {
        // Inversion: the page background on the text colour.
        (Some(c.text), c.bg, c.text, c.bg)
    } else {
        (None, c.text_2, c.line_2, c.muted)
    }
}

type ClickHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// A 34px filter pill with an optional count: `Pill::new("all", "All").count(12).active(true)`.
#[derive(IntoElement)]
pub struct Pill {
    id: ElementId,
    label: SharedString,
    count: Option<usize>,
    active: bool,
    on_click: Option<ClickHandler>,
}

impl Pill {
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            count: None,
            active: false,
            on_click: None,
        }
    }

    /// Count shown after the label, in mono.
    pub fn count(mut self, count: usize) -> Self {
        self.count = Some(count);
        self
    }

    pub fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }

    pub fn on_click(mut self, f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(f));
        self
    }
}

impl RenderOnce for Pill {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.bitacora().clone();
        let (bg, fg, border, count_fg) = pill_colors(self.active, &theme);
        let focus = keyed_focus(&self.id, window, cx);
        let ring = theme.colors.accent;
        let selector = self.id.to_string();
        h_flex()
            .id(self.id)
            .debug_selector(|| selector)
            .track_focus(&focus)
            .tab_stop(true)
            .h(theme.metrics.icon_button_sm)
            .px(theme.metrics.space[7])
            .gap(theme.metrics.space[3])
            .flex_shrink_0()
            .items_center()
            .rounded(theme.metrics.radius_pill)
            .border_1()
            .border_color(border)
            .when_some(bg, |d, bg| d.bg(bg))
            .text_color(fg)
            .type_style(&theme.type_scale.ui_small)
            .cursor_pointer()
            .focus_visible(move |s| s.border_color(ring))
            .when_some(self.on_click, |d, h| {
                d.on_click(move |ev, window, cx| h(ev, window, cx))
            })
            .child(self.label)
            .when_some(self.count, |d, n| {
                d.child(
                    div()
                        .text_color(count_fg)
                        .type_style(&small_mono(&theme))
                        .child(n.to_string()),
                )
            })
    }
}

/// An uppercase section label: `Overline::new("Favorites").count(3)`.
#[derive(IntoElement)]
pub struct Overline {
    label: SharedString,
    count: Option<usize>,
    warn: bool,
}

impl Overline {
    pub fn new(label: impl Into<SharedString>) -> Self {
        Self {
            label: label.into(),
            count: None,
            warn: false,
        }
    }

    /// Count in mono after the label (task groups).
    pub fn count(mut self, count: usize) -> Self {
        self.count = Some(count);
        self
    }

    /// `warn` colour (overdue group header).
    pub fn warn(mut self, warn: bool) -> Self {
        self.warn = warn;
        self
    }
}

impl RenderOnce for Overline {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.bitacora().clone();
        let fg = if self.warn {
            theme.colors.warn
        } else {
            theme.colors.muted
        };
        let style = &theme.type_scale.overline;
        let label: SharedString = if style.uppercase {
            self.label.to_uppercase().into()
        } else {
            self.label
        };
        h_flex()
            .items_baseline()
            .gap(theme.metrics.space[3])
            .text_color(fg)
            .type_style(style)
            .child(label)
            .when_some(self.count, |d, n| {
                d.child(
                    div()
                        .type_style(&theme.type_scale.mono)
                        .child(n.to_string()),
                )
            })
    }
}

opaque_debug!(Kbd, Chip, TaskMarker, Pill, Overline);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::theme::Mode;

    #[test]
    fn markers_map_every_logseq_task_marker() {
        let theme = BitacoraTheme::new(Mode::Light);
        let c = &theme.colors;
        for m in Marker::ALL {
            let spec = marker_spec(m, &theme);
            assert_eq!(spec.label, m.as_str());
        }
        assert_eq!(marker_spec(Marker::Doing, &theme).bg, Some(c.accent_bg));
        assert_eq!(marker_spec(Marker::Now, &theme).fg, c.accent);
        assert_eq!(marker_spec(Marker::Todo, &theme).border, Some(c.line_2));
        assert_eq!(marker_spec(Marker::Todo, &theme).bg, None);
        assert_eq!(marker_spec(Marker::Later, &theme).bg, Some(c.hover));
        let done = marker_spec(Marker::Done, &theme);
        assert!(done.strike && done.mutes_block());
        assert_eq!((done.bg, done.fg), (None, c.muted));
        for m in [Marker::Canceled, Marker::Cancelled] {
            assert!(marker_spec(m, &theme).strike);
        }
        for m in [Marker::Started, Marker::InProgress] {
            assert_eq!(marker_spec(m, &theme).look, MarkerLook::Active);
        }
        for m in [Marker::Waiting, Marker::Wait] {
            assert_eq!(marker_spec(m, &theme).look, MarkerLook::Subtle);
        }
    }

    #[test]
    fn active_pill_inverts_text_and_background() {
        let theme = BitacoraTheme::new(Mode::Dark);
        let c = &theme.colors;
        let (bg, fg, border, count) = pill_colors(true, &theme);
        assert_eq!((bg, fg, border, count), (Some(c.text), c.bg, c.text, c.bg));
        let (bg, fg, border, _) = pill_colors(false, &theme);
        assert_eq!((bg, fg, border), (None, c.text_2, c.line_2));
    }

    #[test]
    fn chip_tones_use_the_design_tokens() {
        let theme = BitacoraTheme::new(Mode::Light);
        let c = &theme.colors;
        assert_eq!(
            chip_colors(ChipTone::Accent, &theme),
            (Some(c.accent_bg), c.accent, None, false)
        );
        assert_eq!(
            chip_colors(ChipTone::Ai, &theme),
            (Some(c.ai_bg), c.ai, Some(c.ai_line), false)
        );
        assert!(chip_colors(ChipTone::Dashed, &theme).3);
    }
}
