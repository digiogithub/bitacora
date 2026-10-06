//! The visual per-block conflict resolver (BIT-US-0054, BIT-T-0367/0368/0369).
//!
//! Conflicts come from the merge state the sync engine persists (`.git/bitacora/merge-state.json`,
//! [`ConflictRecord`]); resolutions go back through `Session::resolve_conflict` /
//! `resolve_conflict_page`, which apply them through the command queue and, after the last
//! one, commit and push the merge. The view never writes files and never shows git markers:
//! every card is "mine | theirs" rendered with a word-level diff against the base.
//!
//! The data model ([`ConflictCard`], [`actions_for`], [`Progress`]) is plain Rust so the state
//! transitions are tested without a window.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bitacora_sync::merge::{ConflictRecord, ConflictType, Resolution};
use bitacora_sync::state::MergeStateStore;
use bitacora_sync::store::JsonMergeStore;
use rust_i18n::t;

use crate::session::SessionHandle;
use crate::ui::button::{Button, ButtonVariants as _};
use crate::ui::input::{Textarea, TextareaState};
use crate::ui::progress::Progress as ProgressBar;
use crate::ui::{
    ActiveTheme as _, AppContext as _, Context, Entity, EventEmitter, FluentBuilder as _,
    FocusHandle, Focusable, IconName, InteractiveElement as _, IntoElement, KeyDownEvent,
    ParentElement as _, Render, Sizable as _, StatefulInteractiveElement as _, Styled as _, Task,
    Window, div, h_flex, px, v_flex,
};
use crate::views::modal::{modal, title_bar, word_diff};
use crate::views::sync_panel::ago;

/// What a card offers the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    /// Keep my side.
    Mine,
    /// Keep their side.
    Theirs,
    /// Keep both (theirs as the next sibling; for a rename, the other title as an alias).
    Both,
    /// Write the result by hand.
    Edit,
}

impl Action {
    /// The choice sent to the engine; `text` is the edited text for [`Action::Edit`].
    pub fn resolution(self, text: &str) -> Resolution {
        match self {
            Self::Mine => Resolution::Ours,
            Self::Theirs => Resolution::Theirs,
            Self::Both => Resolution::Both,
            Self::Edit => Resolution::Edit(text.to_owned()),
        }
    }
}

/// One conflict as the view shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConflictCard {
    /// Conflict id.
    pub id: String,
    /// Graph-relative page or file.
    pub path: String,
    /// Kind.
    pub kind: ConflictType,
    /// Conflicting field.
    pub field: Option<String>,
    /// `id::` of the conflicting block, when it has one.
    pub block_key: Option<String>,
    /// Ancestor titles and the block's first line.
    pub breadcrumb: Vec<String>,
    /// Base text.
    pub base: Option<String>,
    /// Our text.
    pub ours: Option<String>,
    /// Their text.
    pub theirs: Option<String>,
    /// Which side deleted the block or file (`"Ours"` / `"Theirs"`).
    pub deleted_by: Option<String>,
    /// Suggested alias for a rename conflict.
    pub suggestion: Option<String>,
    /// Device that made their commit.
    pub author: Option<String>,
    /// Their commit time (Unix seconds).
    pub time: Option<i64>,
    /// What the user chose, once decided.
    pub resolution: Option<Resolution>,
}

impl From<&ConflictRecord> for ConflictCard {
    fn from(r: &ConflictRecord) -> Self {
        Self {
            id: r.id.clone(),
            path: r.path.clone(),
            kind: r.kind,
            field: r.field.clone(),
            block_key: r.block_key.clone(),
            breadcrumb: r.breadcrumb.clone(),
            base: r.base.clone(),
            ours: r.ours.clone(),
            theirs: r.theirs.clone(),
            deleted_by: r.deleted_by.map(|s| format!("{s:?}")),
            suggestion: r.suggestion.clone(),
            author: r.theirs_author.clone(),
            time: r.theirs_time,
            resolution: r.resolution.clone(),
        }
    }
}

