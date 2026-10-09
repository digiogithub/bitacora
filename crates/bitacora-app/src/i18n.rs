//! UI language selection (BIT-T-0335).
//!
//! Translations live in `assets/locales/<locale>.yml` (plus the `settings.<locale>.yml` and
//! `widgets.<locale>.yml` namespaces) and are loaded by `rust-i18n`. The setting `language`
//! is either an explicit tag from [`LANGUAGES`] or `None`, which follows the operating system
//! and falls back to English.

use rust_i18n::t;

/// Languages the UI ships, as `(tag, native name)`.
pub const LANGUAGES: &[(&str, &str)] = &[
    ("en", "English"),
    ("es", "Espa\u{f1}ol"),
    ("fr", "Fran\u{e7}ais"),
    ("zh", "\u{4e2d}\u{6587}"),
];

/// The fallback language.
pub const DEFAULT: &str = "en";

/// Reduces a locale such as `es_ES.UTF-8` or `en-US` to a supported tag.
///
/// Only the primary language subtag is compared, so every Chinese variant (`zh_CN`, `zh-Hans`,
/// `zh_SG` and also Traditional `zh_TW`, `zh-Hant`, `zh_HK`) maps to `zh`. Only Simplified
/// Chinese ships; Traditional users get it as the closest available translation.
pub fn match_tag(raw: &str) -> Option<&'static str> {
    let primary = raw
        .split(['-', '_', '.', '@'])
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    LANGUAGES
        .iter()
        .map(|(tag, _)| *tag)
        .find(|tag| *tag == primary)
}

/// The tag to use for a stored `setting` (`None` = follow the system).
pub fn resolve(setting: Option<&str>) -> &'static str {
    // Unit tests assert English text; they must not depend on the developer's OS language.
    let system = || {
        if cfg!(test) {
            None
        } else {
            sys_locale::get_locale().as_deref().and_then(match_tag)
        }
    };
    setting
        .and_then(match_tag)
        .or_else(system)
        .unwrap_or(DEFAULT)
}

/// The user-facing text of an editor command refusal (core's `Display` is English-only).
pub fn refusal(refusal: &bitacora_core::editor::Refusal) -> String {
    use bitacora_core::editor::Refusal as R;
    match refusal {
        R::UnknownBlock(_) => t!("editor.refusal.unknown_block"),
        R::NoChange => t!("editor.refusal.no_change"),
        R::EmptySelection => t!("editor.refusal.empty_selection"),
        R::NotSiblings => t!("editor.refusal.not_siblings"),
        R::NoPreviousSibling => t!("editor.refusal.no_previous_sibling"),
        R::AlreadyTopLevel => t!("editor.refusal.already_top_level"),
        R::TargetInsideSelection => t!("editor.refusal.target_inside_selection"),
        R::ReadOnly => t!("editor.refusal.read_only"),
        R::NotAnAsset => t!("editor.refusal.not_an_asset"),
        R::Unrepresentable => t!("editor.refusal.unrepresentable"),
        R::BadCursor => t!("editor.refusal.bad_cursor"),
        R::NotEmptyLastChild => t!("editor.refusal.not_empty_last_child"),
        R::FirstBlock => t!("editor.refusal.first_block"),
        R::LastBlockOfPage => t!("editor.refusal.last_block_of_page"),
        R::BothHaveChildren => t!("editor.refusal.both_have_children"),
        R::NoNextBlock => t!("editor.refusal.no_next_block"),
        R::BothReferenced => t!("editor.refusal.both_referenced"),
        R::AtEdge => t!("editor.refusal.at_edge"),
        R::MoveCaret(offset) => t!("editor.refusal.move_caret", offset = offset),
        R::NothingApplicable => t!("editor.refusal.nothing_applicable"),
    }
    .to_string()
}

/// Makes `setting` the active language.
pub fn apply(setting: Option<&str>) {
    rust_i18n::set_locale(resolve(setting));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn french_tags_are_matched() {
        for raw in ["fr", "fr_FR.UTF-8", "fr-CA", "fr_BE", "FR", "fr_CH@euro"] {
            assert_eq!(match_tag(raw), Some("fr"), "{raw}");
        }
        assert_eq!(resolve(Some("fr_FR.UTF-8")), "fr");
    }

    #[test]
    fn tags_are_matched_loosely() {
        assert_eq!(match_tag("es_ES.UTF-8"), Some("es"));
        assert_eq!(match_tag("en-US"), Some("en"));
        assert_eq!(match_tag("xx"), None);
        assert_eq!(resolve(Some("es")), "es");
        assert_eq!(resolve(Some("xx")).len(), 2);
    }

    #[test]
    fn every_chinese_variant_maps_to_simplified_chinese() {
        for raw in [
            "zh",
            "zh_CN.UTF-8",
            "zh-CN",
            "zh-Hans",
            "zh-Hans-CN",
            "zh_SG",
            "zh_TW.UTF-8",
            "zh-Hant",
            "zh_HK",
            "ZH-cn",
        ] {
            assert_eq!(match_tag(raw), Some("zh"), "{raw}");
        }
        assert_eq!(resolve(Some("zh-Hans")), "zh");
    }
}
