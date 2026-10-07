//! "Pando activity" (BIT-T-0436, BIT-SP-0009.R7): what Bitacora sent to and did with Pando.
//!
//! The rows come from two machine-local sources: the bounded JSONL log of `bitacora-pando`
//! (sync batches by count and block ids, agent runs, approvals, applied edits, status changes) and
//! the MCP audit log filtered to the `pando` token (calls Pando's agents made to Bitacora).
//! Nothing here shows page text. "Clear" empties the Pando log; MCP audit entries belong to the
//! agent activity dialog, which stays reachable from here with its undo buttons.

use crate::views::dims;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bitacora_mcp::{AuditEvent, AuditFilter, AuditRecord, PANDO_TOKEN_NAME};
use bitacora_runtime::{ActivityEntry, ActivityKind, ActivityLog};
use rust_i18n::t;

use crate::session::SessionHandle;
use crate::ui::theme::{ActiveBitacoraTheme as _, TypeStyleExt as _};
use crate::ui::{
    ActiveTheme as _, AnyElement, AppContext as _, Context, EventEmitter, FluentBuilder as _,
    InteractiveElement as _, IntoElement, ParentElement as _, Render,
    StatefulInteractiveElement as _, Styled as _, Task, Window, div, h_flex, v_flex,
};
use crate::views::kit::{Button, Chip, ChipTone};
use crate::views::modal::{modal, title_bar};
use crate::views::sync_panel::ago;

/// Rows read from the log.
const LOG_LIMIT: usize = 500;
/// MCP audit rows merged in.
const AUDIT_LIMIT: usize = 200;

/// What the view tells the workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PandoActivityEvent {
    /// The overlay closed.
    Closed,
    /// The user asked for the agent activity dialog (MCP writes with undo).
    OpenAgentWrites,
}

/// Which family of rows is listed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LogFilter {
    /// Everything.
    #[default]
    All,
    /// Knowledge-base sync batches.
    Sync,
    /// Agent runs, approvals and applied edits.
    Agent,
    /// MCP calls made with the `pando` token.
    Mcp,
    /// Connection status changes.
    Status,
}

impl LogFilter {
    /// Every filter, in display order.
    pub const ALL: [Self; 5] = [Self::All, Self::Sync, Self::Agent, Self::Mcp, Self::Status];

    fn label(self) -> String {
        match self {
            Self::All => t!("pando_status.log.filter_all"),
            Self::Sync => t!("pando_status.log.filter_sync"),
            Self::Agent => t!("pando_status.log.filter_agent"),
            Self::Mcp => t!("pando_status.log.filter_mcp"),
            Self::Status => t!("pando_status.log.filter_status"),
        }
        .to_string()
    }
}

/// One displayed line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogRow {
    /// Unix milliseconds.
    pub at_ms: i64,
    /// The family of the row.
    pub kind: LogFilter,
    /// What happened.
    pub text: String,
    /// Block ids or call ids, comma separated; empty when there are none.
    pub ids: String,
}

fn status_text(word: &str) -> String {
    match word {
        "ok" => t!("pando_status.state.ok"),
        "connecting" => t!("pando_status.state.connecting"),
        "unauthorized" => t!("pando_status.state.unauthorized"),
        "unreachable" => t!("pando_status.state.unreachable"),
        "too_old" => t!("pando_status.state.too_old"),
        "consent_required" => t!("pando_status.state.not_configured"),
        _ => t!("pando_status.state.disabled"),
    }
    .to_string()
}

