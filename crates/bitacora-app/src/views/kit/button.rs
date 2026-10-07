//! `Button` (primary / secondary / ghost / AI) and `IconButton` (`docs/componentes.md`,
//! "Botones").
//!
//! Both are focusable (tab stop, accent focus ring only for keyboard focus) and activate on
//! Enter / Space through GPUI's keyboard click. A disabled button paints at half strength,
//! drops hover and the pointer cursor and never calls its handler.

use std::rc::Rc;

use super::icons::{Glyph, glyph};
use super::keyed_focus;
use crate::ui::text_edit::FontWeight;
use crate::ui::theme::{ActiveBitacoraTheme as _, BitacoraTheme, TypeStyleExt as _};
use crate::ui::{
    App, ClickEvent, ElementId, FluentBuilder as _, Hsla, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, RenderOnce, SharedString, StatefulInteractiveElement as _,
    Styled as _, Window, div, h_flex,
};

type ClickHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// Visual weight of a [`Button`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonVariant {
    /// Accent fill, the one main action of a surface.
    Primary,
    /// Amber AI fill (actions inside AI boxes and the agent panel).
    Ai,
    /// Transparent with a `line_2` border.
    Secondary,
    /// No border, muted text.
    Ghost,
}

/// Height class of a [`Button`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ButtonSize {
    /// 36px, `radius_control`.
    #[default]
    Regular,
    /// 32px, used inside AI boxes and compact panels.
    Compact,
}

/// Resolved look of a button (what the tests assert on).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ButtonSpec {
    pub height: Pixels,
    pub radius: Pixels,
    pub bg: Option<Hsla>,
    pub fg: Hsla,
    pub border: Option<Hsla>,
    pub hover_bg: Option<Hsla>,
    pub weight: FontWeight,
    pub focus_ring: Hsla,
}

/// The look of a button variant and size for `theme`.
#[must_use]
pub fn button_spec(
    variant: ButtonVariant,
    size: ButtonSize,
    disabled: bool,
    theme: &BitacoraTheme,
) -> ButtonSpec {
    let c = &theme.colors;
    let m = &theme.metrics;
    let (height, radius) = match size {
        ButtonSize::Regular => (m.icon_button, m.radius_control),
        // 32px with a 7px radius: one pixel tighter than the regular control.
        ButtonSize::Compact => (
            m.icon_button - m.space[2],
            m.radius_control - m.space[1] / 2.0,
        ),
    };
    let (bg, fg, border, hover_bg, weight) = match variant {
        ButtonVariant::Primary => (
            Some(c.accent),
            c.on_accent,
            None,
            Some(c.accent.opacity(0.9)),
            FontWeight(600.0),
        ),
        ButtonVariant::Ai => (
            Some(c.ai),
            c.on_ai,
            None,
            Some(c.ai.opacity(0.9)),
            FontWeight(600.0),
        ),
        ButtonVariant::Secondary => (
            None,
            c.text,
            Some(c.line_2),
            Some(c.hover),
            theme.type_scale.ui_small.weight,
        ),
        ButtonVariant::Ghost => (
            None,
            c.muted,
            None,
            Some(c.hover),
            theme.type_scale.ui_small.weight,
        ),
    };
    let fade = |h: Hsla| if disabled { h.opacity(0.5) } else { h };
    ButtonSpec {
        height,
        radius,
        bg: bg.map(fade),
        fg: fade(fg),
        border: border.map(fade),
        hover_bg: if disabled { None } else { hover_bg },
        weight,
        focus_ring: c.accent,
    }
}

/// A text button, optionally with a leading icon.
///
/// `Button::new(id).label(text).primary().on_click(|_, _, cx| ...)`
#[derive(IntoElement)]
pub struct Button {
    id: ElementId,
    label: Option<SharedString>,
    icon: Option<Glyph>,
    variant: ButtonVariant,
    size: ButtonSize,
    disabled: bool,
    on_click: Option<ClickHandler>,
}

