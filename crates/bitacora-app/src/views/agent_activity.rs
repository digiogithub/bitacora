//! "Agent activity" (BIT-T-0211, BIT-US-0022, BIT-SP-0007.R8): what MCP clients did in this
//! graph, newest first, with filters and a one-click undo per write.
//!
//! The data is the audit log of the MCP server (`Session::agent_activity`); undo goes through
//! `Session::undo_agent_entry`, which reverts the call as one core transaction (so the page is
//! written by the single writer and the normal undo history is untouched). Both run on the
//! session thread through [`SessionHandle::run`].

use crate::views::dims;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bitacora_mcp::{AuditEvent, AuditFilter, AuditRecord};
use rust_i18n::t;

use crate::session::SessionHandle;
use crate::ui::button::{Button, ButtonVariants as _};
use crate::ui::{
    ActiveTheme as _, Context, Disableable as _, EventEmitter, FluentBuilder as _, IconName,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, Selectable as _,
    Sizable as _, StatefulInteractiveElement as _, Styled as _, Task, Window, div, h_flex, v_flex,
};
use crate::views::modal::{labelled, modal, title_bar};
use crate::views::sync_panel::ago;

/// How many audit entries are loaded.
const ACTIVITY_LIMIT: usize = 500;

/// What the view tells the workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentActivityEvent {
    /// The overlay closed.
    Closed,
    /// Show the block with this uuid.
    OpenBlock(String),
}

/// Which entries are listed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ActivityFilter {
    /// Only this token.
    pub token: Option<String>,
    /// Only this tool.
    pub tool: Option<String>,
    /// Only calls that wrote to the graph.
    pub writes_only: bool,
    /// Only calls that failed.
    pub errors_only: bool,
}

impl ActivityFilter {
    /// Whether `record` passes the filter.
    #[must_use]
    pub fn matches(&self, record: &AuditRecord) -> bool {
        self.token
            .as_ref()
            .is_none_or(|t| record.token.as_deref() == Some(t))
            && self.tool.as_ref().is_none_or(|t| record.tool == *t)
            && (!self.writes_only || record.write)
            && (!self.errors_only || record.result != "ok")
    }
}

/// Whether the undo button of `record` is enabled: a call that wrote, still has its undo data
/// and was not undone yet.
#[must_use]
pub fn can_undo(record: &AuditRecord) -> bool {
    record.event == AuditEvent::Call && record.write && record.undoable && !record.undone
}

/// Why the undo button of `record` is disabled (the tooltip), `None` when it is enabled.
#[must_use]
pub fn undo_blocked_reason(record: &AuditRecord) -> Option<String> {
    if can_undo(record) {
        None
    } else if record.undone {
        Some(t!("activity.already_undone").to_string())
    } else if !record.write {
        Some(t!("activity.not_a_write").to_string())
    } else {
        Some(t!("activity.no_undo_data").to_string())
    }
}

/// The overlay.
pub struct AgentActivityView {
    open: bool,
    handle: Option<SessionHandle>,
    records: Vec<AuditRecord>,
    filter: ActivityFilter,
    selected: Option<String>,
    loading: bool,
    message: Option<String>,
    generation: u64,
    task: Option<Task<()>>,
}

impl std::fmt::Debug for AgentActivityView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentActivityView")
            .field("open", &self.open)
            .field("records", &self.records.len())
            .finish_non_exhaustive()
    }
}

impl EventEmitter<AgentActivityEvent> for AgentActivityView {}

impl Default for AgentActivityView {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentActivityView {
    /// A closed view.
    #[must_use]
    pub fn new() -> Self {
        Self {
            open: false,
            handle: None,
            records: Vec::new(),
            filter: ActivityFilter::default(),
            selected: None,
            loading: false,
            message: None,
            generation: 0,
            task: None,
        }
    }

    /// Whether the overlay is showing.
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// The loaded entries, newest first.
    #[must_use]
    pub fn records(&self) -> &[AuditRecord] {
        &self.records
    }

    /// The filter in effect.
    #[must_use]
    pub fn filter(&self) -> &ActivityFilter {
        &self.filter
    }

