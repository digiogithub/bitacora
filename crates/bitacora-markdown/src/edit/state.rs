//! Task/state edits: `collapsed::`, SCHEDULED/DEADLINE, LOGBOOK clock entries and the task marker.
//!
//! Layout produced when a block is edited (`docs/analysis/logseq/02-markdown-block-syntax.md`
//! §5.3, §7): title, SCHEDULED/DEADLINE, properties, `:LOGBOOK:` drawer, then the body. Lines that
//! are already there are never moved.

use std::fmt;

use crate::edit::properties::{get_property, remove_property, set_property};
use crate::edit::{
    insert_lines, is_planning_line, line_index, remove_line, replace_line, title_block_len,
};
use crate::lines::ParserOptions;
use crate::properties::scan_properties;
use crate::span::Span;
use crate::tasks::timestamp::{days_from_civil, parse_repeater, weekday_name};
use crate::tasks::{Date, Marker, Time, Timestamp as TaskTimestamp};

/// Where the collapsed state lives (the "store collapse state in files" setting).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollapseMode {
    /// `collapsed:: true` is written to / removed from the file.
    InFile,
    /// The state is held by the app; the content never changes.
    AppOnly,
}

/// Sets or clears the collapsed state. In [`CollapseMode::InFile`], `true` writes `collapsed:: true`
/// and `false` removes the line, so expanding restores the original bytes. A `collapsed::` line is
/// never touched when the state is not toggled.
#[must_use]
pub fn set_collapsed(content: &str, collapsed: bool, mode: CollapseMode) -> String {
    if mode == CollapseMode::AppOnly {
        return content.to_owned();
    }
    if collapsed {
        set_property(content, "collapsed", "true")
    } else if get_property(content, "collapsed").is_some() {
        remove_property(content, "collapsed")
    } else {
        content.to_owned()
    }
}

/// An org-mode timestamp to write into SCHEDULED/DEADLINE. The weekday is computed; the text is
/// produced by [`tasks::Timestamp::format`](crate::tasks::Timestamp::format) so reading and
/// writing share one canonical form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Timestamp {
    /// `<...>` (true) or `[...]` (false).
    pub active: bool,
    /// Year.
    pub year: u32,
    /// Month 1-12.
    pub month: u32,
    /// Day 1-31.
    pub day: u32,
    /// Optional `HH:MM`.
    pub time: Option<(u32, u32)>,
    /// Optional repeater written as is (`+1w`, `++1d`, `.+1d`).
    pub repeater: Option<String>,
}

impl Timestamp {
    /// The equivalent read-side timestamp (empty span). A repeater that is not a valid
    /// `+Nu` / `++Nu` / `.+Nu` token is dropped here; [`fmt::Display`] still writes it verbatim.
    #[must_use]
    pub fn to_task_timestamp(&self) -> TaskTimestamp {
        TaskTimestamp {
            span: Span::new(0, 0),
            active: self.active,
            date: Date {
                year: self.year,
                month: self.month,
                day: self.day,
            },
            weekday: weekday_name(
                i32::try_from(self.year).unwrap_or(i32::MAX),
                self.month,
                self.day,
            )
            .to_owned(),
            time: self.time.map(|(hour, min)| Time {
                hour,
                min,
                sec: None,
            }),
            end_time: None,
            repeater: self.repeater.as_deref().and_then(parse_repeater),
        }
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let task = self.to_task_timestamp();
        let text = task.format();
        match (&self.repeater, &task.repeater) {
            // Not a recognised repeater: written as is, like before the unification.
            (Some(raw), None) => {
                let close = text.len() - 1;
                write!(f, "{} {raw}{}", &text[..close], &text[close..])
            }
            _ => f.write_str(&text),
        }
    }
}

/// Date and time of a clock entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClockTime {
    /// Year.
    pub year: i32,
    /// Month 1-12.
    pub month: u32,
    /// Day 1-31.
    pub day: u32,
    /// Hour.
    pub hour: u32,
    /// Minute.
    pub minute: u32,
    /// Second.
    pub second: u32,
}