/// The actions a card of this kind offers, in display order.
pub fn actions_for(kind: ConflictType) -> &'static [Action] {
    match kind {
        ConflictType::Content | ConflictType::Property => {
            &[Action::Mine, Action::Theirs, Action::Both, Action::Edit]
        }
        ConflictType::DeleteVsModify | ConflictType::FileDeleteVsModify => {
            &[Action::Mine, Action::Theirs, Action::Edit]
        }
        ConflictType::Config | ConflictType::Text => &[Action::Mine, Action::Theirs, Action::Edit],
        ConflictType::RenameRename => &[Action::Mine, Action::Theirs, Action::Both],
        ConflictType::ExternalMarkers => &[Action::Edit],
    }
}

impl ConflictCard {
    /// Button label of `action` on this card (delete-vs-modify cards say what happens).
    pub fn label(&self, action: Action) -> String {
        let deleted = matches!(
            self.kind,
            ConflictType::DeleteVsModify | ConflictType::FileDeleteVsModify
        );
        match (action, deleted) {
            (Action::Mine | Action::Theirs, true) => {
                let mine = action == Action::Mine;
                let i_deleted = self.deleted_by.as_deref() == Some("Ours");
                // Keeping the deleting side deletes; keeping the other side keeps the edit.
                if mine == i_deleted {
                    t!("conflicts.delete").to_string()
                } else {
                    t!("conflicts.keep_modified").to_string()
                }
            }
            (Action::Mine, _) => t!("conflicts.keep_mine").to_string(),
            (Action::Theirs, _) => t!("conflicts.keep_theirs").to_string(),
            (Action::Both, _) if self.kind == ConflictType::RenameRename => {
                t!("conflicts.keep_both_alias").to_string()
            }
            (Action::Both, _) => t!("conflicts.keep_both").to_string(),
            (Action::Edit, _) => t!("conflicts.edit").to_string(),
        }
    }

    /// Text the inline editor starts with: my side, never markers.
    pub fn edit_seed(&self) -> String {
        self.ours.clone().unwrap_or_default()
    }

    /// "Kept mine" and friends, for a decided card.
    pub fn resolution_label(&self) -> Option<String> {
        self.resolution.as_ref().map(|r| {
            match r {
                Resolution::Ours => t!("conflicts.chose_mine"),
                Resolution::Theirs => t!("conflicts.chose_theirs"),
                Resolution::Both => t!("conflicts.chose_both"),
                Resolution::Edit(_) => t!("conflicts.chose_edit"),
            }
            .to_string()
        })
    }
}

/// How far the user is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Progress {
    /// Decided conflicts.
    pub resolved: usize,
    /// All conflicts of the merge.
    pub total: usize,
}

impl Progress {
    /// Progress of `cards`.
    pub fn of(cards: &[ConflictCard]) -> Self {
        Self {
            resolved: cards.iter().filter(|c| c.resolution.is_some()).count(),
            total: cards.len(),
        }
    }

    /// Percentage for the bar.
    pub fn percent(self) -> f32 {
        if self.total == 0 {
            100.0
        } else {
            self.resolved as f32 * 100.0 / self.total as f32
        }
    }

    /// Every conflict is decided: the engine commits the merge.
    pub fn complete(self) -> bool {
        self.resolved == self.total
    }
}

/// Reads the conflicts of the pending merge of the graph at `root`.
pub fn load_cards(root: &Path) -> Vec<ConflictCard> {
    JsonMergeStore::for_graph(root)
        .and_then(|store| store.load())
        .map(|pending| pending.conflicts.iter().map(ConflictCard::from).collect())
        .unwrap_or_default()
}

/// What the resolver tells the workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConflictsEvent {
    /// The overlay closed.
    Closed,
    /// A page was decided; its on-screen copy should refresh.
    Resolved,
}

/// The overlay.
pub struct ConflictsView {
    open: bool,
    root: Option<PathBuf>,
    handle: Option<SessionHandle>,
    cards: Vec<ConflictCard>,
    cursor: usize,
    editing: Option<String>,
    editor: Entity<TextareaState>,
    busy: bool,
    message: Option<String>,
    focus: FocusHandle,
    task: Option<Task<()>>,
}

impl std::fmt::Debug for ConflictsView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConflictsView")
            .field("open", &self.open)
            .field("cards", &self.cards.len())
            .finish_non_exhaustive()
    }
}

impl EventEmitter<ConflictsEvent> for ConflictsView {}

impl Focusable for ConflictsView {
    fn focus_handle(&self, _: &crate::ui::App) -> FocusHandle {
        self.focus.clone()
    }
}

