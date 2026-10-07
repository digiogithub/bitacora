//! Block -> Pando document mapping (BIT-US-0142, ADR-030).
//!
//! A document is one Logseq block: id `bitacora/<graph_id>/<block-uuid>`, text made of the page
//! title, the breadcrumb of ancestor blocks and the block content, plus metadata. The mapping is
//! a pure function of the block and the [`ContentPolicy`], so the same input always yields the
//! same [`SemanticDoc::content_hash`]; the sync worker diffs on that hash.
//!
//! Only eligible content is mapped: the page-properties pre-block, excluded pages, blocks that
//! only carry properties or assets and blocks shorter than [`ContentPolicy::min_chars`] are
//! skipped with a [`Skip`] reason.

use bitacora_config::GraphConsent;
use bitacora_index::SemanticBlock;
use serde_json::{Map, Value, json};

/// Version of the document layout. It is part of the hashed content, so bumping it re-sends every
/// document.
pub const DOC_SCHEMA: u32 = 1;

/// Prefix of every document id Bitacora creates in Pando's shared knowledge base.
pub const DOC_ID_ROOT: &str = "bitacora";

/// Default minimum number of letters and digits a block needs to be sent.
pub const DEFAULT_MIN_CHARS: usize = 20;

/// Properties that are bookkeeping, not content: they never reach the document text.
const SYSTEM_PROPERTIES: &[&str] = &[
    "id",
    "collapsed",
    "heading",
    "created-at",
    "updated-at",
    "created_at",
    "updated_at",
    "logseq.order-list-type",
    "logseq.color",
    "background-color",
    "background_color",
];

/// What the graph identity looks like to Pando.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphInfo {
    /// Stable graph id (`bitacora_index::graph_id`), part of every document id.
    pub id: String,
    /// Display name of the graph, kept in the metadata.
    pub name: String,
}

/// Which blocks are eligible.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentPolicy {
    /// Minimum count of letters and digits in the block body.
    pub min_chars: usize,
    /// Graph-relative path prefixes (entries containing `/`) or names (anything else) that are
    /// never sent. A name matches the page, its `ns/` children and any block tagged with it.
    pub exclusions: Vec<String>,
    /// Send journal blocks.
    pub include_journals: bool,
    /// Property that marks a page or block as private (`private:: true`); empty disables it.
    pub privacy_property: String,
    /// Nothing is eligible (consent revoked): documents already sent are deleted by the next
    /// reconcile and none is sent.
    pub deny_all: bool,
}

impl Default for ContentPolicy {
    fn default() -> Self {
        Self {
            min_chars: DEFAULT_MIN_CHARS,
            exclusions: Vec::new(),
            include_journals: true,
            privacy_property: "private".to_owned(),
            deny_all: false,
        }
    }
}

impl ContentPolicy {
    /// The default policy plus the exclusions the user set for this graph.
    #[must_use]
    pub fn from_consent(consent: &GraphConsent) -> Self {
        Self {
            exclusions: consent.exclusions.clone(),
            ..Self::default()
        }
    }

    /// A policy that lets nothing through (the graph has no consent).
    #[must_use]
    pub fn denying_all() -> Self {
        Self {
            deny_all: true,
            ..Self::default()
        }
    }

    /// Whether the page at `file_path` titled `page_title` is excluded.
    #[must_use]
    pub fn is_excluded(&self, file_path: &str, page_title: &str) -> bool {
        self.is_excluded_with_tags(file_path, page_title, &[])
    }

    /// Like [`is_excluded`](Self::is_excluded), also matching `tags` against name entries.
    #[must_use]
    pub fn is_excluded_with_tags(
        &self,
        file_path: &str,
        page_title: &str,
        tags: &[String],
    ) -> bool {
        let path = file_path.to_lowercase();
        let title = page_title.to_lowercase();
        self.exclusions.iter().any(|raw| {
            let e = raw.trim().to_lowercase();
            if e.is_empty() {
                return false;
            }
            // `#tag` entries (as the settings UI and the MCP read exclusions write them) match
            // tags only, with or without the hash on the tag.
            if let Some(tag) = e.strip_prefix('#') {
                let tag = tag.trim();
                !tag.is_empty()
                    && tags
                        .iter()
                        .any(|t| t.trim().trim_start_matches('#').to_lowercase() == tag)
            } else if e.contains('/') {
                path.starts_with(&e)
            } else {
                title == e
                    || title.starts_with(&format!("{e}/"))
                    || tags.iter().any(|t| t.to_lowercase() == e)
            }
        })
    }
}