/// The row of one Pando log entry.
#[must_use]
pub fn row_of_entry(e: &ActivityEntry) -> LogRow {
    let count = e.count;
    let (kind, text) = match (e.kind, e.summary.as_str()) {
        (ActivityKind::Sync, "deleted") => (
            LogFilter::Sync,
            t!("pando_status.log.sync_deleted", count = count).to_string(),
        ),
        (ActivityKind::Sync, _) => (
            LogFilter::Sync,
            t!("pando_status.log.sync_upserted", count = count).to_string(),
        ),
        (ActivityKind::Status, word) => (
            LogFilter::Status,
            t!("pando_status.log.status", status = status_text(word)).to_string(),
        ),
        (ActivityKind::Run, "started") => (
            LogFilter::Agent,
            t!("pando_status.log.run_started").to_string(),
        ),
        (ActivityKind::Run, "failed") => (
            LogFilter::Agent,
            t!("pando_status.log.run_failed").to_string(),
        ),
        (ActivityKind::Run, "cancelled") => (
            LogFilter::Agent,
            t!("pando_status.log.run_cancelled").to_string(),
        ),
        (ActivityKind::Run, _) => (
            LogFilter::Agent,
            t!("pando_status.log.run_finished").to_string(),
        ),
        (ActivityKind::Approval, "approved") => (
            LogFilter::Agent,
            t!("pando_status.log.approval_approved").to_string(),
        ),
        (ActivityKind::Approval, _) => (
            LogFilter::Agent,
            t!("pando_status.log.approval_denied").to_string(),
        ),
        (ActivityKind::Edit, _) => (
            LogFilter::Agent,
            t!("pando_status.log.edit_applied", count = count).to_string(),
        ),
    };
    LogRow {
        at_ms: e.at_ms,
        kind,
        text,
        ids: e.ids.join(", "),
    }
}

/// The row of one MCP audit record made with the `pando` token; `None` for other tokens and
/// non-call lines.
#[must_use]
pub fn row_of_audit(r: &AuditRecord) -> Option<LogRow> {
    if r.event != AuditEvent::Call || r.token.as_deref() != Some(PANDO_TOKEN_NAME) {
        return None;
    }
    Some(LogRow {
        at_ms: r.ts,
        kind: LogFilter::Mcp,
        text: t!(
            "pando_status.log.mcp_call",
            tool = r.tool,
            result = r.result
        )
        .to_string(),
        ids: r.affected.join(", "),
    })
}

/// Merges both sources, newest first.
#[must_use]
pub fn merge_rows(entries: &[ActivityEntry], audit: &[AuditRecord]) -> Vec<LogRow> {
    let mut rows: Vec<LogRow> = entries
        .iter()
        .map(row_of_entry)
        .chain(audit.iter().filter_map(row_of_audit))
        .collect();
    rows.sort_by_key(|r| std::cmp::Reverse(r.at_ms));
    rows
}

/// The overlay.
pub struct PandoActivityView {
    open: bool,
    log: Option<ActivityLog>,
    handle: Option<SessionHandle>,
    rows: Vec<LogRow>,
    filter: LogFilter,
    message: Option<String>,
    generation: u64,
    task: Option<Task<()>>,
}

impl std::fmt::Debug for PandoActivityView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PandoActivityView")
            .field("open", &self.open)
            .field("rows", &self.rows.len())
            .finish_non_exhaustive()
    }
}

impl EventEmitter<PandoActivityEvent> for PandoActivityView {}

impl Default for PandoActivityView {
    fn default() -> Self {
        Self::new()
    }
}

