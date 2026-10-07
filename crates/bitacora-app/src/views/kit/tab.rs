//! `Tab` (title-bar page tab) and `Segmented` (mutually exclusive options).

use std::rc::Rc;

use super::icons::{Glyph, glyph};
use super::keyed_focus;
use crate::ui::text_edit::FontWeight;
use crate::ui::theme::{ActiveBitacoraTheme as _, BitacoraTheme, TypeStyleExt as _};
use crate::ui::{
    App, ClickEvent, ElementId, FluentBuilder as _, Hsla, InteractiveElement as _, IntoElement,
    ParentElement as _, RenderOnce, SharedString, StatefulInteractiveElement as _, Styled as _,
    Window, div, h_flex, px,
};

type ClickHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;
type ChangeHandler = Rc<dyn Fn(&SharedString, &mut Window, &mut App)>;

/// Resolved colours of a tab: `(fill, text, border, weight)`.
#[must_use]
pub fn tab_colors(
    active: bool,
    theme: &BitacoraTheme,
) -> (Option<Hsla>, Hsla, Option<Hsla>, FontWeight) {
    let c = &theme.colors;
    if active {
        // Same fill as the page below, so the tab opens into it.
        (Some(c.bg), c.text, Some(c.line), FontWeight(600.0))
    } else {
        (None, c.muted, None, theme.type_scale.ui_small.weight)
    }
}

/// A page tab: page icon, truncated title and a close button.
///
/// The active tab has the page background, a `line` border on three sides and overlaps the bar's
/// bottom border by one pixel; inactive tabs are transparent.
#[derive(IntoElement)]
pub struct Tab {
    id: ElementId,
    title: SharedString,
    icon: Glyph,
    active: bool,
    on_click: Option<ClickHandler>,
    on_close: Option<ClickHandler>,
}

impl Tab {
    pub fn new(id: impl Into<ElementId>, title: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            icon: Glyph::File,
            active: false,
            on_click: None,
            on_close: None,
        }
    }

    pub fn icon(mut self, icon: Glyph) -> Self {
        self.icon = icon;
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

    /// Shows the "x" button and calls `f` when it is pressed (the tab itself is not activated).
    pub fn on_close(mut self, f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_close = Some(Rc::new(f));
        self
    }
}

impl RenderOnce for Tab {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.bitacora().clone();
        let (bg, fg, border, weight) = tab_colors(self.active, &theme);
        let focus = keyed_focus(&self.id, window, cx);
        let close_id = ElementId::from((self.id.clone(), "close"));
        let ring = theme.colors.accent;
        let m = &theme.metrics;
        let transparent = Hsla::transparent_black();

        let selector = self.id.to_string();
        h_flex()
            .id(self.id)
            .debug_selector(|| selector)
            .track_focus(&focus)
            .tab_stop(true)
            .h(m.tab_height)
            .max_w(m.tab_max)
            .px(m.space[6])
            .gap(m.space[4])
            .flex_shrink_0()
            .items_center()
            .rounded_t(m.radius_tab)
            .border_1()
            .border_b_0()
            .border_color(border.unwrap_or(transparent))
            .when_some(bg, |d, bg| d.bg(bg))
            .when(self.active, |d| d.mb(px(-1.0)))
            .text_color(fg)
            .type_style(&theme.type_scale.ui_small)
            .font_weight(weight)
            .cursor_pointer()
            .focus_visible(move |s| s.border_color(ring))
            .when_some(self.on_click, |d, h| {
                d.on_click(move |ev, window, cx| h(ev, window, cx))
            })
            .child(glyph(self.icon, m.icon_sm - px(1.0), fg, cx))
            .child(div().min_w_0().flex_1().truncate().child(self.title))
            .when_some(self.on_close, |d, h| {
                d.child(
                    div()
                        .id(close_id)
                        .flex()
                        .items_center()
                        .justify_center()
                        .flex_shrink_0()
                        .text_color(theme.colors.muted)
                        .hover(|s| s.text_color(theme.colors.text))
                        .on_click(move |ev, window, cx| {
                            cx.stop_propagation();
                            h(ev, window, cx);
                        })
                        .child(glyph(
                            Glyph::Close,
                            m.icon_sm - px(2.0),
                            theme.colors.muted,
                            cx,
                        )),
                )
            })
    }
}

