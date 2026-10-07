//! `Card`: a bordered surface for tasks, agent messages and grouped content.

use crate::ui::theme::{ActiveBitacoraTheme as _, BitacoraTheme};
use crate::ui::{
    AnyElement, App, Hsla, IntoElement, ParentElement, Pixels, RenderOnce, Styled as _, Window, div,
};

/// Which palette surface a [`Card`] sits on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Surface {
    /// `raised`: floating content over the page (task cards in the context panel).
    #[default]
    Raised,
    /// `panel`: a row on the page itself (task rows in the Tasks view).
    Panel,
}

impl Surface {
    /// The fill colour for `theme`.
    #[must_use]
    pub fn fill(self, theme: &BitacoraTheme) -> Hsla {
        match self {
            Surface::Raised => theme.colors.raised,
            Surface::Panel => theme.colors.panel,
        }
    }
}

/// A 1px `line` bordered container with `radius_card` corners.
///
/// `Card::new().surface(Surface::Panel).child(...)`
#[derive(IntoElement)]
pub struct Card {
    surface: Surface,
    radius: Option<Pixels>,
    padding: Option<(Pixels, Pixels)>,
    children: Vec<AnyElement>,
}

impl Card {
    /// A raised card with the default 10 x 12 padding.
    pub fn new() -> Self {
        Self {
            surface: Surface::Raised,
            radius: None,
            padding: None,
            children: Vec::new(),
        }
    }

    pub fn surface(mut self, surface: Surface) -> Self {
        self.surface = surface;
        self
    }

    /// Overrides the corner radius (default `radius_card`).
    pub fn radius(mut self, radius: Pixels) -> Self {
        self.radius = Some(radius);
        self
    }

    /// Overrides the `(vertical, horizontal)` padding (default 10 x 12 from the space scale).
    pub fn padding(mut self, vertical: Pixels, horizontal: Pixels) -> Self {
        self.padding = Some((vertical, horizontal));
        self
    }
}

impl Default for Card {
    fn default() -> Self {
        Self::new()
    }
}

impl ParentElement for Card {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for Card {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.bitacora();
        let (py, px) = self
            .padding
            .unwrap_or((theme.metrics.space[5], theme.metrics.space[6]));
        div()
            .flex()
            .flex_col()
            .bg(self.surface.fill(theme))
            .border_1()
            .border_color(theme.colors.line)
            .rounded(self.radius.unwrap_or(theme.metrics.radius_card))
            .py(py)
            .px(px)
            .children(self.children)
    }
}

opaque_debug!(Card);
