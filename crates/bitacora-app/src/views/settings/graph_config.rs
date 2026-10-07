//! Edits of the graph's `logseq/config.edn` made from the settings view (BIT-US-0107).
//!
//! Every edit is a [`GraphEdit`] applied to the file text with the comment-preserving
//! [`ConfigEditor`] and written through the command queue (single writer, atomic and hash-checked,
//! AGENTS.md rule 3). Each edit declares an [`ApplyMode`]: keys that change how files are parsed
//! or named need the index rebuilt, the rest take effect when the runtime reloads the config.

use std::path::Path;
use std::time::Duration;

use bitacora_config::{
    ConfigEditor, Edn, EffectiveConfig, GraphForce, GraphToggle, NameFormat, PreferredWorkflow,
};
use bitacora_core::date::{Date, DateFormat};
use bitacora_core::queue::{CommandQueue, FileEdit};

/// The config file, relative to the graph.
const CONFIG_PATH: &str = "logseq/config.edn";

/// How long a config edit waits for exclusive access to the graph.
const LOCK_TIMEOUT: Duration = Duration::from_secs(5);

/// When a changed setting takes effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyMode {
    /// Immediately (the runtime hot-reloads `config.edn`).
    Live,
    /// After the index is rebuilt: the key changes how files are read or named, so the app asks
    /// first and reopens the graph.
    ReindexRequired,
    /// After restarting the app or the MCP server.
    RestartRequired,
}

/// One change to `logseq/config.edn`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraphEdit {
    /// `:file/name-format` (`:triple-lowbar`, or the key removed for the legacy format).
    NameFormat(NameFormat),
    /// `:journal/page-title-format`.
    JournalTitleFormat(String),
    /// `:journal/file-name-format`.
    JournalFileFormat(String),
    /// `:preferred-workflow`.
    PreferredWorkflow(PreferredWorkflow),
    /// `:hidden` path prefixes.
    Hidden(Vec<String>),
    /// `:default-templates {:journals "..."}`; an empty name removes the entry.
    DefaultJournalTemplate(String),
    /// Appends to `:favorites`.
    FavoriteAdd(String),
    /// Removes from `:favorites`.
    FavoriteRemove(String),
    /// One key of `:graph/settings` (the graph view's filters, BIT-US-0158).
    GraphToggle(GraphToggle, bool),
    /// One key of `:graph/forcesettings`.
    GraphForce(GraphForce, i64),
    /// Removes `:graph/forcesettings`.
    GraphForcesReset,
}

impl GraphEdit {
    /// When the edit takes effect.
    pub fn apply_mode(&self) -> ApplyMode {
        match self {
            Self::NameFormat(_)
            | Self::JournalTitleFormat(_)
            | Self::JournalFileFormat(_)
            | Self::Hidden(_) => ApplyMode::ReindexRequired,
            Self::PreferredWorkflow(_)
            | Self::DefaultJournalTemplate(_)
            | Self::FavoriteAdd(_)
            | Self::FavoriteRemove(_)
            | Self::GraphToggle(..)
            | Self::GraphForce(..)
            | Self::GraphForcesReset => ApplyMode::Live,
        }
    }

    /// Checks the value before anything is written.
    ///
    /// # Errors
    /// A user-presentable message.
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::JournalTitleFormat(f) | Self::JournalFileFormat(f) => validate_date_format(f),
            Self::FavoriteAdd(name) if name.trim().is_empty() => {
                Err("the page name is empty".to_owned())
            }
            _ => Ok(()),
        }
    }

    /// Applies the edit to `editor`.
    ///
    /// # Errors
    /// [`bitacora_config::Error`] when the config text cannot be edited.
    #[allow(clippy::cast_precision_loss)]
    pub fn apply(&self, editor: &mut ConfigEditor) -> Result<(), bitacora_config::Error> {
        match self {
            Self::NameFormat(NameFormat::TripleLowbar) => {
                editor.assoc(&["file/name-format"], &Edn::kw("triple-lowbar"))
            }
            Self::NameFormat(NameFormat::Legacy) => {
                editor.dissoc(&["file/name-format"]).map(|_| ())
            }
            Self::JournalTitleFormat(f) => {
                editor.assoc(&["journal/page-title-format"], &Edn::str(f))
            }
            Self::JournalFileFormat(f) => editor.assoc(&["journal/file-name-format"], &Edn::str(f)),
            Self::PreferredWorkflow(PreferredWorkflow::Todo) => {
                editor.assoc(&["preferred-workflow"], &Edn::kw("todo"))
            }
            Self::PreferredWorkflow(PreferredWorkflow::Now) => {
                editor.assoc(&["preferred-workflow"], &Edn::kw("now"))
            }
            Self::Hidden(items) => editor.assoc(&["hidden"], &Edn::strs(items)),
            Self::DefaultJournalTemplate(name) if name.trim().is_empty() => editor
                .dissoc(&["default-templates", "journals"])
                .map(|_| ()),
            Self::DefaultJournalTemplate(name) => {
                editor.assoc(&["default-templates", "journals"], &Edn::str(name.trim()))
            }
            Self::FavoriteAdd(page) => editor.favorites_add(page.trim()),
            Self::FavoriteRemove(page) => editor.favorites_remove(page).map(|_| ()),
            Self::GraphToggle(toggle, value) => editor.set_graph_toggle(*toggle, *value),
            Self::GraphForce(force, value) => editor.set_graph_force(*force, *value as f64),
            Self::GraphForcesReset => editor.reset_graph_forces().map(|_| ()),
        }
    }
}

