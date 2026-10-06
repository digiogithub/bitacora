//! Geometry of a shaped block: visual rows (soft wrap + hard newlines), caret
//! positions, hit testing and selection rectangles. Built from GPUI `WrappedLine`s.

use std::ops::Range;

use crate::ui::text_edit::WrappedLine;
use crate::ui::{Bounds, Pixels, Point, point, px, size};

/// One visual row of a block.
#[derive(Debug, Clone, PartialEq)]
pub struct VisualRow {
    /// First byte of the row (in the whole block text).
    pub start: usize,
    /// End byte (exclusive). For a soft-wrapped row this equals the next row's start.
    pub end: usize,
    /// Index of the hard line (text between `\n`s) holding the row.
    pub line: usize,
    /// `true` for the last row of its hard line (the caret may sit at `end`).
    pub last_in_line: bool,
    /// X of the row start inside the unwrapped line.
    x0: Pixels,
}

/// The shaped block.
#[derive(Debug)]
pub struct BlockLayout {
    lines: Vec<WrappedLine>,
    line_starts: Vec<usize>,
    rows: Vec<VisualRow>,
    line_height: Pixels,
    text_len: usize,
}

impl BlockLayout {
    /// Builds the row table from lines shaped by `shape_text` (`\n` separated).
    pub fn new(lines: Vec<WrappedLine>, line_height: Pixels) -> Self {
        let mut line_starts = Vec::with_capacity(lines.len());
        let mut rows = Vec::new();
        let mut start = 0;
        for (line_ix, line) in lines.iter().enumerate() {
            line_starts.push(start);
            let unwrapped = &line.unwrapped_layout;
            let mut breaks: Vec<usize> = Vec::new();
            for b in &line.wrap_boundaries {
                if let Some(glyph) = unwrapped
                    .runs
                    .get(b.run_ix)
                    .and_then(|run| run.glyphs.get(b.glyph_ix))
                {
                    breaks.push(glyph.index);
                }
            }
            let mut row_start = 0;
            for brk in breaks.into_iter().chain([unwrapped.len]) {
                let last = brk == unwrapped.len;
                rows.push(VisualRow {
                    start: start + row_start,
                    end: start + brk,
                    line: line_ix,
                    last_in_line: last,
                    x0: unwrapped.x_for_index(row_start),
                });
                row_start = brk;
            }
            start += line.text.len() + 1;
        }
        let text_len = lines
            .iter()
            .map(|l| l.text.len() + 1)
            .sum::<usize>()
            .saturating_sub(1);
        Self {
            lines,
            line_starts,
            rows,
            line_height,
            text_len,
        }
    }

    /// Row height.
    pub fn line_height(&self) -> Pixels {
        self.line_height
    }

    /// Visual rows of the block.
    pub fn rows(&self) -> &[VisualRow] {
        &self.rows
    }

    /// Number of visual rows (at least one).
    pub fn row_count(&self) -> usize {
        self.rows.len().max(1)
    }

    /// Total height.
    pub fn height(&self) -> Pixels {
        self.line_height * self.row_count()
    }

    /// The shaped lines (for painting).
    pub fn lines(&self) -> &[WrappedLine] {
        &self.lines
    }

    /// Byte offset where hard line `line` starts.
    pub fn line_start(&self, line: usize) -> usize {
        self.line_starts.get(line).copied().unwrap_or(self.text_len)
    }

    /// The row holding `offset`: the last row starting at or before it, so a caret at a
    /// soft-wrap boundary sits at the start of the next row.
    pub fn row_for_index(&self, offset: usize) -> usize {
        self.rows
            .iter()
            .rposition(|r| r.start <= offset)
            .unwrap_or(0)
    }

    /// Caret position of `offset`, relative to the block's top-left corner.
    pub fn position_for_index(&self, offset: usize) -> Point<Pixels> {
        let Some(row) = self.rows.get(self.row_for_index(offset)) else {
            return point(px(0.), px(0.));
        };
        let line = &self.lines[row.line];
        let local = offset.clamp(row.start, row.end) - self.line_starts[row.line];
        let x = line.unwrapped_layout.x_for_index(local) - row.x0;
        point(x.max(px(0.)), self.line_height * self.row_for_index(offset))
    }

    /// Byte offset closest to `x` on visual row `row_ix`.
    pub fn index_on_row(&self, row_ix: usize, x: Pixels) -> usize {
        let Some(row) = self.rows.get(row_ix.min(self.rows.len().saturating_sub(1))) else {
            return 0;
        };
        let line = &self.lines[row.line];
        let base = self.line_starts[row.line];
        let local = line
            .unwrapped_layout
            .closest_index_for_x(x.max(px(0.)) + row.x0);
        let mut index = (base + local).clamp(row.start, row.end);
        if !row.last_in_line && index == row.end {
            // The end of a soft-wrapped row is the start of the next one: stay on this row.
            let text: &str = &line.text;
            index =
                base + super::text_ops::clamp_to_boundary(text, (row.end - base).saturating_sub(1));
            index = index.max(row.start);
        }
        index
    }

    /// Byte offset closest to `position` (relative to the block's top-left corner).
    pub fn index_for_position(&self, position: Point<Pixels>) -> usize {
        if position.y < px(0.) {
            return 0;
        }
        let row = (position.y / self.line_height) as usize;
        if row >= self.row_count() {
            return self.text_len;
        }
        self.index_on_row(row, position.x)
    }

    /// Rectangles covering `range`, one per visual row (relative to the top-left corner).
    pub fn selection_rects(&self, range: Range<usize>) -> Vec<Bounds<Pixels>> {
        let mut rects = Vec::new();
        for (ix, row) in self.rows.iter().enumerate() {
            let a = range.start.max(row.start);
            let b = range.end.min(row.end);
            let hard_break_selected =
                row.last_in_line && range.end > row.end && range.start <= row.end;
            if a >= b && !hard_break_selected {
                continue;
            }
            let (a, b) = (a.min(b), b.max(a));
            let x0 = self.position_for_index(a.max(row.start)).x;
            let mut x1 = if b >= row.end && !row.last_in_line {
                self.row_width(ix)
            } else {
                self.position_for_index(b).x
            };
            if hard_break_selected {
                x1 += px(4.);
            }
            if x1 > x0 {
                rects.push(Bounds::new(
                    point(x0, self.line_height * ix),
                    size(x1 - x0, self.line_height),
                ));
            }
        }
        rects
    }

    fn row_width(&self, row_ix: usize) -> Pixels {
        let row = &self.rows[row_ix];
        let line = &self.lines[row.line];
        let local_end = row.end - self.line_starts[row.line];
        line.unwrapped_layout.x_for_index(local_end) - row.x0
    }

    /// Bounds for the IME candidate window: the caret or selection start, on one row.
    pub fn bounds_for_range(&self, range: Range<usize>) -> Bounds<Pixels> {
        let start = self.position_for_index(range.start);
        let row = self.row_for_index(range.start);
        let end_x = if range.end > range.start && self.row_for_index(range.end) == row {
            self.position_for_index(range.end).x
        } else {
            start.x
        };
        Bounds::new(start, size((end_x - start.x).max(px(0.)), self.line_height))
    }

    /// Total text length in bytes.
    pub fn text_len(&self) -> usize {
        self.text_len
    }
}
