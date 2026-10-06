//! Spike for ADR-002: a custom block text element on GPUI's `EntityInputHandler`
//! (BIT-US-0040) plus a 1,000-block virtualized page and benchmark (BIT-US-0060).
//!
//! Run with `bitacora --spike-editor [--spike-blocks N] [--spike-page FILE.md]`
//! and `--spike-bench` for the automated measurement run.

pub mod bench;
pub mod buffer;
pub mod doc;
pub mod editor;
pub mod element;
pub mod frame_marker;
pub mod inline;
pub mod layout;
pub mod text_ops;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod textarea_probe;

use std::path::PathBuf;
use std::time::Instant;

use anyhow::Context as _;

use self::doc::SpikeDoc;
use self::editor::{Caret, SpikeEditor};
use crate::ui::{
    self, App, Bounds, Entity, Focusable as _, TitlebarOptions, WindowOptions, px, size,
};

/// Options of the spike window.
#[derive(Debug, Clone, Default)]
pub struct SpikeOptions {
    /// Number of generated blocks (ignored when `page` is set).
    pub blocks: usize,
    /// A Logseq page to load instead of generated content.
    pub page: Option<PathBuf>,
    /// Run the automated benchmark and exit.
    pub bench: bool,
}

/// Deterministic generated page: mixed lengths (some wrap to several rows), nesting,
/// refs, bold, tasks and CJK text.
pub fn generate_doc(blocks: usize) -> SpikeDoc {
    const WORDS: [&str; 16] = [
        "outline", "block", "graph", "journal", "sync", "merge", "index", "search", "editor",
        "markdown", "logseq", "property", "query", "page", "link", "note",
    ];
    let mut seed: u64 = 0x2545_f491_4f6c_dd1d;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let mut items = Vec::with_capacity(blocks);
    let mut depth = 0u8;
    for i in 0..blocks {
        let roll = next() % 100;
        depth = match roll {
            0..=14 => depth.saturating_sub(1),
            15..=34 => (depth + 1).min(5),
            _ => depth,
        };
        let words = match next() % 10 {
            0 => 45 + (next() % 40) as usize, // long: wraps to several rows
            1..=3 => 12 + (next() % 10) as usize,
            _ => 2 + (next() % 8) as usize,
        };
        let mut text = String::new();
        if next() % 12 == 0 {
            text.push_str("TODO ");
        }
        for w in 0..words {
            if w > 0 {
                text.push(' ');
            }
            let word = WORDS[(next() % WORDS.len() as u64) as usize];
            match next() % 14 {
                0 => text.push_str(&format!("[[{word}]]")),
                1 => text.push_str(&format!("**{word}**")),
                2 => text.push_str(&format!("`{word}`")),
                _ => text.push_str(word),
            }
        }
        if i % 97 == 0 {
            text.push_str(" \u{65e5}\u{672c}\u{8a9e}\u{306e}\u{30c6}\u{30b9}\u{30c8} \u{1f600}");
        }
        items.push((depth, format!("{text} #{i}")));
    }
    SpikeDoc::from_blocks(items)
}

/// Loads the content described by `options`.
pub fn load_doc(options: &SpikeOptions) -> anyhow::Result<SpikeDoc> {
    match &options.page {
        Some(path) => {
            let source = std::fs::read_to_string(path)
                .with_context(|| format!("reading spike page {}", path.display()))?;
            Ok(SpikeDoc::from_outline(&source))
        }
        None => Ok(generate_doc(options.blocks.max(1))),
    }
}

/// Opens the spike window (called from `app::start`).
pub fn open(
    cx: &mut App,
    options: &SpikeOptions,
    started: Instant,
) -> anyhow::Result<Entity<SpikeEditor>> {
    let doc = load_doc(options)?;
    tracing::info!(blocks = doc.len(), "spike editor: document ready");
    editor::bind_keys(cx);
    let window_options = WindowOptions {
        titlebar: Some(TitlebarOptions {
            title: Some("Bitacora - block editor spike".into()),
            ..Default::default()
        }),
        window_bounds: Some(ui::WindowBounds::Windowed(Bounds::centered(
            None,
            size(px(1000.), px(760.)),
            cx,
        ))),
        ..Default::default()
    };
    let bench = options.bench;
    let (handle, view) = ui::open_main_window(window_options, cx, |window, cx| {
        let view = editor::new_editor(doc, !bench, cx);
        view.update(cx, |ed, cx| ed.focus_block(0, Caret::End, window, cx));
        view
    })
    .context("opening the spike window")?;
    handle.update(cx, |_, window, cx| {
        view.focus_handle(cx).focus(window, cx);
        if bench {
            bench::start(view.clone(), started, window, cx);
        } else {
            window.on_next_frame(move |window, cx| {
                window.on_next_frame(move |_, _| {
                    tracing::info!(
                        "spike first frame after {} ms",
                        started.elapsed().as_millis()
                    );
                });
                let _ = cx;
            });
        }
    })?;
    Ok(view)
}