impl PandoActivityView {
    /// A closed view.
    #[must_use]
    pub fn new() -> Self {
        Self {
            open: false,
            log: None,
            handle: None,
            rows: Vec::new(),
            filter: LogFilter::All,
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

    /// The rows that pass the filter.
    #[must_use]
    pub fn visible(&self) -> Vec<&LogRow> {
        self.rows
            .iter()
            .filter(|r| self.filter == LogFilter::All || r.kind == self.filter)
            .collect()
    }

    /// The filter in effect.
    #[must_use]
    pub fn filter(&self) -> LogFilter {
        self.filter
    }

    /// Sets the filter.
    pub fn set_filter(&mut self, filter: LogFilter, cx: &mut Context<Self>) {
        self.filter = filter;
        cx.notify();
    }

    /// Opens the overlay and loads both sources.
    pub fn open_with(
        &mut self,
        log: Option<ActivityLog>,
        handle: Option<SessionHandle>,
        cx: &mut Context<Self>,
    ) {
        self.open = true;
        self.log = log;
        self.handle = handle;
        self.message = None;
        self.reload(cx);
    }

    fn reload(&mut self, cx: &mut Context<Self>) {
        self.generation += 1;
        let generation = self.generation;
        let log = self.log.clone();
        let audit = self.handle.as_ref().map(|h| {
            h.run(|s| {
                s.agent_activity(&AuditFilter {
                    token: Some(PANDO_TOKEN_NAME.to_owned()),
                    limit: Some(AUDIT_LIMIT),
                    ..AuditFilter::default()
                })
            })
        });
        let entries =
            cx.background_spawn(async move { log.map(|l| l.read(LOG_LIMIT)).unwrap_or_default() });
        self.task = Some(cx.spawn(async move |this, cx| {
            let entries = entries.await;
            let audit = match audit {
                Some(rx) => rx.recv().await.unwrap_or_default(),
                None => Vec::new(),
            };
            let _ = this.update(cx, |view, cx| {
                if view.generation == generation {
                    view.rows = merge_rows(&entries, &audit);
                    cx.notify();
                }
            });
        }));
        cx.notify();
    }

    /// Empties the Pando log (the MCP audit is not touched).
    pub fn clear(&mut self, cx: &mut Context<Self>) {
        if let Some(log) = &self.log {
            self.message = Some(match log.clear() {
                Ok(()) => t!("pando_status.log.cleared").to_string(),
                Err(e) => e.to_string(),
            });
        }
        self.reload(cx);
    }

    /// Closes the overlay.
    pub fn close(&mut self, cx: &mut Context<Self>) {
        if self.open {
            self.open = false;
            self.task = None;
            cx.emit(PandoActivityEvent::Closed);
            cx.notify();
        }
    }
}

impl Render for PandoActivityView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.open {
            return div().into_any_element();
        }
        let theme = cx.bitacora().clone();
        let kit_theme = cx.theme().clone();
        let this = cx.entity();
        let now = SystemTime::now();
        let c = &theme.colors;
        let m = &theme.metrics;

        let mut filters = h_flex().gap(m.space[2]).flex_wrap().items_center();
        for (ix, f) in LogFilter::ALL.into_iter().enumerate() {
            let pick = this.clone();
            let button = Button::new(("pando-log-filter", ix)).label(f.label());
            let button = if f == self.filter {
                button.secondary()
            } else {
                button.ghost()
            };
            filters = filters.child(
                button
                    .compact()
                    .on_click(move |_, _, cx| pick.update(cx, |v, cx| v.set_filter(f, cx))),
            );
        }

        let visible = self.visible();
        let mut list = v_flex()
            .id("pando-log-list")
            .gap(m.space[2])
            .overflow_y_scroll()
            .h(dims::PX_380);
        if visible.is_empty() {
            list = list.child(
                div()
                    .text_color(c.muted)
                    .child(t!("pando_status.log.empty").to_string()),
            );
        }
        for (ix, row) in visible.iter().enumerate() {
            let when = ago(
                UNIX_EPOCH + Duration::from_millis(row.at_ms.max(0).unsigned_abs()),
                now,
            );
            let tone = match row.kind {
                LogFilter::Sync => ChipTone::Accent,
                LogFilter::Agent => ChipTone::Ai,
                LogFilter::Mcp | LogFilter::Status | LogFilter::All => ChipTone::Outline,
            };
            list = list.child(
                v_flex()
                    .id(("pando-log-row", ix))
                    .gap(m.space[1])
                    .p(m.space[3])
                    .rounded(m.radius_control)
                    .child(
                        h_flex()
                            .gap(m.space[3])
                            .items_center()
                            .child(Chip::new(row.kind.label()).tone(tone))
                            .child(div().flex_1().min_w_0().child(row.text.clone()))
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .text_color(c.muted)
                                    .type_style(&theme.type_scale.caption)
                                    .child(when),
                            ),
                    )
                    .when(!row.ids.is_empty(), |d| {
                        d.child(
                            div()
                                .text_color(c.muted)
                                .type_style(&theme.type_scale.mono)
                                .truncate()
                                .child(t!("pando_status.log.ids", ids = row.ids).to_string()),
                        )
                    }),
            );
        }

        let dismiss = this.clone();
        let close = this.clone();
        let clear = this.clone();
        let writes = this.clone();
        let footer: AnyElement = h_flex()
            .gap(m.space[3])
            .px(m.space[5])
            .pb(m.space[4])
            .items_center()
            .child(
                Button::new("pando-log-clear")
                    .secondary()
                    .label(t!("pando_status.log.clear").to_string())
                    .on_click(move |_, _, cx| clear.update(cx, |v, cx| v.clear(cx))),
            )
            .child(
                Button::new("pando-log-agent-writes")
                    .ghost()
                    .label(t!("pando_status.log.agent_writes").to_string())
                    .on_click(move |_, _, cx| {
                        writes.update(cx, |_, cx| cx.emit(PandoActivityEvent::OpenAgentWrites));
                    }),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_color(c.muted)
                    .type_style(&theme.type_scale.caption)
                    .child(
                        self.message
                            .clone()
                            .unwrap_or_else(|| t!("pando_status.log.local_note").to_string()),
                    ),
            )
            .into_any_element();

