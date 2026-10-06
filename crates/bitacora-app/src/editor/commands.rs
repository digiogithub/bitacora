//! The `/` slash menu and the `<` block-command menu of the block editor (BIT-US-0105): the
//! catalogue, the fuzzy filter and the pure text edit each command makes. The popup state machine
//! lives in [`super::OutlineEditor`]; the behaviour follows Logseq's documented command lists
//! (written from the description, ADR-015).

use std::ops::Range;

use bitacora_core::editor::TemplateContext;
use bitacora_markdown::edit::state::{Timestamp, set_deadline, set_marker, set_scheduled};

/// What a menu entry does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandId {
    /// Set the task marker (`TODO`, `DOING`, ...).
    Marker(&'static str),
    /// Make the block a heading of level 1-6.
    Heading(u8),
    /// `[[]]`.
    PageRef,
    /// `(())`.
    BlockRef,
    /// `{{embed [[]]}}`.
    PageEmbed,
    /// `{{embed (())}}`.
    BlockEmbed,
    /// `{{query }}`.
    Query,
    /// Today's journal link.
    Today,
    /// Tomorrow's journal link.
    Tomorrow,
    /// Yesterday's journal link.
    Yesterday,
    /// The current time (`HH:mm`).
    Time,
    /// Opens the calendar and inserts the chosen day as a page link.
    DatePicker,
    /// Opens the calendar and writes `SCHEDULED:`.
    Scheduled,
    /// Opens the calendar and writes `DEADLINE:`.
    Deadline,
    /// Fenced code block.
    CodeBlock,
    /// `[]()`.
    Link,
    /// `![]()`.
    Image,
    /// Opens the native file dialog and attaches the chosen files as assets.
    Upload,
    /// Switches the menu to the list of templates.
    Template,
    /// `#+BEGIN_<name>` ... `#+END_<name>`.
    Block(&'static str),
}

/// One entry of a menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Command {
    /// What it does.
    pub id: CommandId,
    /// Shown in the popup.
    pub label: &'static str,
    /// Extra words the filter matches (besides the label).
    pub keys: &'static str,
}

const fn c(id: CommandId, label: &'static str, keys: &'static str) -> Command {
    Command { id, label, keys }
}

/// The `/` menu, in the order shown for an empty query.
pub const SLASH: &[Command] = &[
    c(CommandId::Marker("TODO"), "TODO", "task"),
    c(CommandId::Marker("DOING"), "DOING", "task"),
    c(CommandId::Marker("DONE"), "DONE", "task"),
    c(CommandId::Marker("LATER"), "LATER", "task"),
    c(CommandId::Marker("NOW"), "NOW", "task"),
    c(CommandId::Marker("WAITING"), "WAITING", "task"),
    c(CommandId::Marker("CANCELED"), "CANCELED", "task cancelled"),
    c(CommandId::Heading(1), "Heading 1", "h1 title"),
    c(CommandId::Heading(2), "Heading 2", "h2 title"),
    c(CommandId::Heading(3), "Heading 3", "h3 title"),
    c(CommandId::Heading(4), "Heading 4", "h4 title"),
    c(CommandId::Heading(5), "Heading 5", "h5 title"),
    c(CommandId::Heading(6), "Heading 6", "h6 title"),
    c(CommandId::PageRef, "Page reference", "link"),
    c(CommandId::BlockRef, "Block reference", "link"),
    c(CommandId::PageEmbed, "Page embed", "embed"),
    c(CommandId::BlockEmbed, "Block embed", "embed"),
    c(CommandId::Query, "Query", "search"),
    c(CommandId::Today, "Today", "date"),
    c(CommandId::Tomorrow, "Tomorrow", "date"),
    c(CommandId::Yesterday, "Yesterday", "date"),
    c(CommandId::Time, "Current time", "now clock"),
    c(CommandId::DatePicker, "Date picker", "calendar"),
    c(CommandId::Scheduled, "Scheduled", "date task"),
    c(CommandId::Deadline, "Deadline", "date task"),
    c(CommandId::Template, "Template", "insert"),
    c(CommandId::CodeBlock, "Code block", "src fence"),
    c(CommandId::Link, "Link", "url"),
    c(CommandId::Image, "Image link", "picture"),
    c(CommandId::Upload, "Upload an asset", "attach file image"),
];