impl ContentPolicy {
    fn is_private(&self, block: &SemanticBlock) -> bool {
        let key = self.privacy_property.trim().to_lowercase();
        if key.is_empty() {
            return false;
        }
        block
            .page_properties
            .iter()
            .chain(&block.properties)
            .chain(&block.ancestor_properties)
            .any(|(k, v)| k.trim().to_lowercase() == key && v.trim().eq_ignore_ascii_case("true"))
    }
}

/// Why a block produces no document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Skip {
    /// The page-properties pre-block.
    PreBlock,
    /// Journal blocks are switched off.
    Journal,
    /// The page matches an exclusion.
    Excluded,
    /// The page or block carries the privacy property.
    Private,
    /// Nothing but properties (and drawers).
    PropertiesOnly,
    /// Nothing but assets (images, videos, files).
    AssetsOnly,
    /// Fewer letters and digits than [`ContentPolicy::min_chars`].
    TooShort,
}

/// One document ready to be upserted.
#[derive(Debug, Clone, PartialEq)]
pub struct SemanticDoc {
    /// Document id (`bitacora/<graph_id>/<block-uuid>`), the Pando `file_path`.
    pub doc_id: String,
    /// Block UUID.
    pub block_uuid: String,
    /// Graph-relative path of the file holding the block.
    pub file_path: String,
    /// Markdown text sent to Pando.
    pub text: String,
    /// Metadata sent to Pando (includes the hash).
    pub metadata: Map<String, Value>,
    /// Tags (also sent as the document tags).
    pub tags: Vec<String>,
    /// Hash of text and metadata: 32 lower-case hex characters.
    pub content_hash: String,
}

/// The document id of a block.
#[must_use]
pub fn doc_id(graph_id: &str, block_uuid: &str) -> String {
    format!("{DOC_ID_ROOT}/{graph_id}/{block_uuid}")
}

/// The document-id prefix of a graph (for diagnostics and orphan searches).
#[must_use]
pub fn doc_prefix(graph_id: &str) -> String {
    format!("{DOC_ID_ROOT}/{graph_id}/")
}

/// Whether `line` is a `key:: value` property line.
fn property_key(line: &str) -> Option<&str> {
    let t = line.trim_start();
    let (key, rest) = t.split_once("::")?;
    let key = key.trim();
    let ok = !key.is_empty()
        && key
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.' | '/'));
    (ok && (rest.is_empty() || rest.starts_with(' '))).then_some(key)
}

/// Drops `:LOGBOOK:` style drawers.
fn strip_drawers(lines: &[&str]) -> Vec<String> {
    let mut out = Vec::with_capacity(lines.len());
    let mut in_drawer = false;
    for l in lines {
        let t = l.trim();
        if in_drawer {
            if t.eq_ignore_ascii_case(":end:") {
                in_drawer = false;
            }
            continue;
        }
        if t.len() > 2 && t.starts_with(':') && t.ends_with(':') && !t.contains(' ') {
            in_drawer = !t.eq_ignore_ascii_case(":end:");
            continue;
        }
        out.push((*l).to_owned());
    }
    out
}

/// The content with drawers and bookkeeping properties removed, and whether any non-system
/// property line was dropped or kept (used for the property-only test).
struct Cleaned {
    /// Text lines kept (user properties included).
    kept: Vec<String>,
    /// Lines that are neither properties nor blank.
    prose: Vec<String>,
}

fn clean(content: &str) -> Cleaned {
    let raw: Vec<&str> = content.lines().collect();
    let lines = strip_drawers(&raw);
    let mut kept = Vec::new();
    let mut prose = Vec::new();
    for l in lines {
        match property_key(&l) {
            Some(k) => {
                if !SYSTEM_PROPERTIES.contains(&k.to_lowercase().as_str()) {
                    kept.push(l);
                }
            }
            None => {
                if !l.trim().is_empty() {
                    prose.push(l.clone());
                }
                kept.push(l);
            }
        }
    }
    while kept.last().is_some_and(|l| l.trim().is_empty()) {
        kept.pop();
    }
    Cleaned { kept, prose }
}

