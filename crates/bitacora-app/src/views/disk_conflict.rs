//! "Page changed on disk" (BIT-US-0070, BIT-T-0349): a non-modal banner above the page with
//! [Keep mine] [Take disk version] [Show diff], and the block-level diff overlay.
//!
//! The data comes from core's [`ConflictNotice`] (`QueueEvent::PageConflicted`): the page keeps
//! the user's unsaved content, writes to it are stopped, and nothing is overwritten until the
//! user decides. Both resolutions go through `Request::Resolve` on the command queue (the
//! overwritten side is backed up to `logseq/bak` by core).

use std::collections::HashMap;
use std::sync::Arc;

use bitacora_core::editor::{BlockDiff, ConflictNotice, DiffKind};
use bitacora_core::graph::PageKey;
use rust_i18n::t;

use crate::ui::button::{Button, ButtonVariants as _};
use crate::ui::{
    ActiveTheme as _, Context, EventEmitter, FluentBuilder as _, IconName, InteractiveElement as _,
    IntoElement, ParentElement as _, Render, Sizable as _, StatefulInteractiveElement as _,
    Styled as _, Window, div, h_flex, icon, px, v_flex,
};
use crate::views::modal::{modal, title_bar, word_diff};

/// What the banner asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiskConflictEvent {
    /// Overwrite the disk version with mine.
    KeepMine(PageKey),
    /// Replace my unsaved edits with the disk version.
    TakeDisk(PageKey),
    /// Open the block-level diff.
    ShowDiff(PageKey),
    /// Open the sync conflict resolver.
    OpenResolver,
}

/// The banner. Holds every conflicted page and shows the one on screen.
#[derive(Debug, Default)]
pub struct DiskConflictBanner {
    notices: HashMap<PageKey, Arc<ConflictNotice>>,
    current: Option<PageKey>,
    sync_conflicts: usize,
}

impl EventEmitter<DiskConflictEvent> for DiskConflictBanner {}

impl DiskConflictBanner {
    /// An empty banner.
    pub fn new() -> Self {
        Self::default()
    }

    /// Records (or replaces) the notice of a page.
    pub fn set_notice(&mut self, notice: Arc<ConflictNotice>, cx: &mut Context<Self>) {
        self.notices.insert(notice.key.clone(), notice);
        cx.notify();
    }

    /// Forgets the notice of `key` (resolved or reloaded).
    pub fn clear(&mut self, key: &PageKey, cx: &mut Context<Self>) {
        if self.notices.remove(key).is_some() {
            cx.notify();
        }
    }

    /// Forgets everything (another graph opened).
    pub fn clear_all(&mut self, cx: &mut Context<Self>) {
        self.notices.clear();
        self.current = None;
        cx.notify();
    }

    /// The page on screen; the banner shows only when it is conflicted.
    pub fn set_current(&mut self, key: Option<PageKey>, cx: &mut Context<Self>) {
        if self.current != key {
            self.current = key;
            cx.notify();
        }
    }

    /// How many unresolved sync conflicts the page on screen has (BIT-US-0054).
    pub fn set_sync_conflicts(&mut self, count: usize, cx: &mut Context<Self>) {
        if self.sync_conflicts != count {
            self.sync_conflicts = count;
            cx.notify();
        }
    }

    /// Sync conflicts of the page on screen.
    pub fn sync_conflicts(&self) -> usize {
        self.sync_conflicts
    }

    /// The notice of the page on screen.
    pub fn active(&self) -> Option<&Arc<ConflictNotice>> {
        self.current.as_ref().and_then(|k| self.notices.get(k))
    }

    /// The notice of any page.
    pub fn notice(&self, key: &PageKey) -> Option<&Arc<ConflictNotice>> {
        self.notices.get(key)
    }

    /// Number of conflicted pages.
    pub fn count(&self) -> usize {
        self.notices.len()
    }
}

