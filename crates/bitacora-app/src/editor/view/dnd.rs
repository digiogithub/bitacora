//! Drag and drop of blocks in the editor (BIT-US-0106): the drop hint under the pointer, the
//! move or Alt-drop reference command, auto-scroll near the viewport edges and the refresh of the
//! other pages that showed the moved blocks.

use std::time::Duration;

use bitacora_core::editor::{BlockId, Cmd};

use super::{Context, EditorEvent, Entity, OutlineEditor, Selection, Window};
use crate::editor::dnd::{BlockDrag, DropZone, inside, scroll_step, zone_at};
use crate::ui::{App, Bounds, Global, Pixels, Point, WeakEntity};

/// Auto-scroll tick.
const TICK: Duration = Duration::from_millis(16);

/// Every editor of the window, so a move that changes another page refreshes it at once.
#[derive(Default)]
pub struct EditorRegistry(Vec<WeakEntity<OutlineEditor>>);

impl Global for EditorRegistry {}

impl EditorRegistry {
    /// Adds `editor`.
    pub fn register(cx: &mut App, editor: WeakEntity<OutlineEditor>) {
        let registry = cx.default_global::<Self>();
        registry.0.retain(|w| w.upgrade().is_some());
        registry.0.push(editor);
    }

    fn peers(cx: &mut App) -> Vec<WeakEntity<OutlineEditor>> {
        cx.default_global::<Self>().0.clone()
    }
}

