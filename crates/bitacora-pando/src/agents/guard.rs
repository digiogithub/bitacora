//! What may be sent to an agent: consent and exclusions (BIT-SP-0011.R1).
//!
//! Agents read the graph through MCP (which applies the same exclusions server-side) and through
//! context the user attached to a run. [`ContentGuard`] is the gate for that second path and for
//! the results of the `get_selection` frontend tool: nothing is sent without the graph's consent,
//! and excluded or private pages are dropped, using the same [`ContentPolicy`] the semantic
//! indexer applies.

use bitacora_config::pando::GraphConsent;
use bitacora_markdown::edit::properties::get_property;
use pando::agui::ContextEntry;
use serde::{Deserialize, Serialize};

use crate::semantic::ContentPolicy;

/// Most context entries one run carries (the user attaches a handful of blocks, not the graph).
pub const MAX_CONTEXT_BLOCKS: usize = 50;
/// Longest text of one attached block, in characters.
pub const MAX_CONTEXT_CHARS: usize = 8_000;

/// A block the user attached or selected.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttachedBlock {
    /// Title of the page holding the block.
    pub page: String,
    /// Graph-relative path of that page's file.
    pub file_path: String,
    /// Tags of the page or block.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Persisted block uuid, when it has one.
    #[serde(default)]
    pub uuid: Option<String>,
    /// Block text.
    pub text: String,
    /// Whether the page carries the privacy property (the caller knows its page properties).
    #[serde(default)]
    pub page_private: bool,
}

/// Consent and exclusions of one graph, as a gate.
#[derive(Debug, Clone)]
pub struct ContentGuard {
    consent: bool,
    policy: ContentPolicy,
}

impl ContentGuard {
    /// A guard from the graph's consent record. Without `granted` nothing passes.
    #[must_use]
    pub fn from_consent(consent: &GraphConsent) -> Self {
        Self {
            consent: consent.granted,
            policy: ContentPolicy::from_consent(consent),
        }
    }

    /// A guard from explicit parts.
    #[must_use]
    pub fn new(consent: bool, policy: ContentPolicy) -> Self {
        Self { consent, policy }
    }

    /// Whether the graph consented to sharing content with Pando agents.
    #[must_use]
    pub fn has_consent(&self) -> bool {
        self.consent
    }

    /// Whether a page may be named to an agent or suggested to the user (by title only).
    #[must_use]
    pub fn allows_page_name(&self, name: &str) -> bool {
        self.consent && !name.trim().is_empty() && !self.policy.is_excluded("", name)
    }

    /// Whether `block` may be sent.
    #[must_use]
    pub fn allows(&self, block: &AttachedBlock) -> bool {
        if !self.consent || block.page_private {
            return false;
        }
        if self
            .policy
            .is_excluded_with_tags(&block.file_path, &block.page, &block.tags)
        {
            return false;
        }
        let key = self.policy.privacy_property.trim();
        let private = !key.is_empty()
            && get_property(&block.text, key)
                .is_some_and(|v| v.trim().eq_ignore_ascii_case("true"));
        !private
    }

