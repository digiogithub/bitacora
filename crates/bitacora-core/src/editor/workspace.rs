//! The mutable graph state: loaded pages, the id/uuid indexes and the tree primitives the
//! [`Op`](super::op::Op)s are built from. All mutation goes through `Op::apply` (AGENTS.md
//! rule 3); the primitives here are crate-private.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

use uuid::Uuid;

use super::model::{Block, BlockId, IdGen, Page, Position, Subtree};
use super::op::OpError;
use crate::graph::PageKey;
use crate::graph_path::GraphPath;

/// Count of blocks per `id::` uuid plus the net change during the running transaction.
#[derive(Debug, Default)]
struct UuidIndex {
    counts: HashMap<Uuid, u32>,
    delta: HashMap<Uuid, i32>,
}

impl UuidIndex {
    fn add(&mut self, u: Option<Uuid>) {
        if let Some(u) = u {
            *self.counts.entry(u).or_insert(0) += 1;
            *self.delta.entry(u).or_insert(0) += 1;
        }
    }

    fn remove(&mut self, u: Option<Uuid>) {
        if let Some(u) = u {
            if let Some(c) = self.counts.get_mut(&u) {
                *c = c.saturating_sub(1);
                if *c == 0 {
                    self.counts.remove(&u);
                }
            }
            *self.delta.entry(u).or_insert(0) -= 1;
        }
    }
}

/// An edit of a non-page file (`logseq/config.edn`) waiting to be written (see
/// [`Op::EditFile`](super::op::Op::EditFile)).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingEdit {
    /// Content the file is expected to have on disk; the write is refused (reported as a
    /// conflict) when it holds something else.
    pub expected: Arc<[u8]>,
    /// Content to write.
    pub content: Arc<[u8]>,
}

/// The editable graph: every loaded page, with a global block-id index.
#[derive(Debug, Default)]
pub struct Workspace {
    pages: BTreeMap<PageKey, Page>,
    pub(crate) ids: IdGen,
    block_page: HashMap<BlockId, PageKey>,
    uuids: UuidIndex,
    pub(crate) touched: BTreeSet<PageKey>,
    pending_deletes: BTreeMap<GraphPath, Option<Arc<[u8]>>>,
    pending_restores: BTreeSet<GraphPath>,
    pending_creates: BTreeMap<GraphPath, Arc<[u8]>>,
    pending_edits: BTreeMap<GraphPath, PendingEdit>,
    pub(crate) next_tx: u64,
    pub(crate) conflicted: BTreeMap<PageKey, Arc<super::external::ConflictNotice>>,
    pub(crate) editing: Option<BlockId>,
    pub(crate) external_events: Vec<super::external::ExternalEvent>,
    settings: super::settings::EditorSettings,
}

impl Workspace {
    /// An empty workspace.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Editing preferences used by the command planners.
    #[must_use]
    pub fn settings(&self) -> super::settings::EditorSettings {
        self.settings
    }

    /// Replaces the editing preferences (call when `config.edn` changes).
    pub fn set_settings(&mut self, settings: super::settings::EditorSettings) {
        self.settings = settings;
    }

    /// Allocates a fresh [`BlockId`] (usable from pure planners).
    #[must_use]
    pub fn alloc_id(&self) -> BlockId {
        self.ids.next()
    }

    /// Loads (or replaces) a page from bytes. Not an edit: nothing is dirty afterwards.
    pub fn load_page(
        &mut self,
        key: PageKey,
        title: &str,
        path: Option<GraphPath>,
        bytes: &[u8],
    ) -> &Page {
        let page = Page::load(key.clone(), title, path, bytes, &self.ids);
        self.insert_page(page);
        &self.pages[&key]
    }

    /// Inserts a ready page, replacing any page with the same key.
    pub fn insert_page(&mut self, page: Page) {
        self.remove_page(&page.key);
        for b in page.blocks.values() {
            self.block_page.insert(b.id, page.key.clone());
            self.uuids.add(b.uuid);
        }
        self.pages.insert(page.key.clone(), page);
    }

    /// Removes a page and its index entries.
    pub fn remove_page(&mut self, key: &PageKey) -> Option<Page> {
        let page = self.pages.remove(key)?;
        self.conflicted.remove(key);
        for b in page.blocks.values() {
            self.block_page.remove(&b.id);
            self.uuids.remove(b.uuid);
        }
        Some(page)
    }