/// The `<` menu.
pub const ANGLE: &[Command] = &[
    c(CommandId::Block("QUOTE"), "Quote", ""),
    c(CommandId::Block("SRC"), "Src", "code"),
    c(CommandId::Block("QUERY"), "Query", "search"),
    c(CommandId::Block("NOTE"), "Note", ""),
    c(CommandId::Block("TIP"), "Tip", ""),
    c(CommandId::Block("IMPORTANT"), "Important", ""),
    c(CommandId::Block("CAUTION"), "Caution", ""),
    c(CommandId::Block("PINNED"), "Pinned", ""),
    c(CommandId::Block("WARNING"), "Warning", ""),
    c(CommandId::Block("EXAMPLE"), "Example", ""),
    c(CommandId::Block("EXPORT"), "Export", ""),
    c(CommandId::Block("VERSE"), "Verse", ""),
    c(CommandId::Block("ASCII"), "Ascii", ""),
    c(CommandId::Block("CENTER"), "Center", ""),
    c(CommandId::Block("COMMENT"), "Comment", ""),
];

/// Fuzzy score of `query` against `text` (case-insensitive), `None` when it does not match.
/// A prefix beats a word start, which beats a substring, which beats a subsequence.
#[must_use]
pub fn fuzzy_score(query: &str, text: &str) -> Option<i32> {
    let q: Vec<char> = query
        .to_lowercase()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    if q.is_empty() {
        return Some(0);
    }
    let t = text.to_lowercase();
    let squeezed: String = t.chars().filter(|c| !c.is_whitespace()).collect();
    let qs: String = q.iter().collect();
    if squeezed.starts_with(&qs) {
        return Some(1000);
    }
    if t.split_whitespace().any(|w| w.starts_with(&qs)) {
        return Some(800);
    }
    if squeezed.contains(&qs) {
        return Some(600);
    }
    // Subsequence: every query character in order; tighter matches score higher. Prose after a
    // `/` (a query with spaces) never matches loosely, so Enter keeps splitting the block.
    if query.trim().contains(char::is_whitespace) {
        return None;
    }
    let mut at = 0;
    let mut span_start = None;
    let mut last = 0;
    let chars: Vec<char> = squeezed.chars().collect();
    for ch in &q {
        let found = chars[at..].iter().position(|x| x == ch)?;
        let pos = at + found;
        span_start.get_or_insert(pos);
        last = pos;
        at = pos + 1;
    }
    let span = last + 1 - span_start.unwrap_or(0);
    Some(400 - i32::try_from(span).unwrap_or(0))
}

/// Commands of `menu` matching `query`, best first (catalogue order among equals).
#[must_use]
pub fn filter(menu: &[Command], query: &str) -> Vec<Command> {
    let mut scored: Vec<(i32, usize, Command)> = menu
        .iter()
        .enumerate()
        .filter_map(|(i, cmd)| {
            let label = fuzzy_score(query, cmd.label);
            let keys = (!cmd.keys.is_empty())
                .then(|| fuzzy_score(query, cmd.keys).map(|s| s - 100))
                .flatten();
            label.max(keys).map(|s| (s, i, *cmd))
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    scored.into_iter().map(|(_, _, cmd)| cmd).collect()
}

/// The text after `template` when `query` is the template list mode (`template foo`); `None`
/// for the normal command list.
#[must_use]
pub fn template_query(query: &str) -> Option<&str> {
    let q = query.trim_start();
    let (head, rest) = q.split_at_checked(9)?;
    head.eq_ignore_ascii_case("template ")
        .then(|| rest.trim_start())
}

/// A text edit: the whole visible text after the command and where the caret goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rewrite {
    /// The new text of the block.
    pub text: String,
    /// Caret offset in `text`.
    pub cursor: usize,
}

/// What accepting a command does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Applied {
    /// Replace the text of the block.
    Rewrite(Rewrite),
    /// Open the calendar for this purpose.
    Picker(PickerKind),
    /// Show the list of templates (the typed query becomes `template `).
    Templates,
    /// Open the file dialog; the trigger text is removed.
    Upload(Rewrite),
}