    /// Builds the [`AttachedBlock`]s of one page (or a selection of its blocks) from what the
    /// editor holds: the page-properties `preamble` (tags and the privacy property are read from
    /// it) and the `(uuid, text)` of each block. Nothing is filtered here; pass the result to
    /// [`ContentGuard::filter`] or `ChatHandle::send`.
    #[must_use]
    pub fn page_blocks(
        &self,
        page: &str,
        file_path: &str,
        preamble: Option<&str>,
        blocks: &[(Option<String>, String)],
    ) -> Vec<AttachedBlock> {
        let tags = preamble
            .and_then(|p| get_property(p, "tags"))
            .map(|v| {
                v.split(',')
                    .map(|t| {
                        t.trim()
                            .trim_start_matches('#')
                            .trim_start_matches("[[")
                            .trim_end_matches("]]")
                            .trim()
                            .to_owned()
                    })
                    .filter(|t| !t.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        let key = self.policy.privacy_property.trim();
        let page_private = !key.is_empty()
            && preamble
                .and_then(|p| get_property(p, key))
                .is_some_and(|v| v.trim().eq_ignore_ascii_case("true"));
        blocks
            .iter()
            .map(|(uuid, text)| AttachedBlock {
                page: page.to_owned(),
                file_path: file_path.to_owned(),
                tags: Vec::clone(&tags),
                uuid: uuid.clone(),
                text: text.clone(),
                page_private,
            })
            .collect()
    }

    /// The blocks that pass, capped at [`MAX_CONTEXT_BLOCKS`], with long text truncated.
    #[must_use]
    pub fn filter(&self, blocks: &[AttachedBlock]) -> Vec<AttachedBlock> {
        blocks
            .iter()
            .filter(|b| self.allows(b))
            .take(MAX_CONTEXT_BLOCKS)
            .map(|b| {
                let mut b = b.clone();
                if b.text.chars().count() > MAX_CONTEXT_CHARS {
                    b.text = b.text.chars().take(MAX_CONTEXT_CHARS).collect();
                }
                b
            })
            .collect()
    }

    /// `RunAgentInput.context` entries for the allowed blocks: exactly those blocks and nothing
    /// else (BIT-SP-0011.R1, "Attached context").
    #[must_use]
    pub fn context_entries(&self, blocks: &[AttachedBlock]) -> Vec<ContextEntry> {
        self.filter(blocks)
            .into_iter()
            .map(|b| ContextEntry {
                description: match &b.uuid {
                    Some(u) => format!("Block {u} of page [[{}]]", b.page),
                    None => format!("Block of page [[{}]]", b.page),
                },
                value: b.text,
            })
            .collect()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn block(page: &str, path: &str, text: &str) -> AttachedBlock {
        AttachedBlock {
            page: page.into(),
            file_path: path.into(),
            tags: Vec::new(),
            uuid: Some("u1".into()),
            text: text.into(),
            page_private: false,
        }
    }

    fn consent(granted: bool, exclusions: &[&str]) -> GraphConsent {
        GraphConsent {
            granted,
            exclusions: exclusions.iter().map(|s| (*s).to_owned()).collect(),
            ..GraphConsent::default()
        }
    }

    #[test]
    fn nothing_passes_without_consent() {
        let g = ContentGuard::from_consent(&consent(false, &[]));
        assert!(
            g.context_entries(&[block("A", "pages/a.md", "x")])
                .is_empty()
        );
    }

    #[test]
    fn exclusions_tags_and_private_blocks_are_dropped() {
        let g = ContentGuard::from_consent(&consent(true, &["Secrets", "pages/hr/"]));
        let mut tagged = block("Notes", "pages/n.md", "t");
        tagged.tags = vec!["secrets".into()];
        let blocks = vec![
            block("Open", "pages/open.md", "keep"),
            block("Secrets", "pages/secrets.md", "no"),
            block("Secrets/child", "pages/secrets___child.md", "no"),
            block("Review", "pages/hr/review.md", "no"),
            tagged,
            block("Diary", "pages/diary.md", "hidden\nprivate:: true"),
            AttachedBlock {
                page_private: true,
                ..block("Pp", "pages/pp.md", "no")
            },
        ];
        let entries = g.context_entries(&blocks);
        assert_eq!(entries.len(), 1, "{entries:?}");
        assert_eq!(entries[0].value, "keep");
        assert!(entries[0].description.contains("[[Open]]"));
    }

    #[test]
    fn page_blocks_read_tags_and_privacy_from_the_preamble() {
        let g = ContentGuard::from_consent(&consent(true, &["secrets"]));
        let blocks = vec![(Some("u".to_owned()), "text".to_owned())];
        let tagged = g.page_blocks("N", "pages/n.md", Some("tags:: [[Secrets]], #x"), &blocks);
        assert_eq!(tagged[0].tags, ["Secrets", "x"]);
        assert!(g.filter(&tagged).is_empty());
        let private = g.page_blocks("N", "pages/n.md", Some("private:: true"), &blocks);
        assert!(private[0].page_private && g.filter(&private).is_empty());
        let open = g.page_blocks("N", "pages/n.md", None, &blocks);
        assert_eq!(g.filter(&open).len(), 1);
    }

    #[test]
    fn context_is_capped() {
        let g = ContentGuard::from_consent(&consent(true, &[]));
        let blocks: Vec<_> = (0..80)
            .map(|i| block("P", "pages/p.md", &"x".repeat(MAX_CONTEXT_CHARS + i)))
            .collect();
        let out = g.filter(&blocks);
        assert_eq!(out.len(), MAX_CONTEXT_BLOCKS);
        assert!(
            out.iter()
                .all(|b| b.text.chars().count() == MAX_CONTEXT_CHARS)
        );
    }
}
