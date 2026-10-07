//! Inline editing of blocks shown outside their own page (BIT-US-0168).
//!
//! Linked and unlinked references, the zoomed block of a sidebar item, query results and the
//! Tasks view all draw blocks that belong to *another* page, from the index. A click on the
//! paragraph of such a block puts it in edit mode, like in Logseq: [`RemoteEditors`] keeps one
//! [`OutlineEditor`] per source page (created on the first click, the page is loaded into the
//! core workspace on demand), maps the index row to the core block and hands the row renderer
//! the editor's [`RowEdit`]. Typing therefore goes through the single-writer command queue as
//! `EditText` transactions (undoable, with the external-change and hash checks of core) and
//! the rendered row follows the editor, so every view of the block shows the new text.
//!
//! Links inside the text keep navigating: [`crate::views::block_view`] leaves presses on links
//! to the link and only calls the click hook for the plain text around them.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use bitacora_core::editor::BlockId;

use crate::data::GraphHandle;
use crate::editor::row::{Hook, TextHook};
use crate::editor::{self, EditorEvent, OutlineEditor, RowEdit};
use crate::render::model::{BlockModel, Row};
use crate::session::SessionLink;
use crate::ui::{AnyElement, App, AppContext as _, Context, Entity, Subscription, Window};

/// Identity of a block drawn from the index: enough to find it in core's copy of its page.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BlockRef {
    /// Title of the page the block lives on.
    pub page: String,
    /// Index uuid; equals the `id::` of blocks that have one.
    pub uuid: Option<String>,
    /// Position of the block in the file as the index counts it (pre-block included).
    pub ord: Option<usize>,
    /// Raw text from the index: the position is only trusted while core's text is the same.
    pub content: Option<String>,
    /// Weaker check for callers that only know part of the text (the Tasks view).
    pub contains: Option<String>,
}

impl BlockRef {
    /// The identity of an index row of page `page`. Rows without a position (the page
    /// properties pre-block, rows read from files) cannot be edited in place.
    pub fn of_row(page: &str, row: &Row) -> Option<Self> {
        row.block_index?;
        if page.is_empty() || (row.uuid.is_none() && row.content.is_none()) {
            return None;
        }
        Some(Self {
            page: page.to_owned(),
            uuid: row.uuid.clone(),
            ord: row.block_index,
            content: row.content.clone(),
            contains: None,
        })
    }
}

/// What a host draws for one remote row.
pub struct RemoteRow {
    /// The row to draw: the index row with the text of core's copy once an editor exists.
    pub row: Row,
    /// Edit state of the row; `None` until the first click created the editor.
    pub edit: Option<RowEdit>,
    /// Click on the text before the editor exists: creates it and enters edit mode.
    pub activate: Option<TextHook>,
    /// The block is in edit mode (wrap the drawn row with [`RemoteEditors::wrap`]).
    pub editing: bool,
}

impl std::fmt::Debug for RemoteRow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RemoteRow")
            .field("editing", &self.editing)
            .field("activatable", &self.activate.is_some())
            .finish_non_exhaustive()
    }
}

struct Slot {
    ed: Entity<OutlineEditor>,
    /// Blocks the host draws for this page: the editor never leaves them (no invisible edit).
    shown: Rc<RefCell<HashSet<BlockId>>>,
    _subs: Vec<Subscription>,
}

/// Host-side hook run when an editor changed (re-measure the list).
pub type ChangeHook<V> = Rc<dyn Fn(&mut V, &mut Context<V>)>;

/// The editors of the source pages of the blocks a view `V` draws.
pub struct RemoteEditors<V: 'static> {
    get: fn(&mut V) -> &mut RemoteEditors<V>,
    on_change: Option<ChangeHook<V>>,
    link: Option<SessionLink>,
    handle: Option<GraphHandle>,
    slots: HashMap<String, Slot>,
}

impl<V: 'static> std::fmt::Debug for RemoteEditors<V> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RemoteEditors")
            .field("pages", &self.slots.keys().collect::<Vec<_>>())
            .finish_non_exhaustive()
    }
}