/// Removes asset embeds and links so an asset-only block can be recognised.
fn strip_assets(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while !rest.is_empty() {
        if let Some(stripped) = rest.strip_prefix("{{") {
            if let Some(end) = stripped.find("}}") {
                let inner = stripped[..end].trim_start().to_lowercase();
                let is_asset = ["video", "audio", "pdf", "youtube", "bilibili", "vimeo"]
                    .iter()
                    .any(|k| inner.starts_with(k));
                if is_asset {
                    rest = &stripped[end + 2..];
                    continue;
                }
            }
        } else if rest.starts_with("![") || rest.starts_with('[') {
            // `![alt](target)` or `[label](target)` pointing into assets/.
            let after = rest.strip_prefix('!').unwrap_or(rest);
            if let Some(close) = after.find("](")
                && let Some(end) = after[close + 2..].find(')')
            {
                let target = &after[close + 2..close + 2 + end];
                if target.contains("assets/") {
                    rest = &after[close + 2 + end + 1..];
                    continue;
                }
            }
        }
        let mut chars = rest.chars();
        if let Some(c) = chars.next() {
            out.push(c);
        }
        rest = chars.as_str();
    }
    out
}

fn is_task_marker(word: &str) -> bool {
    matches!(
        word,
        "TODO"
            | "DOING"
            | "DONE"
            | "LATER"
            | "NOW"
            | "WAIT"
            | "WAITING"
            | "CANCELED"
            | "CANCELLED"
            | "IN-PROGRESS"
    )
}

fn worthwhile_chars(s: &str) -> usize {
    s.split_whitespace()
        .filter(|w| {
            !is_task_marker(w) && !w.starts_with("SCHEDULED:") && !w.starts_with("DEADLINE:")
        })
        .flat_map(str::chars)
        .filter(|c| c.is_alphanumeric())
        .count()
}

/// Maps `block` to a document, or says why it is not eligible.
///
/// # Errors
/// The [`Skip`] reason when the block is not eligible.
pub fn map_block(
    graph: &GraphInfo,
    block: &SemanticBlock,
    policy: &ContentPolicy,
) -> Result<SemanticDoc, Skip> {
    if policy.deny_all {
        return Err(Skip::Excluded);
    }
    if block.is_pre_block {
        return Err(Skip::PreBlock);
    }
    if block.is_journal && !policy.include_journals {
        return Err(Skip::Journal);
    }
    if policy.is_excluded_with_tags(&block.file_path, &block.page_title, &block.tags) {
        return Err(Skip::Excluded);
    }
    if policy.is_private(block) {
        return Err(Skip::Private);
    }
    let cleaned = clean(&block.content);
    if cleaned.prose.is_empty() {
        return Err(Skip::PropertiesOnly);
    }
    let prose = cleaned.prose.join("\n");
    let without_assets = strip_assets(&prose);
    if worthwhile_chars(&without_assets) == 0 && !is_all_markers(&without_assets) {
        return Err(Skip::AssetsOnly);
    }
    if worthwhile_chars(&without_assets) < policy.min_chars {
        return Err(Skip::TooShort);
    }

    let mut text = format!("# {}\n", block.page_title);
    if !block.breadcrumb.is_empty() {
        text.push_str(&format!("> {}\n", block.breadcrumb.join(" > ")));
    }
    text.push('\n');
    text.push_str(&cleaned.kept.join("\n"));
    text.push('\n');

    let mut metadata = Map::new();
    metadata.insert("source".into(), json!("bitacora"));
    metadata.insert("graph".into(), json!(graph.id));
    metadata.insert("graph_name".into(), json!(graph.name));
    metadata.insert("page".into(), json!(block.page_title));
    metadata.insert("page_path".into(), json!(block.file_path));
    metadata.insert("block_uuid".into(), json!(block.uuid));
    if let Some(day) = block.journal_day {
        metadata.insert("journal_day".into(), json!(day));
    }
    metadata.insert("tags".into(), json!(block.tags));
    if let Some(m) = &block.marker {
        metadata.insert("marker".into(), json!(m));
    }
    metadata.insert("schema".into(), json!(DOC_SCHEMA));
    let content_hash = hash_of(&text, &metadata);
    metadata.insert("content_hash".into(), json!(content_hash));

    Ok(SemanticDoc {
        doc_id: doc_id(&graph.id, &block.uuid),
        block_uuid: block.uuid.clone(),
        file_path: block.file_path.clone(),
        text,
        metadata,
        tags: block.tags.clone(),
        content_hash,
    })
}

