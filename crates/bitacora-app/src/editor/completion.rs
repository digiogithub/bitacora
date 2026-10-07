//! Autocomplete for `[[`, `#` and `((` (BIT-US-0038): trigger detection, candidate search over
//! the index and the text each choice inserts. The popup is drawn by [`super::element`]; the
//! state machine lives in [`super::OutlineEditor`]. Rules follow Logseq's behaviour (written
//! from the documented description, ADR-015).

use std::ops::Range;

use bitacora_core::editor::{CompletionProvider, PageSuggestion, page_candidates};
use bitacora_index::search::{Scope, SearchHit, SearchOptions};

use super::commands::{self, Command};
use crate::data::GraphHandle;

/// Candidates shown at once.
pub const MAX_ITEMS: usize = 8;

/// What the caret is inside of.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Trigger {
    /// `[[query`, with the offset after an automatic `]]` when it follows the caret.
    Page {
        /// Offset of the opening `[[`.
        start: usize,
        /// Text typed after `[[`.
        query: String,
        /// End of the `]]` that follows the caret, if any.
        close: Option<usize>,
    },
    /// `#query` at a word start.
    Tag {
        /// Offset of `#`.
        start: usize,
        /// Text typed after `#`.
        query: String,
    },
    /// `((query`.
    Block {
        /// Offset of the opening `((`.
        start: usize,
        /// Text typed after `((`.
        query: String,
        /// End of the `))` that follows the caret, if any.
        close: Option<usize>,
    },
    /// `/query`: the slash command menu (BIT-US-0105). The query may hold spaces.
    Slash {
        /// Offset of `/`.
        start: usize,
        /// Text typed after `/`.
        query: String,
    },
    /// `<query`: the block command menu.
    Angle {
        /// Offset of `<`.
        start: usize,
        /// Text typed after `<`.
        query: String,
    },
}

impl Trigger {
    /// Offset where the trigger starts (identifies the trigger across keystrokes).
    #[must_use]
    pub fn start(&self) -> usize {
        match self {
            Self::Page { start, .. }
            | Self::Tag { start, .. }
            | Self::Block { start, .. }
            | Self::Slash { start, .. }
            | Self::Angle { start, .. } => *start,
        }
    }

    /// The query typed so far.
    #[must_use]
    pub fn query(&self) -> &str {
        match self {
            Self::Page { query, .. }
            | Self::Tag { query, .. }
            | Self::Block { query, .. }
            | Self::Slash { query, .. }
            | Self::Angle { query, .. } => query,
        }
    }

    /// Range of the text a choice replaces: from the trigger to the caret (or past an automatic
    /// closing pair).
    #[must_use]
    pub fn replace_range(&self, cursor: usize) -> Range<usize> {
        match self {
            Self::Page { start, close, .. } | Self::Block { start, close, .. } => {
                *start..close.unwrap_or(cursor)
            }
            Self::Tag { start, .. } | Self::Slash { start, .. } | Self::Angle { start, .. } => {
                *start..cursor
            }
        }
    }
}

fn open_pair(
    text: &str,
    cursor: usize,
    open: &str,
    close: &str,
) -> Option<(usize, String, Option<usize>)> {
    let before = &text[..cursor];
    let start = before.rfind(open)?;
    let query = &text[start + open.len()..cursor];
    if query.contains(close) || query.contains('\n') || query.contains(open.chars().next()?) {
        return None;
    }
    let end = text[cursor..]
        .starts_with(close)
        .then(|| cursor + close.len());
    Some((start, query.to_owned(), end))
}

/// The trigger the caret at `cursor` is inside of, if any. The innermost (latest) one wins.
#[must_use]
pub fn detect(text: &str, cursor: usize) -> Option<Trigger> {
    if cursor > text.len() || !text.is_char_boundary(cursor) {
        return None;
    }
    let mut found: Option<Trigger> = None;
    if let Some((start, query, close)) = open_pair(text, cursor, "((", "))") {
        found = Some(Trigger::Block {
            start,
            query,
            close,
        });
    }
    if let Some((start, query, close)) = open_pair(text, cursor, "[[", "]]") {
        found = match found {
            Some(b) if b.start() > start => Some(b),
            _ => Some(Trigger::Page {
                start,
                query,
                close,
            }),
        };
    }
    // `#tag`: the hash starts a word and the text after it has no space.
    let before = &text[..cursor];
    if let Some(hash) = before.rfind('#') {
        let word_start = hash == 0 || before[..hash].ends_with(char::is_whitespace);
        let rest = &before[hash + 1..];
        let plain = !rest.is_empty()
            && !rest.starts_with(['[', '#'])
            && !rest.contains(char::is_whitespace);
        if word_start && plain && found.as_ref().is_none_or(|f| f.start() < hash) {
            found = Some(Trigger::Tag {
                start: hash,
                query: rest.to_owned(),
            });
        }
    }
    let found = found.filter(|t| !t.query().is_empty());
    if found.is_some() {
        return found;
    }
    command_trigger(text, cursor)
}