impl Button {
    /// A secondary, regular-size button.
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            label: None,
            icon: None,
            variant: ButtonVariant::Secondary,
            size: ButtonSize::Regular,
            disabled: false,
            on_click: None,
        }
    }

    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Leading icon.
    pub fn icon(mut self, icon: Glyph) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }

    pub fn primary(self) -> Self {
        self.variant(ButtonVariant::Primary)
    }

    pub fn ai(self) -> Self {
        self.variant(ButtonVariant::Ai)
    }

    pub fn secondary(self) -> Self {
        self.variant(ButtonVariant::Secondary)
    }

    pub fn ghost(self) -> Self {
        self.variant(ButtonVariant::Ghost)
    }

    pub fn size(mut self, size: ButtonSize) -> Self {
        self.size = size;
        self
    }

    /// The 32px height used in AI boxes.
    pub fn compact(self) -> Self {
        self.size(ButtonSize::Compact)
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn on_click(mut self, f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(f));
        self
    }
}

impl RenderOnce for Button {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.bitacora().clone();
        let spec = button_spec(self.variant, self.size, self.disabled, &theme);
        let focus = keyed_focus(&self.id, window, cx);
        let pad = theme.metrics.space[6];
        let gap = theme.metrics.space[3];
        let icon_size = theme.metrics.icon_sm;
        let handler = self.on_click.filter(|_| !self.disabled);
        let ring = spec.focus_ring;
        let disabled = self.disabled;

        let selector = self.id.to_string();
        h_flex()
            .id(self.id)
            .debug_selector(|| selector)
            .track_focus(&focus)
            .tab_stop(!disabled)
            .h(spec.height)
            .px(pad)
            .gap(gap)
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .rounded(spec.radius)
            .border_1()
            .border_color(spec.border.unwrap_or_else(Hsla::transparent_black))
            .when_some(spec.bg, |d, bg| d.bg(bg))
            .text_color(spec.fg)
            .type_style(&theme.type_scale.ui_small)
            .font_weight(spec.weight)
            .when(!disabled, |d| d.cursor_pointer())
            .when_some(spec.hover_bg, |d, bg| d.hover(move |s| s.bg(bg)))
            .focus_visible(move |s| s.border_color(ring))
            .when_some(handler, |d, h| {
                d.on_click(move |ev, window, cx| h(ev, window, cx))
            })
            .when_some(self.icon, |d, g| d.child(glyph(g, icon_size, spec.fg, cx)))
            .when_some(self.label, |d, l| d.child(div().child(l)))
    }
}

/// Resolved look of an [`IconButton`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IconButtonSpec {
    pub side: Pixels,
    pub icon: Pixels,
    pub radius: Pixels,
    pub fg: Hsla,
    pub bg: Option<Hsla>,
    pub hover_bg: Option<Hsla>,
}

/// The look of an icon button for `theme`. `small` is the 34px target with the 15px icon.
#[must_use]
pub fn icon_button_spec(
    small: bool,
    active: bool,
    disabled: bool,
    theme: &BitacoraTheme,
) -> IconButtonSpec {
    let c = &theme.colors;
    let m = &theme.metrics;
    let fade = |h: Hsla| if disabled { h.opacity(0.5) } else { h };
    IconButtonSpec {
        side: if small {
            m.icon_button_sm
        } else {
            m.icon_button
        },
        icon: if small { m.icon_sm } else { m.icon },
        radius: m.radius_control,
        fg: fade(if active { c.text } else { c.muted }),
        bg: active.then_some(c.hover),
        hover_bg: (!disabled).then_some(c.hover),
    }
}

/// A square, transparent icon-only button.
#[derive(IntoElement)]
pub struct IconButton {
    id: ElementId,
    glyph: Glyph,
    small: bool,
    active: bool,
    disabled: bool,
    on_click: Option<ClickHandler>,
}

impl IconButton {
    /// A regular (36px, 18px icon) icon button.
    pub fn new(id: impl Into<ElementId>, glyph: Glyph) -> Self {
        Self {
            id: id.into(),
            glyph,
            small: false,
            active: false,
            disabled: false,
            on_click: None,
        }
    }

    /// 34px target with a 15px icon.
    pub fn small(mut self) -> Self {
        self.small = true;
        self
    }

    /// Toggled-on look (e.g. a panel that is open).
    pub fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn on_click(mut self, f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(f));
        self
    }
}

