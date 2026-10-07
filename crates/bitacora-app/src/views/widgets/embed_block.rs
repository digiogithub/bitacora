//! Block and page embeds rendered and editable in place (BIT-US-0104).
//!
//! `{{embed ((uuid))}}` shows the block's subtree and `{{embed [[page]]}}` the whole page,
//! inside a frame with a breadcrumb that links to the source. With a live session the embedded
//! outline is a nested [`OutlineEditor`] on the source page, zoomed to the block for a block
//! embed: typing there goes through the core command queue and changes the source file only
//! (the host page just shows the embed). Without a session the rows are read from the index
//! and are read-only.
//!
//! Embeds nest. [`Chain`] refuses a target that is already being drawn above it ("circular
//! embed") and a nesting deeper than [`DEFAULT_MAX_DEPTH`].

use crate::views::dims;
use std::rc::Rc;
use std::time::Duration;

use bitacora_core::graph::PageKey;
use rust_i18n::t;

use super::{Host, element_id, prepare, row_ancestors};
use crate::data::{self, GraphHandle};
use crate::editor::{self, EditorEvent, OutlineEditor};
use crate::nav::OpenIn;
use crate::render::embed::{DEFAULT_MAX_DEPTH, Refusal};
use crate::render::inline::NavTarget;
use crate::render::model::{Row, toggle_row, visible_rows};
use crate::render::widget::EmbedTarget;
use crate::ui::theme::ActiveBitacoraTheme as _;
use crate::ui::{
    ActiveTheme as _, AnyElement, App, AppContext as _, Context, Entity, FluentBuilder as _,
    IconName, InteractiveElement as _, IntoElement, ParentElement as _, Render,
    StatefulInteractiveElement as _, Styled as _, Subscription, Task, Window, div, h_flex, icon,
    v_flex,
};
use crate::views::block_view::{Action, Nav, RowActions, render_block_row};

/// Pause after the last index event before the embed reloads.
pub const REFRESH_DEBOUNCE: Duration = Duration::from_millis(300);

/// Rows read from the index for a read-only page embed.
const STATIC_PAGE_ROWS: usize = 1000;

/// Between the segments of the source breadcrumb.
const CRUMB_SEP: &str = "\u{203a}";

/// What resolving the target found.
#[derive(Debug, Clone, PartialEq)]
struct Resolved {
    page_title: String,
    page_id: Option<i64>,
    file: Option<String>,
    /// Core's key of the page when it is loaded (the embed is editable).
    key: Option<PageKey>,
    /// UUID of the embedded block (block embeds).
    block: Option<String>,
    /// Source breadcrumb: label and where it goes.
    crumbs: Vec<(String, NavTarget)>,
    /// Rows from the index, for the read-only view.
    rows: Vec<Row>,
}

#[derive(Debug, Clone, PartialEq)]
enum State {
    Loading,
    Refused(Refusal),
    Missing,
    Ready(Box<Resolved>),
}

/// Reads what the target is (blocking: call from a background task).
fn resolve(
    handle: &GraphHandle,
    link: Option<&crate::session::SessionLink>,
    target: &EmbedTarget,
) -> Option<Resolved> {
    let (page_title, page_id, file, block, crumbs, rows) = match target {
        EmbedTarget::Page(name) => {
            let page = handle.reader.page_by_name(name).ok().flatten()?;
            let rows = data::open_page(handle, &page.original_name, STATIC_PAGE_ROWS)
                .map(|l| l.rows)
                .unwrap_or_default();
            (
                page.original_name.clone(),
                Some(page.id),
                page.file_path.clone(),
                None,
                vec![(
                    page.original_name.clone(),
                    NavTarget::Page(page.original_name),
                )],
                rows,
            )
        }
        EmbedTarget::Block(uuid) => {
            let block = handle.reader.block(uuid).ok().flatten()?;
            let page = handle.reader.page_by_id(block.page_id).ok().flatten()?;
            let mut crumbs = vec![(
                page.original_name.clone(),
                NavTarget::Page(page.original_name.clone()),
            )];
            for a in handle.reader.ancestors(uuid).unwrap_or_default() {
                crumbs.push((a.title.clone(), NavTarget::Block(a.uuid)));
            }
            let rows = data::zoom_block(handle, uuid)
                .map(|l| l.rows)
                .unwrap_or_default();
            (
                page.original_name.clone(),
                Some(page.id),
                page.file_path.clone(),
                Some(uuid.clone()),
                crumbs,
                rows,
            )
        }
    };
    let key = link.and_then(|l| editor::ensure_loaded(&l.queue, handle, &l.config, &page_title));
    Some(Resolved {
        page_title,
        page_id,
        file,
        key,
        block,
        crumbs,
        rows,
    })
}