    /// The entries that pass the filter.
    #[must_use]
    pub fn visible(&self) -> Vec<&AuditRecord> {
        self.records
            .iter()
            .filter(|r| r.event == AuditEvent::Call && self.filter.matches(r))
            .collect()
    }

    /// The selected entry.
    #[must_use]
    pub fn selected(&self) -> Option<&AuditRecord> {
        let id = self.selected.as_ref()?;
        self.records.iter().find(|r| r.id == *id)
    }

    /// Status line (undo result, errors).
    #[must_use]
    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }

    /// Whether a load or an undo is running.
    #[must_use]
    pub fn is_busy(&self) -> bool {
        self.loading
    }

    /// Distinct tokens of the loaded entries.
    #[must_use]
    pub fn tokens(&self) -> Vec<String> {
        distinct(self.records.iter().filter_map(|r| r.token.clone()))
    }

    /// Distinct tools of the loaded entries.
    #[must_use]
    pub fn tools(&self) -> Vec<String> {
        distinct(
            self.records
                .iter()
                .filter(|r| r.event == AuditEvent::Call)
                .map(|r| r.tool.clone()),
        )
    }

    /// Opens the overlay and loads the log.
    pub fn open_with(&mut self, handle: SessionHandle, cx: &mut Context<Self>) {
        self.open = true;
        self.handle = Some(handle);
        self.message = None;
        self.reload(cx);
    }

    /// Opens the overlay over entries already in hand (tests).
    pub fn open_with_records(&mut self, records: Vec<AuditRecord>, cx: &mut Context<Self>) {
        self.open = true;
        self.records = records;
        self.loading = false;
        cx.notify();
    }

    fn reload(&mut self, cx: &mut Context<Self>) {
        let Some(handle) = self.handle.clone() else {
            return;
        };
        self.loading = true;
        self.generation += 1;
        let generation = self.generation;
        let rx = handle.run(|s| {
            s.agent_activity(&AuditFilter {
                limit: Some(ACTIVITY_LIMIT),
                ..AuditFilter::default()
            })
        });
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = rx.recv().await;
            let _ = this.update(cx, |view, cx| {
                if view.generation != generation {
                    return;
                }
                view.loading = false;
                match result {
                    Ok(records) => view.records = records,
                    Err(_) => view.message = Some(t!("history.session_gone").to_string()),
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
            cx.emit(AgentActivityEvent::Closed);
            cx.notify();
        }
    }

    /// Selects an entry (shows its detail).
    pub fn select(&mut self, id: &str, cx: &mut Context<Self>) {
        self.selected = Some(id.to_owned());
        cx.notify();
    }

    /// Replaces the filter.
    pub fn set_filter(&mut self, filter: ActivityFilter, cx: &mut Context<Self>) {
        self.filter = filter;
        cx.notify();
    }

    /// Asks the host to show a block the call touched.
    pub fn jump_to(&mut self, uuid: &str, cx: &mut Context<Self>) {
        cx.emit(AgentActivityEvent::OpenBlock(uuid.to_owned()));
    }

    /// Undoes the selected entry (does nothing while [`can_undo`] is false).
    pub fn undo_selected(&mut self, cx: &mut Context<Self>) {
        let (Some(record), Some(handle)) = (self.selected().cloned(), self.handle.clone()) else {
            return;
        };
        if !can_undo(&record) {
            return;
        }
        self.loading = true;
        self.generation += 1;
        let generation = self.generation;
        let id = record.id.clone();
        let rx = handle.run(move |s| s.undo_agent_entry(&id).map_err(|e| e.to_string()));
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = rx.recv().await;
            let _ = this.update(cx, |view, cx| {
                if view.generation != generation {
                    return;
                }
                view.message = Some(match result {
                    Ok(Ok(())) => t!("activity.undone").to_string(),
                    Ok(Err(e)) => e,
                    Err(_) => t!("history.session_gone").to_string(),
                });
                view.reload(cx);
            });
        }));
        cx.notify();
    }
}

fn distinct(items: impl Iterator<Item = String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for item in items {
        if !out.contains(&item) {
            out.push(item);
        }
    }
    out
}

