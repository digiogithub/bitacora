//! Measurement harness for the spike page (BIT-T-0090): cold start, scrolling frame
//! times (with and without the inline cache), keystroke-to-paint latency in a block
//! mid-page, fold/unfold cost, RSS. Frame driven through `Window::on_next_frame`.
//!
//! Targets (stated up front, BIT-US-0060): 60 fps scroll (p95 frame interval below
//! 16.7 ms), keystroke-to-paint below 16 ms (p95), cold start below 1 s, RSS reported.
//! Output: one `SPIKE_BENCH {json}` line on stdout, then the app quits.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use serde::Serialize;

use super::editor::{Caret, SpikeEditor};
use crate::ui::text_edit::EntityInputHandler as _;
use crate::ui::{App, Entity, Window, px};

const WARMUP_FRAMES: usize = 5;
const SCROLL_FRAMES: usize = 240;
const SCROLL_STEP: f32 = 130.;
const TYPING_SAMPLES: usize = 100;
const TYPING_GIVE_UP_FRAMES: usize = 600;

/// Latency/time statistics in milliseconds.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct Stats {
    /// Number of samples.
    pub n: usize,
    /// Mean.
    pub mean: f64,
    /// Median.
    pub p50: f64,
    /// 95th percentile.
    pub p95: f64,
    /// 99th percentile.
    pub p99: f64,
    /// Maximum.
    pub max: f64,
}

/// Computes [`Stats`] (nearest-rank percentiles).
pub fn stats(samples: &[f64]) -> Stats {
    if samples.is_empty() {
        return Stats::default();
    }
    let mut sorted = samples.to_vec();
    sorted.sort_by(f64::total_cmp);
    let rank = |p: f64| {
        let ix = ((p / 100.) * sorted.len() as f64).ceil() as usize;
        sorted[ix.clamp(1, sorted.len()) - 1]
    };
    Stats {
        n: sorted.len(),
        mean: sorted.iter().sum::<f64>() / sorted.len() as f64,
        p50: rank(50.),
        p95: rank(95.),
        p99: rank(99.),
        max: sorted[sorted.len() - 1],
    }
}

/// Resident set size in KiB (Linux `/proc`; `None` elsewhere).
pub fn rss_kib() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|l| l.starts_with("VmRSS:"))?;
    line.split_whitespace().nth(1)?.parse().ok()
}