/// A body made only of task markers ("TODO") has no content but is not an asset block either.
fn is_all_markers(s: &str) -> bool {
    s.split_whitespace().all(is_task_marker) && s.split_whitespace().next().is_some()
}

fn hash_of(text: &str, metadata: &Map<String, Value>) -> String {
    let mut h = blake3::Hasher::new();
    h.update(text.as_bytes());
    h.update(&[0]);
    // `Map` keeps insertion order (preserve_order), which `map_block` fixes.
    h.update(Value::Object(metadata.clone()).to_string().as_bytes());
    h.finalize().to_hex()[..32].to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(content: &str) -> SemanticBlock {
        SemanticBlock {
            uuid: "6650a1b2-0000-4000-8000-00000000000c".into(),
            file_path: "pages/Trip Plan.md".into(),
            page_title: "Trip Plan".into(),
            is_journal: false,
            journal_day: None,
            breadcrumb: vec!["Itinerary".into()],
            content: content.into(),
            marker: None,
            depth: 2,
            is_pre_block: false,
            tags: vec!["travel".into()],
            properties: Vec::new(),
            page_properties: Vec::new(),
            ancestor_properties: Vec::new(),
        }
    }

    fn graph() -> GraphInfo {
        GraphInfo {
            id: "abcd0123abcd0123".into(),
            name: "notes".into(),
        }
    }

    fn map(content: &str) -> Result<SemanticDoc, Skip> {
        map_block(&graph(), &block(content), &ContentPolicy::default())
    }

    #[test]
    fn maps_title_breadcrumb_and_content() {
        let d = map("Book the ferry to the island\nid:: 6650a1b2-0000-4000-8000-00000000000c")
            .expect("doc");
        assert_eq!(
            d.doc_id,
            "bitacora/abcd0123abcd0123/6650a1b2-0000-4000-8000-00000000000c"
        );
        assert_eq!(
            d.text,
            "# Trip Plan\n> Itinerary\n\nBook the ferry to the island\n"
        );
        assert_eq!(d.metadata["schema"], json!(1));
        assert_eq!(d.metadata["content_hash"], json!(d.content_hash));
        assert_eq!(d.content_hash.len(), 32);
    }

    #[test]
    fn hash_is_deterministic_and_tracks_context() {
        let a = map("Book the ferry to the island").expect("a");
        let b = map("Book the ferry to the island").expect("b");
        assert_eq!(a.content_hash, b.content_hash);
        let mut moved = block("Book the ferry to the island");
        moved.file_path = "pages/Other.md".into();
        let c = map_block(&graph(), &moved, &ContentPolicy::default()).expect("c");
        assert_ne!(a.content_hash, c.content_hash);
    }

    #[test]
    fn skips_ineligible_blocks() {
        assert_eq!(map("collapsed:: true\nid:: x"), Err(Skip::PropertiesOnly));
        assert_eq!(
            map("type:: book\nauthor:: Someone"),
            Err(Skip::PropertiesOnly)
        );
        assert_eq!(
            map("![photo](../assets/photo_1.png)"),
            Err(Skip::AssetsOnly)
        );
        assert_eq!(map("{{video https://youtu.be/xyz}}"), Err(Skip::AssetsOnly));
        assert_eq!(
            map("![a](../assets/a.png) [doc](../assets/d.pdf)"),
            Err(Skip::AssetsOnly)
        );
        assert_eq!(map("ok"), Err(Skip::TooShort));
        assert_eq!(map("Nineteen chars here"), Err(Skip::TooShort));
        assert_eq!(map("TODO call mom"), Err(Skip::TooShort));
        assert_eq!(
            map(":LOGBOOK:\nCLOCK: [2024-05-01]\n:END:"),
            Err(Skip::PropertiesOnly)
        );
        let mut pre = block("title:: Trip Plan");
        pre.is_pre_block = true;
        assert_eq!(
            map_block(&graph(), &pre, &ContentPolicy::default()),
            Err(Skip::PreBlock)
        );
    }

    #[test]
    fn text_with_assets_is_kept_whole() {
        let d = map("Lunch by the sea with friends ![photo](../assets/photo_1.png)").expect("doc");
        assert!(d.text.contains("![photo](../assets/photo_1.png)"));
    }

    #[test]
    fn a_denying_policy_lets_nothing_through() {
        assert_eq!(
            map_block(
                &graph(),
                &block("Book the ferry to the island"),
                &ContentPolicy::denying_all()
            ),
            Err(Skip::Excluded)
        );
    }

    #[test]
    fn exclusions_match_path_prefixes_and_page_names() {
        let mut p = ContentPolicy {
            exclusions: vec!["pages/private/".into()],
            ..ContentPolicy::default()
        };
        assert!(p.is_excluded("pages/private/diary.md", "diary"));
        assert!(!p.is_excluded("pages/public.md", "public"));
        p.exclusions = vec!["Secret".into()];
        assert!(p.is_excluded("pages/Secret.md", "secret"));
        assert!(p.is_excluded("pages/Secret___Plans.md", "Secret/Plans"));
        assert!(!p.is_excluded("pages/Secretary.md", "Secretary"));
        assert_eq!(
            map_block(&graph(), &block("Book the ferry to the island"), &p).map(|d| d.doc_id),
            Ok(doc_id(&graph().id, "6650a1b2-0000-4000-8000-00000000000c"))
        );
        p.exclusions = vec!["trip plan".into()];
        assert_eq!(
            map_block(&graph(), &block("Book the ferry to the island"), &p),
            Err(Skip::Excluded)
        );
    }

    #[test]
    fn private_pages_blocks_and_excluded_tags_are_skipped() {
        let mut b = block("Book the ferry to the island");
        b.page_properties = vec![("Private".into(), "true".into())];
        assert_eq!(
            map_block(&graph(), &b, &ContentPolicy::default()),
            Err(Skip::Private)
        );
        let mut b = block("Book the ferry to the island\nprivate:: true");
        b.properties = vec![("private".into(), "true".into())];
        assert_eq!(
            map_block(&graph(), &b, &ContentPolicy::default()),
            Err(Skip::Private)
        );
        let mut b = block("Book the ferry to the island");
        b.page_properties = vec![("private".into(), "false".into())];
        assert!(map_block(&graph(), &b, &ContentPolicy::default()).is_ok());
        let p = ContentPolicy {
            exclusions: vec!["Travel".into()],
            ..ContentPolicy::default()
        };
        assert_eq!(
            map_block(&graph(), &block("Book the ferry to the island"), &p),
            Err(Skip::Excluded)
        );
    }

    #[test]
    fn a_private_ancestor_hides_the_whole_subtree() {
        let mut b = block("Book the ferry to the island");
        b.ancestor_properties = vec![("private".into(), "true".into())];
        assert_eq!(
            map_block(&graph(), &b, &ContentPolicy::default()),
            Err(Skip::Private)
        );
        b.ancestor_properties = vec![("private".into(), "false".into())];
        assert!(map_block(&graph(), &b, &ContentPolicy::default()).is_ok());
    }

    #[test]
    fn hash_tag_exclusions_match_page_and_block_tags() {
        let p = ContentPolicy {
            exclusions: vec!["#Travel".into()],
            ..ContentPolicy::default()
        };
        // `block()` is tagged `travel`.
        assert_eq!(
            map_block(&graph(), &block("Book the ferry to the island"), &p),
            Err(Skip::Excluded)
        );
        let mut b = block("Book the ferry to the island");
        b.tags = vec!["#TRAVEL".into()];
        assert_eq!(map_block(&graph(), &b, &p), Err(Skip::Excluded));
        b.tags = vec!["holiday".into()];
        assert!(map_block(&graph(), &b, &p).is_ok());
        // A hash entry does not hide a page that is merely named like the tag.
        let mut b = block("Book the ferry to the island");
        b.tags.clear();
        b.page_title = "Travel".into();
        assert!(map_block(&graph(), &b, &p).is_ok());
    }

    #[test]
    fn journals_can_be_switched_off() {
        let mut b = block("Went for a long walk along the beach");
        b.is_journal = true;
        b.journal_day = Some(20_240_501);
        let p = ContentPolicy {
            include_journals: false,
            ..ContentPolicy::default()
        };
        assert_eq!(map_block(&graph(), &b, &p), Err(Skip::Journal));
        let d = map_block(&graph(), &b, &ContentPolicy::default()).expect("doc");
        assert_eq!(d.metadata["journal_day"], json!(20_240_501));
    }
}
