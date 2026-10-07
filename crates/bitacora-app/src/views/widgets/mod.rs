//! Query and embed widgets inside block rows (BIT-US-0102, BIT-US-0104).
//!
//! Drawing a block row has no `App` access, so the host view (page view, journals feed, an
//! embed) first calls [`prepare`]: it finds or creates the GPUI entity of every widget of the
//! row and returns a closure that only wraps those entities. Entities live in a global
//! [`Registry`] keyed by where the widget is (scope, row, position) so that their state
//! (collapse, sort, loaded results) survives a redraw; index events reach them through
//! [`on_index_event`].

pub mod embed_block;
pub mod query_block;

use std::collections::HashMap;
use std::rc::Rc;

use bitacora_index::IndexEvent;

use crate::data::GraphHandle;
use crate::editor::RowEdit;
use crate::render::embed::Chain;
use crate::render::model::{BodyItem, Row};
use crate::render::widget::Widget;
use crate::session::SessionLink;
use crate::ui::{AnyElement, App, AppContext as _, Entity, Global, IntoElement as _, Window};
use crate::views::block_view::{Action, Nav, WidgetBuild};

pub use embed_block::EmbedBlock;
pub use query_block::QueryBlock;

/// Widgets kept alive at most; the least recently used are dropped beyond it.
const REGISTRY_CAP: usize = 256;

/// Where a row is drawn: what the widgets in it need to know.
#[derive(Debug, Clone)]
pub struct Host {
    /// The graph to read.
    pub handle: GraphHandle,
    /// The live session (embeds are editable with it).
    pub link: Option<SessionLink>,
    /// Namespace of the registry keys (a page, or an embed of one).
    pub scope: String,
    /// Title of the page the row belongs to (`:current-page`).
    pub page: String,
    /// What is being drawn around the row (cycle guard).
    pub chain: Chain,
    /// Idents of the blocks that enclose the row (and of the row itself).
    pub ancestors: Vec<String>,
}

impl Host {
    /// The host of row `r` of page `title` whose rows are `rows`.
    pub fn for_page(
        handle: GraphHandle,
        link: Option<SessionLink>,
        title: &str,
        rows: &[Row],
        r: usize,
    ) -> Self {
        Self {
            handle,
            link,
            scope: format!("page:{title}"),
            page: title.to_owned(),
            chain: Chain::page(title),
            ancestors: row_ancestors(rows, r),
        }
    }
}

/// `block:<uuid>` of row `r` and of every ancestor that has an `id::`.
pub fn row_ancestors(rows: &[Row], r: usize) -> Vec<String> {
    let mut out = Vec::new();
    let Some(row) = rows.get(r) else { return out };
    if let Some(u) = &row.uuid {
        out.push(format!("block:{u}"));
    }
    let mut depth = row.depth;
    for earlier in rows[..r].iter().rev() {
        if depth == 0 {
            break;
        }
        if earlier.depth < depth {
            depth = earlier.depth;
            if let Some(u) = &earlier.uuid {
                out.push(format!("block:{u}"));
            }
        }
    }
    out
}

/// The "edit the block's source" callback of a row, from its editor state.
pub fn edit_hook(edit: Option<&RowEdit>) -> Option<Action> {
    edit.map(|e| {
        let hook = e.on_text.clone();
        Rc::new(move |window: &mut Window, cx: &mut App| hook(usize::MAX, false, window, cx))
            as Action
    })
}

#[derive(Default)]
struct Registry {
    queries: HashMap<String, (Entity<QueryBlock>, u64)>,
    embeds: HashMap<String, (Entity<EmbedBlock>, u64)>,
    tick: u64,
}

impl Global for Registry {}