impl ClockTime {
    fn bracketed(&self) -> String {
        format!(
            "[{:04}-{:02}-{:02} {} {:02}:{:02}:{:02}]",
            self.year,
            self.month,
            self.day,
            weekday_name(self.year, self.month, self.day),
            self.hour,
            self.minute,
            self.second
        )
    }

    fn epoch_seconds(&self) -> i64 {
        days_from_civil(self.year, self.month, self.day) * 86_400
            + i64::from(self.hour) * 3600
            + i64::from(self.minute) * 60
            + i64::from(self.second)
    }

    /// Parses the inside of `[2024-01-01 Mon 10:00:00]` (brackets included).
    fn parse(s: &str) -> Option<Self> {
        let inner = s.trim().strip_prefix('[')?.strip_suffix(']')?;
        let mut parts = inner.split_whitespace();
        let mut date = parts.next()?.split('-');
        let year = date.next()?.parse().ok()?;
        let month = date.next()?.parse().ok()?;
        let day = date.next()?.parse().ok()?;
        let _weekday = parts.next()?;
        let mut time = parts.next()?.split(':');
        Some(Self {
            year,
            month,
            day,
            hour: time.next()?.parse().ok()?,
            minute: time.next()?.parse().ok()?,
            second: time.next().map_or(Some(0), |s| s.parse().ok())?,
        })
    }
}

fn set_planning(content: &str, keyword: &str, ts: Option<&Timestamp>) -> String {
    let existing = content
        .split('\n')
        .enumerate()
        .skip(1)
        .take(title_block_len(content).saturating_sub(1))
        .find(|(_, l)| is_planning_line(l, Some(keyword)))
        .map(|(i, _)| i);
    match (existing, ts) {
        (Some(i), Some(ts)) => replace_line(content, i, &format!("{keyword} {ts}")),
        (Some(i), None) => remove_line(content, i),
        (None, None) => content.to_owned(),
        (None, Some(ts)) => {
            let title_len = title_block_len(content);
            // SCHEDULED goes right after the title; DEADLINE after SCHEDULED when present.
            let idx = if keyword == "DEADLINE:" {
                content
                    .split('\n')
                    .enumerate()
                    .skip(1)
                    .take(title_len.saturating_sub(1))
                    .find(|(_, l)| is_planning_line(l, Some("SCHEDULED:")))
                    .map_or(title_len.min(1), |(i, _)| i + 1)
            } else {
                title_len.min(1)
            };
            insert_lines(content, idx, &[format!("{keyword} {ts}")])
        }
    }
}

/// Sets, replaces or (with `None`) removes the `SCHEDULED:` line. New lines go right after the
/// title.
#[must_use]
pub fn set_scheduled(content: &str, ts: Option<&Timestamp>) -> String {
    set_planning(content, "SCHEDULED:", ts)
}

/// Sets, replaces or removes the `DEADLINE:` line (after `SCHEDULED:` when both exist).
#[must_use]
pub fn set_deadline(content: &str, ts: Option<&Timestamp>) -> String {
    set_planning(content, "DEADLINE:", ts)
}

/// Finds `(open_line, end_line)` of the first `:LOGBOOK:` drawer outside fences.
fn find_logbook(content: &str) -> Option<(usize, usize)> {
    let mut in_fence = false;
    let mut open = None;
    for (i, l) in content.split('\n').enumerate() {
        let t = l.trim();
        if t.starts_with("```") || t.starts_with("~~~") {
            in_fence = !in_fence;
        }
        if in_fence {
            continue;
        }
        match open {
            None if t.eq_ignore_ascii_case(":LOGBOOK:") => open = Some(i),
            Some(o) if t.eq_ignore_ascii_case(":END:") => return Some((o, i)),
            _ => {}
        }
    }
    None
}

/// Line index where a new logbook goes: after the title, planning lines and the property group.
fn logbook_insert_index(content: &str) -> usize {
    let mut idx = title_block_len(content);
    let scan = scan_properties(
        content.as_bytes(),
        Span::new(0, content.len()),
        ParserOptions::default(),
    );
    if let Some(g) = scan.effective() {
        idx = idx.max(line_index(content, g.span.end.saturating_sub(1)) + 1);
    }
    idx
}