    /// A loaded page.
    #[must_use]
    pub fn page(&self, key: &PageKey) -> Option<&Page> {
        self.pages.get(key)
    }

    /// All loaded pages.
    pub fn pages(&self) -> impl Iterator<Item = &Page> {
        self.pages.values()
    }

    /// The loaded page backed by `path`.
    #[must_use]
    pub fn page_for_path(&self, path: &GraphPath) -> Option<&PageKey> {
        self.pages
            .values()
            .find(|p| p.path.as_ref() == Some(path) || p.disk_path.as_ref() == Some(path))
            .map(|p| &p.key)
    }

    /// Page holding a block.
    #[must_use]
    pub fn locate(&self, id: BlockId) -> Option<&PageKey> {
        self.block_page.get(&id)
    }

    /// A block anywhere in the workspace.
    #[must_use]
    pub fn block(&self, id: BlockId) -> Option<&Block> {
        self.pages.get(self.block_page.get(&id)?)?.block(id)
    }

    /// Current slot of a block (index among its siblings, block included).
    #[must_use]
    pub fn position_of(&self, id: BlockId) -> Option<Position> {
        let page = self.block_page.get(&id)?;
        let b = self.block(id)?;
        let list = self.pages.get(page)?.children_of(b.parent)?;
        Some(Position {
            page: page.clone(),
            parent: b.parent,
            index: list.iter().position(|c| *c == id)?,
        })
    }

    /// True when `node` is `ancestor` or lies below it.
    #[must_use]
    pub fn is_within(&self, ancestor: BlockId, node: BlockId) -> bool {
        let mut cur = Some(node);
        let mut guard = 0usize;
        while let Some(c) = cur {
            if c == ancestor {
                return true;
            }
            guard += 1;
            if guard > self.block_page.len() + 1 {
                return false;
            }
            cur = self.block(c).and_then(|b| b.parent);
        }
        false
    }

    /// Pages that need a write (content changed or file renamed).
    #[must_use]
    pub fn dirty_pages(&self) -> Vec<PageKey> {
        self.pages
            .values()
            .filter(|p| p.needs_write())
            .map(|p| p.key.clone())
            .collect()
    }

    /// Files of deleted pages that still have to be removed from disk.
    #[must_use]
    pub fn pending_deletes(&self) -> &BTreeMap<GraphPath, Option<Arc<[u8]>>> {
        &self.pending_deletes
    }

    /// Attachments that still have to be written (pasted or dropped files).
    #[must_use]
    pub fn pending_creates(&self) -> &BTreeMap<GraphPath, Arc<[u8]>> {
        &self.pending_creates
    }

    /// Files whose recycled copy has to be moved back (undo of an asset delete).
    #[must_use]
    pub fn pending_restores(&self) -> &BTreeSet<GraphPath> {
        &self.pending_restores
    }

    /// Edits of non-page files that still have to be written.
    #[must_use]
    pub fn pending_edits(&self) -> &BTreeMap<GraphPath, PendingEdit> {
        &self.pending_edits
    }

    /// Queues an edit of a non-page file from `before` to `after`. A second edit of the same file
    /// must start from the first one's result; going back to the on-disk content drops the entry.
    pub(crate) fn queue_edit(
        &mut self,
        path: &GraphPath,
        before: &[u8],
        after: &[u8],
    ) -> Result<(), OpError> {
        match self.pending_edits.get_mut(path) {
            Some(e) => {
                if *e.content != *before {
                    return Err(OpError::Stale("file content differs from `before`"));
                }
                if *e.expected == *after {
                    self.pending_edits.remove(path);
                } else {
                    e.content = Arc::from(after);
                }
            }
            None => {
                if before != after {
                    self.pending_edits.insert(
                        path.clone(),
                        PendingEdit {
                            expected: Arc::from(before),
                            content: Arc::from(after),
                        },
                    );
                }
            }
        }
        Ok(())
    }

    pub(crate) fn finish_edit(&mut self, path: &GraphPath) {
        self.pending_edits.remove(path);
    }