impl Registry {
    fn trim(&mut self) {
        let total = self.queries.len() + self.embeds.len();
        if total <= REGISTRY_CAP {
            return;
        }
        let mut stamps: Vec<u64> = self
            .queries
            .values()
            .map(|(_, t)| *t)
            .chain(self.embeds.values().map(|(_, t)| *t))
            .collect();
        stamps.sort_unstable();
        let cut = stamps[total - REGISTRY_CAP + REGISTRY_CAP / 4];
        self.queries.retain(|_, (_, t)| *t >= cut);
        self.embeds.retain(|_, (_, t)| *t >= cut);
    }
}

fn with_registry<R>(cx: &mut App, f: impl FnOnce(&mut Registry) -> R) -> R {
    if !cx.has_global::<Registry>() {
        cx.set_global(Registry::default());
    }
    f(cx.global_mut::<Registry>())
}

/// Finds or creates the widgets of `row` and returns what draws them (`None` without any).
/// `row_key` tells rows of the same host apart; `edit` opens the block's source for editing.
pub fn prepare(
    host: &Host,
    row: &Row,
    row_key: &str,
    nav: Nav,
    edit: Option<Action>,
    cx: &mut App,
) -> Option<WidgetBuild> {
    let mut built: Vec<(usize, Rc<dyn Fn() -> AnyElement>)> = Vec::new();
    for (n, item) in row.block.body.iter().enumerate() {
        let BodyItem::Widget(widget) = item else {
            continue;
        };
        let key = format!("{}|{}|{}", host.scope, row_key, n);
        let draw: Rc<dyn Fn() -> AnyElement> = match widget {
            Widget::Query(spec) => {
                let scope = crate::render::query::Scope {
                    page: Some(host.page.clone()),
                    block: row.uuid.clone().or_else(|| row.block.id.clone()),
                };
                let entity = query_entity(&key, host, spec, scope, nav.clone(), edit.clone(), cx);
                Rc::new(move || entity.clone().into_any_element())
            }
            Widget::Embed(target) => {
                let entity = embed_entity(&key, host, target, nav.clone(), edit.clone(), cx);
                Rc::new(move || entity.clone().into_any_element())
            }
        };
        built.push((n, draw));
    }
    if built.is_empty() {
        return None;
    }
    Some(Rc::new(move |n| {
        built
            .iter()
            .find(|(i, _)| *i == n)
            .map_or_else(|| crate::ui::div().into_any_element(), |(_, d)| d())
    }))
}

fn query_entity(
    key: &str,
    host: &Host,
    spec: &crate::render::widget::QuerySpec,
    scope: crate::render::query::Scope,
    nav: Nav,
    edit: Option<Action>,
    cx: &mut App,
) -> Entity<QueryBlock> {
    let found = with_registry(cx, |r| {
        r.tick += 1;
        let tick = r.tick;
        r.queries.get_mut(key).map(|(e, t)| {
            *t = tick;
            e.clone()
        })
    });
    if let Some(entity) = found {
        entity.update(cx, |q, cx| {
            q.configure(host.handle.clone(), spec.clone(), scope, nav, edit, cx);
        });
        entity.update(cx, |q, _| q.set_link(host.link.clone()));
        return entity;
    }
    let entity =
        cx.new(|cx| QueryBlock::new(host.handle.clone(), spec.clone(), scope, nav, edit, cx));
    entity.update(cx, |q, _| q.set_link(host.link.clone()));
    with_registry(cx, |r| {
        let tick = r.tick;
        r.queries.insert(key.to_owned(), (entity.clone(), tick));
        r.trim();
    });
    entity
}

fn embed_entity(
    key: &str,
    host: &Host,
    target: &crate::render::widget::EmbedTarget,
    nav: Nav,
    edit: Option<Action>,
    cx: &mut App,
) -> Entity<EmbedBlock> {
    let found = with_registry(cx, |r| {
        r.tick += 1;
        let tick = r.tick;
        r.embeds.get_mut(key).map(|(e, t)| {
            *t = tick;
            e.clone()
        })
    });
    if let Some(entity) = found {
        entity.update(cx, |e, cx| {
            e.configure(host.clone(), target.clone(), nav, edit, key, cx);
        });
        return entity;
    }
    let entity = cx.new(|cx| EmbedBlock::new(host.clone(), target.clone(), nav, edit, key, cx));
    with_registry(cx, |r| {
        let tick = r.tick;
        r.embeds.insert(key.to_owned(), (entity.clone(), tick));
        r.trim();
    });
    entity
}

