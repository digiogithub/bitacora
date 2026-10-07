//! The `BitacoraTheme` global: design-system palette, type scale and metrics (BIT-SP-0008.R1).
//!
//! Ported from the design system's `rust/theme.rs`. The GPUI Kit `Theme` keeps styling the kit
//! widgets (it is generated from the same tokens, see `assets/themes/bitacora.json`); this
//! global carries what the kit has no slot for: the AI amber, the outline colours, the type
//! scale and the layout metrics. It is reachable as `cx.bitacora()` and follows the mode chosen
//! by `crate::theme` (System / Light / Dark).

use super::{Metrics, Palette, ThemeMode, TypeScale, TypeStyle};
use crate::ui::text_edit::relative;
use crate::ui::{App, Global, Styled};

/// Colour mode of the design system.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Dark,
    Light,
}

impl From<ThemeMode> for Mode {
    fn from(mode: ThemeMode) -> Self {
        if mode.is_dark() {
            Self::Dark
        } else {
            Self::Light
        }
    }
}

/// Palette, type scale and metrics for one mode.
#[derive(Clone, Debug)]
pub struct BitacoraTheme {
    pub mode: Mode,
    pub colors: Palette,
    pub type_scale: TypeScale,
    pub metrics: Metrics,
}

impl Global for BitacoraTheme {}

impl BitacoraTheme {
    pub fn new(mode: Mode) -> Self {
        Self {
            mode,
            colors: match mode {
                Mode::Dark => Palette::dark(),
                Mode::Light => Palette::light(),
            },
            type_scale: TypeScale::default(),
            metrics: Metrics::default(),
        }
    }

    /// PDF export always uses the light mode, whatever the window shows.
    pub fn print() -> Self {
        Self::new(Mode::Light)
    }

    pub fn is_dark(&self) -> bool {
        self.mode == Mode::Dark
    }

    /// Installs the global for `mode`, or replaces it when the mode changed.
    pub fn set_mode(mode: Mode, cx: &mut App) {
        if cx.try_global::<Self>().is_none_or(|t| t.mode != mode) {
            cx.set_global(Self::new(mode));
        }
    }
}

/// `div().type_style(&cx.bitacora().type_scale.body)`
pub trait TypeStyleExt: Styled + Sized {
    fn type_style(self, style: &TypeStyle) -> Self {
        self.font_family(style.family.clone())
            .text_size(style.size)
            .font_weight(style.weight)
            .line_height(relative(style.line_height))
    }
}

impl<T: Styled> TypeStyleExt for T {}

/// `cx.bitacora().colors.accent`. Named `bitacora()` so it does not clash with the kit's
/// `ActiveTheme::theme()`.
pub trait ActiveBitacoraTheme {
    fn bitacora(&self) -> &BitacoraTheme;
}

impl ActiveBitacoraTheme for App {
    fn bitacora(&self) -> &BitacoraTheme {
        self.global::<BitacoraTheme>()
    }
}
