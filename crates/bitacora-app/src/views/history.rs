//! Page history (BIT-US-0048, BIT-T-0297): the commits that touched a page, a block-level diff
//! of a chosen version against the page as it is now, and selective restore.
//!
//! Everything git-related runs on the session thread through [`SessionHandle::run`]
//! (`Session::{page_history, history_diff, restore_blocks, restore_page, undo_restore}`).
//! Restores are ordinary core transactions, so the page keeps being written by the single
//! writer and the whole restore can be undone from the panel.

use crate::views::dims;
use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bitacora_runtime::RestoreReport;
use bitacora_sync::history::{BlockDiff, DiffKind, HistoryEntry, PageChange, PageDiff};
use rust_i18n::t;

use crate::session::SessionHandle;
use crate::ui::button::{Button, ButtonVariants as _};
use crate::ui::{
    ActiveTheme as _, Context, Disableable as _, EventEmitter, FluentBuilder as _, IconName,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, Selectable as _,
    Sizable as _, StatefulInteractiveElement as _, Styled as _, Task, Window, div, h_flex, v_flex,
};
use crate::views::modal::{modal, title_bar, word_diff};
use crate::views::sync_panel::ago;

/// How many commits the list loads.
const HISTORY_LIMIT: usize = 100;

/// What the history tells the workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryEvent {
    /// The overlay closed.
    Closed,
}

/// The overlay.
pub struct HistoryView {
    open: bool,
    handle: Option<SessionHandle>,
    rel: String,
    title: String,
    entries: Vec<HistoryEntry>,
    loading: bool,
    error: Option<String>,
    selected: Option<usize>,
    diff: Option<PageDiff>,
    diff_loading: bool,
    show_meta: bool,
    checked: BTreeSet<usize>,
    restored: Option<Arc<RestoreReport>>,
    message: Option<String>,
    generation: u64,
    task: Option<Task<()>>,
}

impl std::fmt::Debug for HistoryView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HistoryView")
            .field("open", &self.open)
            .field("rel", &self.rel)
            .field("entries", &self.entries.len())
            .finish_non_exhaustive()
    }
}

impl EventEmitter<HistoryEvent> for HistoryView {}

impl Default for HistoryView {
    fn default() -> Self {
        Self::new()
    }
}

impl HistoryView {
    /// A closed view.
    pub fn new() -> Self {
        Self {
            open: false,
            handle: None,
            rel: String::new(),
            title: String::new(),
            entries: Vec::new(),
            loading: false,
            error: None,
            selected: None,
            diff: None,
            diff_loading: false,
            show_meta: false,
            checked: BTreeSet::new(),
            restored: None,
            message: None,
            generation: 0,
            task: None,
        }
    }

    /// Whether the overlay is showing.
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// The commits listed, newest first.
    pub fn entries(&self) -> &[HistoryEntry] {
        &self.entries
    }

    /// The diff of the selected version.
    pub fn diff(&self) -> Option<&PageDiff> {
        self.diff.as_ref()
    }

    /// Index of the selected commit.
    pub fn selected(&self) -> Option<usize> {
        self.selected
    }

