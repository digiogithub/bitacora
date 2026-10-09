//! Live language switching (BIT-T-0335). The locale is process-global, so this lives in its own
//! integration-test binary instead of the unit tests that assert English text.

use bitacora_app::editor::commands;
use bitacora_app::settings::AppSettings;
use bitacora_app::{i18n, theme, ui};
use bitacora_core::editor::Refusal;

#[gpui_kit::test]
fn language_follows_the_setting_and_switches_live(cx: &mut gpui_kit::TestAppContext) {
    cx.update(|cx| {
        ui::init(cx);
        theme::install(
            cx,
            AppSettings {
                language: Some("es".into()),
                ..AppSettings::default()
            },
            None,
        );
        assert_eq!(&*rust_i18n::locale(), "es");
        assert_eq!(
            i18n::refusal(&Refusal::ReadOnly),
            "Esta p\u{e1}gina es de solo lectura."
        );

        // Slash-menu labels follow the language; markers are keywords; the English label
        // still filters.
        let by_label = |label: &str| {
            commands::SLASH
                .iter()
                .chain(commands::ANGLE)
                .find(|c| c.label == label)
                .copied()
                .expect("command")
        };
        assert_eq!(by_label("Today").title(), "Hoy");
        assert_eq!(by_label("Heading 2").title(), "Encabezado 2");
        assert_eq!(by_label("Quote").title(), "Cita");
        assert_eq!(by_label("TODO").title(), "TODO");
        let hits = commands::filter(commands::SLASH, "encab");
        assert_eq!(hits.first().map(|c| c.label), Some("Heading 1"));
        let hits = commands::filter(commands::SLASH, "heading 3");
        assert_eq!(hits.first().map(|c| c.label), Some("Heading 3"));

        theme::set_language(cx, None, Some("en"));
        assert_eq!(&*rust_i18n::locale(), "en");
        assert_eq!(i18n::refusal(&Refusal::ReadOnly), "This page is read-only.");
        assert_eq!(by_label("Today").title(), "Today");

        // French follows the same path.
        theme::set_language(cx, None, Some("fr"));
        assert_eq!(&*rust_i18n::locale(), "fr");
        assert_eq!(
            i18n::refusal(&Refusal::ReadOnly),
            "Cette page est en lecture seule."
        );
        assert_eq!(by_label("Today").title(), "Aujourd'hui");
        assert_eq!(by_label("Quote").title(), "Citation");
        theme::set_language(cx, None, Some("en"));

        // An unknown tag falls back to a supported language instead of breaking the UI.
        theme::set_language(cx, None, Some("xx"));
        assert!(
            i18n::LANGUAGES
                .iter()
                .any(|(t, _)| *t == &*rust_i18n::locale())
        );
    });
}