/// One embed.
pub struct EmbedBlock {
    host: Host,
    target: EmbedTarget,
    nav: Nav,
    edit: Option<Action>,
    key: String,
    state: State,
    collapsed: bool,
    /// Rows of the read-only view (fold state lives here).
    static_rows: Vec<Row>,
    editor: Option<Entity<OutlineEditor>>,
    /// Creating the editor failed (the block has no `id::` in core): stay read-only.
    live_failed: bool,
    _subs: Vec<Subscription>,
    generation: u64,
    task: Option<Task<()>>,
    refresh: Option<Task<()>>,
}

impl std::fmt::Debug for EmbedBlock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EmbedBlock")
            .field("target", &self.target)
            .field("state", &self.state)
            .finish_non_exhaustive()
    }
}

impl EmbedBlock {
    /// An embed that starts loading at once.
    pub fn new(
        host: Host,
        target: EmbedTarget,
        nav: Nav,
        edit: Option<Action>,
        key: &str,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut this = Self {
            host,
            target,
            nav,
            edit,
            key: key.to_owned(),
            state: State::Loading,
            collapsed: false,
            static_rows: Vec::new(),
            editor: None,
            live_failed: false,
            _subs: Vec::new(),
            generation: 0,
            task: None,
            refresh: None,
        };
        this.load(cx);
        this
    }

    /// Updates what the host knows; reloads when the target, graph or surroundings changed.
    pub fn configure(
        &mut self,
        host: Host,
        target: EmbedTarget,
        nav: Nav,
        edit: Option<Action>,
        key: &str,
        cx: &mut Context<Self>,
    ) {
        self.nav = nav;
        self.edit = edit;
        let changed = host.handle != self.host.handle
            || target != self.target
            || host.chain != self.host.chain
            || host.ancestors != self.host.ancestors;
        self.host = host;
        self.key = key.to_owned();
        if changed {
            self.target = target;
            self.editor = None;
            self._subs.clear();
            self.live_failed = false;
            self.state = State::Loading;
            self.load(cx);
        }
    }

