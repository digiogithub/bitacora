//! The icon set: the Lucide subset used by the mockups plus the custom four-point AI sparkle.
//!
//! Lucide icons come from the kit's embedded catalogue and are repainted with the design
//! system stroke width (`metrics.icon_stroke`, 1.7) instead of Lucide's 2. The sparkle is ours:
//! a filled four-point star (the Lucide `sparkle` is stroked and reads as a flower at 15px).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use crate::ui::assets::icon_svg;
use crate::ui::theme::ActiveBitacoraTheme as _;
use crate::ui::{App, Hsla, Icon, Pixels, Styled as _};

/// An icon of the design system.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Glyph {
    PanelLeft,
    PanelRight,
    ChevronLeft,
    ChevronRight,
    ChevronDown,
    Plus,
    Search,
    Sun,
    Moon,
    Printer,
    Calendar,
    File,
    SquareCheck,
    Network,
    Check,
    Close,
    Pin,
    Database,
    Globe,
    ArrowUp,
    CircleAlert,
    Settings,
    /// The custom filled four-point sparkle used for everything AI.
    Sparkle,
}

impl Glyph {
    /// Every glyph, in declaration order.
    pub const ALL: [Glyph; 23] = [
        Glyph::PanelLeft,
        Glyph::PanelRight,
        Glyph::ChevronLeft,
        Glyph::ChevronRight,
        Glyph::ChevronDown,
        Glyph::Plus,
        Glyph::Search,
        Glyph::Sun,
        Glyph::Moon,
        Glyph::Printer,
        Glyph::Calendar,
        Glyph::File,
        Glyph::SquareCheck,
        Glyph::Network,
        Glyph::Check,
        Glyph::Close,
        Glyph::Pin,
        Glyph::Database,
        Glyph::Globe,
        Glyph::ArrowUp,
        Glyph::CircleAlert,
        Glyph::Settings,
        Glyph::Sparkle,
    ];

    /// The Lucide asset path, or `None` for the custom sparkle.
    #[must_use]
    pub fn lucide_path(self) -> Option<&'static str> {
        Some(match self {
            Glyph::PanelLeft => "icons/panel-left.svg",
            Glyph::PanelRight => "icons/panel-right.svg",
            Glyph::ChevronLeft => "icons/chevron-left.svg",
            Glyph::ChevronRight => "icons/chevron-right.svg",
            Glyph::ChevronDown => "icons/chevron-down.svg",
            Glyph::Plus => "icons/plus.svg",
            Glyph::Search => "icons/search.svg",
            Glyph::Sun => "icons/sun.svg",
            Glyph::Moon => "icons/moon.svg",
            Glyph::Printer => "icons/printer.svg",
            Glyph::Calendar => "icons/calendar.svg",
            Glyph::File => "icons/file-text.svg",
            Glyph::SquareCheck => "icons/square-check.svg",
            Glyph::Network => "icons/network.svg",
            Glyph::Check => "icons/check.svg",
            Glyph::Close => "icons/x.svg",
            Glyph::Pin => "icons/pin.svg",
            Glyph::Database => "icons/database.svg",
            Glyph::Globe => "icons/globe.svg",
            Glyph::ArrowUp => "icons/arrow-up.svg",
            Glyph::CircleAlert => "icons/circle-alert.svg",
            Glyph::Settings => "icons/settings.svg",
            Glyph::Sparkle => return None,
        })
    }

    /// The SVG painted for this glyph at stroke width `stroke`.
    #[must_use]
    pub fn svg(self, stroke: f32) -> Option<Arc<[u8]>> {
        type Cache = Mutex<HashMap<(Glyph, u32), Arc<[u8]>>>;
        static CACHE: OnceLock<Cache> = OnceLock::new();
        let key = (self, stroke.to_bits());
        let cache = CACHE.get_or_init(Default::default);
        if let Some(hit) = cache.lock().ok()?.get(&key) {
            return Some(hit.clone());
        }
        let bytes: Arc<[u8]> = match self.lucide_path() {
            Some(path) => {
                let text = String::from_utf8(icon_svg(path)?).ok()?;
                restroke(&text, stroke).into_bytes().into()
            }
            None => SPARKLE.as_bytes().into(),
        };
        cache.lock().ok()?.insert(key, bytes.clone());
        Some(bytes)
    }
}

/// Four-point star, filled with the current colour.
const SPARKLE: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="currentColor"><path d="M12 1.5c.7 5.9 4.6 9.8 10.5 10.5-5.9.7-9.8 4.6-10.5 10.5-.7-5.9-4.6-9.8-10.5-10.5C7.4 11.3 11.3 7.4 12 1.5z"/></svg>"#;

/// Replaces Lucide's `stroke-width="2"` with the design system stroke.
fn restroke(svg: &str, stroke: f32) -> String {
    svg.replace("stroke-width=\"2\"", &format!("stroke-width=\"{stroke}\""))
}

/// A painted icon of `size` in `color`, with the design-system stroke.
///
/// ```ignore
/// glyph(Glyph::Search, cx.bitacora().metrics.icon, cx.bitacora().colors.muted, cx)
/// ```
pub fn glyph(g: Glyph, size: Pixels, color: Hsla, cx: &App) -> Icon {
    let stroke = cx.bitacora().metrics.icon_stroke;
    let icon = match g.svg(stroke) {
        Some(bytes) => Icon::empty().data(&bytes),
        None => Icon::empty(),
    };
    icon.size(size).text_color(color)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_glyph_has_an_embedded_svg_with_the_design_stroke() {
        for g in Glyph::ALL {
            let svg = g
                .svg(1.7)
                .unwrap_or_else(|| panic!("{g:?} is not embedded"));
            let text = std::str::from_utf8(&svg).expect("utf8");
            assert!(text.contains("<svg"), "{g:?}");
            if g.lucide_path().is_some() {
                assert!(text.contains("stroke-width=\"1.7\""), "{g:?}: {text}");
                assert!(!text.contains("stroke-width=\"2\""), "{g:?}");
            }
        }
    }

    #[test]
    fn the_sparkle_is_the_custom_filled_star() {
        assert_eq!(Glyph::Sparkle.lucide_path(), None);
        let svg = Glyph::Sparkle.svg(1.7).expect("sparkle");
        let text = std::str::from_utf8(&svg).expect("utf8");
        assert!(text.contains("fill=\"currentColor\""));
    }

    #[test]
    fn glyphs_are_distinct() {
        let mut paths: Vec<_> = Glyph::ALL.iter().filter_map(|g| g.lucide_path()).collect();
        paths.sort_unstable();
        paths.dedup();
        assert_eq!(paths.len(), Glyph::ALL.len() - 1);
    }
}
