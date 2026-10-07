//! The `--perf-bench` driver: scripted interaction with the real workspace, one phase per
//! frame tick (BIT-T-0337). Needs a graph with a 5,000-block page named `Big page` (see the
//! `generate_app_bench_graph` test of `bitacora-index`); every phase degrades to "skipped"
//! when its page is missing.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

use serde::Serialize;

use super::{Stats, stats};
use crate::editor::Caret;
use crate::editor::actions;
use crate::nav::Route;
use crate::ui::text_edit::EntityInputHandler as _;
use crate::ui::{App, Entity, Window, px};
use crate::views::page_view::LoadState;
use crate::views::workspace::Workspace;

/// Page with 5,000 blocks.
const BIG_PAGE: &str = "Big page";
/// Page with three query blocks.
const QUERY_PAGE: &str = "Query bench";
const SCROLL_WARM_FRAMES: usize = 120;
const SCROLL_FRAMES: usize = 300;
const SCROLL_STEP: f32 = 220.0;
const TYPING_SAMPLES: usize = 100;
const STRUCTURAL_SAMPLES: usize = 20;
const GIVE_UP_FRAMES: usize = 900;
const SEARCH_RUNS: usize = 20;
const QUERIES: &[&str] = &[
    "roadmap",
    "budget meeting",
    "release plan",
    "customer and risk",
    "decision not bug",
    "sprint backl",
    "\"design review\"",
    "pag 42",
    "pg4217",
    "oadma",
    "ab",
    "zzzz-no-such-term",
    "TODO launch",
    "garden health travel",
];

