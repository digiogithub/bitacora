//! Graph picker: "Open graph folder..." plus the list of recent graphs.

use std::path::PathBuf;

use rust_i18n::t;

use crate::recent::RecentGraph;
use crate::ui::button::{Button, ButtonVariants as _};
use crate::ui::{
    ActiveTheme as _, ClickEvent, Context, Disableable as _, EventEmitter, IconName,
    InteractiveElement as _, IntoElement, ParentElement as _, PathPromptOptions, Render,
    Sizable as _, Styled as _, Window, div, h_flex, px, v_flex,
};

/// Events emitted by the picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PickerEvent {
    /// Open this graph folder.
    Open(PathBuf),
    /// Drop this folder from the recent list.
    Forget(PathBuf),
    /// Open the "open graph from a remote" dialog (BIT-T-0282).
    CloneFromRemote,
}

/// The picker view.
#[derive(Debug, Default)]
pub struct GraphPicker {
    recents: Vec<RecentGraph>,
}

impl EventEmitter<PickerEvent> for GraphPicker {}

impl GraphPicker {
    /// A picker showing `recents` (most recent first).
    pub fn new(recents: Vec<RecentGraph>) -> Self {
        Self { recents }
    }

    /// Replaces the recent list.
    pub fn set_recents(&mut self, recents: Vec<RecentGraph>, cx: &mut Context<Self>) {
        self.recents = recents;
        cx.notify();
    }

    /// The recent graphs shown.
    pub fn recents(&self) -> &[RecentGraph] {
        &self.recents
    }

    /// Opens the `ix`-th recent graph; folders that vanished are ignored.
    pub fn open_recent(&mut self, ix: usize, cx: &mut Context<Self>) {
        if let Some(graph) = self.recents.get(ix)
            && graph.exists()
        {
            cx.emit(PickerEvent::Open(graph.path.clone()));
        }
    }

    /// Forgets the `ix`-th recent graph.
    pub fn forget_recent(&mut self, ix: usize, cx: &mut Context<Self>) {
        if ix < self.recents.len() {
            let removed = self.recents.remove(ix);
            cx.emit(PickerEvent::Forget(removed.path));
            cx.notify();
        }
    }

    /// Shows the native folder dialog; a chosen folder is emitted as [`PickerEvent::Open`].
    pub fn browse(&mut self, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(t!("picker.dialog_prompt").to_string().into()),
        });
        cx.spawn(async move |this, cx| {
            // A cancelled dialog, a closed channel or a portal error all mean "nothing chosen".
            if let Ok(Ok(Some(paths))) = rx.await
                && let Some(path) = paths.into_iter().next()
            {
                let _ = this.update(cx, |_, cx| cx.emit(PickerEvent::Open(path)));
            }
        })
        .detach();
    }
}

impl Render for GraphPicker {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let mut recent = v_flex().gap_1().w_full();
        if self.recents.is_empty() {
            recent = recent.child(
                div()
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child(t!("picker.no_recent").to_string()),
            );
        }
        for (ix, graph) in self.recents.iter().enumerate() {
            let exists = graph.exists();
            let row = h_flex()
                .gap_2()
                .items_center()
                .child(
                    Button::new(("recent", ix))
                        .ghost()
                        .icon(IconName::Folder)
                        .label(graph.name())
                        .disabled(!exists)
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.open_recent(ix, cx);
                        })),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(if exists {
                            graph.path.display().to_string()
                        } else {
                            t!("picker.missing").to_string()
                        }),
                )
                .child(
                    Button::new(("forget", ix))
                        .ghost()
                        .xsmall()
                        .icon(IconName::Close)
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.forget_recent(ix, cx);
                        })),
                );
            recent = recent.child(row);
        }
        div()
            .size_full()
            .bg(theme.background)
            .text_color(theme.foreground)
            .flex()
            .items_center()
            .justify_center()
            .child(
                v_flex()
                    .id("graph-picker")
                    .w(px(560.))
                    .gap_4()
                    .child(
                        div()
                            .text_size(px(26.))
                            .child(t!("picker.title").to_string()),
                    )
                    .child(
                        div()
                            .text_color(theme.muted_foreground)
                            .child(t!("picker.subtitle").to_string()),
                    )
                    .child(
                        Button::new("open-folder")
                            .primary()
                            .icon(IconName::FolderOpen)
                            .label(t!("picker.open_folder").to_string())
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.browse(cx))),
                    )
                    .child(
                        Button::new("clone-from-remote")
                            .icon(IconName::Globe)
                            .label(t!("picker.clone_from_remote").to_string())
                            .on_click(cx.listener(|_, _: &ClickEvent, _, cx| {
                                cx.emit(PickerEvent::CloneFromRemote);
                            })),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .child(t!("picker.recent").to_string()),
                    )
                    .child(recent),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::testing::{TestAppContext, gpui_test};
    use crate::{settings::AppSettings, theme};

    fn recent(path: &std::path::Path) -> RecentGraph {
        RecentGraph {
            path: path.to_path_buf(),
        }
    }

    #[gpui_test]
    fn recent_rows_open_forget_and_skip_missing_folders(cx: &mut TestAppContext) {
        cx.update(|cx| {
            crate::ui::init(cx);
            theme::install(cx, AppSettings::default(), None);
        });
        let tmp = tempfile::tempdir().expect("tmp");
        let missing = tmp.path().join("gone");
        let (picker, cx) =
            cx.add_window_view(|_, _| GraphPicker::new(vec![recent(tmp.path()), recent(&missing)]));
        let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let sink = events.clone();
        let _sub = cx.update(|_, cx| {
            cx.subscribe(&picker, move |_, event: &PickerEvent, _| {
                sink.borrow_mut().push(event.clone());
            })
        });
        picker.update(cx, |p, cx| p.open_recent(1, cx));
        assert!(events.borrow().is_empty(), "missing folders are not opened");
        picker.update(cx, |p, cx| p.open_recent(0, cx));
        picker.update(cx, |p, cx| p.forget_recent(1, cx));
        assert_eq!(
            *events.borrow(),
            vec![
                PickerEvent::Open(tmp.path().to_path_buf()),
                PickerEvent::Forget(missing)
            ]
        );
        assert_eq!(picker.read_with(cx, |p, _| p.recents().len()), 1);
    }
}