/// Why the calendar is open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickerKind {
    /// Insert the day as a page reference.
    Date,
    /// Write `SCHEDULED:`.
    Scheduled,
    /// Write `DEADLINE:`.
    Deadline,
}

fn remove(text: &str, range: &Range<usize>) -> (String, usize) {
    let end = range.end.min(text.len());
    let start = range.start.min(end);
    let mut out = String::with_capacity(text.len());
    out.push_str(&text[..start]);
    out.push_str(&text[end..]);
    (out, start)
}

/// [`remove`], and the spaces the trigger leaves at the end of its line go too.
pub fn remove_trimmed(text: &str, range: &Range<usize>) -> (String, usize) {
    let (mut rest, mut at) = remove(text, range);
    if rest[at..].is_empty() || rest[at..].starts_with('\n') {
        let kept = rest[..at].trim_end_matches(' ').len();
        rest.replace_range(kept..at, "");
        at = kept;
    }
    (rest, at)
}

fn insert(text: &str, range: &Range<usize>, ins: &str, caret_in_ins: usize) -> Rewrite {
    let end = range.end.min(text.len());
    let start = range.start.min(end);
    let mut out = String::with_capacity(text.len() + ins.len());
    out.push_str(&text[..start]);
    out.push_str(ins);
    out.push_str(&text[end..]);
    Rewrite {
        text: out,
        cursor: start + caret_in_ins.min(ins.len()),
    }
}

/// Moves `cursor` along with an edit that changed the text length at or before it.
fn shifted(old_len: usize, new: String, cursor: usize) -> Rewrite {
    let delta = new.len() as isize - old_len as isize;
    let cursor = usize::try_from(cursor as isize + delta)
        .unwrap_or(0)
        .min(new.len());
    Rewrite { text: new, cursor }
}

/// A `#+BEGIN_<name>` block that starts on its own line; the caret goes to the empty body line
/// (after `SRC ` for source blocks, where the language is typed).
fn begin_block(text: &str, range: &Range<usize>, name: &str) -> Rewrite {
    let start = range.start.min(text.len());
    let at_line_start = start == 0 || text[..start].ends_with('\n');
    let rest = &text[range.end.min(text.len())..];
    let lead = if at_line_start { "" } else { "\n" };
    let tail = if rest.is_empty() || rest.starts_with('\n') {
        ""
    } else {
        "\n"
    };
    let head = if name == "SRC" {
        format!("#+BEGIN_{name} ")
    } else {
        format!("#+BEGIN_{name}")
    };
    let body = format!("{lead}{head}\n\n#+END_{name}{tail}");
    let caret = if name == "SRC" {
        lead.len() + head.len()
    } else {
        lead.len() + head.len() + 1
    };
    insert(text, range, &body, caret)
}

/// A fenced code block that starts on its own line, caret after the opening fence.
fn code_fence(text: &str, range: &Range<usize>) -> Rewrite {
    let start = range.start.min(text.len());
    let at_line_start = start == 0 || text[..start].ends_with('\n');
    let rest = &text[range.end.min(text.len())..];
    let lead = if at_line_start { "" } else { "\n" };
    let tail = if rest.is_empty() || rest.starts_with('\n') {
        ""
    } else {
        "\n"
    };
    let body = format!("{lead}```\n\n```{tail}");
    insert(text, range, &body, lead.len() + 3)
}