impl ConflictsView {
    /// A closed view.
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            open: false,
            root: None,
            handle: None,
            cards: Vec::new(),
            cursor: 0,
            editing: None,
            editor: cx.new(|cx| TextareaState::new(window, cx)),
            busy: false,
            message: None,
            focus: cx.focus_handle(),
            task: None,
        }
    }

    /// Whether the overlay is showing.
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// The cards, grouped by page in file order.
    pub fn cards(&self) -> &[ConflictCard] {
        &self.cards
    }

    /// Resolved / total.
    pub fn progress(&self) -> Progress {
        Progress::of(&self.cards)
    }

    /// The status line.
    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }

    /// The id of the card being edited.
    pub fn editing(&self) -> Option<&str> {
        self.editing.as_deref()
    }

    /// Index of the highlighted card.
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Opens the resolver for the graph at `root`.
    pub fn open_for(
        &mut self,
        root: PathBuf,
        handle: SessionHandle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open = true;
        self.root = Some(root);
        self.handle = Some(handle);
        self.editing = None;
        self.message = None;
        self.reload(cx);
        window.focus(&self.focus, cx);
        cx.notify();
    }

    /// Re-reads the merge state (after a resolution or a sync status change).
    pub fn reload(&mut self, cx: &mut Context<Self>) {
        let Some(root) = &self.root else { return };
        let mut cards = load_cards(root);
        // Group by page; the engine already numbers conflicts in file order.
        cards.sort_by(|a, b| a.path.cmp(&b.path).then(a.id.cmp(&b.id)));
        self.cards = cards;
        self.cursor = self.cursor.min(self.cards.len().saturating_sub(1));
        if self.open && self.cards.is_empty() && self.message.is_none() {
            self.message = Some(t!("conflicts.none").to_string());
        }
        cx.notify();
    }

    /// Closes the overlay.
    pub fn close(&mut self, cx: &mut Context<Self>) {
        if self.open {
            self.open = false;
            self.editing = None;
            self.task = None;
            cx.emit(ConflictsEvent::Closed);
            cx.notify();
        }
    }

    /// Moves the highlight to the next card that is not decided yet (or just the next one).
    pub fn move_cursor(&mut self, forward: bool, cx: &mut Context<Self>) {
        let n = self.cards.len();
        if n == 0 {
            return;
        }
        let step = |i: usize| {
            if forward {
                (i + 1) % n
            } else {
                (i + n - 1) % n
            }
        };
        let mut next = step(self.cursor);
        for _ in 0..n {
            if self.cards[next].resolution.is_none() {
                break;
            }
            next = step(next);
        }
        self.cursor = next;
        cx.notify();
    }

    /// Applies `action` to the card `ix`.
    pub fn apply(
        &mut self,
        ix: usize,
        action: Action,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(card) = self.cards.get(ix).cloned() else {
            return;
        };
        if self.busy {
            return;
        }
        self.cursor = ix;
        if action == Action::Edit {
            let seed = card.edit_seed();
            self.editor
                .update(cx, |e, cx| e.set_value(seed, window, cx));
            self.editing = Some(card.id.clone());
            let focus = self.editor.read(cx).focus_handle(cx);
            window.focus(&focus, cx);
            cx.notify();
            return;
        }
        self.resolve(card.id, action.resolution(""), cx);
    }

    /// Saves the inline editor as the resolution of the card being edited.
    pub fn save_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.editing.take() else {
            return;
        };
        let text = self.editor.read(cx).value().to_string();
        window.focus(&self.focus, cx);
        self.resolve(id, Action::Edit.resolution(&text), cx);
    }

    /// Leaves the inline editor without resolving.
    pub fn cancel_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.editing.take().is_some() {
            window.focus(&self.focus, cx);
            cx.notify();
        }
    }

    fn resolve(&mut self, id: String, resolution: Resolution, cx: &mut Context<Self>) {
        let Some(handle) = self.handle.clone() else {
            return;
        };
        self.busy = true;
        self.message = None;
        let rx = handle.run(move |s| {
            s.resolve_conflict(&id, resolution)
                .map_err(|e| e.to_string())
        });
        self.finish(rx, cx);
    }

    /// Decides every open conflict of `path` with one choice (`mine` or theirs).
    pub fn resolve_page(&mut self, path: &str, mine: bool, cx: &mut Context<Self>) {
        let Some(handle) = self.handle.clone() else {
            return;
        };
        if self.busy {
            return;
        }
        self.busy = true;
        self.message = None;
        let path = path.to_owned();
        let resolution = if mine {
            Resolution::Ours
        } else {
            Resolution::Theirs
        };
        let rx = handle.run(move |s| {
            s.resolve_conflict_page(&path, resolution)
                .map_err(|e| e.to_string())
        });
        self.finish(rx, cx);
    }

    fn finish(
        &mut self,
        rx: async_channel::Receiver<Result<bitacora_sync::resolve::ResolveOutcome, String>>,
        cx: &mut Context<Self>,
    ) {
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = rx.recv().await;
            let _ = this.update(cx, |view, cx| {
                view.busy = false;
                match result {
                    Ok(Ok(outcome)) => {
                        view.message = Some(if outcome.remaining == 0 {
                            t!("conflicts.finished").to_string()
                        } else {
                            t!("conflicts.remaining", count = outcome.remaining).to_string()
                        });
                        cx.emit(ConflictsEvent::Resolved);
                    }
                    Ok(Err(e)) => view.message = Some(e),
                    Err(_) => view.message = Some(t!("history.session_gone").to_string()),
                }
                view.reload(cx);
                if view.progress().complete() && view.progress().total > 0 {
                    view.cursor = 0;
                } else {
                    view.move_cursor(true, cx);
                }
            });
        }));
        cx.notify();
    }

    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.editing.is_some() {
            if event.keystroke.key == "escape" {
                self.cancel_edit(window, cx);
            }
            return;
        }
        let ix = self.cursor;
        match event.keystroke.key.as_str() {
            "j" | "down" => self.move_cursor(true, cx),
            "k" | "up" => self.move_cursor(false, cx),
            "m" => self.apply(ix, Action::Mine, window, cx),
            "t" => self.apply(ix, Action::Theirs, window, cx),
            "b" => self.apply(ix, Action::Both, window, cx),
            "e" => self.apply(ix, Action::Edit, window, cx),
            "escape" => self.close(cx),
            _ => {}
        }
    }
}

