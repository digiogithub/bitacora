//! Hash of the config keys that change what the parser produces (design §4.1 step 1).

use bitacora_config::EffectiveConfig;

use crate::index::OpenOptions;
use crate::normalize::NORMALIZER_VERSION;
use crate::parse::PARSER_VERSION;

/// `hex(blake3(relevant config))[..16]`. A change forces a full reparse.
///
/// Relevant keys: `:journal/page-title-format`, `:journal/file-name-format`, `:file/name-format`,
/// `:journals-directory`, `:pages-directory`, `:whiteboards-directory`, `:hidden`,
/// `:property/separated-by-commas`, `:property-pages/enabled?`, `:property-pages/excludelist`,
/// `:ignored-page-references-keywords`, `:preferred-format`.
#[must_use]
pub fn config_hash(cfg: &EffectiveConfig) -> String {
    let mut h = blake3::Hasher::new();
    let mut field = |name: &str, value: String| {
        h.update(name.as_bytes());
        h.update(&[0]);
        h.update(value.as_bytes());
        h.update(&[0xff]);
    };
    field("title-format", cfg.journal_page_title_format());
    field("file-name-format", cfg.journal_file_name_format());
    field("name-format", format!("{:?}", cfg.name_format()));
    field("journals-dir", cfg.journals_directory());
    field("pages-dir", cfg.pages_directory());
    field("whiteboards-dir", cfg.whiteboards_directory());
    field("hidden", cfg.hidden().join("\u{1}"));
    field(
        "separated",
        cfg.property_separated_by_commas().join("\u{1}"),
    );
    field("prop-pages", cfg.property_pages_enabled().to_string());
    field(
        "prop-exclude",
        cfg.property_pages_excludelist().join("\u{1}"),
    );
    field(
        "ignored-refs",
        cfg.ignored_page_references_keywords().join("\u{1}"),
    );
    field("format", format!("{:?}", cfg.preferred_format()));
    h.finalize().to_hex()[..16].to_owned()
}

impl OpenOptions {
    /// Options for a graph and its effective config: current parser/normalizer versions and the
    /// config hash.
    pub fn for_config(graph_root: impl Into<std::path::PathBuf>, cfg: &EffectiveConfig) -> Self {
        let mut o = Self::new(graph_root, config_hash(cfg));
        o.parser_version = i64::from(PARSER_VERSION);
        o.normalizer_version = i64::from(NORMALIZER_VERSION);
        o
    }
}
