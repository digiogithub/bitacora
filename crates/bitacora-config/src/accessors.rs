//! Typed accessors for the config keys that affect layout and parsing
//! (`docs/analysis/logseq/01-file-graph-layout.md` section 2.2). Wrong-typed values fall back to
//! the documented default, like an absent key.

use crate::config::EffectiveConfig;
use crate::edn::Edn;

/// Page file name codec selected by `:file/name-format`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameFormat {
    /// `:legacy` (also the meaning of an absent key): `.` stands for `/`.
    Legacy,
    /// `:triple-lowbar`: `/` is encoded as `___`.
    TripleLowbar,
}

/// File format of new pages (`:preferred-format`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreferredFormat {
    /// Markdown, extension `md`.
    Markdown,
    /// Org mode, extension `org`.
    Org,
}

impl PreferredFormat {
    /// Extension used for new pages.
    pub fn extension(self) -> &'static str {
        match self {
            PreferredFormat::Markdown => "md",
            PreferredFormat::Org => "org",
        }
    }
}

/// Indentation written to files (`:export/bullet-indentation`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BulletIndentation {
    /// One tab per level.
    Tab,
    /// Two spaces per level.
    TwoSpaces,
    /// Four spaces per level.
    FourSpaces,
    /// Eight spaces per level.
    EightSpaces,
}

impl BulletIndentation {
    /// The indentation string for one level.
    pub fn unit(self) -> &'static str {
        match self {
            BulletIndentation::Tab => "\t",
            BulletIndentation::TwoSpaces => "  ",
            BulletIndentation::FourSpaces => "    ",
            BulletIndentation::EightSpaces => "        ",
        }
    }
}

/// Task workflow for new tasks (`:preferred-workflow`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreferredWorkflow {
    /// `NOW` / `LATER`.
    Now,
    /// `TODO` / `DOING`.
    Todo,
}

/// `:default-home` map.
#[derive(Debug, Clone, PartialEq)]
pub struct DefaultHome {
    /// `:page`, when set.
    pub page: Option<String>,
    /// `:sidebar` value, kept raw.
    pub sidebar: Option<Edn>,
}

impl EffectiveConfig {
    fn string_or(&self, key: &str, default: &str) -> String {
        self.get(key)
            .and_then(Edn::as_str)
            .filter(|s| !s.is_empty())
            .unwrap_or(default)
            .to_owned()
    }

    fn bool_or(&self, key: &str, default: bool) -> bool {
        self.get(key).and_then(Edn::as_bool).unwrap_or(default)
    }