/// Tells every widget that the index changed: the ones on screen refresh (debounced), the
/// others only remember to.
pub fn on_index_event(event: &IndexEvent, cx: &mut App) {
    if !cx.has_global::<Registry>() {
        return;
    }
    let (queries, embeds): (Vec<_>, Vec<_>) = {
        let r = cx.global::<Registry>();
        (
            r.queries.values().map(|(e, _)| e.clone()).collect(),
            r.embeds.values().map(|(e, _)| e.clone()).collect(),
        )
    };
    for q in queries {
        q.update(cx, |q, cx| q.on_index_event(event, cx));
    }
    for e in embeds {
        e.update(cx, |e, cx| e.on_index_event(event, cx));
    }
}

/// The block being edited changed on disk: the editors inside the widgets look at it.
pub fn on_editing_conflict(conflict: &bitacora_core::editor::EditingConflict, cx: &mut App) {
    if !cx.has_global::<Registry>() {
        return;
    }
    let (queries, embeds): (Vec<_>, Vec<_>) = {
        let r = cx.global::<Registry>();
        (
            r.queries.values().map(|(e, _)| e.clone()).collect(),
            r.embeds.values().map(|(e, _)| e.clone()).collect(),
        )
    };
    for q in queries {
        q.update(cx, |q, cx| q.on_editing_conflict(conflict, cx));
    }
    for e in embeds {
        let Some(ed) = e.read(cx).editor().cloned() else {
            continue;
        };
        let (block, mine, disk) = (conflict.block, conflict.mine.clone(), conflict.disk.clone());
        ed.update(cx, |ed, cx| ed.on_editing_conflict(block, mine, disk, cx));
    }
}

/// Forgets every widget (the graph was closed).
pub fn clear(cx: &mut App) {
    if cx.has_global::<Registry>() {
        cx.set_global(Registry::default());
    }
}

/// Ids of interactive elements inside widgets must not collide with the host's (they are keyed
/// by row index) nor with other widgets: a high marker, the entity and the position.
pub(crate) fn element_id(entity: u64, part: usize) -> usize {
    (1usize << 36) | (((entity & 0xFFF) as usize) << 22) | (part & 0x3F_FFFF)
}

/// Every embed alive (tests).
#[cfg(test)]
pub(crate) fn embeds(cx: &App) -> Vec<Entity<EmbedBlock>> {
    cx.global::<Registry>()
        .embeds
        .values()
        .map(|(e, _)| e.clone())
        .collect()
}

#[cfg(test)]
pub(crate) mod tests;

#[cfg(test)]
mod unit {
    use super::*;

    fn row(depth: usize, uuid: Option<&str>) -> Row {
        Row {
            depth,
            uuid: uuid.map(str::to_owned),
            ..Row::default()
        }
    }

    #[test]
    fn ancestors_list_the_row_and_enclosing_blocks_with_ids() {
        let rows = vec![
            row(0, Some("a")),
            row(1, None),
            row(2, Some("c")),
            row(1, Some("d")),
            row(2, Some("e")),
        ];
        assert_eq!(row_ancestors(&rows, 2), ["block:c", "block:a"]);
        assert_eq!(row_ancestors(&rows, 4), ["block:e", "block:d", "block:a"]);
        assert_eq!(row_ancestors(&rows, 0), ["block:a"]);
        assert!(row_ancestors(&rows, 9).is_empty());
    }

    #[test]
    fn element_ids_differ_per_entity_and_part() {
        assert_ne!(element_id(1, 0), element_id(2, 0));
        assert_ne!(element_id(1, 0), element_id(1, 1));
    }
}