    /// Checks the guards, then reads the target in the background.
    pub fn load(&mut self, cx: &mut Context<Self>) {
        if let Err(r) = self
            .host
            .chain
            .check(&self.target, &self.host.ancestors, DEFAULT_MAX_DEPTH)
        {
            self.state = State::Refused(r);
            self.task = None;
            cx.notify();
            return;
        }
        self.generation += 1;
        let generation = self.generation;
        let (handle, link, target) = (
            self.host.handle.clone(),
            self.host.link.clone(),
            self.target.clone(),
        );
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { resolve(&handle, link.as_ref(), &target) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.task = None;
                match result {
                    Some(r) => {
                        this.static_rows = r.rows.clone();
                        if let Some(ed) = &this.editor {
                            ed.update(cx, |e, cx| e.refresh(cx));
                        }
                        this.state = State::Ready(Box::new(r));
                    }
                    None => this.state = State::Missing,
                }
                cx.notify();
            });
        }));
    }

    /// Why the embed is not shown, when a guard refused it.
    pub fn refusal(&self) -> Option<Refusal> {
        match self.state {
            State::Refused(r) => Some(r),
            _ => None,
        }
    }

    /// The embedded rows as drawn: from the nested editor when live, else from the index.
    pub fn rows(&self, cx: &App) -> Vec<Row> {
        match &self.editor {
            Some(ed) => ed.read(cx).rows().to_vec(),
            None => self.static_rows.clone(),
        }
    }

    /// Whether the embedded rows are editable (a nested editor is attached).
    pub fn is_live(&self) -> bool {
        self.editor.is_some()
    }

    /// The nested editor, when live.
    pub fn editor(&self) -> Option<&Entity<OutlineEditor>> {
        self.editor.as_ref()
    }

    /// The source breadcrumb labels.
    pub fn crumb_labels(&self) -> Vec<String> {
        match &self.state {
            State::Ready(r) => r.crumbs.iter().map(|(l, _)| l.clone()).collect(),
            _ => Vec::new(),
        }
    }

    /// Whether the body is folded away.
    pub fn is_collapsed(&self) -> bool {
        self.collapsed
    }

    /// Folds or unfolds the embed.
    pub fn toggle_collapsed(&mut self, cx: &mut Context<Self>) {
        self.collapsed = !self.collapsed;
        cx.notify();
    }

    /// Reacts to an index change: reloads (debounced) when the source file changed.
    pub fn on_index_event(&mut self, event: &bitacora_index::IndexEvent, cx: &mut Context<Self>) {
        let State::Ready(r) = &self.state else {
            return;
        };
        if !data::event_touches(event, r.page_id, r.file.as_deref()) {
            return;
        }
        self.refresh = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(REFRESH_DEBOUNCE).await;
            let _ = this.update(cx, |this, cx| this.load(cx));
        }));
    }

    fn toggle_static(&mut self, r: usize, cx: &mut Context<Self>) {
        toggle_row(&mut self.static_rows, r);
        cx.notify();
    }

    /// Creates the nested editor once the page is loaded in core.
    fn ensure_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.editor.is_some() || self.live_failed {
            return;
        }
        let (State::Ready(res), Some(link)) = (&self.state, self.host.link.clone()) else {
            return;
        };
        let Some(key) = res.key.clone() else { return };
        let block = res.block.clone();
        let hidden =
            bitacora_core::editor::HiddenKeys::with_extra(link.config.block_hidden_properties());
        let queue = link.queue.clone();
        let ed = cx.new(|cx| OutlineEditor::new(queue, hidden, true, window, cx));
        let handle = self.host.handle.clone();
        let (config, gate) = (link.config.clone(), link.gate.clone());
        ed.update(cx, |e, cx| {
            e.set_handle(handle);
            e.set_config(config);
            e.set_gate(gate);
            e.set_page(key, cx);
        });
        if let Some(uuid) = block {
            let id = ed.read(cx).outline().and_then(|o| {
                o.blocks()
                    .iter()
                    .find(|b| b.uuid.map(|u| u.to_string()).as_deref() == Some(uuid.as_str()))
                    .map(|b| b.id)
            });
            match id {
                Some(id) => ed.update(cx, |e, cx| e.zoom_to(Some(id), cx)),
                None => {
                    // Core's copy of the page has no such block (no `id::` written yet).
                    self.live_failed = true;
                    return;
                }
            }
        }
        self._subs = vec![
            cx.observe(&ed, |_, _, cx| cx.notify()),
            cx.subscribe(&ed, |_, _, _: &EditorEvent, cx| cx.notify()),
        ];
        self.editor = Some(ed);
    }

    fn message(
        &self,
        text: String,
        color: crate::ui::Hsla,
        warn: bool,
        theme: &crate::ui::theme::Theme,
    ) -> AnyElement {
        h_flex()
            .gap_2()
            .items_center()
            .px_2()
            .py(dims::PX_4)
            .text_sm()
            .text_color(color)
            .when(warn, |d| {
                d.child(icon(IconName::TriangleAlert).size(dims::PX_14))
            })
            .child(text)
            .border_1()
            .border_color(theme.border)
            .rounded(dims::PX_6)
            .into_any_element()
    }

    fn render_header(
        &self,
        eid: u64,
        res: &Resolved,
        theme: &crate::ui::theme::Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut bar = h_flex().gap_1().items_center().px_2().py(dims::PX_2).child(
            div()
                .id(("emb-fold", element_id(eid, 1)))
                .cursor_pointer()
                .child(
                    icon(if self.collapsed {
                        IconName::ChevronRight
                    } else {
                        IconName::ChevronDown
                    })
                    .size(dims::PX_14),
                )
                .on_click(cx.listener(|this, _, _, cx| this.toggle_collapsed(cx))),
        );
        for (n, (label, target)) in res.crumbs.iter().enumerate() {
            if n > 0 {
                bar = bar.child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(CRUMB_SEP),
                );
            }
            let nav = self.nav.clone();
            let target = target.clone();
            bar = bar.child(
                div()
                    .id(("emb-crumb", element_id(eid, 10 + n)))
                    .text_xs()
                    .text_color(theme.info)
                    .cursor_pointer()
                    .child(label.chars().take(40).collect::<String>())
                    .on_click(move |_, window, cx| {
                        nav(
                            target.clone(),
                            OpenIn::from_shift(window.modifiers().shift),
                            cx,
                        );
                    }),
            );
        }
        bar = bar.child(div().flex_1());
        if let Some(edit) = self.edit.clone() {
            bar = bar.child(
                div()
                    .id(("emb-edit", element_id(eid, 2)))
                    .px(dims::PX_6)
                    .rounded(dims::PX_4)
                    .text_xs()
                    .cursor_pointer()
                    .text_color(theme.muted_foreground)
                    .hover(|d| d.bg(theme.muted))
                    .child(t!("widgets.edit_source").to_string())
                    .on_click(move |_, window, cx| edit(window, cx)),
            );
        }
        bar.into_any_element()
    }

    fn render_rows(
        &mut self,
        eid: u64,
        res: &Resolved,
        theme: &crate::ui::theme::Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let bt = cx.bitacora().clone();
        let rows = self.rows(cx);
        let visible = visible_rows(&rows);
        let root = Some(self.host.handle.root.clone());
        let this = cx.entity();
        let child_chain = self.host.chain.enter(&self.target, Some(&res.page_title));
        let mut col = v_flex().w_full().pb_1();
        if rows.is_empty() {
            col = col.child(
                div()
                    .px_2()
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child(t!("page.empty").to_string()),
            );
        }
        for r in visible {
            let edit = self
                .editor
                .as_ref()
                .and_then(|ed| OutlineEditor::row_edit(ed, r, cx));
            let child_host = Host {
                handle: self.host.handle.clone(),
                link: self.host.link.clone(),
                scope: format!("{}>", self.key),
                page: res.page_title.clone(),
                chain: child_chain.clone(),
                ancestors: row_ancestors(&rows, r),
            };
            let widgets = prepare(
                &child_host,
                &rows[r],
                &r.to_string(),
                self.nav.clone(),
                super::edit_hook(edit.as_ref()),
                cx,
            );
            let toggle_this = this.clone();
            let actions = RowActions {
                nav: self.nav.clone(),
                toggle: Some(Rc::new(move |_, cx| {
                    toggle_this.update(cx, |v, cx| v.toggle_static(r, cx));
                })),
                referrers: None,
                focus: None,
                edit,
                widgets,
            };
            col = col.child(render_block_row(
                element_id(eid, 5_000 + r),
                &rows[r],
                root.as_deref(),
                theme,
                &bt,
                &actions,
            ));
        }
        match &self.editor {
            Some(ed) => editor::element::wrap(col.into_any_element(), ed, cx),
            None => col.into_any_element(),
        }
    }
}

