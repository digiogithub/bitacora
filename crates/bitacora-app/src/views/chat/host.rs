//! What the chat gives the backend and takes from the workspace: the [`FrontendHost`] the agent
//! drives the UI through (`open_page`, `get_selection`), the [`PageResolver`] that lets
//! `propose_edit` load pages on demand and address blocks that have no persisted `id::`, and the
//! context the user attaches to a question (BIT-US-0150).
//!
//! Nothing here writes to the graph. The resolver gives blocks without `id::` the uuid the index
//! (and so the agent) knows them by *in memory*; no `id::` property is ever written into a user
//! file for it.

use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use bitacora_core::editor::BlockId;
use bitacora_core::graph::PageKey;
use bitacora_core::queue::{CommandQueue, PageSnapshot};
use bitacora_runtime::ai::{AttachedBlock, ContentGuard, FrontendHost, IndexedBlock, PageResolver};
use serde_json::{Value, json};

use crate::data::GraphHandle;
use crate::ui::App;

/// How long `get_selection` waits for the UI thread.
const SELECTION_TIMEOUT: Duration = Duration::from_secs(3);
/// Most blocks of a page or selection kept as attached context (the guard caps what is sent).
const MAX_ATTACHED: usize = 200;

/// A page or a selection of its blocks, as the editor held it when the user attached it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageBlocks {
    /// Page title.
    pub page: String,
    /// Graph-relative path of its file (empty for a page that has no file yet).
    pub file_path: String,
    /// Page-properties text (tags and privacy are read from it).
    pub preamble: Option<String>,
    /// `(persisted uuid, text)` of each block.
    pub blocks: Vec<(Option<String>, String)>,
}

impl PageBlocks {
    /// Reads `snap`; `ids` limits it to those blocks (a selection), `None` takes the whole page.
    #[must_use]
    pub fn from_snapshot(snap: &PageSnapshot, ids: Option<&[BlockId]>) -> Self {
        Self {
            page: snap.title.clone(),
            file_path: snap
                .path
                .as_ref()
                .map(|p| p.as_str().to_owned())
                .unwrap_or_default(),
            preamble: snap.preamble.clone(),
            blocks: snap
                .blocks
                .iter()
                .filter(|b| ids.is_none_or(|ids| ids.contains(&b.id)))
                // A private block hides its subtree: never attach a block under one.
                .filter(|b| !bitacora_runtime::ai::under_private_block(&snap.blocks, b.id))
                .take(MAX_ATTACHED)
                .map(|b| (b.uuid.map(|u| u.to_string()), b.text.clone()))
                .collect(),
        }
    }

    /// The blocks as agent context, before the guard filters them.
    #[must_use]
    pub fn attached(&self, guard: &ContentGuard) -> Vec<AttachedBlock> {
        guard.page_blocks(
            &self.page,
            &self.file_path,
            self.preamble.as_deref(),
            &self.blocks,
        )
    }
}

/// Which chip a [`ContextItem`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextKind {
    /// The page on screen.
    Page,
    /// The journal page of a day.
    Journal,
    /// The blocks the user selected.
    Selection,
}

/// One attached item: it is exactly what the composer chip shows and what is sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextItem {
    /// Kind of chip.
    pub kind: ContextKind,
    /// The content, frozen when it was attached.
    pub data: PageBlocks,
}

impl ContextItem {
    /// Chip text: the page title, or the page title with the number of selected blocks.
    #[must_use]
    pub fn label(&self) -> String {
        match self.kind {
            ContextKind::Selection => format!("{} ({})", self.data.page, self.data.blocks.len()),
            ContextKind::Page | ContextKind::Journal => self.data.page.clone(),
        }
    }

    /// Whether `other` attaches the same thing.
    #[must_use]
    pub fn same_as(&self, other: &Self) -> bool {
        self.kind == other.kind && self.data.page == other.data.page
    }
}

/// Every attached item as the blocks sent with a message (guard not applied yet).
#[must_use]
pub fn attached_blocks(items: &[ContextItem], guard: &ContentGuard) -> Vec<AttachedBlock> {
    items.iter().flat_map(|i| i.data.attached(guard)).collect()
}

/// What the workspace can offer to attach right now.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChatContext {
    /// The page on screen, and whether it is a journal page.
    pub page: Option<(PageBlocks, bool)>,
    /// The selected blocks.
    pub selection: Option<PageBlocks>,
}

/// Asks the workspace for the current [`ChatContext`]. Only call it from the chat view's own
/// handlers (clicks, the event task), never while the workspace is being updated.
pub type ContextProvider = Rc<dyn Fn(&mut App) -> ChatContext>;

/// A request from the agent to the UI thread.
#[derive(Debug)]
pub enum HostRequest {
    /// Show the page.
    OpenPage(String),
    /// Reply with the selected blocks.
    Selection(std::sync::mpsc::Sender<Result<PageBlocks, String>>),
}

/// The app's [`FrontendHost`]: forwards to the UI thread through a channel and answers the agent
/// without waiting for anything slow.
pub struct AppHost {
    tx: async_channel::Sender<HostRequest>,
    resolver: Arc<AppResolver>,
    guard: Arc<std::sync::Mutex<Option<ContentGuard>>>,
}