/// Starts a clock entry: `CLOCK: [2024-01-01 Mon 10:00:00]` as the first line of the `:LOGBOOK:`
/// drawer, creating the drawer after title, planning lines and properties when missing.
#[must_use]
pub fn clock_in(content: &str, now: ClockTime) -> String {
    let line = format!("CLOCK: {}", now.bracketed());
    match find_logbook(content) {
        Some((open, _)) => insert_lines(content, open + 1, &[line]),
        None => insert_lines(
            content,
            logbook_insert_index(content),
            &[":LOGBOOK:".to_owned(), line, ":END:".to_owned()],
        ),
    }
}

/// Closes the first open clock entry of the logbook:
/// `CLOCK: [start]--[end] =>  HH:MM:SS` (two spaces after `=>`). No open entry: unchanged.
#[must_use]
pub fn clock_out(content: &str, now: ClockTime) -> String {
    let Some((open, end)) = find_logbook(content) else {
        return content.to_owned();
    };
    for (i, l) in content.split('\n').enumerate().take(end).skip(open + 1) {
        let t = l.trim();
        let Some(rest) = t.strip_prefix("CLOCK:") else {
            continue;
        };
        if rest.contains("--") {
            continue;
        }
        let Some(start) = ClockTime::parse(rest) else {
            continue;
        };
        let secs = (now.epoch_seconds() - start.epoch_seconds()).max(0);
        let new = format!(
            "CLOCK: {}--{} =>  {:02}:{:02}:{:02}",
            start.bracketed(),
            now.bracketed(),
            secs / 3600,
            secs % 3600 / 60,
            secs % 60
        );
        return replace_line(content, i, &new);
    }
    content.to_owned()
}

/// Offset just after optional ATX hashes + space on the head line.
fn marker_offset(head: &str) -> usize {
    let hashes = head.bytes().take_while(|&b| b == b'#').count();
    if (1..=6).contains(&hashes) && head.as_bytes().get(hashes) == Some(&b' ') {
        hashes + 1
    } else {
        0
    }
}

