//! Text buffer of the focused block: UTF-8 text, selection, IME marked range and a
//! snapshot-based in-block undo stack. Pure data, no GPUI types.

use std::ops::Range;

use super::text_ops::{
    clamp_range, clamp_to_boundary, next_grapheme, next_word, prev_grapheme, prev_word,
    range_from_utf16,
};

const UNDO_LIMIT: usize = 200;

/// What produced an edit; consecutive typing coalesces into one undo step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditKind {
    /// Characters typed (or committed by an IME) at the caret.
    Typing,
    /// Anything else (delete, paste, cut, structural replace).
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Snapshot {
    text: String,
    selection: Range<usize>,
    reversed: bool,
}

/// The editable state of one block.
#[derive(Debug, Clone)]
pub struct BlockBuffer {
    text: String,
    selection: Range<usize>,
    reversed: bool,
    marked: Option<Range<usize>>,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    last_kind: Option<EditKind>,
}

impl BlockBuffer {
    /// A buffer with the caret at the end of `text`.
    pub fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        let end = text.len();
        Self {
            text,
            selection: end..end,
            reversed: false,
            marked: None,
            undo: Vec::new(),
            redo: Vec::new(),
            last_kind: None,
        }
    }

    /// A buffer with the caret at `offset`.
    pub fn with_cursor(text: impl Into<String>, offset: usize) -> Self {
        let mut buffer = Self::new(text);
        buffer.set_cursor(offset);
        buffer
    }

    /// The full text.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Selection in UTF-8 bytes (empty = caret).
    pub fn selection(&self) -> Range<usize> {
        self.selection.clone()
    }

    /// `true` if the selection head is at its start.
    pub fn reversed(&self) -> bool {
        self.reversed
    }

    /// The IME composition range in UTF-8 bytes.
    pub fn marked(&self) -> Option<Range<usize>> {
        self.marked.clone()
    }

    /// The caret (selection head).
    pub fn cursor(&self) -> usize {
        if self.reversed {
            self.selection.start
        } else {
            self.selection.end
        }
    }

    /// The selected text.
    pub fn selected_text(&self) -> &str {
        &self.text[self.selection.clone()]
    }

    /// Collapses the selection to `offset`.
    pub fn set_cursor(&mut self, offset: usize) {
        let offset = clamp_to_boundary(&self.text, offset);
        self.selection = offset..offset;
        self.reversed = false;
        self.last_kind = None;
    }

    /// Replaces the selection with `range` (anchor/head order given by `reversed`).
    pub fn set_selection(&mut self, range: Range<usize>, reversed: bool) {
        self.selection = clamp_range(&self.text, range);
        self.reversed = reversed && !self.selection.is_empty();
        self.last_kind = None;
    }

    /// Moves the head to `offset`, keeping the anchor.
    pub fn select_to(&mut self, offset: usize) {
        let offset = clamp_to_boundary(&self.text, offset);
        if self.reversed {
            self.selection.start = offset;
        } else {
            self.selection.end = offset;
        }
        if self.selection.end < self.selection.start {
            self.reversed = !self.reversed;
            self.selection = self.selection.end..self.selection.start;
        }
        self.last_kind = None;
    }

    /// Selects everything.
    pub fn select_all(&mut self) {
        self.selection = 0..self.text.len();
        self.reversed = false;
        self.last_kind = None;
    }

    /// Left arrow: collapses a selection to its start, else one grapheme back.
    pub fn move_left(&mut self) {
        if self.selection.is_empty() {
            self.set_cursor(prev_grapheme(&self.text, self.cursor()));
        } else {
            self.set_cursor(self.selection.start);
        }
    }

    /// Right arrow: collapses a selection to its end, else one grapheme forward.
    pub fn move_right(&mut self) {
        if self.selection.is_empty() {
            self.set_cursor(next_grapheme(&self.text, self.cursor()));
        } else {
            self.set_cursor(self.selection.end);
        }
    }

    /// Offset one grapheme before the caret.
    pub fn prev_grapheme_offset(&self) -> usize {
        prev_grapheme(&self.text, self.cursor())
    }

    /// Offset one grapheme after the caret.
    pub fn next_grapheme_offset(&self) -> usize {
        next_grapheme(&self.text, self.cursor())
    }

    /// Offset of the previous word start.
    pub fn prev_word_offset(&self) -> usize {
        prev_word(&self.text, self.cursor())
    }

    /// Offset of the next word end.
    pub fn next_word_offset(&self) -> usize {
        next_word(&self.text, self.cursor())
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            text: self.text.clone(),
            selection: self.selection.clone(),
            reversed: self.reversed,
        }
    }

    fn push_undo(&mut self) {
        if self.undo.len() == UNDO_LIMIT {
            self.undo.remove(0);
        }
        let snap = self.snapshot();
        self.undo.push(snap);
        self.redo.clear();
    }

    /// Splices `new_text` over `range`, records undo and puts the caret after it.
    /// Does not touch the marked range (callers decide).
    fn splice(&mut self, range: Range<usize>, new_text: &str, kind: EditKind) {
        let range = clamp_range(&self.text, range);
        let coalesce = kind == EditKind::Typing
            && self.last_kind == Some(EditKind::Typing)
            && self.selection.is_empty()
            && self.selection.start == range.start
            && range.is_empty()
            && !new_text.chars().any(char::is_whitespace);
        if self.marked.is_none() && !coalesce {
            self.push_undo();
        }
        self.text.replace_range(range.clone(), new_text);
        let caret = range.start + new_text.len();
        self.selection = caret..caret;
        self.reversed = false;
        self.last_kind = Some(kind);
    }

    /// Replaces the selection (or `range`) with typed text. Used by key handlers.
    pub fn insert(&mut self, new_text: &str) {
        let range = self.marked.clone().unwrap_or_else(|| self.selection());
        self.splice(range, new_text, EditKind::Typing);
        self.marked = None;
    }

    /// Replaces an arbitrary range (paste, programmatic edit).
    pub fn replace_range(&mut self, range: Range<usize>, new_text: &str) {
        self.splice(range, new_text, EditKind::Other);
        self.marked = None;
    }

    /// Backspace: deletes the selection or one grapheme. Returns `false` at offset 0.
    pub fn delete_backward(&mut self) -> bool {
        if self.selection.is_empty() {
            let prev = self.prev_grapheme_offset();
            if prev == self.cursor() {
                return false;
            }
            self.replace_range(prev..self.cursor(), "");
        } else {
            self.replace_range(self.selection(), "");
        }
        true
    }

    /// Delete: deletes the selection or one grapheme forward. Returns `false` at the end.
    pub fn delete_forward(&mut self) -> bool {
        if self.selection.is_empty() {
            let next = self.next_grapheme_offset();
            if next == self.cursor() {
                return false;
            }
            self.replace_range(self.cursor()..next, "");
        } else {
            self.replace_range(self.selection(), "");
        }
        true
    }

    /// Deletes `range` (word deletion and similar).
    pub fn delete_range(&mut self, range: Range<usize>) {
        self.replace_range(range, "");
    }

    // ----- Platform input method API (offsets in UTF-16) -----

    /// `replaceTextInRange`: commits `new_text`. The range defaults to the marked
    /// range, then to the selection.
    pub fn ime_replace(&mut self, range16: Option<Range<usize>>, new_text: &str) {
        let range = range16
            .map(|r| range_from_utf16(&self.text, &r))
            .or_else(|| self.marked.clone())
            .unwrap_or_else(|| self.selection());
        self.splice(range, new_text, EditKind::Typing);
        self.marked = None;
    }

    /// `setMarkedText`: replaces the range with composition text and marks it.
    /// `selected16` is relative to the start of `new_text` (UTF-16).
    pub fn ime_replace_and_mark(
        &mut self,
        range16: Option<Range<usize>>,
        new_text: &str,
        selected16: Option<Range<usize>>,
    ) {
        let range = range16
            .map(|r| range_from_utf16(&self.text, &r))
            .or_else(|| self.marked.clone())
            .unwrap_or_else(|| self.selection());
        // One undo step for the whole composition: snapshot only when it starts.
        let starting = self.marked.is_none();
        let range = clamp_range(&self.text, range);
        if starting {
            self.push_undo();
        }
        self.text.replace_range(range.clone(), new_text);
        self.marked = (!new_text.is_empty()).then(|| range.start..range.start + new_text.len());
        let selected = selected16
            .map(|r| range_from_utf16(new_text, &r))
            .map(|r| range.start + r.start..range.start + r.end)
            .unwrap_or_else(|| {
                let caret = range.start + new_text.len();
                caret..caret
            });
        self.selection = selected;
        self.reversed = false;
        self.last_kind = Some(EditKind::Other);
    }

    /// `unmarkText`: keeps the text, ends the composition.
    pub fn ime_unmark(&mut self) {
        self.marked = None;
    }

    // ----- Undo -----

    /// Undoes the last step. Returns `false` when there is nothing to undo.
    pub fn undo(&mut self) -> bool {
        let Some(snap) = self.undo.pop() else {
            return false;
        };
        let current = self.snapshot();
        self.redo.push(current);
        self.restore(snap);
        true
    }

    /// Redoes the last undone step.
    pub fn redo(&mut self) -> bool {
        let Some(snap) = self.redo.pop() else {
            return false;
        };
        let current = self.snapshot();
        self.undo.push(current);
        self.restore(snap);
        true
    }

    fn restore(&mut self, snap: Snapshot) {
        self.text = snap.text;
        self.selection = snap.selection;
        self.reversed = snap.reversed;
        self.marked = None;
        self.last_kind = None;
    }

    /// Number of undo steps (tests, HUD).
    pub fn undo_depth(&self) -> usize {
        self.undo.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typing_coalesces_into_one_undo_step_per_word() {
        let mut b = BlockBuffer::new("");
        for ch in "hello".chars() {
            b.insert(&ch.to_string());
        }
        assert_eq!(b.undo_depth(), 1);
        b.insert(" ");
        b.insert("w");
        assert_eq!(b.text(), "hello w");
        assert!(b.undo());
        assert_eq!(b.text(), "hello");
        assert!(b.undo());
        assert_eq!(b.text(), "");
        assert!(!b.undo());
        assert!(b.redo());
        assert_eq!(b.text(), "hello");
    }

    #[test]
    fn caret_motion_breaks_coalescing() {
        let mut b = BlockBuffer::new("");
        b.insert("a");
        b.set_cursor(0);
        b.set_cursor(1);
        b.insert("b");
        assert_eq!(b.undo_depth(), 2);
    }

    #[test]
    fn selection_extends_and_flips() {
        let mut b = BlockBuffer::with_cursor("hello", 2);
        b.select_to(4);
        assert_eq!(b.selection(), 2..4);
        assert!(!b.reversed());
        b.select_to(0);
        assert_eq!(b.selection(), 0..2);
        assert!(b.reversed());
        assert_eq!(b.cursor(), 0);
        b.move_right();
        assert_eq!(b.selection(), 2..2);
    }

    #[test]
    fn delete_backward_removes_whole_grapheme_and_reports_start() {
        let mut b = BlockBuffer::new("e\u{301}");
        assert!(b.delete_backward());
        assert_eq!(b.text(), "");
        assert!(!b.delete_backward());
        let mut b = BlockBuffer::with_cursor("abc", 1);
        b.select_to(3);
        assert!(b.delete_forward());
        assert_eq!(b.text(), "a");
    }

    #[test]
    fn ime_composition_replaces_marked_text_and_is_one_undo_step() {
        let mut b = BlockBuffer::new("x");
        // Japanese-style composition: "k" -> "か" -> candidate "漢" committed.
        b.ime_replace_and_mark(None, "k", Some(1..1));
        assert_eq!(b.text(), "xk");
        assert_eq!(b.marked(), Some(1..2));
        b.ime_replace_and_mark(None, "\u{304b}", Some(1..1));
        assert_eq!(b.text(), "x\u{304b}");
        assert_eq!(b.marked(), Some(1..4));
        assert_eq!(b.selection(), 4..4);
        b.ime_replace(None, "\u{6f22}");
        assert_eq!(b.text(), "x\u{6f22}");
        assert_eq!(b.marked(), None);
        assert_eq!(b.undo_depth(), 1);
        assert!(b.undo());
        assert_eq!(b.text(), "x");
    }

    #[test]
    fn marked_selection_is_relative_to_the_composition() {
        let mut b = BlockBuffer::new("ab");
        b.set_cursor(1);
        // Pinyin-like: composition "ni hao" with the caret after "ni" (UTF-16 2..2).
        b.ime_replace_and_mark(None, "ni hao", Some(2..2));
        assert_eq!(b.text(), "ani haob");
        assert_eq!(b.marked(), Some(1..7));
        assert_eq!(b.selection(), 3..3);
    }

    #[test]
    fn explicit_utf16_range_replaces_through_surrogates() {
        let mut b = BlockBuffer::new("a\u{1f600}b");
        // UTF-16: a=0..1, emoji=1..3, b=3..4.
        b.ime_replace(Some(1..3), "z");
        assert_eq!(b.text(), "azb");
    }

    #[test]
    fn empty_marked_text_ends_composition() {
        let mut b = BlockBuffer::new("");
        b.ime_replace_and_mark(None, "a", None);
        b.ime_replace_and_mark(None, "", None);
        assert_eq!(b.marked(), None);
        assert_eq!(b.text(), "");
    }
}
