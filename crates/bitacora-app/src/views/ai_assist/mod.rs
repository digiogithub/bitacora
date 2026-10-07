//! AI assistance surfaces (BIT-US-0151, BIT-US-0152): the journal review card and the
//! recommendation chips of the page on screen.
//!
//! The backend (`bitacora_runtime::ai`, `docs/design/ai-agents.md`) runs the agents and validates
//! what they answer; this module only asks for runs, shows the results in the amber AI treatment
//! and, when the user accepts a suggestion, commits it through the core command queue like any
//! other edit (single writer, undoable). Runs never touch the UI thread: the session thread
//! builds the dependencies and the tokio bridge runs the agent call.
//!
//! * [`review_card`]: the card at the top of the journals feed.
//! * [`suggestions`]: the chips in the right panel's Context tab.
//! * [`dismiss`]: machine-local memory of dismissed suggestions.
//! * this file: the shared gate and context, and the pure text edits of accepted suggestions.

pub mod dismiss;
pub mod review_card;
pub mod suggestions;

use bitacora_config::{PandoFeature, PandoSettings};
use bitacora_runtime::PandoStatus;
use bitacora_runtime::ai::recommend::LinkSuggestion;

use crate::data::GraphHandle;
use crate::session::{SessionHandle, SessionLink};

/// Everything a run or an accepted edit needs from the open graph.
#[derive(Clone)]
pub struct AiContext {
    /// Runs closures on the session thread (to build run dependencies and the edit applier).
    pub session: SessionHandle,
    /// The reader of the graph (page resolution for accepted edits).
    pub graph: GraphHandle,
    /// The command queue and configuration of the session.
    pub link: SessionLink,
}

impl std::fmt::Debug for AiContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AiContext")
    }
}

/// Whether an AI feature can be offered and used right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AiGate {
    /// The feature is switched on, Pando is active and the graph consented: the surface shows.
    pub enabled: bool,
    /// Pando is connected: runs can start.
    pub connected: bool,
}

impl AiGate {
    /// The gate of `feature` for the given settings, graph consent and live Pando status.
    #[must_use]
    pub fn derive(
        settings: &PandoSettings,
        feature: PandoFeature,
        consented: bool,
        live: Option<&PandoStatus>,
    ) -> Self {
        Self {
            enabled: settings.feature_enabled(feature) && consented,
            connected: matches!(live, Some(PandoStatus::Connected { .. })),
        }
    }

    /// Runs may start.
    #[must_use]
    pub fn usable(self) -> bool {
        self.enabled && self.connected
    }
}

/// The markup that replaces a link suggestion's span: `[[target]]` when the text already names
/// the page (any case), else `[text]([[target]])` so the sentence keeps reading the same.
#[must_use]
pub fn link_markup(text: &str, target: &str) -> String {
    if text.to_lowercase() == target.to_lowercase() {
        format!("[[{target}]]")
    } else {
        format!("[{text}]([[{target}]])")
    }
}

/// `block_text` with the span of `s` linked. `None` when the text no longer holds that span
/// (the block changed since the suggestion was made): nothing may be edited then.
#[must_use]
pub fn apply_link(block_text: &str, s: &LinkSuggestion) -> Option<String> {
    let span = block_text.get(s.start..s.end)?;
    if span != s.text {
        return None;
    }
    let mut out = String::with_capacity(block_text.len() + s.target.len() + 8);
    out.push_str(&block_text[..s.start]);
    out.push_str(&link_markup(&s.text, &s.target));
    out.push_str(&block_text[s.end..]);
    Some(out)
}

/// What adding a tag to a page does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TagEdit {
    /// Set the page property `tags` to this value.
    Set(String),
    /// The page already has the tag.
    Already,
    /// The page properties are YAML front matter: not edited automatically.
    Unsupported,
}

fn tag_key(item: &str) -> String {
    item.trim()
        .trim_start_matches('#')
        .trim_start_matches("[[")
        .trim_end_matches("]]")
        .trim()
        .to_lowercase()
}

/// The new value of the `tags` page property after adding `tag` to `preamble` (the page's
/// properties text). Existing entries are kept as written.
#[must_use]
pub fn merge_tags(preamble: Option<&str>, tag: &str) -> TagEdit {
    let tag = tag.trim().trim_start_matches('#');
    if let Some(p) = preamble
        && p.trim_start().starts_with("---")
    {
        return TagEdit::Unsupported;
    }
    let current = preamble
        .and_then(|p| bitacora_markdown::edit::properties::get_property(p, "tags"))
        .unwrap_or_default();
    let mut items: Vec<String> = current
        .split(',')
        .map(|i| i.trim().to_owned())
        .filter(|i| !i.is_empty())
        .collect();
    let want = tag_key(tag);
    if items.iter().any(|i| tag_key(i) == want) {
        return TagEdit::Already;
    }
    items.push(if tag.contains(char::is_whitespace) {
        format!("[[{tag}]]")
    } else {
        tag.to_owned()
    });
    TagEdit::Set(items.join(", "))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn link(text: &str, target: &str, start: usize, end: usize) -> LinkSuggestion {
        LinkSuggestion {
            block_uuid: "u".into(),
            text: text.into(),
            target: target.into(),
            start,
            end,
        }
    }

    #[test]
    fn link_markup_keeps_the_sentence_readable() {
        assert_eq!(link_markup("rust", "Rust"), "[[Rust]]");
        assert_eq!(
            link_markup("the borrow checker", "Borrowing"),
            "[the borrow checker]([[Borrowing]])"
        );
    }

    #[test]
    fn apply_link_replaces_only_the_validated_span() {
        let text = "Learn rust today";
        let s = link("rust", "Rust", 6, 10);
        assert_eq!(apply_link(text, &s).unwrap(), "Learn [[Rust]] today");
        // The block changed under the suggestion: refuse instead of editing something else.
        assert!(apply_link("Learn go today", &s).is_none());
        assert!(apply_link("short", &s).is_none());
        // A range inside a multi-byte character is not a span.
        assert!(apply_link("é rust", &link("rust", "Rust", 1, 5)).is_none());
    }

    #[test]
    fn tags_are_merged_without_duplicates() {
        assert_eq!(merge_tags(None, "ai"), TagEdit::Set("ai".into()));
        assert_eq!(
            merge_tags(Some("title:: T\ntags:: rust, [[Big Idea]]"), "#Rust"),
            TagEdit::Already
        );
        assert_eq!(
            merge_tags(Some("tags:: rust, [[Big Idea]]"), "big idea"),
            TagEdit::Already
        );
        assert_eq!(
            merge_tags(Some("tags:: rust"), "project plan"),
            TagEdit::Set("rust, [[project plan]]".into())
        );
        assert_eq!(
            merge_tags(Some("---\ntags: [a]\n---\n"), "b"),
            TagEdit::Unsupported
        );
    }

    #[test]
    fn gate_needs_the_feature_consent_and_a_connection() {
        let mut s = PandoSettings {
            enabled: true,
            ..PandoSettings::default()
        };
        let f = PandoFeature::Recommendations;
        assert!(!AiGate::derive(&s, f, false, None).enabled);
        let g = AiGate::derive(&s, f, true, None);
        assert!(g.enabled && !g.connected && !g.usable());
        s.features.insert(f, false);
        assert!(!AiGate::derive(&s, f, true, None).enabled);
        s.enabled = false;
        s.features.clear();
        assert!(!AiGate::derive(&s, f, true, None).enabled);
    }
}