impl Render for AgentActivityView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.open {
            return div().into_any_element();
        }
        let theme = cx.theme().clone();
        let this = cx.entity();
        let dismiss = this.clone();
        let now = SystemTime::now();

        // Filters.
        let mut filters = h_flex().gap_1().flex_wrap().items_center();
        let toggle = |id: &'static str, label: String, on: bool| {
            Button::new(id).ghost().xsmall().selected(on).label(label)
        };
        {
            let (a, b) = (this.clone(), this.clone());
            let (writes, errors) = (self.filter.writes_only, self.filter.errors_only);
            filters = filters
                .child(
                    toggle(
                        "activity-writes",
                        t!("activity.writes_only").to_string(),
                        writes,
                    )
                    .on_click(move |_, _, cx| {
                        a.update(cx, |v, cx| {
                            let mut f = v.filter.clone();
                            f.writes_only = !f.writes_only;
                            v.set_filter(f, cx);
                        });
                    }),
                )
                .child(
                    toggle(
                        "activity-errors",
                        t!("activity.errors_only").to_string(),
                        errors,
                    )
                    .on_click(move |_, _, cx| {
                        b.update(cx, |v, cx| {
                            let mut f = v.filter.clone();
                            f.errors_only = !f.errors_only;
                            v.set_filter(f, cx);
                        });
                    }),
                );
        }
        for (n, token) in self.tokens().into_iter().enumerate() {
            let pick = this.clone();
            let on = self.filter.token.as_deref() == Some(token.as_str());
            let name = token.clone();
            filters = filters.child(
                Button::new(("activity-token", n))
                    .ghost()
                    .xsmall()
                    .selected(on)
                    .label(token)
                    .on_click(move |_, _, cx| {
                        pick.update(cx, |v, cx| {
                            let mut f = v.filter.clone();
                            f.token = if on { None } else { Some(name.clone()) };
                            v.set_filter(f, cx);
                        });
                    }),
            );
        }
        for (n, tool) in self.tools().into_iter().enumerate() {
            let pick = this.clone();
            let on = self.filter.tool.as_deref() == Some(tool.as_str());
            let name = tool.clone();
            filters = filters.child(
                Button::new(("activity-tool", n))
                    .ghost()
                    .xsmall()
                    .selected(on)
                    .label(tool)
                    .on_click(move |_, _, cx| {
                        pick.update(cx, |v, cx| {
                            let mut f = v.filter.clone();
                            f.tool = if on { None } else { Some(name.clone()) };
                            v.set_filter(f, cx);
                        });
                    }),
            );
        }

        // List.
        let mut list = v_flex()
            .id("activity-list")
            .gap_1()
            .overflow_y_scroll()
            .w(dims::PX_380);
        let visible = self.visible();
        if self.loading && visible.is_empty() {
            list = list.child(
                div()
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child(t!("activity.loading").to_string()),
            );
        } else if visible.is_empty() {
            list = list.child(
                div()
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child(t!("activity.empty").to_string()),
            );
        }
        for (ix, record) in visible.iter().enumerate() {
            let pick = this.clone();
            let id = record.id.clone();
            let when = ago(
                UNIX_EPOCH + Duration::from_millis(record.ts.max(0).unsigned_abs()),
                now,
            );
            let who = record
                .client
                .clone()
                .or_else(|| record.token.clone())
                .unwrap_or_default();
            let failed = record.result != "ok";
            list = list.child(
                v_flex()
                    .id(("activity-entry", ix))
                    .p_2()
                    .gap_0p5()
                    .rounded(dims::PX_6)
                    .cursor_pointer()
                    .when(self.selected.as_deref() == Some(record.id.as_str()), |d| {
                        d.bg(theme.secondary)
                    })
                    .hover(|d| d.bg(theme.secondary))
                    .on_click(move |_, _, cx| pick.update(cx, |v, cx| v.select(&id, cx)))
                    .child(
                        h_flex()
                            .gap_2()
                            .child(div().text_sm().child(record.tool.clone()))
                            .when(record.undone, |d| {
                                d.child(
                                    div()
                                        .text_xs()
                                        .text_color(theme.warning)
                                        .child(t!("activity.undone_badge").to_string()),
                                )
                            })
                            .when(failed, |d| {
                                d.child(
                                    div()
                                        .text_xs()
                                        .text_color(theme.danger)
                                        .child(record.result.clone()),
                                )
                            }),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(format!("{when} \u{b7} {who}")),
                    ),
            );
        }

        // Detail.
        let mut detail = v_flex()
            .id("activity-detail")
            .flex_1()
            .min_w_0()
            .gap_2()
            .overflow_y_scroll();
        let mut undo_button = None;
        match self.selected() {
            None => {
                detail = detail.child(
                    div()
                        .text_sm()
                        .text_color(theme.muted_foreground)
                        .child(t!("activity.pick").to_string()),
                );
            }
            Some(record) => {
                detail = detail
                    .child(labelled(
                        &theme,
                        t!("activity.tool").to_string(),
                        record.tool.clone(),
                    ))
                    .child(labelled(
                        &theme,
                        t!("activity.result").to_string(),
                        record.result.clone(),
                    ))
                    .child(labelled(
                        &theme,
                        t!("activity.client").to_string(),
                        format!(
                            "{} \u{b7} {}",
                            record.client.clone().unwrap_or_default(),
                            record.token.clone().unwrap_or_default()
                        ),
                    ))
                    .child(labelled(
                        &theme,
                        t!("activity.args").to_string(),
                        record.args.clone(),
                    ))
                    .child(labelled(
                        &theme,
                        t!("activity.pages").to_string(),
                        record.pages.join(", "),
                    ));
                let mut blocks = v_flex().gap_1();
                for (n, uuid) in record.affected.iter().enumerate() {
                    let jump = this.clone();
                    let target = uuid.clone();
                    blocks = blocks.child(
                        div()
                            .id(("activity-block", n))
                            .text_sm()
                            .text_color(theme.info)
                            .cursor_pointer()
                            .child(uuid.clone())
                            .on_click(move |_, _, cx| {
                                jump.update(cx, |v, cx| v.jump_to(&target, cx));
                            }),
                    );
                }
                detail = detail.child(labelled(
                    &theme,
                    t!("activity.affected").to_string(),
                    blocks,
                ));
                let enabled = can_undo(record) && !self.loading;
                let reason = undo_blocked_reason(record);
                let undo = this.clone();
                let button = Button::new("activity-undo")
                    .small()
                    .icon(IconName::Undo2)
                    .label(t!("activity.undo").to_string())
                    .disabled(!enabled)
                    .on_click(move |_, _, cx| undo.update(cx, |v, cx| v.undo_selected(cx)));
                undo_button = Some(match reason {
                    Some(reason) => button.tooltip(reason).into_any_element(),
                    None => button.into_any_element(),
                });
            }
        }

        let close = this.clone();
        modal(
            "agent-activity",
            &theme,
            980.,
            move |_, cx| dismiss.update(cx, |v, cx| v.close(cx)),
            v_flex()
                .child(title_bar(
                    &theme,
                    t!("activity.title").to_string(),
                    Button::new("activity-close")
                        .ghost()
                        .small()
                        .icon(IconName::Close)
                        .on_click(move |_, _, cx| close.update(cx, |v, cx| v.close(cx))),
                ))
                .child(div().px_4().pt_3().child(filters))
                .child(
                    h_flex()
                        .gap_3()
                        .p_4()
                        .h(dims::PX_420)
                        .items_start()
                        .child(list.h_full())
                        .child(detail.h_full()),
                )
                .child(
                    h_flex()
                        .gap_2()
                        .px_4()
                        .pb_3()
                        .items_center()
                        .children(undo_button)
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child(self.message.clone().unwrap_or_default()),
                        ),
                ),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme;
    use crate::ui::testing::{TestAppContext, gpui_test};

    fn record(id: &str, tool: &str, token: &str, write: bool) -> AuditRecord {
        AuditRecord {
            id: id.to_owned(),
            ts: 1_731_580_000_000,
            event: AuditEvent::Call,
            token: Some(token.to_owned()),
            client: Some("claude/1".to_owned()),
            tool: tool.to_owned(),
            args_hash: String::new(),
            args: "page=Home".to_owned(),
            affected: vec!["6f2c1b7a-0000-4000-8000-000000000001".to_owned()],
            pages: vec!["Home".to_owned()],
            result: "ok".to_owned(),
            undoable: write,
            write,
            undone: false,
            undo_of: None,
        }
    }

    #[test]
    fn filters_combine_and_undo_is_only_enabled_for_live_writes() {
        let read = record("1", "get_page", "a", false);
        let write = record("2", "append_block", "a", true);
        let mut failed = record("3", "append_block", "b", true);
        failed.result = "BLOCK_BUSY".into();
        failed.undoable = false;
        let mut undone = record("4", "update_block", "b", true);
        undone.undone = true;

        let all = ActivityFilter::default();
        assert!(
            [&read, &write, &failed, &undone]
                .iter()
                .all(|r| all.matches(r))
        );
        let writes = ActivityFilter {
            writes_only: true,
            ..all.clone()
        };
        assert!(!writes.matches(&read) && writes.matches(&write));
        let errors = ActivityFilter {
            errors_only: true,
            ..all.clone()
        };
        assert!(errors.matches(&failed) && !errors.matches(&write));
        let token_b = ActivityFilter {
            token: Some("b".into()),
            tool: Some("update_block".into()),
            ..all
        };
        assert!(token_b.matches(&undone) && !token_b.matches(&failed));

        assert!(can_undo(&write));
        assert!(!can_undo(&read), "reads cannot be undone");
        assert!(!can_undo(&failed), "no undo data");
        assert!(!can_undo(&undone), "already undone");
        assert_eq!(undo_blocked_reason(&write), None);
        assert!(undo_blocked_reason(&undone).is_some());
    }

    #[gpui_test]
    fn the_view_lists_filters_and_enables_undo_per_entry(cx: &mut TestAppContext) {
        cx.update(|cx| {
            crate::ui::init(cx);
            theme::install(cx, crate::settings::AppSettings::default(), None);
        });
        let (view, cx) = cx.add_window_view(|_, _| AgentActivityView::new());
        let mut undone = record("3", "update_block", "a", true);
        undone.undone = true;
        view.update(cx, |v, cx| {
            v.open_with_records(
                vec![
                    undone,
                    record("2", "append_block", "a", true),
                    record("1", "get_page", "b", false),
                ],
                cx,
            );
        });
        assert!(view.read_with(cx, |v, _| v.is_open()));
        assert_eq!(view.read_with(cx, |v, _| v.visible().len()), 3);
        assert_eq!(view.read_with(cx, |v, _| v.tokens()), ["a", "b"]);
        assert_eq!(
            view.read_with(cx, |v, _| v.tools()),
            ["update_block", "append_block", "get_page"]
        );
        view.update(cx, |v, cx| {
            v.set_filter(
                ActivityFilter {
                    writes_only: true,
                    ..ActivityFilter::default()
                },
                cx,
            );
        });
        assert_eq!(view.read_with(cx, |v, _| v.visible().len()), 2);
        view.update(cx, |v, cx| v.select("2", cx));
        assert!(view.read_with(cx, |v, _| v.selected().is_some_and(can_undo)));
        view.update(cx, |v, cx| v.select("3", cx));
        assert!(!view.read_with(cx, |v, _| v.selected().is_some_and(can_undo)));
        // Jumping to a block is reported to the host.
        let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let sink = seen.clone();
        let _sub = cx.update(|_, cx| {
            cx.subscribe(&view, move |_, event: &AgentActivityEvent, _| {
                sink.borrow_mut().push(event.clone());
            })
        });
        view.update(cx, |v, cx| v.jump_to("abc", cx));
        assert_eq!(
            *seen.borrow(),
            [AgentActivityEvent::OpenBlock("abc".into())]
        );
        view.update(cx, |v, cx| v.close(cx));
        assert!(!view.read_with(cx, |v, _| v.is_open()));
    }
}