/// The edit that command `id` makes to `text` (the visible block text) in place of `range` (the
/// typed trigger, `/query` or `<query`). `ctx` supplies the dates and the time.
#[must_use]
pub fn apply(id: CommandId, text: &str, range: &Range<usize>, ctx: &TemplateContext) -> Applied {
    let simple = |ins: &str, caret: usize| Applied::Rewrite(insert(text, range, ins, caret));
    match id {
        CommandId::Marker(m) => {
            let (rest, at) = remove_trimmed(text, range);
            Applied::Rewrite(shifted(rest.len(), set_marker(&rest, Some(m)), at))
        }
        CommandId::Heading(level) => {
            let (rest, at) = remove_trimmed(text, range);
            let stripped = strip_heading(&rest);
            let removed = rest.len() - stripped.len();
            let prefix = format!("{} ", "#".repeat(usize::from(level.clamp(1, 6))));
            let new = format!("{prefix}{stripped}");
            // The caret moves with the text: the old prefix goes, the new one comes.
            let cursor = (at + prefix.len()).saturating_sub(removed).min(new.len());
            Applied::Rewrite(Rewrite { text: new, cursor })
        }
        CommandId::PageRef => simple("[[]]", 2),
        CommandId::BlockRef => simple("(())", 2),
        CommandId::PageEmbed => simple("{{embed [[]]}}", 10),
        CommandId::BlockEmbed => simple("{{embed (())}}", 10),
        CommandId::Query => simple("{{query }}", 8),
        CommandId::Today => simple(&ctx.today, ctx.today.len()),
        CommandId::Tomorrow => simple(&ctx.tomorrow, ctx.tomorrow.len()),
        CommandId::Yesterday => simple(&ctx.yesterday, ctx.yesterday.len()),
        CommandId::Time => simple(&ctx.time, ctx.time.len()),
        CommandId::DatePicker => Applied::Picker(PickerKind::Date),
        CommandId::Scheduled => Applied::Picker(PickerKind::Scheduled),
        CommandId::Deadline => Applied::Picker(PickerKind::Deadline),
        CommandId::CodeBlock => Applied::Rewrite(code_fence(text, range)),
        CommandId::Link => simple("[]()", 1),
        CommandId::Image => simple("![]()", 2),
        CommandId::Upload => {
            let (rest, at) = remove_trimmed(text, range);
            Applied::Upload(Rewrite {
                text: rest,
                cursor: at,
            })
        }
        CommandId::Template => Applied::Templates,
        CommandId::Block(name) => Applied::Rewrite(begin_block(text, range, name)),
    }
}

fn strip_heading(text: &str) -> &str {
    let hashes = text.chars().take_while(|c| *c == '#').count();
    if (1..=6).contains(&hashes) && text[hashes..].starts_with(' ') {
        &text[hashes + 1..]
    } else {
        text
    }
}