fn side_text(
    theme: &crate::ui::theme::Theme,
    text: Option<&str>,
    base: Option<&str>,
    highlight: crate::ui::Hsla,
) -> crate::ui::AnyElement {
    match text {
        None => div()
            .text_sm()
            .text_color(theme.muted_foreground)
            .child(t!("conflicts.deleted").to_string())
            .into_any_element(),
        Some(text) => {
            let mut line = h_flex().flex_wrap().text_sm();
            for piece in word_diff(base.unwrap_or(""), text) {
                line = line.child(
                    div()
                        .when(piece.changed, |d| d.bg(highlight))
                        .child(piece.text),
                );
            }
            line.into_any_element()
        }
    }
}

impl Render for ConflictsView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.open {
            return div().into_any_element();
        }
        let theme = cx.theme().clone();
        let this = cx.entity();
        let dismiss = this.clone();
        let progress = self.progress();

        let mut groups: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
        for (ix, card) in self.cards.iter().enumerate() {
            groups.entry(card.path.as_str()).or_default().push(ix);
        }
        let mut body = v_flex()
            .id("conflicts-body")
            .gap_4()
            .p_4()
            .overflow_y_scroll()
            .max_h(px(560.));
        if self.cards.is_empty() {
            body = body.child(
                div().text_sm().text_color(theme.muted_foreground).child(
                    self.message
                        .clone()
                        .unwrap_or_else(|| t!("conflicts.none").to_string()),
                ),
            );
        }
        for (path, indices) in groups {
            let page_mine = this.clone();
            let page_theirs = this.clone();
            let (mine_path, theirs_path) = (path.to_owned(), path.to_owned());
            let mut group = v_flex().gap_2().child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(div().flex_1().min_w_0().text_sm().child(path.to_owned()))
                    .child(
                        Button::new(("conflicts-page-mine", indices[0]))
                            .xsmall()
                            .label(t!("conflicts.all_mine").to_string())
                            .on_click(move |_, _, cx| {
                                let path = mine_path.clone();
                                page_mine.update(cx, |v, cx| v.resolve_page(&path, true, cx));
                            }),
                    )
                    .child(
                        Button::new(("conflicts-page-theirs", indices[0]))
                            .xsmall()
                            .label(t!("conflicts.all_theirs").to_string())
                            .on_click(move |_, _, cx| {
                                let path = theirs_path.clone();
                                page_theirs.update(cx, |v, cx| v.resolve_page(&path, false, cx));
                            }),
                    ),
            );
            for ix in indices {
                let card = &self.cards[ix];
                let decided = card.resolution.is_some();
                let selected = ix == self.cursor && !decided;
                let theirs_by = card
                    .author
                    .clone()
                    .map(|a| {
                        let when = card
                            .time
                            .map(|t| {
                                ago(
                                    UNIX_EPOCH + Duration::from_secs(t.max(0).unsigned_abs()),
                                    SystemTime::now(),
                                )
                            })
                            .unwrap_or_default();
                        t!("conflicts.theirs_by", author = a, when = when).to_string()
                    })
                    .unwrap_or_else(|| t!("conflicts.theirs").to_string());
                let crumbs = if card.kind == ConflictType::RenameRename {
                    card.breadcrumb.join(" \u{2192} ")
                } else {
                    card.breadcrumb.join(" \u{203a} ")
                };
                let mut actions = h_flex().gap_2().flex_wrap();
                if !decided {
                    for action in actions_for(card.kind) {
                        let action = *action;
                        let click = this.clone();
                        actions = actions.child(
                            Button::new(("conflicts-action", ix * 8 + action as usize))
                                .small()
                                .when(action == Action::Mine, |b| b.primary())
                                .label(card.label(action))
                                .on_click(move |_, window, cx| {
                                    click.update(cx, |v, cx| v.apply(ix, action, window, cx));
                                }),
                        );
                    }
                }
                let mut column = v_flex()
                    .id(("conflict-card", ix))
                    .gap_2()
                    .p_3()
                    .rounded(px(6.))
                    .border_1()
                    .border_color(if selected {
                        theme.primary
                    } else {
                        theme.border
                    })
                    .when(decided, |c| c.opacity(0.6))
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child(crumbs),
                            )
                            .children(
                                card.resolution_label()
                                    .map(|l| div().text_xs().text_color(theme.success).child(l)),
                            ),
                    )
                    .child(
                        h_flex()
                            .gap_3()
                            .items_start()
                            .child(
                                v_flex()
                                    .flex_1()
                                    .min_w_0()
                                    .gap_0p5()
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(theme.muted_foreground)
                                            .child(t!("conflicts.mine").to_string()),
                                    )
                                    .child(side_text(
                                        &theme,
                                        card.ours.as_deref(),
                                        card.base.as_deref(),
                                        theme.warning.opacity(0.3),
                                    )),
                            )
                            .child(
                                v_flex()
                                    .flex_1()
                                    .min_w_0()
                                    .gap_0p5()
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(theme.muted_foreground)
                                            .child(theirs_by),
                                    )
                                    .child(side_text(
                                        &theme,
                                        card.theirs.as_deref(),
                                        card.base.as_deref(),
                                        theme.info.opacity(0.3),
                                    )),
                            ),
                    );
                if let Some(suggestion) = &card.suggestion {
                    column =
                        column.child(div().text_xs().text_color(theme.muted_foreground).child(
                            t!("conflicts.suggestion", text = suggestion.clone()).to_string(),
                        ));
                }
                if self.editing.as_deref() == Some(card.id.as_str()) {
                    let save = this.clone();
                    let cancel = this.clone();
                    column = column
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child(t!("conflicts.edit_hint").to_string()),
                        )
                        .child(Textarea::new(&self.editor))
                        .child(
                            h_flex()
                                .gap_2()
                                .child(
                                    Button::new(("conflicts-save", ix))
                                        .small()
                                        .primary()
                                        .label(t!("conflicts.save_edit").to_string())
                                        .on_click(move |_, window, cx| {
                                            save.update(cx, |v, cx| v.save_edit(window, cx));
                                        }),
                                )
                                .child(
                                    Button::new(("conflicts-cancel-edit", ix))
                                        .small()
                                        .label(t!("conflicts.cancel_edit").to_string())
                                        .on_click(move |_, window, cx| {
                                            cancel.update(cx, |v, cx| v.cancel_edit(window, cx));
                                        }),
                                ),
                        );
                } else {
                    column = column.child(actions);
                }
                group = group.child(column);
            }
            body = body.child(group);
        }

        let close = this.clone();
        let header = v_flex()
            .gap_1()
            .px_4()
            .py_2()
            .border_b_1()
            .border_color(theme.border)
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .when(progress.total > 0, |row| {
                        row.child(
                            div().id("conflicts-progress").text_sm().child(
                                t!(
                                    "conflicts.progress",
                                    done = progress.resolved,
                                    total = progress.total
                                )
                                .to_string(),
                            ),
                        )
                        .child(
                            ProgressBar::new("conflicts-bar")
                                .w(px(160.))
                                .value(progress.percent()),
                        )
                    })
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(if self.cards.is_empty() {
                                String::new()
                            } else {
                                self.message.clone().unwrap_or_default()
                            }),
                    ),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(t!("conflicts.shortcuts").to_string()),
            );

        let card = v_flex()
            .id("conflicts-card")
            .key_context("Conflicts")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                this.key_down(event, window, cx);
            }))
            .child(title_bar(
                &theme,
                t!("conflicts.title").to_string(),
                Button::new("conflicts-close")
                    .ghost()
                    .small()
                    .icon(IconName::Close)
                    .on_click(move |_, _, cx| close.update(cx, |v, cx| v.close(cx))),
            ))
            .child(header)
            .child(body);
        modal(
            "conflicts-view",
            &theme,
            900.,
            move |_, cx| dismiss.update(cx, |v, cx| v.close(cx)),
            card,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(kind: ConflictType) -> ConflictCard {
        ConflictCard {
            id: "c-0001".into(),
            path: "pages/p.md".into(),
            kind,
            field: None,
            block_key: None,
            breadcrumb: vec!["p".into(), "second block".into()],
            base: Some("second block".into()),
            ours: Some("second block (alice)".into()),
            theirs: Some("second block (bob)".into()),
            deleted_by: None,
            suggestion: None,
            author: Some("bob".into()),
            time: Some(0),
            resolution: None,
        }
    }

    #[test]
    fn actions_depend_on_the_kind() {
        assert_eq!(
            actions_for(ConflictType::Content),
            [Action::Mine, Action::Theirs, Action::Both, Action::Edit]
        );
        assert_eq!(actions_for(ConflictType::ExternalMarkers), [Action::Edit]);
        assert!(!actions_for(ConflictType::RenameRename).contains(&Action::Edit));
        assert!(!actions_for(ConflictType::Config).contains(&Action::Both));
    }

    #[test]
    fn delete_vs_modify_labels_say_what_happens() {
        let mut c = card(ConflictType::DeleteVsModify);
        c.deleted_by = Some("Theirs".into());
        // They deleted: keeping mine keeps the modified block, keeping theirs deletes it.
        assert_eq!(c.label(Action::Mine), t!("conflicts.keep_modified"));
        assert_eq!(c.label(Action::Theirs), t!("conflicts.delete"));
        c.deleted_by = Some("Ours".into());
        assert_eq!(c.label(Action::Mine), t!("conflicts.delete"));
        assert_eq!(c.label(Action::Theirs), t!("conflicts.keep_modified"));
    }

    #[test]
    fn actions_map_to_engine_choices_and_edits_start_from_my_side() {
        assert_eq!(Action::Mine.resolution(""), Resolution::Ours);
        assert_eq!(Action::Theirs.resolution(""), Resolution::Theirs);
        assert_eq!(Action::Both.resolution(""), Resolution::Both);
        assert_eq!(Action::Edit.resolution("x"), Resolution::Edit("x".into()));
        assert_eq!(
            card(ConflictType::Content).edit_seed(),
            "second block (alice)"
        );
    }

    #[test]
    fn progress_counts_decided_conflicts() {
        let mut a = card(ConflictType::Content);
        let b = card(ConflictType::Property);
        assert_eq!(Progress::of(&[a.clone(), b.clone()]).resolved, 0);
        a.resolution = Some(Resolution::Theirs);
        let p = Progress::of(&[a.clone(), b.clone()]);
        assert_eq!((p.resolved, p.total), (1, 2));
        assert!(!p.complete());
        assert!((p.percent() - 50.0).abs() < f32::EPSILON);
        let mut b = b;
        b.resolution = Some(Resolution::Ours);
        assert!(Progress::of(&[a, b]).complete());
        assert!(Progress::of(&[]).complete());
    }
}