/// Resolved colours of one segment: `(fill, text, border)`.
#[must_use]
pub fn segment_colors(selected: bool, theme: &BitacoraTheme) -> (Option<Hsla>, Hsla, Option<Hsla>) {
    let c = &theme.colors;
    if selected {
        (Some(c.raised), c.text, Some(c.line))
    } else {
        (None, c.muted, None)
    }
}

/// A row of mutually exclusive options in a `hover` track.
///
/// `Segmented::new("view").option("list", "List").option("grid", "Grid").selected("list")
/// .on_change(|key, window, cx| ...)`
#[derive(IntoElement)]
pub struct Segmented {
    id: ElementId,
    options: Vec<(SharedString, SharedString)>,
    selected: Option<SharedString>,
    on_change: Option<ChangeHandler>,
}

impl Segmented {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            options: Vec::new(),
            selected: None,
            on_change: None,
        }
    }

    /// Adds an option with a stable `key` and a visible `label`.
    pub fn option(mut self, key: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        self.options.push((key.into(), label.into()));
        self
    }

    /// The key of the selected option.
    pub fn selected(mut self, key: impl Into<SharedString>) -> Self {
        self.selected = Some(key.into());
        self
    }

    /// Called with the key of the pressed option (also when it is already selected).
    pub fn on_change(mut self, f: impl Fn(&SharedString, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }
}

impl RenderOnce for Segmented {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.bitacora().clone();
        let m = &theme.metrics;
        let transparent = Hsla::transparent_black();
        let ring = theme.colors.accent;
        let mut track = h_flex()
            .id(self.id.clone())
            .flex_shrink_0()
            .p(m.space[1])
            .gap(m.space[1])
            .bg(theme.colors.hover)
            .rounded(m.radius_control + m.space[1]);
        for (key, label) in self.options {
            let selected = self.selected.as_ref() == Some(&key);
            let (bg, fg, border) = segment_colors(selected, &theme);
            let seg_id = ElementId::from((self.id.clone(), key.clone()));
            let focus = keyed_focus(&seg_id, window, cx);
            let on_change = self.on_change.clone();
            let selector = seg_id.to_string();
            track = track.child(
                h_flex()
                    .id(seg_id)
                    .debug_selector(|| selector)
                    .track_focus(&focus)
                    .tab_stop(true)
                    .h(m.icon_button_sm - m.space[3] - m.space[2])
                    .px(m.space[6])
                    .items_center()
                    .justify_center()
                    .rounded(m.radius_control)
                    .border_1()
                    .border_color(border.unwrap_or(transparent))
                    .when_some(bg, |d, bg| d.bg(bg))
                    .text_color(fg)
                    .type_style(&theme.type_scale.ui_small)
                    .when(selected, |d| d.font_weight(FontWeight(600.0)))
                    .cursor_pointer()
                    .focus_visible(move |s| s.border_color(ring))
                    .when_some(on_change, |d, h| {
                        let key = key.clone();
                        d.on_click(move |_, window, cx| h(&key, window, cx))
                    })
                    .child(label),
            );
        }
        track
    }
}

opaque_debug!(Tab, Segmented);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::theme::Mode;

    #[test]
    fn active_tab_opens_into_the_page() {
        let theme = BitacoraTheme::new(Mode::Light);
        let c = &theme.colors;
        let (bg, fg, border, weight) = tab_colors(true, &theme);
        assert_eq!(
            (bg, fg, border, weight),
            (Some(c.bg), c.text, Some(c.line), FontWeight(600.0))
        );
        let (bg, fg, border, _) = tab_colors(false, &theme);
        assert_eq!((bg, fg, border), (None, c.muted, None));
    }

    #[test]
    fn selected_segment_is_raised() {
        let theme = BitacoraTheme::new(Mode::Dark);
        let c = &theme.colors;
        assert_eq!(
            segment_colors(true, &theme),
            (Some(c.raised), c.text, Some(c.line))
        );
        assert_eq!(segment_colors(false, &theme), (None, c.muted, None));
    }
}