impl<V: 'static> RemoteEditors<V> {
    /// An unconnected set; `get` finds it inside the host (the click hooks run on the host).
    pub fn new(get: fn(&mut V) -> &mut RemoteEditors<V>, on_change: Option<ChangeHook<V>>) -> Self {
        Self {
            get,
            on_change,
            link: None,
            handle: None,
            slots: HashMap::new(),
        }
    }

    /// Connects to the live session and graph. Forgets the editors when either changed.
    pub fn configure(&mut self, link: Option<SessionLink>, handle: Option<GraphHandle>) {
        if self.handle != handle || self.link.is_some() != link.is_some() {
            self.slots.clear();
        }
        self.link = link;
        self.handle = handle;
    }

    /// Whether blocks can be edited in place (a live session is attached).
    pub fn enabled(&self) -> bool {
        self.link.is_some() && self.handle.is_some()
    }

    /// The editor of source page `page`, once a click created it.
    pub fn editor(&self, page: &str) -> Option<&Entity<OutlineEditor>> {
        self.slots.get(page).map(|s| &s.ed)
    }

    /// Whether any of the editors has a block in edit mode.
    pub fn any_editing(&self, cx: &App) -> bool {
        self.slots
            .values()
            .any(|s| s.ed.read(cx).editing().is_some())
    }

    /// Hands an external change of the edited block to the editors (BIT-T-0344).
    pub fn on_editing_conflict(
        &self,
        conflict: &bitacora_core::editor::EditingConflict,
        cx: &mut Context<V>,
    ) {
        for slot in self.slots.values() {
            let (block, mine, disk) =
                (conflict.block, conflict.mine.clone(), conflict.disk.clone());
            slot.ed
                .update(cx, |e, cx| e.on_editing_conflict(block, mine, disk, cx));
        }
    }

    /// Finds the block in core's copy of its page: `(row of the editor, block id)`.
    fn locate(ed: &OutlineEditor, target: &BlockRef) -> Option<(usize, BlockId)> {
        let outline = ed.outline()?;
        let blocks = outline.blocks();
        let by_uuid = target.uuid.as_deref().and_then(|u| {
            blocks
                .iter()
                .find(|b| b.uuid.is_some_and(|x| x.to_string() == u))
        });
        let found = by_uuid.or_else(|| {
            let preamble = usize::from(outline.snapshot().preamble.is_some());
            let block = blocks.get(target.ord?.checked_sub(preamble)?)?;
            let same = match (&target.content, &target.contains) {
                (Some(c), _) => block.text.trim_end() == c.trim_end(),
                (None, Some(part)) => block.text.contains(part.as_str()),
                (None, None) => false,
            };
            same.then_some(block)
        })?;
        let r = ed.block_ids().iter().position(|id| *id == found.id)?;
        Some((r, found.id))
    }

    /// What to draw for `row` of page `page`. `toggle` is the host's view-only fold and
    /// `bullet` what the bullet does (the editor's own versions would change the file or zoom
    /// an editor nobody sees). `None` when the row cannot be edited in place.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare(
        &mut self,
        host: &Entity<V>,
        page: &str,
        row: &Row,
        toggle: Option<Hook>,
        bullet: Option<Hook>,
        window: &mut Window,
        cx: &mut Context<V>,
    ) -> Option<RemoteRow> {
        if !self.enabled() {
            return None;
        }
        let target = BlockRef::of_row(page, row)?;
        self.prepare_target(host, &target, row, toggle, bullet, window, cx)
    }

    /// [`Self::prepare`] for a caller that built the [`BlockRef`] itself.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_target(
        &mut self,
        host: &Entity<V>,
        target: &BlockRef,
        row: &Row,
        toggle: Option<Hook>,
        bullet: Option<Hook>,
        window: &mut Window,
        cx: &mut Context<V>,
    ) -> Option<RemoteRow> {
        if !self.enabled() {
            return None;
        }
        // Planning chips (SCHEDULED/DEADLINE) are only clickable on rows built by an editor:
        // blocks that have them get their page's editor at once instead of on the first click.
        if !row.block.planning.is_empty() && !self.slots.contains_key(&target.page) {
            self.ensure(&target.page, window, cx);
        }
        let activate = self.activate_hook(host, target.clone());
        let live = self.slots.get(&target.page).and_then(|slot| {
            // Core's copy may have moved on (another editor, an MCP write).
            slot.ed.update(cx, |e, cx| e.refresh(cx));
            let (r, id) = Self::locate(slot.ed.read(cx), target)?;
            slot.shown.borrow_mut().insert(id);
            Some((slot.ed.clone(), r))
        });
        let Some((ed, r)) = live else {
            return Some(RemoteRow {
                row: row.clone(),
                edit: None,
                activate: Some(activate),
                editing: false,
            });
        };
        let mut edit = OutlineEditor::row_edit(&ed, r, cx)?;
        if let Some(toggle) = toggle {
            edit.on_toggle = toggle;
        }
        edit.on_bullet = bullet.unwrap_or_else(|| Rc::new(|_, _| {}));
        // Dragging over the row must not select blocks of an editor nobody sees.
        edit.on_drag = Rc::new(|_, _| {});
        let editing = edit.editing.is_some();
        let mut shown = row.clone();
        if let Some(live) = ed.read(cx).rows().get(r) {
            shown.block = live.block.clone();
        }
        Some(RemoteRow {
            row: shown,
            edit: Some(edit),
            activate: None,
            editing,
        })
    }

    /// The model of the block as core holds it, when an editor exists (what a card shows).
    pub fn live_block(&self, target: &BlockRef, cx: &App) -> Option<BlockModel> {
        let ed = self.slots.get(&target.page)?.ed.read(cx);
        let (r, _) = Self::locate(ed, target)?;
        ed.rows().get(r).map(|r| r.block.clone())
    }

    fn activate_hook(&self, host: &Entity<V>, target: BlockRef) -> TextHook {
        let host = host.clone();
        let get = self.get;
        Rc::new(move |offset, shift, window, cx| {
            host.update(cx, |v, cx| {
                get(v).activate(&target, offset, shift, window, cx);
            });
        })
    }

    /// The first click on a block: loads its page into core, creates the page's editor and
    /// puts the block in edit mode with the caret at `offset` (`usize::MAX` = end).
    pub fn activate(
        &mut self,
        target: &BlockRef,
        offset: usize,
        shift: bool,
        window: &mut Window,
        cx: &mut Context<V>,
    ) {
        let Some(ed) = self.ensure(&target.page, window, cx) else {
            tracing::warn!(page = %target.page, "cannot load the page to edit a block in place");
            return;
        };
        let Some((r, id)) = Self::locate(ed.read(cx), target) else {
            tracing::debug!(page = %target.page, "block moved since it was indexed");
            return;
        };
        if let Some(slot) = self.slots.get(&target.page) {
            slot.shown.borrow_mut().insert(id);
        }
        ed.update(cx, |e, cx| e.click_row(r, offset, shift, window, cx));
        cx.notify();
    }

    fn ensure(
        &mut self,
        page: &str,
        window: &mut Window,
        cx: &mut Context<V>,
    ) -> Option<Entity<OutlineEditor>> {
        if let Some(slot) = self.slots.get(page) {
            slot.ed.update(cx, |e, cx| e.refresh(cx));
            return Some(slot.ed.clone());
        }
        let (link, handle) = (self.link.clone()?, self.handle.clone()?);
        let key = editor::ensure_loaded(&link.queue, &handle, &link.config, page)?;
        let hidden =
            bitacora_core::editor::HiddenKeys::with_extra(link.config.block_hidden_properties());
        let settings = bitacora_core::editor::EditorSettings::from_config(&link.config);
        let queue = link.queue.clone();
        let ed = cx.new(|cx| OutlineEditor::new(queue, hidden, true, window, cx));
        ed.read(cx).apply_settings(settings);
        let (config, gate) = (link.config.clone(), link.gate.clone());
        ed.update(cx, |e, cx| {
            e.set_handle(handle);
            e.set_config(config);
            e.set_gate(gate);
            e.set_page(key, cx);
        });
        let shown: Rc<RefCell<HashSet<BlockId>>> = Rc::default();
        let guard = shown.clone();
        let on_change = self.on_change.clone();
        let on_event = self.on_change.clone();
        let subs = vec![
            cx.observe(&ed, move |v, ed, cx| {
                // Enter or the arrows moved the caret to a block that is not on screen: leave
                // edit mode instead of typing into a block nobody can see.
                let lost = ed
                    .read(cx)
                    .editing()
                    .is_some_and(|id| !guard.borrow().contains(&id));
                if lost {
                    ed.update(cx, |e, cx| e.exit_edit(cx));
                }
                if let Some(hook) = &on_change {
                    hook(v, cx);
                }
                cx.notify();
            }),
            cx.subscribe(&ed, move |v, _, event: &EditorEvent, cx| {
                if matches!(event, EditorEvent::Structure | EditorEvent::Row(_))
                    && let Some(hook) = &on_event
                {
                    hook(v, cx);
                }
                cx.notify();
            }),
        ];
        self.slots.insert(
            page.to_owned(),
            Slot {
                ed: ed.clone(),
                shown,
                _subs: subs,
            },
        );
        Some(ed)
    }

    /// Wraps a drawn row that is in edit mode with the editor's key context and actions.
    pub fn wrap(&self, page: &str, element: AnyElement, cx: &App) -> AnyElement {
        match self.editor(page) {
            Some(ed) => editor::element::wrap(element, ed, cx),
            None => element,
        }
    }
}