/// Longest query the command menus keep open for.
const MAX_COMMAND_QUERY: usize = 32;

/// `/query` or `<query` at a word start on the caret's line. A `/` query may hold spaces (the
/// menu closes by itself once nothing matches) but never starts with one.
fn command_trigger(text: &str, cursor: usize) -> Option<Trigger> {
    let before = &text[..cursor];
    let line_start = before.rfind('\n').map_or(0, |i| i + 1);
    let line = &before[line_start..];
    let word_start = |at: usize| at == 0 || line[..at].ends_with(char::is_whitespace);
    let slash = line
        .rfind('/')
        .filter(|i| word_start(*i))
        .map(|i| (line_start + i, line[i + 1..].to_owned(), true));
    let angle = line
        .rfind('<')
        .filter(|i| word_start(*i))
        .map(|i| (line_start + i, line[i + 1..].to_owned(), false));
    // The later opener wins.
    let (start, query, is_slash) = match (slash, angle) {
        (Some(s), Some(a)) => {
            if s.0 > a.0 {
                s
            } else {
                a
            }
        }
        (Some(s), None) => s,
        (None, Some(a)) => a,
        (None, None) => return None,
    };
    if query.len() > MAX_COMMAND_QUERY || query.starts_with(char::is_whitespace) {
        return None;
    }
    if is_slash {
        Some(Trigger::Slash { start, query })
    } else if query.contains(char::is_whitespace) || query.starts_with('%') {
        None
    } else {
        Some(Trigger::Angle { start, query })
    }
}

/// The text a page choice inserts for `trigger`.
#[must_use]
pub fn page_text(trigger: &Trigger, title: &str) -> String {
    match trigger {
        Trigger::Tag { .. } => {
            if title.is_empty() || title.contains(|c: char| c.is_whitespace() || "[]#,".contains(c))
            {
                format!("#[[{title}]]")
            } else {
                format!("#{title}")
            }
        }
        _ => format!("[[{title}]]"),
    }
}

/// One candidate of the popup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    /// A page; `exists` is false for the trailing "New page" entry.
    Page {
        /// Title.
        title: String,
        /// The page is in the graph.
        exists: bool,
    },
    /// A block (`((` completion).
    Block {
        /// Block UUID in the index.
        uuid: String,
        /// Title of the containing page.
        page: String,
        /// First line of the block.
        text: String,
    },
    /// A slash or angle command.
    Command(Command),
    /// A template of the graph (`/template` list).
    Template {
        /// Name (`template:: name`).
        name: String,
    },
}

impl Item {
    /// Text shown in the popup.
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Self::Page {
                title,
                exists: true,
            } => title.clone(),
            Self::Page { title, .. } => rust_i18n::t!("editor.new_page", title = title).to_string(),
            Self::Block { page, text, .. } => format!("{text}  ({page})"),
            Self::Command(c) => c.title(),
            Self::Template { name } => name.clone(),
        }
    }
}

/// [`CompletionProvider`] over the SQLite index.
#[derive(Debug)]
pub struct IndexProvider<'a>(pub &'a GraphHandle);