impl OutlineEditor {
    /// What dragging the bullet of block `id` carries: the selected top-level blocks when `id`
    /// is one of them, otherwise just `id`.
    pub fn drag_payload(&self, id: BlockId) -> BlockDrag {
        let ids = if self.has_selection() && self.selected_blocks().contains(&id) {
            self.targets()
        } else {
            vec![id]
        };
        let label = self
            .outline
            .as_ref()
            .and_then(|o| o.block(*ids.first().unwrap_or(&id)))
            .map(|b| b.text.lines().next().unwrap_or("").trim().to_owned())
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| "Block".to_owned());
        BlockDrag {
            ids,
            label: label.into(),
        }
    }

    /// The zone to draw as the drop indicator on row `r`.
    pub fn drop_zone(&self, r: usize) -> Option<DropZone> {
        let id = self.ids.get(r)?;
        self.drop_hint.filter(|(b, _)| b == id).map(|(_, z)| z)
    }

    /// The pointer moved at `pos` while dragging `drag`; row `r` occupies `bounds`.
    pub fn drag_move(
        &mut self,
        r: usize,
        drag: &BlockDrag,
        pos: Point<Pixels>,
        bounds: Bounds<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let Some(id) = self.ids.get(r).copied() else {
            return;
        };
        let next = if inside(&bounds, pos) && self.can_drop_on(drag, id) {
            let local = Point {
                x: pos.x - bounds.origin.x,
                y: pos.y - bounds.origin.y,
            };
            let row = self.rows.get(r);
            let visible_children = row.is_some_and(|row| row.has_children && !row.is_collapsed());
            let depth = row.map_or(0, |row| row.depth);
            Some((
                id,
                zone_at(local, bounds.size.height, depth, visible_children),
            ))
        } else if self.drop_hint.is_some_and(|(b, _)| b == id) {
            None
        } else {
            return;
        };
        if self.drop_hint != next {
            self.drop_hint = next;
            cx.notify();
        }
    }

    /// Whether `drag` may land next to or in `target`: never inside its own subtrees.
    fn can_drop_on(&self, drag: &BlockDrag, target: BlockId) -> bool {
        let Some(outline) = &self.outline else {
            return true;
        };
        !drag
            .ids
            .iter()
            .any(|d| outline.index_of(*d).is_some() && outline.is_within(*d, target))
    }

    /// The pointer left every row or the drag ended: no hint, no auto-scroll.
    pub fn drag_cancel(&mut self, cx: &mut Context<Self>) {
        self.scroll_dir = 0.;
        if self.drop_hint.take().is_some() {
            cx.notify();
        }
    }

    /// `drag` was released on row `r`: moves the blocks there (or, with Alt, inserts references
    /// to them) as one undoable transaction.
    pub fn drop_on(
        &mut self,
        r: usize,
        drag: &BlockDrag,
        alt: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(id) = self.ids.get(r).copied() else {
            return;
        };
        let zone = self
            .drop_hint
            .filter(|(b, _)| *b == id)
            .map_or(DropZone::After, |(_, z)| z);
        self.drag_cancel(cx);
        self.drag_anchor = None;
        let target = zone.target(id);
        let (label, cmd) = if alt {
            (
                "Insert block references",
                Cmd::DropBlockRef {
                    sources: drag.ids.clone(),
                    target,
                },
            )
        } else {
            (
                "Move blocks",
                Cmd::MoveBlocks {
                    ids: drag.ids.clone(),
                    target,
                },
            )
        };
        self.exit_edit(cx);
        if self.run(label, cmd, window, cx).is_none() {
            return;
        }
        if !alt {
            // The moved blocks stay selected (when this page shows them).
            let first = drag.ids.first().copied();
            let last = drag.ids.last().copied();
            self.sel = match (first, last) {
                (Some(a), Some(b)) if self.row_of(a).is_some() && self.row_of(b).is_some() => {
                    Selection {
                        anchor: Some(a),
                        head: Some(b),
                    }
                }
                _ => Selection::default(),
            };
            self.focus_handle.focus(window, cx);
        }
        self.refresh_peers(cx);
        cx.notify();
    }

    /// Other editors (the sidebar, other journal days) pick up what this one changed.
    fn refresh_peers(&mut self, cx: &mut Context<Self>) {
        let me = cx.entity_id();
        for peer in EditorRegistry::peers(cx) {
            if peer.entity_id() != me {
                let _ = peer.update(cx, |e, cx| e.refresh(cx));
            }
        }
    }

    /// The pointer is at `pos` inside the scroll viewport `viewport` while dragging: near the top
    /// or bottom edge the host scrolls (a repeating tick, so it keeps going while the pointer
    /// rests there).
    pub fn autoscroll(
        &mut self,
        pos: Point<Pixels>,
        viewport: Bounds<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let dir = scroll_step(pos.y, viewport);
        self.scroll_dir = dir;
        if dir != 0. && self.scroll_task.is_none() {
            self.scroll_task = Some(cx.spawn(async move |this, cx| {
                loop {
                    cx.background_executor().timer(TICK).await;
                    let go = this
                        .update(cx, |this, cx| {
                            if this.scroll_dir == 0. || !cx.has_active_drag() {
                                this.scroll_dir = 0.;
                                this.scroll_task = None;
                                return false;
                            }
                            cx.emit(EditorEvent::Scroll(this.scroll_dir));
                            true
                        })
                        .unwrap_or(false);
                    if !go {
                        break;
                    }
                }
            }));
        }
    }

    /// Row `r`'s drag callbacks for the row renderer.
    pub(super) fn row_drag(
        editor: &Entity<Self>,
        r: usize,
        id: BlockId,
        cx: &App,
    ) -> crate::editor::row::RowDrag {
        use std::rc::Rc;
        let this = editor.read(cx);
        let move_ed = editor.clone();
        let drop_ed = editor.clone();
        crate::editor::row::RowDrag {
            payload: this.drag_payload(id),
            zone: this.drop_zone(r),
            depth: this.rows.get(r).map_or(0, |row| row.depth),
            on_move: Rc::new(move |drag, pos, bounds, _window, cx| {
                move_ed.update(cx, |this, cx| this.drag_move(r, drag, pos, bounds, cx));
            }),
            on_drop: Rc::new(move |drag, window, cx| {
                let alt = window.modifiers().alt;
                drop_ed.update(cx, |this, cx| this.drop_on(r, drag, alt, window, cx));
            }),
        }
    }
}