        modal(
            "pando-activity",
            &kit_theme,
            640.,
            move |_, cx| dismiss.update(cx, |v, cx| v.close(cx)),
            v_flex()
                .child(title_bar(
                    &kit_theme,
                    t!("pando_status.log.title").to_string(),
                    Button::new("pando-log-close")
                        .ghost()
                        .label(t!("pando_status.log.close").to_string())
                        .on_click(move |_, _, cx| close.update(cx, |v, cx| v.close(cx))),
                ))
                .child(div().px(m.space[5]).pt(m.space[4]).child(filters))
                .child(div().p(m.space[5]).child(list))
                .child(footer),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme;
    use crate::ui::testing::{TestAppContext, gpui_test};

    fn entry(kind: ActivityKind, summary: &str, count: u64, at: i64) -> ActivityEntry {
        let mut e = ActivityEntry::new(kind, summary, count, vec!["b1".into(), "b2".into()]);
        e.at_ms = at;
        e
    }

    fn audit(token: &str, at: i64) -> AuditRecord {
        AuditRecord {
            id: format!("{at}-1"),
            ts: at,
            event: AuditEvent::Call,
            token: Some(token.to_owned()),
            client: None,
            tool: "search".to_owned(),
            args_hash: String::new(),
            args: String::new(),
            affected: Vec::new(),
            pages: Vec::new(),
            result: "ok".to_owned(),
            undoable: false,
            write: false,
            undone: false,
            undo_of: None,
        }
    }

    #[test]
    fn rows_merge_newest_first_and_keep_only_the_pando_token() {
        let rows = merge_rows(
            &[
                entry(ActivityKind::Sync, "upserted", 3, 100),
                entry(ActivityKind::Run, "started", 1, 300),
            ],
            &[audit(PANDO_TOKEN_NAME, 200), audit("claude", 250)],
        );
        let kinds: Vec<_> = rows.iter().map(|r| r.kind).collect();
        assert_eq!(kinds, [LogFilter::Agent, LogFilter::Mcp, LogFilter::Sync]);
        assert!(rows[2].text.contains('3'));
        assert_eq!(rows[2].ids, "b1, b2");
    }

    #[test]
    fn rows_carry_counts_and_ids_only() {
        let r = row_of_entry(&entry(ActivityKind::Edit, "applied", 2, 1));
        assert_eq!(r.kind, LogFilter::Agent);
        assert!(r.text.contains('2'));
    }

    #[gpui_test]
    fn filter_clear_and_close(cx: &mut TestAppContext) {
        cx.update(|cx| {
            crate::ui::init(cx);
            theme::install(cx, crate::settings::AppSettings::default(), None);
        });
        let dir = tempfile::tempdir().unwrap();
        let log = ActivityLog::new(dir.path().join("a.jsonl"));
        log.record(&entry(ActivityKind::Sync, "upserted", 4, 10))
            .unwrap();
        log.record(&entry(ActivityKind::Status, "ok", 0, 20))
            .unwrap();
        let (view, cx) = cx.add_window_view(|_, _| PandoActivityView::new());
        view.update(cx, |v, cx| v.open_with(Some(log.clone()), None, cx));
        cx.run_until_parked();
        assert_eq!(view.read_with(cx, |v, _| v.visible().len()), 2);
        view.update(cx, |v, cx| v.set_filter(LogFilter::Sync, cx));
        assert_eq!(view.read_with(cx, |v, _| v.visible().len()), 1);
        view.update(cx, |v, cx| v.clear(cx));
        cx.run_until_parked();
        assert!(log.read(10).is_empty());
        assert_eq!(view.read_with(cx, |v, _| v.visible().len()), 0);
        view.update(cx, |v, cx| v.close(cx));
        assert!(!view.read_with(cx, |v, _| v.is_open()));
    }
}