/// Result of choosing a day in the calendar: the text after writing it at `at` (where the
/// trigger was).
#[must_use]
pub fn apply_date(
    kind: PickerKind,
    text: &str,
    at: usize,
    day: (i32, u32, u32),
    page_title: &str,
) -> Rewrite {
    let at = at.min(text.len());
    match kind {
        PickerKind::Date => {
            let link = format!("[[{page_title}]]");
            insert(text, &(at..at), &link, link.len())
        }
        PickerKind::Scheduled | PickerKind::Deadline => {
            let ts = Timestamp {
                active: true,
                year: u32::try_from(day.0).unwrap_or(0),
                month: day.1,
                day: day.2,
                time: None,
                repeater: None,
            };
            let new = if kind == PickerKind::Scheduled {
                set_scheduled(text, Some(&ts))
            } else {
                set_deadline(text, Some(&ts))
            };
            // The planning line goes after the title: the caret stays where it was in it.
            let cursor = at.min(text.find('\n').unwrap_or(text.len()));
            Rewrite { text: new, cursor }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bitacora_config::EffectiveConfig;
    use bitacora_core::date::Date;

    fn ctx() -> TemplateContext {
        let cfg = EffectiveConfig::from_texts(None, Some("{}"));
        TemplateContext::new(Date::new(2025, 11, 14).expect("d"), "09:05", &cfg, "P")
    }

    fn rewrite(a: Applied) -> Rewrite {
        match a {
            Applied::Rewrite(r) => r,
            other => panic!("not a rewrite: {other:?}"),
        }
    }

    fn labels(v: &[Command]) -> Vec<&'static str> {
        v.iter().map(|c| c.label).collect()
    }

    #[test]
    fn fuzzy_ranks_prefix_before_substring_before_subsequence() {
        let all = filter(SLASH, "");
        assert_eq!(all.len(), SLASH.len());
        assert_eq!(labels(&filter(SLASH, "tod"))[0], "TODO");
        assert_eq!(labels(&filter(SLASH, "h2"))[0], "Heading 2");
        assert_eq!(
            labels(&filter(SLASH, "page"))[..2],
            ["Page reference", "Page embed"]
        );
        // Subsequence.
        assert!(labels(&filter(SLASH, "cdbk")).contains(&"Code block"));
        assert!(filter(SLASH, "zzzz").is_empty());
        // The empty-query order is the catalogue order.
        assert_eq!(labels(&all)[0], "TODO");
    }

    #[test]
    fn template_mode_is_detected_from_the_query() {
        assert_eq!(template_query("template "), Some(""));
        assert_eq!(template_query("Template mee"), Some("mee"));
        assert_eq!(template_query("template"), None);
        assert_eq!(template_query("temp"), None);
    }

    #[test]
    fn marker_replaces_the_trigger_and_an_existing_marker() {
        let r = rewrite(apply(CommandId::Marker("TODO"), "/to", &(0..3), &ctx()));
        assert_eq!(
            r,
            Rewrite {
                text: "TODO ".into(),
                cursor: 5
            }
        );
        let r = rewrite(apply(
            CommandId::Marker("DOING"),
            "TODO write /do",
            &(11..14),
            &ctx(),
        ));
        assert_eq!(r.text, "DOING write");
        assert_eq!(r.cursor, r.text.len());
    }

    #[test]
    fn heading_prefix_replaces_the_previous_level() {
        let r = rewrite(apply(CommandId::Heading(2), "/h2", &(0..3), &ctx()));
        assert_eq!(
            r,
            Rewrite {
                text: "## ".into(),
                cursor: 3
            }
        );
        let r = rewrite(apply(
            CommandId::Heading(3),
            "# Title /h3",
            &(8..11),
            &ctx(),
        ));
        assert_eq!(r.text, "### Title");
    }

    #[test]
    fn inline_insertions_place_the_caret() {
        let r = rewrite(apply(CommandId::PageRef, "see /pa", &(4..7), &ctx()));
        assert_eq!(
            r,
            Rewrite {
                text: "see [[]]".into(),
                cursor: 6
            }
        );
        let r = rewrite(apply(CommandId::BlockEmbed, "/be", &(0..3), &ctx()));
        assert_eq!(r.text, "{{embed (())}}");
        assert_eq!(&r.text[..r.cursor], "{{embed ((");
        let r = rewrite(apply(CommandId::Query, "/q", &(0..2), &ctx()));
        assert_eq!(&r.text[r.cursor..], "}}");
        let r = rewrite(apply(CommandId::Image, "/i", &(0..2), &ctx()));
        assert_eq!(
            r,
            Rewrite {
                text: "![]()".into(),
                cursor: 2
            }
        );
    }

    #[test]
    fn dates_use_the_journal_title_and_time() {
        let r = rewrite(apply(CommandId::Today, "on /to", &(3..6), &ctx()));
        assert_eq!(r.text, "on [[Nov 14th, 2025]]");
        let r = rewrite(apply(CommandId::Tomorrow, "/t", &(0..2), &ctx()));
        assert_eq!(r.text, "[[Nov 15th, 2025]]");
        let r = rewrite(apply(CommandId::Yesterday, "/y", &(0..2), &ctx()));
        assert_eq!(r.text, "[[Nov 13th, 2025]]");
        let r = rewrite(apply(CommandId::Time, "at /ti", &(3..6), &ctx()));
        assert_eq!(r.text, "at 09:05");
    }

    #[test]
    fn angle_commands_write_logseq_blocks() {
        let r = rewrite(apply(CommandId::Block("QUOTE"), "<qu", &(0..3), &ctx()));
        assert_eq!(r.text, "#+BEGIN_QUOTE\n\n#+END_QUOTE");
        assert_eq!(r.cursor, "#+BEGIN_QUOTE\n".len());
        let r = rewrite(apply(CommandId::Block("SRC"), "<src", &(0..4), &ctx()));
        assert_eq!(r.text, "#+BEGIN_SRC \n\n#+END_SRC");
        assert_eq!(r.cursor, "#+BEGIN_SRC ".len());
        // Mid-line: the block starts on a new line.
        let r = rewrite(apply(CommandId::Block("NOTE"), "text <no", &(5..8), &ctx()));
        assert_eq!(r.text, "text \n#+BEGIN_NOTE\n\n#+END_NOTE");
        for name in [
            "TIP",
            "IMPORTANT",
            "CAUTION",
            "WARNING",
            "EXAMPLE",
            "EXPORT",
            "CENTER",
            "VERSE",
            "COMMENT",
            "QUERY",
        ] {
            let r = rewrite(apply(CommandId::Block(name), "<x", &(0..2), &ctx()));
            assert_eq!(r.text, format!("#+BEGIN_{name}\n\n#+END_{name}"));
        }
        assert_eq!(ANGLE.len(), 15);
    }

    #[test]
    fn code_block_and_text_after_the_trigger() {
        let r = rewrite(apply(CommandId::CodeBlock, "/co", &(0..3), &ctx()));
        assert_eq!(r.text, "```\n\n```");
        assert_eq!(r.cursor, 3);
        let r = rewrite(apply(
            CommandId::Block("QUOTE"),
            "<q after",
            &(0..2),
            &ctx(),
        ));
        assert_eq!(r.text, "#+BEGIN_QUOTE\n\n#+END_QUOTE\n after");
    }

    #[test]
    fn calendar_results() {
        let r = apply_date(
            PickerKind::Date,
            "see ",
            4,
            (2025, 11, 20),
            "Nov 20th, 2025",
        );
        assert_eq!(r.text, "see [[Nov 20th, 2025]]");
        let r = apply_date(PickerKind::Scheduled, "TODO task ", 10, (2025, 11, 20), "x");
        assert_eq!(r.text, "TODO task \nSCHEDULED: <2025-11-20 Thu>");
        let r = apply_date(PickerKind::Deadline, &r.text, 3, (2025, 12, 1), "x");
        assert_eq!(
            r.text,
            "TODO task \nSCHEDULED: <2025-11-20 Thu>\nDEADLINE: <2025-12-01 Mon>"
        );
        // Picking again replaces the line.
        let again = apply_date(PickerKind::Scheduled, &r.text, 3, (2025, 11, 21), "x");
        assert!(again.text.contains("SCHEDULED: <2025-11-21 Fri>"));
        assert_eq!(again.text.matches("SCHEDULED").count(), 1);
    }

    #[test]
    fn upload_and_templates_defer_to_the_editor() {
        assert!(matches!(
            apply(CommandId::Upload, "x /up", &(2..5), &ctx()),
            Applied::Upload(Rewrite { ref text, cursor: 1 }) if text == "x"
        ));
        assert_eq!(
            apply(CommandId::Template, "/t", &(0..2), &ctx()),
            Applied::Templates
        );
        assert_eq!(
            apply(CommandId::Scheduled, "/s", &(0..2), &ctx()),
            Applied::Picker(PickerKind::Scheduled)
        );
    }
}