impl std::fmt::Debug for AppHost {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AppHost")
    }
}

impl AppHost {
    /// A host that sends its requests to `tx`; `guard` is the live consent gate the view keeps
    /// current.
    #[must_use]
    pub fn new(
        tx: async_channel::Sender<HostRequest>,
        resolver: Arc<AppResolver>,
        guard: Arc<std::sync::Mutex<Option<ContentGuard>>>,
    ) -> Self {
        Self {
            tx,
            resolver,
            guard,
        }
    }
}

impl FrontendHost for AppHost {
    fn open_page(&self, name: &str) -> Result<Value, String> {
        if !self.resolver.page_exists(name) {
            return Err(format!("there is no page named `{name}`"));
        }
        self.tx
            .send_blocking(HostRequest::OpenPage(name.to_owned()))
            .map_err(|_| "the editor is closed".to_owned())?;
        Ok(json!({"opened": name}))
    }

    fn get_selection(&self) -> Result<Vec<AttachedBlock>, String> {
        let (reply, rx) = std::sync::mpsc::channel();
        self.tx
            .send_blocking(HostRequest::Selection(reply))
            .map_err(|_| "the editor is closed".to_owned())?;
        let data = rx
            .recv_timeout(SELECTION_TIMEOUT)
            .map_err(|_| "the editor did not answer".to_owned())??;
        let guard = self
            .guard
            .lock()
            .map_err(|_| "consent state unavailable".to_owned())?
            .clone()
            .ok_or_else(|| "consent state unavailable".to_owned())?;
        Ok(data.attached(&guard))
    }
}

/// [`PageResolver`] over the open graph: loads pages the index knows into the writer and maps
/// index uuids to the blocks of a loaded page.
#[derive(Clone)]
pub struct AppResolver {
    queue: CommandQueue,
    handle: GraphHandle,
    config: Arc<bitacora_config::EffectiveConfig>,
}

impl std::fmt::Debug for AppResolver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AppResolver")
    }
}

impl AppResolver {
    /// A resolver for the graph of `handle` and its writer `queue`.
    #[must_use]
    pub fn new(
        queue: CommandQueue,
        handle: GraphHandle,
        config: Arc<bitacora_config::EffectiveConfig>,
    ) -> Self {
        Self {
            queue,
            handle,
            config,
        }
    }

    /// The page exists in the graph (it has a file) or is already loaded.
    #[must_use]
    pub fn page_exists(&self, page: &str) -> bool {
        self.queue.snapshot(&PageKey::from_title(page)).is_some()
            || self
                .handle
                .reader
                .page_by_name(page)
                .ok()
                .flatten()
                .is_some_and(|p| !p.is_placeholder())
    }
}

impl PageResolver for AppResolver {
    fn ensure_loaded(&self, page: &str) -> bool {
        if self.queue.snapshot(&PageKey::from_title(page)).is_some() {
            return true;
        }
        // Never create a page for an agent: only pages that have a file in the graph.
        if !self.page_exists(page) {
            return false;
        }
        crate::editor::ensure_loaded(&self.queue, &self.handle, &self.config, page).is_some()
    }

    fn indexed_blocks(&self, page: &str) -> Vec<IndexedBlock> {
        let Some(row) = self.handle.reader.page_by_name(page).ok().flatten() else {
            return Vec::new();
        };
        self.handle
            .reader
            .outline(row.id, 0, 100_000, false)
            .map(|rows| {
                rows.into_iter()
                    .filter(|b| !b.is_pre_block)
                    .map(|b| IndexedBlock {
                        uuid: b.uuid,
                        text: b.content,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bitacora_config::pando::GraphConsent;

    fn guard(exclusions: &[&str]) -> ContentGuard {
        ContentGuard::from_consent(&GraphConsent {
            granted: true,
            exclusions: exclusions.iter().map(|s| (*s).to_owned()).collect(),
            ..GraphConsent::default()
        })
    }

    fn item(page: &str, kind: ContextKind) -> ContextItem {
        ContextItem {
            kind,
            data: PageBlocks {
                page: page.into(),
                file_path: format!("pages/{page}.md"),
                preamble: None,
                blocks: vec![(Some("u1".into()), "alpha".into()), (None, "beta".into())],
            },
        }
    }

    #[test]
    fn only_attached_items_become_context_and_the_guard_filters_them() {
        let items = vec![
            item("Open", ContextKind::Page),
            item("Secret", ContextKind::Page),
        ];
        let g = guard(&["Secret"]);
        let attached = attached_blocks(&items, &g);
        assert_eq!(attached.len(), 4);
        // What reaches `RunAgentInput.context` is exactly the allowed attached blocks.
        let entries = g.context_entries(&attached);
        assert_eq!(entries.len(), 2);
        assert!(entries.iter().all(|e| e.description.contains("[[Open]]")));
        assert!(attached_blocks(&[], &g).is_empty());
    }

    #[test]
    fn labels_and_identity() {
        let sel = item("P", ContextKind::Selection);
        assert_eq!(sel.label(), "P (2)");
        assert_eq!(item("P", ContextKind::Page).label(), "P");
        assert!(!sel.same_as(&item("P", ContextKind::Page)));
        assert!(sel.same_as(&item("P", ContextKind::Selection)));
    }
}
