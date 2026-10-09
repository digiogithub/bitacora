//! Embedded design-system fonts (BIT-SP-0008.R3).
//!
//! The static TTFs of Atkinson Hyperlegible Next (UI), Atkinson Hyperlegible Mono (code)
//! and Literata (display) ship inside the binary so the UI looks the same on every
//! machine. They are registered with the GPUI text system before the first window.
//! All three families are licensed under the SIL OFL 1.1 (`assets/fonts/OFL-*.txt`).
//!
//! The user's `font-family` setting / custom CSS still override `Theme::font_family`;
//! these fonts only provide the default families.

use std::borrow::Cow;

use crate::ui::{App, TextSystem};

/// UI family.
pub const FONT_UI: &str = "Atkinson Hyperlegible Next";
/// Serif display family (page titles). The static TTFs name themselves
/// "Literata 36pt <Weight>" internally but share the "Literata" family name.
pub const FONT_DISPLAY: &str = "Literata";
/// Monospace family (code, properties).
pub const FONT_MONO: &str = "Atkinson Hyperlegible Mono";

/// The embedded static TTFs.
const EMBEDDED_FONTS: &[&[u8]] = &[
    include_bytes!("../assets/fonts/AtkinsonHyperlegibleNext-Regular.ttf"),
    include_bytes!("../assets/fonts/AtkinsonHyperlegibleNext-Medium.ttf"),
    include_bytes!("../assets/fonts/AtkinsonHyperlegibleNext-SemiBold.ttf"),
    include_bytes!("../assets/fonts/AtkinsonHyperlegibleNext-Bold.ttf"),
    include_bytes!("../assets/fonts/AtkinsonHyperlegibleMono-Regular.ttf"),
    include_bytes!("../assets/fonts/AtkinsonHyperlegibleMono-Medium.ttf"),
    include_bytes!("../assets/fonts/Literata-Medium.ttf"),
    include_bytes!("../assets/fonts/Literata-SemiBold.ttf"),
];

/// Registers the embedded fonts. A failure is logged, not fatal: GPUI falls back to
/// system fonts.
pub fn register(cx: &mut App) {
    add(cx.text_system());
}

fn add(system: &TextSystem) {
    let fonts = EMBEDDED_FONTS.iter().map(|b| Cow::Borrowed(*b)).collect();
    if let Err(err) = system.add_fonts(fonts) {
        tracing::warn!("cannot register the embedded fonts: {err}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::testing::platform_text_system;
    use crate::ui::text_edit::{Font, FontWeight};

    // The test needs the real platform font stack on the test thread. macOS creates its
    // platform on the main thread only (libtest runs tests elsewhere) and the Windows
    // DirectWrite `all_font_names` does not list fonts added from memory, so it only
    // runs against the Linux (cosmic-text) stack.
    #[test]
    #[cfg_attr(
        not(target_os = "linux"),
        ignore = "needs the Linux cosmic-text font stack"
    )]
    fn embedded_families_resolve() {
        let system = platform_text_system();
        add(&system);
        let names = system.all_font_names();
        for family in [FONT_UI, FONT_DISPLAY, FONT_MONO] {
            assert!(
                names.iter().any(|n| n == family),
                "{family} missing: {names:?}"
            );
        }
        // Every embedded weight resolves without falling back (resolve_font panics when
        // neither the font nor any fallback loads).
        for (family, weight) in [
            (FONT_UI, FontWeight::NORMAL),
            (FONT_UI, FontWeight::MEDIUM),
            (FONT_UI, FontWeight::SEMIBOLD),
            (FONT_UI, FontWeight::BOLD),
            (FONT_MONO, FontWeight::NORMAL),
            (FONT_MONO, FontWeight::MEDIUM),
            (FONT_DISPLAY, FontWeight::MEDIUM),
            (FONT_DISPLAY, FontWeight::SEMIBOLD),
        ] {
            let font = Font {
                family: family.into(),
                weight,
                ..Font::default()
            };
            system.resolve_font(&font);
        }
    }

    // GPUI's cosmic-text stack (gpui-pre-wgpu `cosmic_text_system.rs`, `font_id_for_cosmic_id`)
    // substitutes a system font for glyphs the requested family lacks, so no CJK fallback list
    // is configured. Skipped when the host has no CJK font installed.
    #[test]
    #[cfg_attr(
        not(target_os = "linux"),
        ignore = "needs the Linux cosmic-text font stack"
    )]
    fn chinese_text_falls_back_to_a_system_font() {
        use crate::ui::text_edit::TextRun;
        let system = platform_text_system();
        add(&system);
        let host_has_cjk = system
            .all_font_names()
            .iter()
            .any(|n| n.contains("CJK") || n.contains("WenQuanYi") || n.contains("Han Sans"));
        if !host_has_cjk {
            return;
        }
        let text = "\u{8bbe}\u{7f6e}\u{641c}\u{7d22}";
        let run = TextRun {
            len: text.len(),
            font: Font {
                family: FONT_UI.into(),
                ..Font::default()
            },
            color: Default::default(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let ids = crate::ui::testing::shaped_glyph_ids(system, text, run);
        assert_eq!(ids.len(), 4);
        assert!(ids.iter().all(|id| *id != 0), "notdef glyph: {ids:?}");
    }
}