impl CompletionProvider for IndexProvider<'_> {
    fn search_pages(&self, query: &str, limit: usize) -> Vec<PageSuggestion> {
        let opts = SearchOptions {
            limit,
            scope: Scope::Pages,
            ..SearchOptions::default()
        };
        self.0
            .reader
            .search(query, &opts)
            .map(|hits| {
                hits.into_iter()
                    .filter_map(|h| match h {
                        SearchHit::Page { title, .. } => Some(PageSuggestion {
                            title,
                            exists: true,
                        }),
                        SearchHit::Block { .. } => None,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    fn search_blocks(
        &self,
        query: &str,
        limit: usize,
    ) -> Vec<bitacora_core::editor::BlockSuggestion> {
        let opts = SearchOptions {
            limit,
            scope: Scope::All,
            ..SearchOptions::default()
        };
        self.0
            .reader
            .search(query, &opts)
            .map(|hits| {
                hits.into_iter()
                    .filter_map(|h| match h {
                        SearchHit::Block {
                            uuid, page_title, ..
                        } => {
                            let text = self
                                .0
                                .reader
                                .block(&uuid)
                                .ok()
                                .flatten()
                                .map(|b| b.content)
                                .unwrap_or_default();
                            Some(bitacora_core::editor::BlockSuggestion {
                                page: page_title,
                                text,
                                uuid: uuid.parse().ok(),
                                block: None,
                            })
                        }
                        SearchHit::Page { .. } => None,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// Candidates for `trigger` while editing the block with text `own_text` on `current_page`.
#[must_use]
pub fn candidates(
    handle: &GraphHandle,
    trigger: &Trigger,
    current_page: &str,
    own_text: &str,
) -> Vec<Item> {
    let provider = IndexProvider(handle);
    match trigger {
        Trigger::Page { query, .. } | Trigger::Tag { query, .. } => {
            page_candidates(&provider, query, current_page, MAX_ITEMS)
                .into_iter()
                .map(|p| Item::Page {
                    title: p.title,
                    exists: p.exists,
                })
                .collect()
        }
        Trigger::Slash { query, .. } => match commands::template_query(query) {
            Some(q) => template_items(handle, q),
            None => commands::filter(commands::SLASH, query)
                .into_iter()
                .map(Item::Command)
                .collect(),
        },
        Trigger::Angle { query, .. } => commands::filter(commands::ANGLE, query)
            .into_iter()
            .map(Item::Command)
            .collect(),
        Trigger::Block { query, .. } => provider
            .search_blocks(query, MAX_ITEMS * 3)
            .into_iter()
            .filter(|b| b.text.trim() != own_text.trim() && !b.text.trim().is_empty())
            .take(MAX_ITEMS)
            .map(|b| Item::Block {
                uuid: b.uuid.map(|u| u.to_string()).unwrap_or_default(),
                page: b.page,
                text: b.text.lines().next().unwrap_or("").to_owned(),
            })
            .collect(),
    }
}

/// Templates of the graph matching `query`, best first.
fn template_items(handle: &GraphHandle, query: &str) -> Vec<Item> {
    let mut names: Vec<String> = handle
        .reader
        .templates()
        .unwrap_or_default()
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    names.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    let mut scored: Vec<(i32, String)> = names
        .into_iter()
        .filter_map(|n| commands::fuzzy_score(query, &n).map(|s| (s, n)))
        .collect();
    scored.sort_by_key(|a| std::cmp::Reverse(a.0));
    scored
        .into_iter()
        .map(|(_, name)| Item::Template { name })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_trigger_with_an_automatic_closer() {
        let t = detect("see [[Al]] now", 8).expect("trigger");
        assert_eq!(
            t,
            Trigger::Page {
                start: 4,
                query: "Al".into(),
                close: Some(10)
            }
        );
        assert_eq!(t.replace_range(8), 4..10);
        assert_eq!(page_text(&t, "Alpha"), "[[Alpha]]");
        // No query yet, a closed pair or a line break: no trigger.
        assert_eq!(detect("[[", 2), None);
        assert_eq!(detect("[[a]] b", 7), None);
        assert_eq!(detect("[[a\nb", 5), None);
    }

    #[test]
    fn tag_trigger_needs_a_word_start() {
        let t = detect("note #pro", 9).expect("tag");
        assert_eq!(
            t,
            Trigger::Tag {
                start: 5,
                query: "pro".into()
            }
        );
        assert_eq!(t.replace_range(9), 5..9);
        assert_eq!(page_text(&t, "project"), "#project");
        assert_eq!(page_text(&t, "two words"), "#[[two words]]");
        assert_eq!(detect("a#b", 3), None, "not at a word start");
        assert_eq!(detect("#", 1), None);
        assert_eq!(detect("# heading", 9), None);
        assert_eq!(detect("##x", 3), None);
    }

    #[test]
    fn block_trigger_and_nesting() {
        let t = detect("ref ((abc))", 9).expect("block");
        assert!(matches!(
            t,
            Trigger::Block {
                start: 4,
                close: Some(11),
                ..
            }
        ));
        // The innermost trigger wins: a page ref opened inside text after a block ref.
        let t = detect("((a)) [[b", 9).expect("page");
        assert!(matches!(t, Trigger::Page { .. }));
    }

    #[test]
    fn command_triggers_need_a_word_start() {
        let t = detect("/", 1).expect("slash");
        assert_eq!(
            t,
            Trigger::Slash {
                start: 0,
                query: String::new()
            }
        );
        let t = detect("note /todo", 10).expect("slash");
        assert_eq!(t.start(), 5);
        assert_eq!(t.query(), "todo");
        assert_eq!(t.replace_range(10), 5..10);
        // Spaces stay in a slash query, but not at its start.
        assert_eq!(detect("/page ref", 9).expect("slash").query(), "page ref");
        assert_eq!(detect("/ x", 3), None);
        // Not at a word start, or on another line.
        assert_eq!(detect("and/or", 6), None);
        assert_eq!(detect("http://x", 8), None);
        assert_eq!(detect("/a\nb", 5), None);
        let t = detect("<qu", 3).expect("angle");
        assert_eq!(
            t,
            Trigger::Angle {
                start: 0,
                query: "qu".into()
            }
        );
        assert_eq!(detect("<a b", 4), None);
        assert_eq!(detect("a<b", 3), None);
        // A page reference wins over a slash inside it.
        assert!(matches!(detect("[[a /b", 6), Some(Trigger::Page { .. })));
    }

    #[test]
    fn never_panics_on_multibyte_text() {
        for text in [
            "\u{e9}[[\u{e9}",
            "#\u{6f22}",
            "((\u{1f600}",
            "[[\u{6f22}]]",
            "/\u{6f22}\u{e9}",
            "<\u{1f600}",
        ] {
            for (i, _) in text.char_indices().chain([(text.len(), ' ')]) {
                let _ = detect(text, i);
            }
        }
        assert_eq!(detect("abc", 99), None);
    }
}