impl Render for DiskConflictBanner {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let this = cx.entity();
        let sync_strip =
            (self.sync_conflicts > 0).then(|| {
                let open = this.clone();
                h_flex()
                    .id("sync-conflict-banner")
                    .w_full()
                    .gap_2()
                    .items_center()
                    .px_4()
                    .py_2()
                    .bg(theme.danger.opacity(0.12))
                    .border_b_1()
                    .border_color(theme.danger)
                    .child(icon(IconName::TriangleAlert).text_color(theme.danger))
                    .child(
                        div().flex_1().min_w_0().text_sm().child(
                            t!("disk.sync_conflicts", count = self.sync_conflicts).to_string(),
                        ),
                    )
                    .child(
                        Button::new("sync-conflict-resolve")
                            .small()
                            .label(t!("disk.resolve").to_string())
                            .on_click(move |_, _, cx| {
                                open.update(cx, |_, cx| cx.emit(DiskConflictEvent::OpenResolver));
                            }),
                    )
            });
        let Some(notice) = self.active().cloned() else {
            return match sync_strip {
                Some(strip) => strip.into_any_element(),
                None => div().into_any_element(),
            };
        };
        let key = notice.key.clone();
        let button = |id: &'static str, label: String, event: DiskConflictEvent| {
            let this = this.clone();
            Button::new(id)
                .small()
                .label(label)
                .on_click(move |_, _, cx| {
                    let event = event.clone();
                    this.update(cx, |_, cx| cx.emit(event));
                })
        };
        let banner = h_flex()
            .id("disk-conflict-banner")
            .w_full()
            .gap_2()
            .items_center()
            .px_4()
            .py_2()
            .bg(theme.warning.opacity(0.18))
            .border_b_1()
            .border_color(theme.warning)
            .child(icon(IconName::TriangleAlert).text_color(theme.warning))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(div().text_sm().child(t!("disk.title").to_string()))
                    .child(div().text_xs().text_color(theme.muted_foreground).child(
                        if notice.base_available {
                            t!("disk.body").to_string()
                        } else {
                            t!("disk.body_no_base").to_string()
                        },
                    )),
            )
            .child(button(
                "disk-keep-mine",
                t!("disk.keep_mine").to_string(),
                DiskConflictEvent::KeepMine(key.clone()),
            ))
            .child(button(
                "disk-take-disk",
                t!("disk.take_disk").to_string(),
                DiskConflictEvent::TakeDisk(key.clone()),
            ))
            .child(button(
                "disk-show-diff",
                t!("disk.show_diff").to_string(),
                DiskConflictEvent::ShowDiff(key),
            ));
        v_flex()
            .w_full()
            .children(sync_strip)
            .child(banner)
            .into_any_element()
    }
}

/// The diff overlay: mine against the disk version, block by block.
#[derive(Debug, Default)]
pub struct DiskDiffView {
    open: bool,
    title: String,
    diffs: Vec<BlockDiff>,
}

/// What the overlay tells the workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiskDiffEvent {
    /// Closed.
    Closed,
}

impl EventEmitter<DiskDiffEvent> for DiskDiffView {}

impl DiskDiffView {
    /// A closed overlay.
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether the overlay is showing.
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// The blocks shown.
    pub fn diffs(&self) -> &[BlockDiff] {
        &self.diffs
    }

    /// Shows the diff of `notice` for the page `title`.
    pub fn show(&mut self, title: String, notice: &ConflictNotice, cx: &mut Context<Self>) {
        self.open = true;
        self.title = title;
        self.diffs = notice.diff.clone();
        cx.notify();
    }

    /// Hides the overlay.
    pub fn close(&mut self, cx: &mut Context<Self>) {
        if self.open {
            self.open = false;
            cx.emit(DiskDiffEvent::Closed);
            cx.notify();
        }
    }
}

fn kind_label(kind: DiffKind) -> String {
    match kind {
        DiffKind::Changed => t!("disk.kind_changed"),
        DiffKind::OnlyMine => t!("disk.kind_only_mine"),
        DiffKind::OnlyDisk => t!("disk.kind_only_disk"),
    }
    .to_string()
}

impl Render for DiskDiffView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.open {
            return div().into_any_element();
        }
        let theme = cx.theme().clone();
        let this = cx.entity();
        let dismiss = this.clone();
        let mut body = v_flex()
            .id("disk-diff-body")
            .gap_2()
            .p_4()
            .overflow_y_scroll()
            .max_h(px(520.));
        if self.diffs.is_empty() {
            body = body.child(
                div()
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child(t!("disk.no_diff").to_string()),
            );
        }
        for (ix, diff) in self.diffs.iter().enumerate() {
            let side = |text: &Option<String>, reference: &Option<String>, color| match text {
                None => div()
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child(t!("disk.absent").to_string())
                    .into_any_element(),
                Some(text) => {
                    let mut line = h_flex().flex_wrap().text_sm();
                    for piece in word_diff(reference.as_deref().unwrap_or(""), text) {
                        line = line
                            .child(div().when(piece.changed, |d| d.bg(color)).child(piece.text));
                    }
                    line.into_any_element()
                }
            };
            body = body.child(
                v_flex()
                    .id(("disk-diff-block", ix))
                    .gap_1()
                    .p_2()
                    .rounded(px(6.))
                    .border_1()
                    .border_color(theme.border)
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.info)
                                    .child(kind_label(diff.kind)),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child(diff.breadcrumb.join(" \u{203a} ")),
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
                                            .child(t!("disk.mine").to_string()),
                                    )
                                    .child(side(
                                        &diff.mine,
                                        &diff.disk,
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
                                            .child(t!("disk.disk").to_string()),
                                    )
                                    .child(side(&diff.disk, &diff.mine, theme.info.opacity(0.3))),
                            ),
                    ),
            );
        }
        let close = this.clone();
        modal(
            "disk-diff",
            &theme,
            860.,
            move |_, cx| dismiss.update(cx, |v, cx| v.close(cx)),
            v_flex()
                .child(title_bar(
                    &theme,
                    t!("disk.diff_title", page = self.title).to_string(),
                    Button::new("disk-diff-close")
                        .ghost()
                        .small()
                        .icon(IconName::Close)
                        .on_click(move |_, _, cx| close.update(cx, |v, cx| v.close(cx))),
                ))
                .child(body),
        )
    }
}