impl Render for EmbedBlock {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let eid = cx.entity_id().as_u64();
        self.ensure_editor(window, cx);
        if let Some(ed) = self.editor.clone() {
            // The host page may have changed the same page: pick up core's latest copy.
            ed.update(cx, |e, cx| e.refresh(cx));
        }
        let frame = v_flex()
            .w_full()
            .rounded(dims::PX_6)
            .border_l_2()
            .border_color(theme.primary)
            .bg(theme.secondary.opacity(0.5))
            // Presses inside the embed must not put the host block in edit mode.
            .on_mouse_down(crate::ui::text_edit::MouseButton::Left, |_, _, cx| {
                cx.stop_propagation();
            });
        match self.state.clone() {
            State::Loading => frame
                .child(self.message(
                    t!("widgets.embed.loading").to_string(),
                    theme.muted_foreground,
                    false,
                    &theme,
                ))
                .into_any_element(),
            State::Missing => frame
                .child(self.message(
                    t!("widgets.embed.missing").to_string(),
                    theme.warning,
                    true,
                    &theme,
                ))
                .into_any_element(),
            State::Refused(Refusal::Circular) => frame
                .child(self.message(
                    t!("widgets.embed.circular").to_string(),
                    theme.warning,
                    true,
                    &theme,
                ))
                .into_any_element(),
            State::Refused(Refusal::TooDeep) => frame
                .child(self.message(
                    t!("widgets.embed.too_deep", max = DEFAULT_MAX_DEPTH).to_string(),
                    theme.warning,
                    true,
                    &theme,
                ))
                .into_any_element(),
            State::Ready(res) => {
                let mut frame = frame.child(self.render_header(eid, &res, &theme, cx));
                if !self.collapsed {
                    frame = frame.child(self.render_rows(eid, &res, &theme, cx));
                }
                frame.into_any_element()
            }
        }
    }
}