/// The report printed at the end as `PERF_BENCH {json}`.
#[derive(Debug, Default, Serialize)]
pub struct Report {
    /// OS and architecture.
    pub platform: String,
    /// Software rasterizer in use (no real GPU).
    pub software_gpu: Option<bool>,
    /// Named instants in ms since process start.
    pub marks: BTreeMap<String, f64>,
    /// RSS once the graph is indexed and the journals feed is showing.
    pub rss_settled_kib: Option<u64>,
    /// CPU side of a frame while scrolling the journals feed.
    pub journals_scroll_cpu: Stats,
    /// Navigate to a loaded and painted 5,000-block page (quantized by the display on Xvfb).
    pub page_open_ms: Option<f64>,
    /// Navigate until the page is loaded and adopted by the view (before its first paint).
    pub page_loaded_ms: Option<f64>,
    /// Blocks the page view holds right after opening (the first chunk).
    pub page_first_chunk_rows: usize,
    /// Interval between frames while scrolling the big page (paced by the display).
    pub scroll_frame_interval: Stats,
    /// CPU side of a frame while scrolling: frame start to end of paint.
    pub scroll_frame_cpu: Stats,
    /// Rows the page holds after scrolling (chunks fetched on demand).
    pub page_rows_after_scroll: usize,
    /// Keystroke to the end of the paint that shows it (block in the middle of the big page).
    pub keystroke_to_paint: Stats,
    /// Indent/outdent command to the end of the paint that shows it (core queue round-trip).
    pub structural_to_paint: Stats,
    /// Search over the open graph, run on the UI thread (a floor for the palette round-trip).
    pub search: Stats,
    /// RSS right after the big page opened.
    pub rss_page_open_kib: Option<u64>,
    /// RSS after scrolling the whole big page.
    pub rss_scrolled_kib: Option<u64>,
    /// RSS after the typing phase.
    pub rss_typed_kib: Option<u64>,
    /// RSS after the structural-edit phase.
    pub rss_structural_kib: Option<u64>,
    /// Anonymous memory once the graph is indexed and the journals feed is showing.
    pub rss_anon_settled_kib: Option<u64>,
    /// Anonymous memory after the whole script.
    pub rss_anon_end_kib: Option<u64>,
    /// RSS after the whole script.
    pub rss_end_kib: Option<u64>,
    /// Duration series recorded by `perf::span` (UI-thread hot spots).
    pub spans: BTreeMap<String, Stats>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Phase {
    Startup,
    Journals,
    OpenPage,
    ScrollWarm,
    Scroll,
    Typing,
    Structural,
    Search,
    QueryPage,
    Done,
}

struct Bench {
    workspace: Entity<Workspace>,
    phase: Phase,
    frame: usize,
    phase_started: Instant,
    last_tick: Option<Instant>,
    intervals: Vec<f64>,
    cpu: Vec<f64>,
    pending: Option<Instant>,
    samples: Vec<f64>,
    typed: usize,
    done_wait: usize,
    report: Report,
}

/// Starts the benchmark; call once after the window opened and the graph was requested.
pub fn start(workspace: Entity<Workspace>, window: &mut Window, _cx: &mut App) {
    super::enable();
    let report = Report {
        platform: format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
        software_gpu: window.gpu_specs().map(|s| s.is_software_emulated),
        ..Report::default()
    };
    let bench = Rc::new(RefCell::new(Bench {
        workspace,
        phase: Phase::Startup,
        frame: 0,
        phase_started: Instant::now(),
        last_tick: None,
        intervals: Vec::new(),
        cpu: Vec::new(),
        pending: None,
        samples: Vec::new(),
        typed: 0,
        done_wait: 0,
        report,
    }));
    window.on_next_frame(move |window, cx| tick(bench, window, cx));
}

fn tick(bench: Rc<RefCell<Bench>>, window: &mut Window, cx: &mut App) {
    let finished = {
        let mut b = bench.borrow_mut();
        advance(&mut b, window, cx);
        b.phase == Phase::Done
    };
    if finished {
        let mut b = bench.borrow_mut();
        b.report.marks = super::marks();
        b.report.spans = super::span_stats();
        b.report.rss_end_kib = super::rss_kib();
        b.report.rss_anon_end_kib = super::rss_anon_kib();
        match serde_json::to_string(&b.report) {
            Ok(json) => println!("PERF_BENCH {json}"),
            Err(err) => tracing::error!("serializing the perf report failed: {err}"),
        }
        cx.quit();
    } else {
        // Keep frames coming even while the UI is idle.
        window.refresh();
        window.on_next_frame(move |window, cx| tick(bench, window, cx));
    }
}

fn begin(b: &mut Bench, phase: Phase) {
    tracing::info!(target: "bitacora::perf", ?phase, "perf bench phase");
    b.phase = phase;
    b.frame = 0;
    b.phase_started = Instant::now();
    b.intervals.clear();
    b.cpu.clear();
    b.samples.clear();
    b.pending = None;
    b.typed = 0;
    b.done_wait = 0;
}

fn page_view(b: &Bench, cx: &App) -> Entity<crate::views::page_view::PageView> {
    b.workspace.read(cx).main_view().read(cx).page().clone()
}

fn navigate(b: &Bench, title: &str, cx: &mut App) {
    let route = Route::Page(title.to_owned());
    b.workspace
        .read(cx)
        .main_view()
        .clone()
        .update(cx, |main, cx| main.navigate(route, cx));
}

fn shown(b: &Bench, title: &str, cx: &App) -> bool {
    let page = page_view(b, cx);
    let page = page.read(cx);
    *page.state() == LoadState::Loaded
        && page.title() == Some(title)
        && page.is_live()
        && page.editor().is_some()
}

fn advance(b: &mut Bench, window: &mut Window, cx: &mut App) {
    let now = Instant::now();
    b.frame += 1;
    match b.phase {
        Phase::Startup => {
            if b.frame == 2 {
                super::mark("first_frame");
            }
            if super::marked("index_ready").is_some() && b.frame >= 5 {
                super::mark("startup_settled");
                b.report.rss_settled_kib = super::rss_kib();
                b.report.rss_anon_settled_kib = super::rss_anon_kib();
                begin(b, Phase::Journals);
            } else if b.phase_started.elapsed() > Duration::from_secs(300) {
                tracing::error!("perf bench: the index never became ready");
                begin(b, Phase::Done);
            }
        }
        Phase::Journals => {
            let journals = b.workspace.read(cx).main_view().read(cx).journals().clone();
            if let Some(last) = b.last_tick
                && let Some(end) = super::frame::last_paint_end()
                && end >= last
            {
                b.cpu.push(end.duration_since(last).as_secs_f64() * 1000.0);
            }
            if b.frame >= SCROLL_FRAMES / 2 {
                b.report.journals_scroll_cpu = stats(&b.cpu);
                begin(b, Phase::OpenPage);
                navigate(b, BIG_PAGE, cx);
            } else {
                journals.update(cx, |j, cx| {
                    j.list_state().scroll_by(px(SCROLL_STEP));
                    cx.notify();
                });
            }
        }
        Phase::OpenPage => {
            if shown(b, BIG_PAGE, cx) {
                if b.done_wait == 0 {
                    // The frame that painted the loaded page ends with the next tick.
                    b.done_wait = 1;
                    return;
                }
                // Navigate to the end of the paint that shows the loaded page.
                let end = super::frame::last_paint_end().unwrap_or(now);
                b.report.page_loaded_ms = super::stamped("page_loaded")
                    .map(|t| t.duration_since(b.phase_started).as_secs_f64() * 1000.0);
                b.report.page_open_ms =
                    Some(end.duration_since(b.phase_started).as_secs_f64() * 1000.0);
                b.report.page_first_chunk_rows = page_view(b, cx).read(cx).rows().len();
                b.report.rss_page_open_kib = super::rss_kib();
                begin(b, Phase::ScrollWarm);
            } else if b.phase_started.elapsed() > Duration::from_secs(60) {
                tracing::warn!("perf bench: `{BIG_PAGE}` never opened; skipping page phases");
                begin(b, Phase::Search);
            }
        }
        Phase::ScrollWarm | Phase::Scroll => {
            let page = page_view(b, cx);
            if let Some(last) = b.last_tick
                && b.phase == Phase::Scroll
            {
                b.intervals
                    .push(now.duration_since(last).as_secs_f64() * 1000.0);
                if let Some(end) = super::frame::last_paint_end()
                    && end >= last
                {
                    b.cpu.push(end.duration_since(last).as_secs_f64() * 1000.0);
                }
            }
            let limit = if b.phase == Phase::Scroll {
                SCROLL_FRAMES
            } else {
                SCROLL_WARM_FRAMES
            };
            if b.frame >= limit {
                if b.phase == Phase::ScrollWarm {
                    page.update(cx, |p, cx| {
                        p.list_state().scroll_by(px(-1.0e8));
                        cx.notify();
                    });
                    begin(b, Phase::Scroll);
                } else {
                    b.report.scroll_frame_interval = stats(&b.intervals);
                    b.report.scroll_frame_cpu = stats(&b.cpu);
                    b.report.page_rows_after_scroll = page.read(cx).rows().len();
                    b.report.rss_scrolled_kib = super::rss_kib();
                    page.update(cx, |p, cx| {
                        p.list_state().scroll_by(px(-1.0e8));
                        cx.notify();
                    });
                    begin(b, Phase::Typing);
                    start_typing_block(b, window, cx);
                }
            } else {
                page.update(cx, |p, cx| {
                    p.list_state().scroll_by(px(SCROLL_STEP));
                    cx.notify();
                });
            }
        }
        Phase::Typing => {
            if b.frame < 5 {
                return;
            }
            if b.frame > GIVE_UP_FRAMES {
                tracing::warn!("perf bench: typing phase gave up");
                begin(b, Phase::Search);
                return;
            }
            if let Some(t0) = b.pending.take()
                && let Some(end) = super::frame::last_paint_end()
                && end >= t0
            {
                b.samples
                    .push(end.duration_since(t0).as_secs_f64() * 1000.0);
            }
            if b.samples.len() >= TYPING_SAMPLES {
                b.report.keystroke_to_paint = stats(&b.samples);
                b.report.rss_typed_kib = super::rss_kib();
                begin(b, Phase::Structural);
                // Let the debounced flush of the typed text land first.
                b.done_wait = 30;
                return;
            }
            let Some(editor) = page_view(b, cx).read(cx).editor().cloned() else {
                begin(b, Phase::Search);
                return;
            };
            b.pending = Some(Instant::now());
            editor.update(cx, |ed, cx| ed.replace_text_in_range(None, "x", window, cx));
        }
        Phase::Structural => {
            if b.done_wait > 0 {
                b.done_wait -= 1;
                return;
            }
            if let Some(t0) = b.pending.take()
                && let Some(end) = super::frame::last_paint_end()
                && end >= t0
            {
                b.samples
                    .push(end.duration_since(t0).as_secs_f64() * 1000.0);
            }
            if b.samples.len() >= structural_samples() {
                b.report.structural_to_paint = stats(&b.samples);
                b.report.rss_structural_kib = super::rss_kib();
                begin(b, Phase::Search);
                return;
            }
            b.pending = Some(Instant::now());
            // Indent then outdent the edited block: both go through the core queue and rebuild
            // the row model of the whole page.
            if b.samples.len().is_multiple_of(2) {
                window.dispatch_action(Box::new(actions::Indent), cx);
            } else {
                window.dispatch_action(Box::new(actions::Outdent), cx);
            }
        }
        Phase::Search => {
            let handle = b.workspace.read(cx).graph_handle().cloned();
            if let Some(handle) = handle {
                let mut samples = Vec::new();
                for _ in 0..SEARCH_RUNS {
                    for q in QUERIES {
                        let t = Instant::now();
                        let _ = crate::views::palette::run_search(
                            &handle,
                            q,
                            crate::views::palette::SearchScope::All,
                            None,
                        );
                        samples.push(t.elapsed().as_secs_f64() * 1000.0);
                    }
                }
                b.report.search = stats(&samples);
            }
            begin(b, Phase::QueryPage);
            navigate(b, QUERY_PAGE, cx);
        }
        Phase::QueryPage => {
            // Wait for the page and for the query widgets (background tasks) to settle.
            if shown(b, QUERY_PAGE, cx) {
                b.done_wait += 1;
            }
            if b.done_wait > 120 || b.phase_started.elapsed() > Duration::from_secs(30) {
                begin(b, Phase::Done);
            }
        }
        Phase::Done => {}
    }
    b.last_tick = Some(now);
}

/// Puts a block from the middle of the big page in edit mode (it scrolls into view).
fn start_typing_block(b: &mut Bench, window: &mut Window, cx: &mut App) {
    let Some(editor) = page_view(b, cx).read(cx).editor().cloned() else {
        return;
    };
    editor.update(cx, |ed, cx| {
        let ids = ed.block_ids();
        if ids.is_empty() {
            return;
        }
        let id = ids[ids.len() / 2];
        ed.enter(id, Caret::End, window, cx);
    });
}

/// Structural samples to take (`BITACORA_PERF_STRUCTURAL` overrides the default).
fn structural_samples() -> usize {
    std::env::var("BITACORA_PERF_STRUCTURAL")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(STRUCTURAL_SAMPLES)
}