/// Rewrites only the marker token of the head line: replaces, inserts (`Some`) or removes (`None`)
/// it, after the heading hashes when there are any. `marker` must be one of Logseq's markers
/// (`TODO DOING DONE LATER NOW WAITING WAIT CANCELED CANCELLED STARTED IN-PROGRESS`); anything else
/// leaves the content unchanged.
#[must_use]
pub fn set_marker(content: &str, marker: Option<&str>) -> String {
    if marker.is_some_and(|m| Marker::from_word(m).is_none()) {
        return content.to_owned();
    }
    let head_end = content.find('\n').unwrap_or(content.len());
    let head = &content[..head_end];
    let at = marker_offset(head);
    let rest = &head[at..];
    let existing = Marker::ALL.iter().find(|m| {
        rest.strip_prefix(m.as_str())
            .is_some_and(|r| r.starts_with(' '))
    });
    let (from, to) = match existing {
        Some(m) => (at, at + m.as_str().len() + 1),
        None => (at, at),
    };
    let insert = marker.map_or_else(String::new, |m| format!("{m} "));
    let mut out = content.to_owned();
    out.replace_range(from..to, &insert);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::identity::ensure_block_id;
    use uuid::Uuid;

    fn t(h: u32, m: u32, s: u32) -> ClockTime {
        ClockTime {
            year: 2024,
            month: 1,
            day: 1,
            hour: h,
            minute: m,
            second: s,
        }
    }

    #[test]
    fn collapse_round_trips() {
        let c = "collapsed parent";
        let on = set_collapsed(c, true, CollapseMode::InFile);
        assert_eq!(on, "collapsed parent\ncollapsed:: true");
        assert_eq!(set_collapsed(&on, false, CollapseMode::InFile), c);
        assert_eq!(set_collapsed(c, true, CollapseMode::AppOnly), c);
        assert_eq!(set_collapsed(c, false, CollapseMode::InFile), c);
        assert_eq!(set_collapsed(&on, true, CollapseMode::InFile), on);
    }

    #[test]
    fn timestamps_format_with_weekday() {
        let ts = Timestamp {
            active: true,
            year: 2024,
            month: 1,
            day: 1,
            time: None,
            repeater: Some(".+1d".into()),
        };
        assert_eq!(ts.to_string(), "<2024-01-01 Mon .+1d>");
        let ts = Timestamp {
            active: false,
            year: 2000,
            month: 2,
            day: 29,
            time: Some((9, 5)),
            repeater: None,
        };
        assert_eq!(ts.to_string(), "[2000-02-29 Tue 09:05]");
    }

    #[test]
    fn scheduled_and_deadline_placement() {
        let s = Timestamp {
            active: true,
            year: 2024,
            month: 1,
            day: 1,
            time: None,
            repeater: None,
        };
        let d = Timestamp {
            day: 5,
            ..s.clone()
        };
        let c = set_deadline("title\nbody", Some(&d));
        assert_eq!(c, "title\nDEADLINE: <2024-01-05 Fri>\nbody");
        let c = set_scheduled(&c, Some(&s));
        assert_eq!(
            c,
            "title\nSCHEDULED: <2024-01-01 Mon>\nDEADLINE: <2024-01-05 Fri>\nbody"
        );
        let c = set_scheduled(&c, Some(&Timestamp { day: 2, ..s }));
        assert!(c.contains("SCHEDULED: <2024-01-02 Tue>"));
        let c = set_deadline(&set_scheduled(&c, None), None);
        assert_eq!(c, "title\nbody");
    }

    #[test]
    fn canonical_order_title_scheduled_id_logbook() {
        let c = "TODO [#A] Parent block #tag";
        let s = Timestamp {
            active: true,
            year: 2024,
            month: 1,
            day: 1,
            time: None,
            repeater: Some(".+1d".into()),
        };
        let c = set_scheduled(c, Some(&s));
        let id = Uuid::parse_str("6500c1a4-0000-4000-8000-000000000001").expect("uuid");
        let (c, _) = ensure_block_id(&c, id, false).expect("id");
        let c = clock_in(&c, t(10, 0, 0));
        let c = clock_out(&c, t(11, 0, 0));
        assert_eq!(
            c,
            "TODO [#A] Parent block #tag\nSCHEDULED: <2024-01-01 Mon .+1d>\nid:: 6500c1a4-0000-4000-8000-000000000001\n:LOGBOOK:\nCLOCK: [2024-01-01 Mon 10:00:00]--[2024-01-01 Mon 11:00:00] =>  01:00:00\n:END:"
        );
    }

    #[test]
    fn clock_in_merges_into_existing_logbook() {
        let c = "DOING x\n:LOGBOOK:\nCLOCK: [2024-01-01 Mon 08:00:00]--[2024-01-01 Mon 08:30:00] =>  00:30:00\n:END:";
        let c = clock_in(c, t(10, 0, 0));
        assert_eq!(c.matches(":LOGBOOK:").count(), 1);
        assert!(c.contains(":LOGBOOK:\nCLOCK: [2024-01-01 Mon 10:00:00]\nCLOCK:"));
        // The open entry (first) is closed, the closed one is untouched.
        let c = clock_out(&c, t(10, 0, 5));
        assert!(c.contains("--[2024-01-01 Mon 10:00:05] =>  00:00:05"));
        assert_eq!(clock_out(&c, t(11, 0, 0)), c);
    }

    #[test]
    fn marker_edits_touch_only_the_token() {
        assert_eq!(set_marker("TODO a\nb", Some("DOING")), "DOING a\nb");
        assert_eq!(set_marker("a", Some("TODO")), "TODO a");
        assert_eq!(set_marker("DONE a", None), "a");
        assert_eq!(set_marker("## TODO a", Some("DONE")), "## DONE a");
        assert_eq!(set_marker("## a", Some("TODO")), "## TODO a");
        assert_eq!(set_marker("TODOx a", Some("NOW")), "NOW TODOx a");
        assert_eq!(set_marker("a", Some("BOGUS")), "a");
        assert_eq!(set_marker("LATER", Some("NOW")), "NOW LATER");
    }
}