    /// Strings and keywords of a vector/set/list value (keywords by name).
    fn names(&self, key: &str) -> Vec<String> {
        self.get(key)
            .and_then(Edn::as_items)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|e| match e {
                        Edn::Str(s) | Edn::Keyword(s) => Some(s.clone()),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// `:file/name-format`; anything but `:triple-lowbar` is legacy.
    pub fn name_format(&self) -> NameFormat {
        match self.get("file/name-format").and_then(Edn::as_keyword) {
            Some("triple-lowbar") => NameFormat::TripleLowbar,
            _ => NameFormat::Legacy,
        }
    }

    /// `:pages-directory`, default `pages`.
    pub fn pages_directory(&self) -> String {
        self.string_or("pages-directory", "pages")
    }

    /// `:journals-directory`, default `journals`.
    pub fn journals_directory(&self) -> String {
        self.string_or("journals-directory", "journals")
    }

    /// `:whiteboards-directory`, default `whiteboards`.
    pub fn whiteboards_directory(&self) -> String {
        self.string_or("whiteboards-directory", "whiteboards")
    }

    /// `:journal/page-title-format` (legacy alias `:date-formatter`), default `MMM do, yyyy`.
    pub fn journal_page_title_format(&self) -> String {
        let default = self.string_or("date-formatter", "MMM do, yyyy");
        self.string_or("journal/page-title-format", &default)
    }

    /// `:journal/file-name-format`, default `yyyy_MM_dd`.
    pub fn journal_file_name_format(&self) -> String {
        self.string_or("journal/file-name-format", "yyyy_MM_dd")
    }

    /// `:preferred-format`, default Markdown. Accepts strings or keywords, any case.
    pub fn preferred_format(&self) -> PreferredFormat {
        let name = match self.get("preferred-format") {
            Some(Edn::Str(s) | Edn::Keyword(s)) => s.to_ascii_lowercase(),
            _ => String::new(),
        };
        match name.as_str() {
            "org" => PreferredFormat::Org,
            _ => PreferredFormat::Markdown,
        }
    }

    /// `:hidden` path prefixes, default empty.
    pub fn hidden(&self) -> Vec<String> {
        self.names("hidden")
    }

    /// Whether `rel_path` (graph-relative, `/` separated) is excluded by `:hidden`.
    ///
    /// Patterns are plain path prefixes; a leading `/` is optional on both sides.
    pub fn is_hidden(&self, rel_path: &str) -> bool {
        let norm = |s: &str| {
            if s.starts_with('/') {
                s.to_owned()
            } else {
                format!("/{s}")
            }
        };
        let path = norm(rel_path);
        self.hidden()
            .iter()
            .any(|p| !p.is_empty() && path.starts_with(&norm(p)))
    }

    /// `:default-templates {:journals "name"}`, default empty.
    pub fn default_journal_template(&self) -> String {
        self.get("default-templates")
            .and_then(|m| m.get("journals"))
            .and_then(Edn::as_str)
            .unwrap_or("")
            .to_owned()
    }

    /// `:feature/enable-journals?`, default true.
    pub fn journals_enabled(&self) -> bool {
        self.bool_or("feature/enable-journals?", true)
    }

    /// `:feature/enable-whiteboards?`, default true.
    pub fn whiteboards_enabled(&self) -> bool {
        self.bool_or("feature/enable-whiteboards?", true)
    }

    /// `:property-pages/enabled?`, default true.
    pub fn property_pages_enabled(&self) -> bool {
        self.bool_or("property-pages/enabled?", true)
    }

    /// `:property-pages/excludelist`, default empty.
    pub fn property_pages_excludelist(&self) -> Vec<String> {
        self.names("property-pages/excludelist")
    }

    /// `:property/separated-by-commas`, default empty.
    pub fn property_separated_by_commas(&self) -> Vec<String> {
        self.names("property/separated-by-commas")
    }

    /// `:ignored-page-references-keywords`, default empty.
    pub fn ignored_page_references_keywords(&self) -> Vec<String> {
        self.names("ignored-page-references-keywords")
    }

    /// `:block-hidden-properties`, default empty.
    pub fn block_hidden_properties(&self) -> Vec<String> {
        self.names("block-hidden-properties")
    }

    /// `:export/bullet-indentation`, default tab.
    pub fn bullet_indentation(&self) -> BulletIndentation {
        match self
            .get("export/bullet-indentation")
            .and_then(Edn::as_keyword)
        {
            Some("two-spaces") => BulletIndentation::TwoSpaces,
            Some("four-spaces") => BulletIndentation::FourSpaces,
            Some("eight-spaces") => BulletIndentation::EightSpaces,
            _ => BulletIndentation::Tab,
        }
    }

    /// `:org-mode/insert-file-link?`, default false.
    pub fn org_insert_file_link(&self) -> bool {
        self.bool_or("org-mode/insert-file-link?", false)
    }

    /// `:favorites` page names, default empty.
    pub fn favorites(&self) -> Vec<String> {
        self.get("favorites")
            .and_then(Edn::as_items)
            .map(|v| {
                v.iter()
                    .filter_map(|e| e.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// `:default-home`, when present and a map.
    pub fn default_home(&self) -> Option<DefaultHome> {
        let m = self.get("default-home")?;
        m.as_map()?;
        Some(DefaultHome {
            page: m.get("page").and_then(Edn::as_str).map(str::to_owned),
            sidebar: m.get("sidebar").cloned(),
        })
    }

    /// `:preferred-workflow`, default `:now`.
    pub fn preferred_workflow(&self) -> PreferredWorkflow {
        match self.get("preferred-workflow").and_then(Edn::as_keyword) {
            Some("todo") => PreferredWorkflow::Todo,
            _ => PreferredWorkflow::Now,
        }
    }

    /// `:file-sync/ignore-files`, default empty.
    pub fn file_sync_ignore_files(&self) -> Vec<String> {
        self.names("file-sync/ignore-files")
    }

    /// `:meta/version`, default 1.
    pub fn meta_version(&self) -> i64 {
        self.get("meta/version").and_then(Edn::as_int).unwrap_or(1)
    }
}
