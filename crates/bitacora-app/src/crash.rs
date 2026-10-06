//! Crash reporting glue (BIT-US-0111). Reports are written by `bitacora_runtime::crash`; this
//! module installs the hook for the app, records a report for an abnormal previous exit, and
//! shows the "previous session crashed" notice with view / copy / open-issue choices.
//! Nothing is ever uploaded: the issue URL only opens the browser on a prefilled form.

use std::path::PathBuf;

use bitacora_runtime::crash::{
    CrashConfig, CrashReport, GraphRoots, abnormal_exit_report, dismiss, install_panic_hook,
    pending,
};
use rust_i18n::t;

use crate::paths::AppDirs;
use crate::recent::RecentGraphs;
use crate::ui::{
    AnyWindowHandle, App, Confirmation, Level, SharedString, choose, notify_action, text_edit,
};

/// Maximum report text shown inside the dialog (the full text is what "Copy" puts on the
/// clipboard).
const DIALOG_PREVIEW_CHARS: usize = 1200;

/// The crash configuration of the desktop app.
pub fn config(dirs: &AppDirs) -> CrashConfig {
    let recent_file = dirs.recent_graphs_file();
    CrashConfig {
        dir: dirs.data_dir.join("crashes"),
        log_dir: Some(dirs.log_dir.clone()),
        binary: "bitacora".to_owned(),
        graphs: GraphRoots::default().with_source(move || {
            RecentGraphs::load(&recent_file)
                .graphs()
                .iter()
                .map(|g| g.path.clone())
                .collect()
        }),
    }
}

/// Installs the panic hook and, when the previous owner left its instance marker behind (it
/// ended without a clean shutdown), records an `abnormal_exit` report from the log tail.
pub fn init(cfg: &CrashConfig, previous_abnormal_pid: Option<u32>) {
    install_panic_hook(cfg.clone());
    if let Some(pid) = previous_abnormal_pid
        && let Some(path) = abnormal_exit_report(cfg, pid)
    {
        tracing::warn!(report = %path.display(), "previous session ended abnormally");
    }
}

/// Reports waiting for review, oldest first.
pub fn pending_reports(cfg: &CrashConfig) -> Vec<(PathBuf, CrashReport)> {
    pending(&cfg.dir)
}

/// Text shown in the review dialog: the head of the report.
fn preview(report: &CrashReport) -> String {
    let text = report.to_text();
    if text.chars().count() <= DIALOG_PREVIEW_CHARS {
        return text;
    }
    let mut cut: String = text.chars().take(DIALOG_PREVIEW_CHARS).collect();
    cut.push_str("\n...");
    cut
}

/// Shows the notice for the newest pending report. Showing the dialog counts as reviewed: the
/// report moves to `crashes/dismissed/` (kept on disk), so the notice appears once.
pub fn show_pending(handle: AnyWindowHandle, cfg: &CrashConfig, cx: &mut App) {
    let mut reports = pending_reports(cfg);
    let Some((path, report)) = reports.pop() else {
        return;
    };
    // Older reports are archived silently: only the newest is worth the user's attention.
    for (old, _) in reports {
        let _ = dismiss(&old);
    }
    let _ = handle.update(cx, |_, window, cx| {
        notify_action(
            window,
            cx,
            Level::Warning,
            t!("crash.notice").to_string(),
            t!("crash.review").to_string(),
            move |window, cx| {
                let _ = dismiss(&path);
                let (for_issue, for_copy) = (report.clone(), report.clone());
                let shown = choose(
                    window,
                    cx,
                    Confirmation {
                        title: t!("crash.title").to_string(),
                        description: format!("{}\n\n{}", t!("crash.explain"), preview(&report)),
                        ok_text: t!("crash.open_issue").to_string(),
                        cancel_text: t!("crash.copy").to_string(),
                    },
                    move |_, cx| {
                        cx.open_url(&for_issue.issue_url(env!("CARGO_PKG_REPOSITORY")));
                    },
                    move |_, cx| {
                        let text: SharedString = for_copy.to_text().into();
                        cx.write_to_clipboard(text_edit::ClipboardItem::new_string(
                            text.to_string(),
                        ));
                    },
                );
                if !shown {
                    tracing::info!("crash dialog could not be shown (no root window)");
                }
            },
        );
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use bitacora_runtime::crash::{CrashKind, write_report};

    #[test]
    fn config_lives_in_the_data_dir_and_lists_pending_reports() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let dirs = AppDirs::with_root(tmp.path());
        let cfg = config(&dirs);
        assert!(cfg.dir.starts_with(&dirs.data_dir));
        assert!(pending_reports(&cfg).is_empty());
        let report = CrashReport {
            kind: CrashKind::Panic,
            binary: "bitacora".into(),
            version: "0".into(),
            os: "linux".into(),
            arch: "x86_64".into(),
            timestamp: "2026-01-01T00:00:00Z".into(),
            pid: 1,
            thread: None,
            message: Some("x".into()),
            location: None,
            backtrace: None,
            log_tail: Vec::new(),
        };
        write_report(&cfg.dir, &report).expect("write");
        assert_eq!(pending_reports(&cfg).len(), 1);
        assert!(preview(&report).contains("bitacora 0"));
    }

    #[test]
    fn abnormal_marker_produces_a_report_once() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let cfg = config(&AppDirs::with_root(tmp.path()));
        // `init` also installs the panic hook; exercise only the report path here.
        assert!(abnormal_exit_report(&cfg, 99).is_some());
        assert!(pending_reports(&cfg).iter().any(|(_, r)| r.pid == 99));
    }
}