/// A journal date pattern must compile and survive format -> parse, otherwise journals written
/// with it could not be found again.
///
/// # Errors
/// A user-presentable message.
pub fn validate_date_format(pattern: &str) -> Result<(), String> {
    if pattern.trim().is_empty() {
        return Err("the format is empty".to_owned());
    }
    let format = DateFormat::new(pattern).map_err(|e| e.0)?;
    let sample = Date::new(2026, 10, 6).ok_or_else(|| "internal date error".to_owned())?;
    let text = format.format(sample);
    match format.parse(&text) {
        Some(back) if back == sample => Ok(()),
        _ => Err(format!(
            "`{pattern}` does not identify a day (it formats 2026-10-06 as `{text}`)"
        )),
    }
}

/// Preview of `pattern` for today's date (`None` for an invalid pattern).
pub fn date_preview(pattern: &str, day: Date) -> Option<String> {
    DateFormat::new(pattern).ok().map(|f| f.format(day))
}

/// Why a config edit failed.
#[derive(Debug, thiserror::Error)]
pub enum GraphConfigError {
    /// The value is not acceptable.
    #[error("{0}")]
    Invalid(String),
    /// The file could not be read.
    #[error("cannot read {CONFIG_PATH}: {0}")]
    Read(#[source] std::io::Error),
    /// The text could not be edited.
    #[error("config: {0}")]
    Edit(#[from] bitacora_config::Error),
    /// The command queue refused the write (including a config file that changed meanwhile).
    #[error(transparent)]
    Queue(#[from] bitacora_core::queue::QueueError),
}

/// Result of [`edit_config`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edited {
    /// The file text after the edit.
    pub text: String,
    /// Whether the file changed (an idempotent edit writes nothing).
    pub changed: bool,
}

/// Applies `edits` to `<root>/logseq/config.edn` in one atomic write through the queue.
///
/// # Errors
/// See [`GraphConfigError`]; nothing is written on error.
pub fn edit_config(
    queue: &CommandQueue,
    root: &Path,
    edits: &[GraphEdit],
) -> Result<Edited, GraphConfigError> {
    for edit in edits {
        edit.validate().map_err(GraphConfigError::Invalid)?;
    }
    let file = root.join(CONFIG_PATH);
    // A missing config is edited as an empty map; the queue creates the file.
    let (old, existed) = match std::fs::read(&file) {
        Ok(bytes) => (bytes, true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (b"{}\n".to_vec(), false),
        Err(e) => return Err(GraphConfigError::Read(e)),
    };
    let text = String::from_utf8_lossy(&old).into_owned();
    let mut editor = ConfigEditor::parse(&text)?;
    for edit in edits {
        edit.apply(&mut editor)?;
    }
    let new = editor.into_text();
    if new == text {
        return Ok(Edited {
            text: new,
            changed: false,
        });
    }
    let mut lock = queue.acquire(LOCK_TIMEOUT)?;
    lock.apply(vec![FileEdit::Write {
        path: CONFIG_PATH.to_owned(),
        content: new.clone().into_bytes(),
        expected: existed.then_some(old),
    }])?;
    Ok(Edited {
        text: new,
        changed: true,
    })
}

/// The values the "General" section shows, read from the config text on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphValues {
    /// `:file/name-format`.
    pub name_format: NameFormat,
    /// `:journal/page-title-format`.
    pub journal_title_format: String,
    /// `:journal/file-name-format`.
    pub journal_file_format: String,
    /// `:preferred-workflow`.
    pub workflow: PreferredWorkflow,
    /// `:hidden`.
    pub hidden: Vec<String>,
    /// `:default-templates {:journals ...}`.
    pub journal_template: String,
    /// `:favorites`.
    pub favorites: Vec<String>,
}

impl GraphValues {
    /// Values of an effective config.
    pub fn from_config(cfg: &EffectiveConfig) -> Self {
        Self {
            name_format: cfg.name_format(),
            journal_title_format: cfg.journal_page_title_format(),
            journal_file_format: cfg.journal_file_name_format(),
            workflow: cfg.preferred_workflow(),
            hidden: cfg.hidden(),
            journal_template: cfg.default_journal_template(),
            favorites: cfg.favorites(),
        }
    }

    /// Reads `<root>/logseq/config.edn` (merged over the defaults, no global config).
    pub fn load(root: &Path, global: Option<&Path>) -> Self {
        Self::from_config(&EffectiveConfig::load(root, global))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edited(src: &str, edit: &GraphEdit) -> String {
        let mut ed = ConfigEditor::parse(src).expect("parse");
        edit.apply(&mut ed).expect("apply");
        ed.into_text()
    }

    const SRC: &str = "{;; my graph\n :meta/version 1 ;; keep\n :favorites [\"A\" \"B\"]\n :hidden [\"/archived\"]}\n";

    #[test]
    fn edits_preserve_comments_and_other_keys() {
        for edit in [
            GraphEdit::NameFormat(NameFormat::TripleLowbar),
            GraphEdit::JournalTitleFormat("yyyy-MM-dd".into()),
            GraphEdit::JournalFileFormat("yyyy_MM_dd".into()),
            GraphEdit::PreferredWorkflow(PreferredWorkflow::Todo),
            GraphEdit::Hidden(vec!["/a".into(), "/b".into()]),
            GraphEdit::DefaultJournalTemplate("Daily".into()),
            GraphEdit::FavoriteAdd("C".into()),
            GraphEdit::FavoriteRemove("A".into()),
        ] {
            let out = edited(SRC, &edit);
            assert!(out.contains(";; my graph"), "{edit:?}: {out}");
            assert!(out.contains(":meta/version 1 ;; keep"), "{edit:?}: {out}");
            assert_ne!(out, SRC, "{edit:?} changed nothing");
        }
    }

    #[test]
    fn edited_values_read_back() {
        let out = edited(SRC, &GraphEdit::NameFormat(NameFormat::TripleLowbar));
        let cfg = EffectiveConfig::from_texts(None, Some(&out));
        assert_eq!(cfg.name_format(), NameFormat::TripleLowbar);
        let out = edited(&out, &GraphEdit::PreferredWorkflow(PreferredWorkflow::Todo));
        let out = edited(&out, &GraphEdit::DefaultJournalTemplate("Daily".into()));
        let out = edited(&out, &GraphEdit::Hidden(vec!["/x".into()]));
        let cfg = EffectiveConfig::from_texts(None, Some(&out));
        let v = GraphValues::from_config(&cfg);
        assert_eq!(v.workflow, PreferredWorkflow::Todo);
        assert_eq!(v.journal_template, "Daily");
        assert_eq!(v.hidden, vec!["/x".to_owned()]);
        assert_eq!(v.favorites, vec!["A".to_owned(), "B".to_owned()]);
        // Clearing the template removes the entry; legacy removes the name-format key.
        let out = edited(&out, &GraphEdit::DefaultJournalTemplate(String::new()));
        let out = edited(&out, &GraphEdit::NameFormat(NameFormat::Legacy));
        let cfg = EffectiveConfig::from_texts(None, Some(&out));
        assert_eq!(cfg.default_journal_template(), "");
        assert_eq!(cfg.name_format(), NameFormat::Legacy);
    }

    #[test]
    fn apply_modes_follow_what_the_key_changes() {
        assert_eq!(
            GraphEdit::JournalTitleFormat("x".into()).apply_mode(),
            ApplyMode::ReindexRequired
        );
        assert_eq!(
            GraphEdit::NameFormat(NameFormat::Legacy).apply_mode(),
            ApplyMode::ReindexRequired
        );
        assert_eq!(
            GraphEdit::Hidden(vec![]).apply_mode(),
            ApplyMode::ReindexRequired
        );
        assert_eq!(
            GraphEdit::PreferredWorkflow(PreferredWorkflow::Now).apply_mode(),
            ApplyMode::Live
        );
        assert_eq!(
            GraphEdit::FavoriteAdd("a".into()).apply_mode(),
            ApplyMode::Live
        );
    }

    #[test]
    fn date_formats_are_validated() {
        assert!(validate_date_format("yyyy_MM_dd").is_ok());
        assert!(validate_date_format("MMM do, yyyy").is_ok());
        assert!(validate_date_format("EEEE, dd-MM-yyyy").is_ok());
        assert!(validate_date_format("").is_err());
        // Month only: cannot identify a day.
        assert!(validate_date_format("yyyy-MM").is_err());
        assert!(validate_date_format("yyyy 'oops").is_err());
        assert_eq!(
            date_preview("yyyy_MM_dd", Date::new(2026, 10, 6).expect("date")).as_deref(),
            Some("2026_10_06")
        );
        assert_eq!(
            date_preview("yyyy 'oops", Date::new(2026, 10, 6).expect("date")),
            None
        );
        assert!(
            GraphEdit::JournalFileFormat("yyyy-MM".into())
                .validate()
                .is_err()
        );
        assert!(GraphEdit::FavoriteAdd("  ".into()).validate().is_err());
    }
}
