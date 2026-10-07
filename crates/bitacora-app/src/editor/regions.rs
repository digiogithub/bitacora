//! Where the caret sits relative to fenced code and `#+BEGIN_X` regions of the block text
//! (BIT-US-0169). Inside such a region Enter inserts a newline and Tab indents (Logseq keeps
//! the caret in the region instead of splitting the block).

/// One level of indentation inserted by Tab inside a region.
pub const INDENT: &str = "  ";

fn is_fence(line: &str) -> bool {
    line.trim_start().starts_with("```")
}

/// The upper-case name of a `#+BEGIN_<name>` opener line.
fn begin_name(line: &str) -> Option<String> {
    let t = line.trim_start();
    let head = t.get(..8)?;
    if !head.eq_ignore_ascii_case("#+BEGIN_") {
        return None;
    }
    let name: String = t[8..]
        .chars()
        .take_while(|c| !c.is_whitespace())
        .collect::<String>()
        .to_uppercase();
    (!name.is_empty()).then_some(name)
}

fn is_end(line: &str, name: &str) -> bool {
    let t = line.trim();
    t.get(..6).is_some_and(|h| h.eq_ignore_ascii_case("#+END_"))
        && t[6..].trim().eq_ignore_ascii_case(name)
}

#[derive(PartialEq)]
enum Open {
    None,
    Fence,
    Begin(String),
}

/// Whether `cursor` (a byte offset into `text`) is inside an open fence or `#+BEGIN_X` region:
/// after the first character of the opener line and before the closer line's end.
#[must_use]
pub fn in_region(text: &str, cursor: usize) -> bool {
    let mut open = Open::None;
    let mut start = 0;
    for raw in text.split_inclusive('\n') {
        let end = start + raw.trim_end_matches(['\n', '\r']).len();
        let line = &text[start..end];
        let on_line = cursor >= start && cursor <= end;
        match &open {
            Open::None => {
                if is_fence(line) {
                    if on_line {
                        return cursor > start;
                    }
                    open = Open::Fence;
                } else if let Some(name) = begin_name(line) {
                    if on_line {
                        return cursor > start;
                    }
                    open = Open::Begin(name);
                } else if on_line {
                    return false;
                }
            }
            Open::Fence => {
                if is_fence(line) {
                    if on_line {
                        return false;
                    }
                    open = Open::None;
                } else if on_line {
                    return true;
                }
            }
            Open::Begin(name) => {
                if is_end(line, name) {
                    if on_line {
                        return false;
                    }
                    open = Open::None;
                } else if on_line {
                    return true;
                }
            }
        }
        start += raw.len();
    }
    // The caret after a trailing newline sits on an empty last line.
    cursor >= text.len() && open != Open::None && text.ends_with('\n')
}

/// Start of the line containing `cursor`.
#[must_use]
pub fn line_start(text: &str, cursor: usize) -> usize {
    text[..cursor.min(text.len())]
        .rfind('\n')
        .map_or(0, |i| i + 1)
}

/// Number of leading spaces (up to [`INDENT`]'s width) that Shift+Tab removes from the line at
/// `cursor`.
#[must_use]
pub fn dedent_width(text: &str, cursor: usize) -> usize {
    let s = line_start(text, cursor);
    text[s..]
        .bytes()
        .take(INDENT.len())
        .take_while(|b| *b == b' ')
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The caret is where `@` sits in `text`.
    fn at(text: &str) -> bool {
        let pos = text.find('@').expect("marker");
        in_region(&text.replacen('@', "", 1), pos)
    }

    #[test]
    fn fences_and_begin_regions_keep_the_caret_inside() {
        assert!(at("```rust\nfn x() {@}\n```"));
        assert!(at("```rust@\nx\n```"));
        assert!(at("```\n@\n```"));
        assert!(at("#+BEGIN_NOTE\ntext@\n#+END_NOTE"));
        assert!(at("#+BEGIN_SRC rust\n@\n#+END_SRC"));
        assert!(at("a\n#+begin_tip\n@\n#+end_tip"));
    }

    #[test]
    fn outside_the_region_enter_still_splits() {
        assert!(!at("before@\n```\nx\n```"));
        assert!(!at("```\nx\n```@\nafter"));
        assert!(!at("#+BEGIN_NOTE\nx\n#+END_NOTE@"));
        assert!(!at("```\nx\n```\nafter@"));
        assert!(!at("@```\nx\n```"));
        assert!(!at("plain text@"));
    }

    #[test]
    fn unclosed_regions_stay_open_and_end_names_must_match() {
        assert!(at("```\nnever closed@"));
        assert!(at("#+BEGIN_NOTE\nx\n#+END_TIP\ny@"));
    }

    #[test]
    fn dedent_removes_at_most_one_level() {
        assert_eq!(dedent_width("    x", 5), 2);
        assert_eq!(dedent_width(" x", 2), 1);
        assert_eq!(dedent_width("x", 1), 0);
        assert_eq!(dedent_width("a\n  b", 5), 2);
    }
}