/// The report printed at the end.
#[derive(Debug, Default, Serialize)]
pub struct Report {
    /// OS and architecture.
    pub platform: String,
    /// Software rasterizer in use (no real GPU).
    pub software_gpu: Option<bool>,
    /// Block count.
    pub blocks: usize,
    /// Visible rows at start.
    pub rows: usize,
    /// Process start to second rendered frame.
    pub cold_start_ms: f64,
    /// RSS after the first frames.
    pub rss_start_kib: Option<u64>,
    /// RSS after scrolling the whole page.
    pub rss_end_kib: Option<u64>,
    /// Frame interval while scrolling, inline cache on (paced by the display on Xvfb).
    pub scroll_cached: Stats,
    /// Frame interval while scrolling, inline cache off.
    pub scroll_uncached: Stats,
    /// CPU time per frame while scrolling (render to end of paint), cache on.
    pub scroll_cpu_cached: Stats,
    /// CPU time per frame while scrolling, cache off.
    pub scroll_cpu_uncached: Stats,
    /// Rows rendered per frame while scrolling (mean).
    pub rows_per_frame: f64,
    /// CPU time spent building row elements per frame (mean, ms).
    pub row_build_ms_per_frame: f64,
    /// Edit to end of the paint of the edited block (CPU side).
    pub keystroke_to_paint: Stats,
    /// Collapse of the largest subtree: `splice` + rebuild, CPU ms.
    pub fold_ms: f64,
    /// Rows hidden by that collapse.
    pub fold_rows: usize,
    /// Pass/fail against the stated targets.
    pub targets: Vec<(String, bool)>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Phase {
    Warmup,
    // Unrecorded pass: lets the list measure every row once.
    ScrollWarm,
    ScrollCached,
    ScrollUncached,
    Typing,
    Fold,
    Done,
}

struct Bench {
    view: Entity<SpikeEditor>,
    started: Instant,
    phase: Phase,
    frame: usize,
    last_tick: Option<Instant>,
    frames: Vec<f64>,
    cpu: Vec<f64>,
    typing_t0: Option<Instant>,
    typing: Vec<f64>,
    rows_before: u64,
    build_before: Duration,
    report: Report,
}

/// Starts the benchmark; call once after the window opened.
pub fn start(view: Entity<SpikeEditor>, started: Instant, window: &mut Window, cx: &mut App) {
    let (blocks, rows) = view.read_with(cx, |ed, _| (ed.doc().len(), ed.rows().len()));
    let report = Report {
        platform: format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
        software_gpu: window.gpu_specs().map(|s| s.is_software_emulated),
        blocks,
        rows,
        ..Report::default()
    };
    let bench = Rc::new(RefCell::new(Bench {
        view,
        started,
        phase: Phase::Warmup,
        frame: 0,
        last_tick: None,
        frames: Vec::new(),
        cpu: Vec::new(),
        typing_t0: None,
        typing: Vec::new(),
        rows_before: 0,
        build_before: Duration::ZERO,
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
        let b = bench.borrow();
        match serde_json::to_string(&b.report) {
            Ok(json) => println!("SPIKE_BENCH {json}"),
            Err(err) => tracing::error!("serializing the bench report failed: {err}"),
        }
        cx.quit();
    } else {
        window.on_next_frame(move |window, cx| tick(bench, window, cx));
    }
}

fn scroll_step(b: &Bench, cx: &mut App) {
    b.view.update(cx, |ed, cx| {
        let state = ed.list_state();
        if state.is_scrolled_to_end() == Some(true) {
            state.scroll_by(px(-1.0e7));
        } else {
            state.scroll_by(px(SCROLL_STEP));
        }
        cx.notify();
    });
}

fn advance(b: &mut Bench, window: &mut Window, cx: &mut App) {
    let now = Instant::now();
    b.frame += 1;
    match b.phase {
        Phase::Warmup => {
            if b.frame == 2 {
                b.report.cold_start_ms = b.started.elapsed().as_secs_f64() * 1000.;
            }
            if b.frame >= WARMUP_FRAMES {
                b.report.rss_start_kib = rss_kib();
                begin(b, Phase::ScrollWarm, cx);
            }
        }
        Phase::ScrollWarm | Phase::ScrollCached | Phase::ScrollUncached => {
            if let Some(last) = b.last_tick {
                b.frames
                    .push(now.duration_since(last).as_secs_f64() * 1000.);
                if let Some(cpu) = b.view.read_with(cx, |ed, _| ed.frame_cpu()) {
                    b.cpu.push(cpu.as_secs_f64() * 1000.);
                }
            }
            if b.frames.len() >= SCROLL_FRAMES {
                let measured = stats(&b.frames);
                let (rows, build) = b
                    .view
                    .read_with(cx, |ed, _| (ed.rows_rendered(), ed.row_build_time()));
                if b.phase == Phase::ScrollWarm {
                    b.report.rss_end_kib = rss_kib();
                    begin(b, Phase::ScrollCached, cx);
                } else if b.phase == Phase::ScrollCached {
                    let frames = b.frames.len().max(1) as f64;
                    b.report.rows_per_frame = (rows - b.rows_before) as f64 / frames;
                    b.report.row_build_ms_per_frame =
                        (build - b.build_before).as_secs_f64() * 1000. / frames;
                    b.report.scroll_cached = measured;
                    b.report.scroll_cpu_cached = stats(&b.cpu);
                    begin(b, Phase::ScrollUncached, cx);
                } else {
                    b.report.scroll_uncached = measured;
                    b.report.scroll_cpu_uncached = stats(&b.cpu);
                    b.view.update(cx, |ed, _| ed.set_cache_enabled(true));
                    begin(b, Phase::Typing, cx);
                    // Focus a block in the middle of the page and let it settle.
                    b.view.update(cx, |ed, cx| {
                        let mid = ed.rows()[ed.rows().len() / 2];
                        ed.focus_block(mid, Caret::End, window, cx);
                    });
                }
            } else {
                scroll_step(b, cx);
            }
        }
        Phase::Typing => {
            // Frames right after the focus change are warm-up; then one edit per frame.
            if b.frame < 4 {
                return;
            }
            if b.frame > TYPING_GIVE_UP_FRAMES {
                tracing::warn!("typing phase gave up: the focused row never painted");
                begin(b, Phase::Fold, cx);
                return;
            }
            // A far-away row is unmeasured: keep revealing it until it has been laid out.
            let visible = b.view.update(cx, |ed, cx| {
                let row = ed.focused().and_then(|f| ed.rows().binary_search(&f).ok());
                let laid_out = row.is_some_and(|r| ed.list_state().bounds_for_item(r).is_some());
                if !laid_out {
                    if let Some(r) = row {
                        ed.scroll_to_row(r.saturating_sub(2));
                    }
                    cx.notify();
                }
                laid_out
            });
            if !visible {
                return;
            }
            if let Some(t0) = b.typing_t0.take()
                && let Some(done) = b.view.read_with(cx, |ed, _| ed.last_paint_at())
                && done >= t0
            {
                b.typing.push(done.duration_since(t0).as_secs_f64() * 1000.);
            }
            if b.typing.len() >= TYPING_SAMPLES {
                b.report.keystroke_to_paint = stats(&b.typing);
                begin(b, Phase::Fold, cx);
                return;
            }
            b.typing_t0 = Some(Instant::now());
            b.view.update(cx, |ed, cx| {
                ed.replace_text_in_range(None, "x", window, cx);
            });
        }
        Phase::Fold => {
            // Largest subtree of the page: collapse it and time the splice.
            let (target, hidden) = b.view.read_with(cx, |ed, _| {
                let doc = ed.doc();
                (0..doc.len())
                    .map(|ix| (ix, doc.descendants(ix).len()))
                    .max_by_key(|(_, n)| *n)
                    .unwrap_or((0, 0))
            });
            let t0 = Instant::now();
            b.view
                .update(cx, |ed, cx| ed.set_collapsed(target, true, cx));
            b.report.fold_ms = t0.elapsed().as_secs_f64() * 1000.;
            b.report.fold_rows = hidden;
            finish(b);
        }
        Phase::Done => {}
    }
    b.last_tick = Some(now);
}

fn begin(b: &mut Bench, phase: Phase, cx: &mut App) {
    tracing::info!(?phase, "spike bench phase");
    b.phase = phase;
    b.frame = 0;
    b.frames.clear();
    b.cpu.clear();
    b.last_tick = None;
    if matches!(
        phase,
        Phase::ScrollWarm | Phase::ScrollCached | Phase::ScrollUncached
    ) {
        let cached = phase != Phase::ScrollUncached;
        b.view.update(cx, |ed, cx| {
            ed.set_cache_enabled(cached);
            ed.list_state().scroll_by(px(-1.0e7));
            cx.notify();
        });
        let (rows, build) = b
            .view
            .read_with(cx, |ed, _| (ed.rows_rendered(), ed.row_build_time()));
        b.rows_before = rows;
        b.build_before = build;
    }
}

fn finish(b: &mut Bench) {
    let r = &mut b.report;
    r.targets = vec![
        (
            "scroll CPU p95 per frame < 16.7 ms".to_owned(),
            r.scroll_cpu_cached.p95 < 16.7,
        ),
        (
            "scroll p95 frame interval < 16.7 ms (display-paced)".to_owned(),
            r.scroll_cached.p95 < 16.7,
        ),
        (
            "keystroke-to-paint p95 < 16 ms".to_owned(),
            r.keystroke_to_paint.p95 < 16.0,
        ),
        ("cold start < 1000 ms".to_owned(), r.cold_start_ms < 1000.),
    ];
    b.phase = Phase::Done;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentiles_use_nearest_rank() {
        let samples: Vec<f64> = (1..=100).map(f64::from).collect();
        let s = stats(&samples);
        assert_eq!(
            (s.n, s.p50, s.p95, s.p99, s.max),
            (100, 50., 95., 99., 100.)
        );
        assert!((s.mean - 50.5).abs() < 1e-9);
        assert_eq!(stats(&[]), Stats::default());
        assert_eq!(stats(&[7.]).p99, 7.);
    }
}
