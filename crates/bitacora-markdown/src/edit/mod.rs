//! Surgical text edits on a block's de-indented content (BIT-US-0093).
//!
//! Every function takes the content of one block as returned by
//! [`content_of`](crate::content_of) and returns the new content, changing only the intended
//! lines. Nothing is ever hoisted or reordered (unlike Logseq's `with-built-in-properties`).
//! Feed the result to [`Document::set_block_content`](crate::Document::set_block_content).

pub mod identity;
pub mod properties;
pub mod state;

/// Number of `\n` before byte offset `byte`: the line index of that offset.
pub(crate) fn line_index(content: &str, byte: usize) -> usize {
    content.as_bytes()[..byte.min(content.len())]
        .iter()
        .filter(|&&b| b == b'\n')
        .count()
}

/// Inserts `new_lines` so that the first one becomes line `idx` (clamped to the end).
pub(crate) fn insert_lines(content: &str, idx: usize, new_lines: &[String]) -> String {
    if content.is_empty() {
        return new_lines.join("\n");
    }
    let mut lines: Vec<&str> = content.split('\n').collect();
    let idx = idx.min(lines.len());
    for (k, l) in new_lines.iter().enumerate() {
        lines.insert(idx + k, l.as_str());
    }
    lines.join("\n")
}

/// Removes line `idx` (and its line break).
pub(crate) fn remove_line(content: &str, idx: usize) -> String {
    let mut lines: Vec<&str> = content.split('\n').collect();
    if idx < lines.len() {
        lines.remove(idx);
    }
    lines.join("\n")
}

/// Replaces line `idx`.
pub(crate) fn replace_line(content: &str, idx: usize, text: &str) -> String {
    let mut lines: Vec<&str> = content.split('\n').collect();
    if idx < lines.len() {
        lines[idx] = text;
    }
    lines.join("\n")
}

/// True when the first line is a "title": text Logseq's parser reads as a paragraph or heading
/// (as opposed to a fence, quote, table, drawer or `#+BEGIN_` block).
pub(crate) fn is_title_line(line: &str) -> bool {
    let t = line.trim_start();
    !(t.is_empty()
        || t.starts_with("```")
        || t.starts_with("~~~")
        || t.starts_with('>')
        || t.starts_with('|')
        || t.starts_with(":PROPERTIES:")
        || t.starts_with(":LOGBOOK:")
        || t.to_ascii_uppercase().starts_with("#+BEGIN"))
}

/// True for a `SCHEDULED:` / `DEADLINE:` line.
pub(crate) fn is_planning_line(line: &str, keyword: Option<&str>) -> bool {
    let t = line.trim_start();
    match keyword {
        Some(k) => t.starts_with(k),
        None => t.starts_with("SCHEDULED:") || t.starts_with("DEADLINE:"),
    }
}

/// Number of leading lines that form the title plus the planning lines that follow it; 0 when the
/// content has no title.
pub(crate) fn title_block_len(content: &str) -> usize {
    let mut lines = content.split('\n');
    match lines.next() {
        Some(first) if is_title_line(first) => {
            1 + lines.take_while(|l| is_planning_line(l, None)).count()
        }
        _ => 0,
    }
}
