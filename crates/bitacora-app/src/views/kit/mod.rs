//! Design-system component kit (BIT-US-0117).
//!
//! Reusable building blocks from the design system's `docs/componentes.md`, styled only from
//! the [`BitacoraTheme`](crate::ui::theme::BitacoraTheme) global (`cx.bitacora()`): colours from
//! `colors`, text from `type_scale`, sizes and radii from `metrics`. Nothing here hardcodes a
//! colour or a token-sized dimension, so a mode switch or a token change reaches every screen.
//!
//! Every component is a `RenderOnce` builder (`Button::new(id).label(text).primary()`).
//! Look decisions that can be checked without a window are plain functions returning a `*Spec`
//! (`button_spec`, `marker_spec`, ...), which is what the unit tests assert on. See
//! `docs/design/component-kit.md` for the API summary.

/// `Debug` for builders that hold closures or elements (printing only the type name).
macro_rules! opaque_debug {
    ($($t:ty),+ $(,)?) => {$(
        impl std::fmt::Debug for $t {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.debug_struct(stringify!($t)).finish_non_exhaustive()
            }
        }
    )+};
}

mod button;
mod card;
mod gallery;
mod icons;
mod inline;
mod popover;
mod tab;
#[cfg(test)]
mod tests;

pub use button::{
    Button, ButtonSize, ButtonSpec, ButtonVariant, IconButton, IconButtonSpec, button_spec,
    icon_button_spec,
};
pub use card::{Card, Surface};
pub use gallery::Gallery;
pub use icons::{Glyph, glyph};
pub use inline::{
    Chip, ChipTone, Kbd, MarkerLook, MarkerSpec, Overline, Pill, TaskMarker, chip_colors,
    marker_spec, pill_colors,
};
pub use popover::{PopoverShell, fade_duration};
pub use tab::{Segmented, Tab, segment_colors, tab_colors};

use crate::ui::{App, ElementId, FocusHandle, Window};

/// A focus handle that survives re-renders of the keyed element (tab stop and focus ring).
fn keyed_focus(id: &ElementId, window: &mut Window, cx: &mut App) -> FocusHandle {
    window
        .use_keyed_state(id.clone(), cx, |_, cx| cx.focus_handle())
        .read(cx)
        .clone()
}
