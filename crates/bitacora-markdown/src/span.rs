//! Byte spans into the original input.

use std::ops::Range;

/// A half-open `[start, end)` range of **UTF-8 byte offsets** into the original input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Span {
    /// First byte of the span.
    pub start: usize,
    /// One past the last byte of the span.
    pub end: usize,
}

impl Span {
    /// Builds a span; `start <= end` is expected.
    #[must_use]
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    /// Number of bytes covered.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.end.saturating_sub(self.start)
    }

    /// True when the span covers no bytes.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.end <= self.start
    }

    /// The span as a `Range<usize>`.
    #[must_use]
    pub const fn range(&self) -> Range<usize> {
        self.start..self.end
    }

    /// The bytes of `input` covered by this span (empty when out of bounds).
    #[must_use]
    pub fn slice<'a>(&self, input: &'a [u8]) -> &'a [u8] {
        input.get(self.range()).unwrap_or(&[])
    }
}