    /// Gives a page a new key and title (blocks keep their ids). With `from == to` only the title
    /// changes (case-only rename).
    pub(crate) fn rekey_page(
        &mut self,
        from: &PageKey,
        to: &PageKey,
        title: &str,
    ) -> Result<(), OpError> {
        if from != to && self.pages.contains_key(to) {
            return Err(OpError::PageExists(to.clone()));
        }
        self.writable_page(from)?;
        let mut page = self
            .pages
            .remove(from)
            .ok_or_else(|| OpError::UnknownPage(from.clone()))?;
        page.key = to.clone();
        page.title = title.to_owned();
        for id in page.blocks.keys() {
            self.block_page.insert(*id, to.clone());
        }
        self.pages.insert(to.clone(), page);
        self.touched.insert(from.clone());
        self.mark_touched(to);
        Ok(())
    }

    pub(crate) fn queue_create(&mut self, path: GraphPath, bytes: Arc<[u8]>) {
        self.pending_creates.insert(path, bytes);
    }

    /// Cancels a pending create; `true` when there was one.
    pub(crate) fn unqueue_create(&mut self, path: &GraphPath) -> bool {
        self.pending_creates.remove(path).is_some()
    }

    pub(crate) fn finish_create(&mut self, path: &GraphPath) {
        self.pending_creates.remove(path);
    }

    pub(crate) fn queue_restore(&mut self, path: GraphPath) {
        self.pending_restores.insert(path);
    }

    pub(crate) fn unqueue_restore(&mut self, path: &GraphPath) {
        self.pending_restores.remove(path);
    }

    pub(crate) fn finish_restore(&mut self, path: &GraphPath) {
        self.pending_restores.remove(path);
    }

    pub(crate) fn pages_mut(&mut self) -> &mut BTreeMap<PageKey, Page> {
        &mut self.pages
    }

    pub(crate) fn queue_delete(&mut self, path: GraphPath, expected: Option<Arc<[u8]>>) {
        self.pending_deletes.insert(path, expected);
    }

    pub(crate) fn unqueue_delete(&mut self, path: &GraphPath) {
        self.pending_deletes.remove(path);
    }

    pub(crate) fn finish_delete(&mut self, path: &GraphPath) {
        self.pending_deletes.remove(path);
    }

    pub(crate) fn begin_tx(&mut self) {
        self.touched.clear();
        self.uuids.delta.clear();
    }

    /// Uuids whose count grew during the running transaction beyond one.
    pub(crate) fn new_duplicate_uuids(&self) -> Vec<Uuid> {
        self.uuids
            .delta
            .iter()
            .filter(|(u, d)| **d > 0 && self.uuids.counts.get(*u).copied().unwrap_or(0) > 1)
            .map(|(u, _)| *u)
            .collect()
    }

    pub(crate) fn uuid_count(&self, u: &Uuid) -> u32 {
        self.uuids.counts.get(u).copied().unwrap_or(0)
    }

    pub(crate) fn writable_page(&mut self, key: &PageKey) -> Result<&mut Page, OpError> {
        let p = self
            .pages
            .get_mut(key)
            .ok_or_else(|| OpError::UnknownPage(key.clone()))?;
        if p.read_only {
            return Err(OpError::ReadOnly(key.clone()));
        }
        Ok(p)
    }

    pub(crate) fn mark_touched(&mut self, key: &PageKey) {
        if let Some(p) = self.pages.get_mut(key) {
            p.dirty = true;
        }
        self.touched.insert(key.clone());
    }

    /// Inserts `st` as child `index` of `parent` in `page`.
    pub(crate) fn attach(
        &mut self,
        page: &PageKey,
        parent: Option<BlockId>,
        index: usize,
        st: Subtree,
    ) -> Result<(), OpError> {
        let p = self.writable_page(page)?;
        let len = p
            .children_of(parent)
            .ok_or(OpError::UnknownBlock(parent.unwrap_or(st.id)))?
            .len();
        if index > len {
            return Err(OpError::BadIndex { index, len });
        }
        for id in st.ids() {
            if self.block_page.contains_key(&id) {
                return Err(OpError::DuplicateBlock(id));
            }
        }
        let root = st.id;
        let mut flat = Vec::new();
        flatten(st, parent, &mut flat);
        let mut added = Vec::new();
        for mut b in flat {
            b.refresh_uuid();
            added.push((b.id, b.uuid));
            if let Some(p) = self.pages.get_mut(page) {
                p.blocks.insert(b.id, b);
            }
        }
        for (id, u) in added {
            self.block_page.insert(id, page.clone());
            self.uuids.add(u);
        }
        if let Some(p) = self.pages.get_mut(page) {
            match parent {
                None => p.roots.insert(index, root),
                Some(par) => {
                    if let Some(pb) = p.blocks.get_mut(&par) {
                        pb.children.insert(index, root);
                    }
                }
            }
        }
        self.mark_touched(page);
        Ok(())
    }

