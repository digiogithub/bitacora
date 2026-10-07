//! `PopoverShell`: the floating surface of menus, pickers and the AI box. It is the only
//! surface in the design system that casts a shadow.
//!
//! The shell does not position itself: place it with `deferred(anchored().child(shell))` (or in
//! an overlay layer) and keep the "open" state in the owning view. It asks the owner to close in
//! two cases, through [`PopoverShell::on_dismiss`]: Escape while focus is inside the shell (the
//! shell takes focus when it opens), and a mouse press outside it. Opening fades in over
//! [`FADE`] unless the user asked for reduced motion.

use std::rc::Rc;
use std::time::Duration;

use super::keyed_focus;
use crate::ui::theme::ActiveBitacoraTheme as _;
use crate::ui::{
    Animation, AnimationExt as _, AnyElement, App, ElementId, FluentBuilder as _,
    InteractiveElement as _, IntoElement, ParentElement, Pixels, RenderOnce, Styled as _, Window,
    div,
};

/// Fade-in duration of a popover.
pub const FADE: Duration = Duration::from_millis(120);

/// The fade-in a popover plays, or `None` when the user reduced motion.
#[must_use]
pub fn fade_duration(reduce_motion: bool) -> Option<Duration> {
    (!reduce_motion).then_some(FADE)
}

type Dismiss = Rc<dyn Fn(&mut Window, &mut App)>;

/// A raised, bordered, shadowed container with `radius_popover` corners.
///
/// `PopoverShell::new("menu").width(px(280.)).on_dismiss(|_, cx| ...).child(...)`
#[derive(IntoElement)]
pub struct PopoverShell {
    id: ElementId,
    width: Option<Pixels>,
    on_dismiss: Option<Dismiss>,
    children: Vec<AnyElement>,
}

impl PopoverShell {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            width: None,
            on_dismiss: None,
            children: Vec::new(),
        }
    }

    pub fn width(mut self, width: Pixels) -> Self {
        self.width = Some(width);
        self
    }

    /// Called on Escape and on a mouse press outside the shell. The owner must stop rendering
    /// the shell; nothing is closed here.
    pub fn on_dismiss(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_dismiss = Some(Rc::new(f));
        self
    }
}

impl ParentElement for PopoverShell {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for PopoverShell {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let focus = keyed_focus(&self.id, window, cx);
        if !focus.contains_focused(window, cx) {
            window.focus(&focus, cx);
        }
        let theme = cx.bitacora();
        let fade = fade_duration(cx.reduce_motion());
        let on_key = self.on_dismiss.clone();
        let on_outside = self.on_dismiss;
        let id = self.id.clone();
        let selector = self.id.to_string();

        let shell = div()
            .id(self.id)
            .debug_selector(|| selector)
            .track_focus(&focus)
            .key_context("KitPopover")
            .occlude()
            .flex()
            .flex_col()
            .when_some(self.width, |d, w| d.w(w))
            .bg(theme.colors.raised)
            .border_1()
            .border_color(theme.colors.line)
            .rounded(theme.metrics.radius_popover)
            .shadow_lg()
            .p(theme.metrics.space[4])
            .on_key_down(move |ev, window, cx| {
                if ev.keystroke.key == "escape"
                    && let Some(f) = &on_key
                {
                    cx.stop_propagation();
                    f(window, cx);
                }
            })
            .on_mouse_down_out(move |_, window, cx| {
                if let Some(f) = &on_outside {
                    f(window, cx);
                }
            })
            .children(self.children);

        match fade {
            Some(d) => shell
                .with_animation((id, "fade"), Animation::new(d), |el, t| el.opacity(t))
                .into_any_element(),
            None => shell.into_any_element(),
        }
    }
}

opaque_debug!(PopoverShell);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reduced_motion_skips_the_fade() {
        assert_eq!(fade_duration(false), Some(FADE));
        assert_eq!(fade_duration(true), None);
    }
}
