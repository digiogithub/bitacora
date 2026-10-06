//! Journal detection, journal-day and journal file naming.
//! Spec: `docs/analysis/logseq/01-file-graph-layout.md` §4.

use bitacora_config::{EffectiveConfig, PreferredFormat};

use crate::date::{Date, DateFormat};
use crate::graph_path::GraphPath;
use crate::naming::page_key;

/// First-letter upper-case, rest lower-case, per space-separated word.
pub fn capitalize_all(s: &str) -> String {
    s.split(' ')
        .map(|w| {
            let mut cs = w.chars();
            match cs.next() {
                Some(c) => c
                    .to_uppercase()
                    .chain(cs.flat_map(char::to_lowercase))
                    .collect(),
                None => String::new(),
            }
        })
        .collect::<Vec<String>>()
        .join(" ")
}

/// Ordered, de-duplicated formatter list used to detect journal titles:
/// `[:journal/page-title-format, "MMM do, yyyy", "yyyy-MM-dd", "yyyy_MM_dd"]`.
pub fn journal_title_formatters(cfg: &EffectiveConfig) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for f in [
        cfg.journal_page_title_format(),
        "MMM do, yyyy".to_owned(),
        "yyyy-MM-dd".to_owned(),
        "yyyy_MM_dd".to_owned(),
    ] {
        if !f.trim().is_empty() && !out.contains(&f) {
            out.push(f);
        }
    }
    out
}

/// Parse a page title as a journal date using the first formatter that matches.
pub fn parse_journal_title(title: &str, formatters: &[String]) -> Option<Date> {
    if title.trim().is_empty() {
        return None;
    }
    let cap = capitalize_all(title);
    formatters
        .iter()
        .filter_map(|f| DateFormat::new(f).ok())
        .find_map(|f| f.parse(&cap))
}

/// A recognised journal page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JournalPage {
    /// The date.
    pub date: Date,
    /// `yyyyMMdd` (`:block/journal-day`).
    pub journal_day: u32,
    /// Display title, re-rendered with `:journal/page-title-format`.
    pub title: String,
    /// Lower-cased page key (`:block/name`).
    pub key: String,
}

impl JournalPage {
    /// Journals are never renamed on disk by a page rename.
    pub const FILE_RENAME_ALLOWED: bool = false;
}

/// Detect whether a derived page title is a journal. Detection is by title, not by directory.
pub fn detect_journal(title: &str, cfg: &EffectiveConfig) -> Option<JournalPage> {
    let date = parse_journal_title(title, &journal_title_formatters(cfg))?;
    Some(journal_page(date, cfg))
}

/// Build the journal page description for a date.
pub fn journal_page(date: Date, cfg: &EffectiveConfig) -> JournalPage {
    let rendered = DateFormat::new(&cfg.journal_page_title_format())
        .or_else(|_| DateFormat::new("MMM do, yyyy"))
        .map(|f| f.format(date))
        .unwrap_or_else(|_| date.to_string());
    JournalPage {
        date,
        journal_day: date.journal_day(),
        key: page_key(&rendered),
        title: rendered,
    }
}

/// Graph-relative path of a new journal file for `date`
/// (`<journals-directory>/<file-name-format>.<ext>`).
pub fn journal_file_path(cfg: &EffectiveConfig, date: Date) -> Option<GraphPath> {
    let name = DateFormat::new(&cfg.journal_file_name_format())
        .or_else(|_| DateFormat::new("yyyy_MM_dd"))
        .ok()?
        .format(date);
    let ext = match cfg.preferred_format() {
        PreferredFormat::Org => "org",
        PreferredFormat::Markdown => "md",
    };
    GraphPath::new(&format!("{}/{name}.{ext}", cfg.journals_directory())).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::naming::derive_title;

    fn cfg(src: &str) -> EffectiveConfig {
        EffectiveConfig::from_texts(None, Some(src))
    }
    fn d(y: i32, m: u8, day: u8) -> Date {
        Date::new(y, m, day).expect("date")
    }

    #[test]
    fn capitalize() {
        assert_eq!(capitalize_all("NOV 14TH, 2025"), "Nov 14th, 2025");
        assert_eq!(capitalize_all("a  b"), "A  B");
    }

    #[test]
    fn journal_file_is_detected() {
        let c = cfg("{:file/name-format :triple-lowbar}");
        let title = derive_title("journals/2025_11_14.md", None, &c);
        let j = detect_journal(&title, &c).expect("journal");
        assert_eq!(j.title, "Nov 14th, 2025");
        assert_eq!(j.key, "nov 14th, 2025");
        assert_eq!(j.journal_day, 20251114);
    }

    #[test]
    fn page_dir_journal_name_is_still_journal() {
        let c = cfg("{}");
        let title = derive_title("pages/2024_01_01.md", None, &c);
        assert_eq!(
            detect_journal(&title, &c).map(|j| j.journal_day),
            Some(20240101)
        );
    }

    #[test]
    fn custom_title_format() {
        let c = cfg(r#"{:journal/page-title-format "yyyy-MM-dd"}"#);
        let j = detect_journal("2025-11-14", &c).expect("journal");
        assert_eq!(j.title, "2025-11-14");
        assert_eq!(j.key, "2025-11-14");
        // The default format still parses; it is rendered in the configured format.
        assert_eq!(
            detect_journal("Nov 14th, 2025", &c).expect("j").title,
            "2025-11-14"
        );
        assert_eq!(detect_journal("lower nov 14th, 2025", &c), None);
        assert_eq!(detect_journal("My Page", &c), None);
        assert_eq!(detect_journal("", &c), None);
    }

    #[test]
    fn unlisted_file_name_format_is_not_detected() {
        // yyyyMMdd is not in the parse list and does not match the title format.
        let c = cfg(r#"{:journal/file-name-format "yyyyMMdd"}"#);
        assert_eq!(detect_journal("20251114", &c), None);
    }

    #[test]
    fn file_paths() {
        let date = d(2025, 11, 14);
        let p = journal_file_path(&cfg("{}"), date).expect("p");
        assert_eq!(p.as_str(), "journals/2025_11_14.md");
        let c = cfg(r#"{:journals-directory "daily" :journal/file-name-format "yyyy-MM-dd"}"#);
        assert_eq!(
            journal_file_path(&c, date).expect("p").as_str(),
            "daily/2025-11-14.md"
        );
        let c = cfg(r#"{:preferred-format "org"}"#);
        assert_eq!(
            journal_file_path(&c, date).expect("p").as_str(),
            "journals/2025_11_14.org"
        );
    }

    #[test]
    fn rename_guard() {
        const { assert!(!JournalPage::FILE_RENAME_ALLOWED) };
    }
}