    /// Detaches the subtree rooted at `id`, returning its slot and content.
    pub(crate) fn detach(&mut self, id: BlockId) -> Result<(Position, Subtree), OpError> {
        let pos = self.position_of(id).ok_or(OpError::UnknownBlock(id))?;
        self.writable_page(&pos.page)?;
        if let Some(p) = self.pages.get_mut(&pos.page) {
            match pos.parent {
                None => {
                    p.roots.remove(pos.index);
                }
                Some(par) => {
                    if let Some(pb) = p.blocks.get_mut(&par) {
                        pb.children.remove(pos.index);
                    }
                }
            }
        }
        let st = self.take(&pos.page, id);
        self.mark_touched(&pos.page);
        Ok((pos, st))
    }

    fn take(&mut self, page: &PageKey, id: BlockId) -> Subtree {
        let block = self.pages.get_mut(page).and_then(|p| p.blocks.remove(&id));
        self.block_page.remove(&id);
        let Some(b) = block else {
            return Subtree::new(id, "");
        };
        self.uuids.remove(b.uuid);
        let children = b.children.iter().map(|c| self.take(page, *c)).collect();
        Subtree {
            id,
            text: b.text,
            origin: b.origin,
            children,
        }
    }

    /// Replaces a block's text, keeping the uuid index in sync.
    pub(crate) fn set_text_raw(&mut self, id: BlockId, text: String) -> Result<(), OpError> {
        let key = self
            .block_page
            .get(&id)
            .cloned()
            .ok_or(OpError::UnknownBlock(id))?;
        let p = self.writable_page(&key)?;
        let b = p.blocks.get_mut(&id).ok_or(OpError::UnknownBlock(id))?;
        let loaded = b
            .origin
            .as_ref()
            .is_some_and(|o| o.text_hash == super::model::text_hash(&text));
        if !loaded && !super::model::text_is_representable(&text) {
            return Err(OpError::Unrepresentable);
        }
        let old = b.uuid;
        b.text = text;
        b.refresh_uuid();
        let new = b.uuid;
        self.uuids.remove(old);
        self.uuids.add(new);
        self.mark_touched(&key);
        Ok(())
    }

    /// Moves `ids` (a contiguous run of children of `from`) under `to` at `at`.
    pub(crate) fn reparent(
        &mut self,
        page: &PageKey,
        from: BlockId,
        to: BlockId,
        at: usize,
        ids: &[BlockId],
    ) -> Result<usize, OpError> {
        let p = self.writable_page(page)?;
        let fb = p.blocks.get(&from).ok_or(OpError::UnknownBlock(from))?;
        let src_at = if ids.is_empty() {
            0
        } else {
            fb.children
                .iter()
                .position(|c| *c == ids[0])
                .ok_or(OpError::UnknownBlock(ids[0]))?
        };
        if fb.children.get(src_at..src_at + ids.len()) != Some(ids) {
            return Err(OpError::Invalid(
                "adopted children are not a contiguous run",
            ));
        }
        let tb = p.blocks.get(&to).ok_or(OpError::UnknownBlock(to))?;
        if at > tb.children.len() {
            return Err(OpError::BadIndex {
                index: at,
                len: tb.children.len(),
            });
        }
        if let Some(fb) = p.blocks.get_mut(&from) {
            fb.children.drain(src_at..src_at + ids.len());
        }
        if let Some(tb) = p.blocks.get_mut(&to) {
            for (k, id) in ids.iter().enumerate() {
                tb.children.insert(at + k, *id);
            }
        }
        for id in ids {
            if let Some(b) = p.blocks.get_mut(id) {
                b.parent = Some(to);
            }
        }
        self.mark_touched(page);
        Ok(src_at)
    }
}

fn flatten(st: Subtree, parent: Option<BlockId>, out: &mut Vec<Block>) {
    let idx = out.len();
    out.push(Block {
        id: st.id,
        parent,
        children: st.children.iter().map(|c| c.id).collect(),
        text: st.text,
        uuid: None,
        origin: st.origin,
    });
    debug_assert_eq!(out.len(), idx + 1);
    for c in st.children {
        flatten(c, Some(st.id), out);
    }
}