impl RenderOnce for IconButton {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.bitacora().clone();
        let spec = icon_button_spec(self.small, self.active, self.disabled, &theme);
        let focus = keyed_focus(&self.id, window, cx);
        let handler = self.on_click.filter(|_| !self.disabled);
        let ring = theme.colors.accent;
        let disabled = self.disabled;

        let selector = self.id.to_string();
        div()
            .id(self.id)
            .debug_selector(|| selector)
            .track_focus(&focus)
            .tab_stop(!disabled)
            .flex()
            .items_center()
            .justify_center()
            .flex_shrink_0()
            .size(spec.side)
            .rounded(spec.radius)
            .border_1()
            .border_color(Hsla::transparent_black())
            .when_some(spec.bg, |d, bg| d.bg(bg))
            .when(!disabled, |d| d.cursor_pointer())
            .when_some(spec.hover_bg, |d, bg| d.hover(move |s| s.bg(bg)))
            .focus_visible(move |s| s.border_color(ring))
            .when_some(handler, |d, h| {
                d.on_click(move |ev, window, cx| h(ev, window, cx))
            })
            .child(glyph(self.glyph, spec.icon, spec.fg, cx))
    }
}

opaque_debug!(Button, IconButton);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::theme::Mode;

    #[test]
    fn variants_use_the_design_tokens() {
        for mode in [Mode::Light, Mode::Dark] {
            let theme = BitacoraTheme::new(mode);
            let c = &theme.colors;
            let spec = |v| button_spec(v, ButtonSize::Regular, false, &theme);
            let p = spec(ButtonVariant::Primary);
            assert_eq!((p.bg, p.fg, p.border), (Some(c.accent), c.on_accent, None));
            let a = spec(ButtonVariant::Ai);
            assert_eq!((a.bg, a.fg, a.border), (Some(c.ai), c.on_ai, None));
            let s = spec(ButtonVariant::Secondary);
            assert_eq!((s.bg, s.fg, s.border), (None, c.text, Some(c.line_2)));
            let g = spec(ButtonVariant::Ghost);
            assert_eq!((g.bg, g.fg, g.border), (None, c.muted, None));
            assert_eq!(p.focus_ring, c.accent);
        }
    }

    #[test]
    fn sizes_are_36_and_32_pixels() {
        let theme = BitacoraTheme::new(Mode::Light);
        let h = |s| button_spec(ButtonVariant::Primary, s, false, &theme).height;
        assert_eq!(h(ButtonSize::Regular), crate::ui::px(36.0));
        assert_eq!(h(ButtonSize::Compact), crate::ui::px(32.0));
        let small = icon_button_spec(true, false, false, &theme);
        let regular = icon_button_spec(false, false, false, &theme);
        assert_eq!(
            (regular.side, regular.icon),
            (crate::ui::px(36.0), crate::ui::px(18.0))
        );
        assert_eq!(
            (small.side, small.icon),
            (crate::ui::px(34.0), crate::ui::px(15.0))
        );
    }

    #[test]
    fn disabled_buttons_fade_and_lose_hover() {
        let theme = BitacoraTheme::new(Mode::Dark);
        let on = button_spec(ButtonVariant::Primary, ButtonSize::Regular, false, &theme);
        let off = button_spec(ButtonVariant::Primary, ButtonSize::Regular, true, &theme);
        assert_eq!(off.hover_bg, None);
        assert!(on.hover_bg.is_some());
        assert!(off.fg.a < on.fg.a);
        assert!(off.bg.is_some_and(|b| b.a < theme.colors.accent.a));
        let icon = icon_button_spec(false, false, true, &theme);
        assert_eq!(icon.hover_bg, None);
        assert!(icon.fg.a < theme.colors.muted.a);
    }

    #[test]
    fn an_active_icon_button_is_filled_and_brighter() {
        let theme = BitacoraTheme::new(Mode::Light);
        let off = icon_button_spec(false, false, false, &theme);
        let on = icon_button_spec(false, true, false, &theme);
        assert_eq!((off.bg, off.fg), (None, theme.colors.muted));
        assert_eq!(
            (on.bg, on.fg),
            (Some(theme.colors.hover), theme.colors.text)
        );
    }
}