    /// The last status line (restored, error).
    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }

    /// Whether the load or an operation is still running.
    pub fn is_busy(&self) -> bool {
        self.loading || self.diff_loading
    }

    /// Opens the history of the page stored in `rel` (graph-relative).
    pub fn open_for(
        &mut self,
        handle: SessionHandle,
        rel: String,
        title: String,
        cx: &mut Context<Self>,
    ) {
        self.open = true;
        self.handle = Some(handle.clone());
        self.rel = rel.clone();
        self.title = title;
        self.entries.clear();
        self.selected = None;
        self.diff = None;
        self.checked.clear();
        self.restored = None;
        self.message = None;
        self.error = None;
        self.loading = true;
        self.generation += 1;
        let generation = self.generation;
        let rx = handle.run(move |s| {
            s.page_history(&rel, HISTORY_LIMIT)
                .map_err(|e| e.to_string())
        });
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = rx.recv().await;
            let _ = this.update(cx, |view, cx| {
                if view.generation != generation {
                    return;
                }
                view.loading = false;
                match result {
                    Ok(Ok(entries)) => view.entries = entries,
                    Ok(Err(e)) => view.error = Some(e),
                    Err(_) => view.error = Some(t!("history.session_gone").to_string()),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    /// Closes the overlay.
    pub fn close(&mut self, cx: &mut Context<Self>) {
        if self.open {
            self.open = false;
            self.task = None;
            cx.emit(HistoryEvent::Closed);
            cx.notify();
        }
    }

    /// Selects a commit and loads its block-level diff against the current page.
    pub fn select(&mut self, ix: usize, cx: &mut Context<Self>) {
        let (Some(entry), Some(handle)) = (self.entries.get(ix).cloned(), self.handle.clone())
        else {
            return;
        };
        self.selected = Some(ix);
        self.diff = None;
        self.checked.clear();
        self.message = None;
        self.diff_loading = true;
        self.generation += 1;
        let generation = self.generation;
        let rel = self.rel.clone();
        let rx = handle.run(move |s| s.history_diff(&rel, &entry).map_err(|e| e.to_string()));
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = rx.recv().await;
            let _ = this.update(cx, |view, cx| {
                if view.generation != generation {
                    return;
                }
                view.diff_loading = false;
                match result {
                    Ok(Ok(diff)) => view.diff = Some(diff),
                    Ok(Err(e)) => view.message = Some(e),
                    Err(_) => view.message = Some(t!("history.session_gone").to_string()),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    /// Shows or hides the metadata-only differences.
    pub fn set_show_meta(&mut self, show: bool, cx: &mut Context<Self>) {
        self.show_meta = show;
        self.checked.clear();
        cx.notify();
    }

    fn visible(&self) -> Vec<(usize, &BlockDiff)> {
        self.diff
            .as_ref()
            .map(|d| {
                d.blocks
                    .iter()
                    .enumerate()
                    .filter(|(_, b)| self.show_meta || b.kind != DiffKind::MetaOnly)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Ticks or unticks the `ix`-th block of the diff for restoring.
    pub fn toggle_block(&mut self, ix: usize, cx: &mut Context<Self>) {
        if !self.checked.remove(&ix) {
            self.checked.insert(ix);
        }
        cx.notify();
    }

    /// Indices (into the diff) of the ticked blocks.
    pub fn checked(&self) -> Vec<usize> {
        self.checked.iter().copied().collect()
    }

    /// Restores the ticked blocks as undoable core transactions.
    pub fn restore_selected(&mut self, cx: &mut Context<Self>) {
        let Some(diff) = &self.diff else { return };
        let blocks: Vec<BlockDiff> = self
            .checked
            .iter()
            .filter_map(|&i| diff.blocks.get(i).cloned())
            .collect();
        if blocks.is_empty() {
            return;
        }
        self.restore(Some(blocks), cx);
    }

    /// Restores the whole page to the selected version.
    pub fn restore_page(&mut self, cx: &mut Context<Self>) {
        self.restore(None, cx);
    }

    fn restore(&mut self, blocks: Option<Vec<BlockDiff>>, cx: &mut Context<Self>) {
        let (Some(entry), Some(handle)) = (
            self.selected.and_then(|i| self.entries.get(i)).cloned(),
            self.handle.clone(),
        ) else {
            return;
        };
        self.diff_loading = true;
        self.generation += 1;
        let generation = self.generation;
        let rel = self.rel.clone();
        let rx = handle.run(move |s| {
            match blocks {
                Some(blocks) => s.restore_blocks(&rel, &entry, &blocks),
                None => s.restore_page(&rel, &entry),
            }
            .map(Arc::new)
            .map_err(|e| e.to_string())
        });
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = rx.recv().await;
            let _ = this.update(cx, |view, cx| {
                if view.generation != generation {
                    return;
                }
                view.diff_loading = false;
                match result {
                    Ok(Ok(report)) => {
                        let skipped = report.skipped.len();
                        view.message = Some(if skipped == 0 {
                            t!("history.restored", count = report.restored).to_string()
                        } else {
                            t!(
                                "history.restored_skipped",
                                count = report.restored,
                                skipped = skipped
                            )
                            .to_string()
                        });
                        view.restored = Some(report);
                        view.checked.clear();
                    }
                    Ok(Err(e)) => view.message = Some(e),
                    Err(_) => view.message = Some(t!("history.session_gone").to_string()),
                }
                // The page changed: recompute the diff against it.
                let again = view.selected;
                if let Some(ix) = again {
                    view.reselect(ix, cx);
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    /// Reloads the diff of `ix` without clearing the status line.
    fn reselect(&mut self, ix: usize, cx: &mut Context<Self>) {
        let message = self.message.take();
        let restored = self.restored.clone();
        self.select(ix, cx);
        self.message = message;
        self.restored = restored;
    }

    /// Reverts the last restore.
    pub fn undo_restore(&mut self, cx: &mut Context<Self>) {
        let (Some(report), Some(handle)) = (self.restored.take(), self.handle.clone()) else {
            return;
        };
        let rx = handle.run(move |s| s.undo_restore(&report).map_err(|e| e.to_string()));
        self.generation += 1;
        let generation = self.generation;
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = rx.recv().await;
            let _ = this.update(cx, |view, cx| {
                if view.generation != generation {
                    return;
                }
                view.message = Some(match result {
                    Ok(Ok(())) => t!("history.undone").to_string(),
                    Ok(Err(e)) => e,
                    Err(_) => t!("history.session_gone").to_string(),
                });
                if let Some(ix) = view.selected {
                    view.reselect(ix, cx);
                }
                cx.notify();
            });
        }));
        cx.notify();
    }
}

fn change_label(change: &PageChange) -> String {
    match change {
        PageChange::Added => t!("history.change_added").to_string(),
        PageChange::Modified => t!("history.change_modified").to_string(),
        PageChange::Renamed { from } => t!("history.change_renamed", from = from).to_string(),
        PageChange::Deleted => t!("history.change_deleted").to_string(),
    }
}

fn kind_label(kind: DiffKind) -> String {
    match kind {
        DiffKind::Added => t!("history.kind_added"),
        DiffKind::Removed => t!("history.kind_removed"),
        DiffKind::Changed => t!("history.kind_changed"),
        DiffKind::MetaOnly => t!("history.kind_meta"),
    }
    .to_string()
}

impl Render for HistoryView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.open {
            return div().into_any_element();
        }
        let theme = cx.theme().clone();
        let this = cx.entity();
        let dismiss = this.clone();
        let now = SystemTime::now();

        // Commit list.
        let mut list = v_flex()
            .id("history-list")
            .gap_1()
            .overflow_y_scroll()
            .w(dims::PX_260);
        if self.loading {
            list = list.child(
                div()
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child(t!("history.loading").to_string()),
            );
        } else if let Some(error) = &self.error {
            list = list.child(
                div()
                    .text_sm()
                    .text_color(theme.danger)
                    .child(error.clone()),
            );
        } else if self.entries.is_empty() {
            list = list.child(
                div()
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child(t!("history.empty").to_string()),
            );
        }
        for (ix, entry) in self.entries.iter().enumerate() {
            let pick = this.clone();
            let when = ago(
                UNIX_EPOCH + Duration::from_secs(entry.time.max(0).unsigned_abs()),
                now,
            );
            let by = entry
                .agent
                .clone()
                .map(|a| t!("history.by_agent", agent = a).to_string())
                .or_else(|| entry.device.clone())
                .unwrap_or_default();
            list = list.child(
                v_flex()
                    .id(("history-entry", ix))
                    .p_2()
                    .gap_0p5()
                    .rounded(dims::PX_6)
                    .cursor_pointer()
                    .when(self.selected == Some(ix), |d| d.bg(theme.secondary))
                    .hover(|d| d.bg(theme.secondary))
                    .on_click(move |_, _, cx| pick.update(cx, |v, cx| v.select(ix, cx)))
                    .child(div().text_sm().child(entry.subject.clone()))
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(format!(
                                "{when} \u{b7} {by} \u{b7} {}",
                                change_label(&entry.change)
                            )),
                    ),
            );
        }

        // Diff.
        let mut diff_col = v_flex()
            .id("history-diff")
            .flex_1()
            .min_w_0()
            .gap_2()
            .overflow_y_scroll();
        let visible = self.visible();
        match (&self.diff, self.selected) {
            (_, None) => {
                diff_col = diff_col.child(
                    div()
                        .text_sm()
                        .text_color(theme.muted_foreground)
                        .child(t!("history.pick_version").to_string()),
                );
            }
            (None, Some(_)) => {
                diff_col =
                    diff_col.child(div().text_sm().text_color(theme.muted_foreground).child(
                        if self.diff_loading {
                            t!("history.loading_diff").to_string()
                        } else {
                            self.message.clone().unwrap_or_default()
                        },
                    ));
            }
            (Some(diff), Some(_)) => {
                if visible.is_empty() {
                    diff_col =
                        diff_col.child(div().text_sm().text_color(theme.muted_foreground).child(
                            if diff.is_empty() {
                                t!("history.same").to_string()
                            } else {
                                t!("history.only_meta").to_string()
                            },
                        ));
                }
                for (ix, block) in &visible {
                    let ix = *ix;
                    let toggle = this.clone();
                    let ticked = self.checked.contains(&ix);
                    let (old, new) = (
                        block.old_text.clone().unwrap_or_default(),
                        block.new_text.clone().unwrap_or_default(),
                    );
                    let side = |text: &str, reference: &str, color| {
                        let mut line = h_flex().flex_wrap().text_sm();
                        for piece in word_diff(reference, text) {
                            line = line.child(
                                div().when(piece.changed, |d| d.bg(color)).child(piece.text),
                            );
                        }
                        line
                    };
                    let crumbs = block.breadcrumb.join(" \u{203a} ");
                    diff_col = diff_col.child(
                        v_flex()
                            .id(("history-block", ix))
                            .gap_1()
                            .p_2()
                            .rounded(dims::PX_6)
                            .border_1()
                            .border_color(theme.border)
                            .child(
                                h_flex()
                                    .gap_2()
                                    .items_center()
                                    .child(
                                        Button::new(("history-tick", ix))
                                            .ghost()
                                            .xsmall()
                                            .icon(if ticked {
                                                IconName::Check
                                            } else {
                                                IconName::Square
                                            })
                                            .on_click(move |_, _, cx| {
                                                toggle.update(cx, |v, cx| v.toggle_block(ix, cx))
                                            }),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(theme.info)
                                            .child(kind_label(block.kind)),
                                    )
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .text_xs()
                                            .text_color(theme.muted_foreground)
                                            .child(crumbs),
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
                                                    .child(t!("history.that_version").to_string()),
                                            )
                                            .child(side(&old, &new, theme.danger.opacity(0.25))),
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
                                                    .child(t!("history.now").to_string()),
                                            )
                                            .child(side(&new, &old, theme.success.opacity(0.25))),
                                    ),
                            ),
                    );
                }
            }
        }

        // Actions.
        let has_diff = self.diff.is_some();
        let n_checked = self.checked.len();
        let restore_sel = this.clone();
        let restore_all = this.clone();
        let undo = this.clone();
        let meta = this.clone();
        let actions = h_flex()
            .gap_2()
            .items_center()
            .px_4()
            .py_3()
            .border_t_1()
            .border_color(theme.border)
            .child(
                Button::new("history-restore-selected")
                    .small()
                    .primary()
                    .disabled(n_checked == 0 || self.diff_loading)
                    .label(t!("history.restore_selected", count = n_checked).to_string())
                    .on_click(move |_, _, cx| {
                        restore_sel.update(cx, |v, cx| v.restore_selected(cx))
                    }),
            )
            .child(
                Button::new("history-restore-page")
                    .small()
                    .disabled(!has_diff || self.diff_loading)
                    .label(t!("history.restore_page").to_string())
                    .on_click(move |_, _, cx| restore_all.update(cx, |v, cx| v.restore_page(cx))),
            )
            .child(
                Button::new("history-undo")
                    .small()
                    .ghost()
                    .disabled(self.restored.is_none())
                    .icon(IconName::Undo2)
                    .label(t!("history.undo").to_string())
                    .on_click(move |_, _, cx| undo.update(cx, |v, cx| v.undo_restore(cx))),
            )
            .child(
                Button::new("history-meta")
                    .small()
                    .ghost()
                    .selected(self.show_meta)
                    .label(t!("history.show_meta").to_string())
                    .on_click(move |_, _, cx| {
                        meta.update(cx, |v, cx| {
                            let next = !v.show_meta;
                            v.set_show_meta(next, cx);
                        });
                    }),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(self.message.clone().unwrap_or_default()),
            );

        let close = this.clone();
        modal(
            "history-view",
            &theme,
            980.,
            move |_, cx| dismiss.update(cx, |v, cx| v.close(cx)),
            v_flex()
                .child(title_bar(
                    &theme,
                    t!("history.title", page = self.title).to_string(),
                    Button::new("history-close")
                        .ghost()
                        .small()
                        .icon(IconName::Close)
                        .on_click(move |_, _, cx| close.update(cx, |v, cx| v.close(cx))),
                ))
                .child(
                    h_flex()
                        .gap_3()
                        .p_4()
                        .h(dims::PX_440)
                        .items_start()
                        .child(list.h_full())
                        .child(diff_col.h_full()),
                )
                .child(actions),
        )
    }
}
