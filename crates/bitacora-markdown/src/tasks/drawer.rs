//! Generic `:NAME:` ... `:END:` drawers and the `:LOGBOOK:` `CLOCK:` entries inside them.
//!
//! Drawer syntax (mldoc `drawer.ml`, observed): an opening line that is exactly `:name:` (leading
//! blanks allowed, nothing after the closing colon, not even a space), content lines, and a
//! closing line that is exactly `:END:` (case-insensitive). An unclosed drawer is plain text.
//! `:PROPERTIES:` drawers belong to the property scanner and are skipped here.
//!
//! Clock lines (`src/main/frontend/util/clock.cljs:69-93`): `CLOCK: [2024-01-01 Mon 10:00:00]` is
//! an open entry and `CLOCK: [start]--[end] =>  01:00:00` a closed one, with **two spaces** after
//! `=>`. [`format_clock`] emits exactly those strings.

use crate::lines::{Lines, ParserOptions};
use crate::span::Span;
use crate::tasks::timestamp::{Time, Timestamp, parse_timestamp};

/// A drawer found in a block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Drawer {
    /// The name as written (`LOGBOOK`).
    pub name: String,
    /// From the start of the opening line to the end of the closing line (EOL included).
    pub span: Span,
    /// The opening line, EOL included.
    pub open: Span,
    /// The closing line, EOL included.
    pub close: Span,
    /// Each content line without its EOL.
    pub lines: Vec<Span>,
}

impl Drawer {
    /// True for the `:LOGBOOK:` drawer.
    #[must_use]
    pub fn is_logbook(&self) -> bool {
        self.name.eq_ignore_ascii_case("logbook")
    }
}

fn trim_blanks(b: &[u8]) -> &[u8] {
    let s = b.iter().take_while(|&&c| c == b' ' || c == b'\t').count();
    &b[s..]
}

fn drawer_name(content: &[u8]) -> Option<&str> {
    let t = trim_blanks(content);
    let inner = t.strip_prefix(b":")?.strip_suffix(b":")?;
    if inner.is_empty() || inner.iter().any(|&c| c == b':' || c.is_ascii_whitespace()) {
        return None;
    }
    std::str::from_utf8(inner).ok()
}

/// Finds the non-`PROPERTIES` drawers in `text` (a block's content).
#[must_use]
pub fn find_drawers(text: &str, opts: ParserOptions) -> Vec<Drawer> {
    let lines: Vec<_> = Lines::with_options(text.as_bytes(), opts).collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let l = &lines[i];
        i += 1;
        if l.region.is_some() {
            continue;
        }
        let Some(name) = drawer_name(l.content) else {
            continue;
        };
        if name.eq_ignore_ascii_case("properties") || name.eq_ignore_ascii_case("end") {
            continue;
        }
        let close = (i..lines.len()).find(|&j| {
            lines[j].region.is_none()
                && trim_blanks(lines[j].content).eq_ignore_ascii_case(b":END:")
        });
        let Some(close) = close else {
            continue;
        };
        out.push(Drawer {
            name: name.to_owned(),
            span: Span::new(l.start, lines[close].end),
            open: l.span(),
            close: lines[close].span(),
            lines: lines[i..close]
                .iter()
                .map(|c| Span::new(c.start, c.content_end()))
                .collect(),
        });
        i = close + 1;
    }
    out
}

/// A clock entry of a logbook.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Clock {
    /// The line without its EOL (leading blanks excluded).
    pub span: Span,
    /// The clock-in time.
    pub start: Timestamp,
    /// The clock-out time of a closed entry.
    pub end: Option<Timestamp>,
    /// The duration of a closed entry as written (`01:00:00`).
    pub duration: Option<String>,
}

/// One line of a logbook drawer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogEntry {
    /// A `CLOCK:` line.
    Clock(Box<Clock>),
    /// Any other line (state-change notes such as `* State "DONE" from "TODO" [...]`).
    Other(Span),
}

/// Parses one `CLOCK:` line occupying `text[lo..hi]`.
#[must_use]
pub fn parse_clock(text: &str, lo: usize, hi: usize) -> Option<Clock> {
    let line = &text[..hi];
    let b = line.as_bytes();
    let mut i = lo;
    while i < hi && matches!(b[i], b' ' | b'\t') {
        i += 1;
    }
    let start_of_line = i;
    let rest = line[i..].strip_prefix("CLOCK:")?;
    i += "CLOCK:".len();
    let blanks = rest.bytes().take_while(|&c| c == b' ').count();
    if blanks == 0 {
        return None;
    }
    i += blanks;
    let start = parse_timestamp(line, i)?;
    if start.active {
        return None;
    }
    i = start.span.end;
    let mut end = None;
    let mut duration = None;
    if line[i..].starts_with("--")
        && let Some(e) = parse_timestamp(line, i + 2)
    {
        i = e.span.end;
        end = Some(e);
        let tail = line[i..].trim_start_matches(' ');
        if let Some(d) = tail.strip_prefix("=>") {
            let d = d.trim();
            if !d.is_empty() {
                duration = Some(d.to_owned());
            }
            i = hi;
        }
    }
    if i < hi && !line[i..].trim().is_empty() {
        return None;
    }
    Some(Clock {
        span: Span::new(start_of_line, line.trim_end().len().max(start_of_line)),
        start,
        end,
        duration,
    })
}

/// Parses the content lines of a logbook drawer.
#[must_use]
pub fn parse_logbook(text: &str, drawer: &Drawer) -> Vec<LogEntry> {
    drawer
        .lines
        .iter()
        .map(|l| match parse_clock(text, l.start, l.end) {
            Some(c) => LogEntry::Clock(Box::new(c)),
            None => LogEntry::Other(*l),
        })
        .collect()
}

/// Seconds since an arbitrary epoch for a calendar date and time (proleptic Gregorian). Dates are
/// not validated, which is fine for differences.
fn seconds(date: &crate::tasks::timestamp::Date, time: Option<&Time>) -> i64 {
    let (y, m, d) = (
        i64::from(date.year),
        i64::from(date.month),
        i64::from(date.day),
    );
    // Days from civil (Howard Hinnant's algorithm), valid for any year >= 0.
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe;
    let t = time.map_or(0, |t| {
        i64::from(t.hour) * 3600 + i64::from(t.min) * 60 + i64::from(t.sec.unwrap_or(0))
    });
    days * 86_400 + t
}

/// Elapsed seconds between two clock timestamps (end minus start).
#[must_use]
pub fn elapsed_seconds(start: &Timestamp, end: &Timestamp) -> i64 {
    seconds(&end.date, end.time.as_ref()) - seconds(&start.date, start.time.as_ref())
}

/// `HH:MM:SS` for a number of seconds (hours may exceed two digits).
#[must_use]
pub fn format_duration(total: i64) -> String {
    let total = total.max(0);
    format!(
        "{:02}:{:02}:{:02}",
        total / 3600,
        total % 3600 / 60,
        total % 60
    )
}

/// Parses a duration written `HH:MM:SS` into seconds.
#[must_use]
pub fn parse_duration(s: &str) -> Option<i64> {
    let mut it = s.split(':');
    let h: i64 = it.next()?.parse().ok()?;
    let m: i64 = it.next()?.parse().ok()?;
    let sec: i64 = it.next()?.parse().ok()?;
    it.next().is_none().then_some(h * 3600 + m * 60 + sec)
}

/// Total seconds of the closed entries of a logbook: the written duration when it parses, else
/// the difference of the two timestamps.
#[must_use]
pub fn total_seconds(entries: &[LogEntry]) -> i64 {
    entries
        .iter()
        .filter_map(|e| match e {
            LogEntry::Clock(c) => c.end.as_ref().map(|end| {
                c.duration
                    .as_deref()
                    .and_then(parse_duration)
                    .unwrap_or_else(|| elapsed_seconds(&c.start, end))
            }),
            LogEntry::Other(_) => None,
        })
        .sum()
}

/// The clock line Logseq writes: `CLOCK: [start]` when open, `CLOCK: [start]--[end] =>  HH:MM:SS`
/// when closed (the duration is computed from the two times).
#[must_use]
pub fn format_clock(start: &Timestamp, end: Option<&Timestamp>) -> String {
    match end {
        None => format!("CLOCK: {}", start.format()),
        Some(e) => format!(
            "CLOCK: {}--{} =>  {}",
            start.format(),
            e.format(),
            format_duration(elapsed_seconds(start, e))
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOG: &str = "title\n  :LOGBOOK:\n  CLOCK: [2024-01-01 Mon 10:00:00]\n  CLOCK: [2024-01-01 Mon 10:00:00]--[2024-01-01 Mon 11:00:00] =>  01:00:00\n  * State \"DONE\" from \"TODO\" [2024-01-01 Mon 10:00]\n  :END:\n  tail\n";

    #[test]
    fn finds_drawers_and_their_lines() {
        let ds = find_drawers(LOG, ParserOptions::default());
        assert_eq!(ds.len(), 1);
        let d = &ds[0];
        assert!(d.is_logbook());
        assert_eq!(d.lines.len(), 3);
        assert_eq!(&LOG[d.open.range()], "  :LOGBOOK:\n");
        assert_eq!(&LOG[d.close.range()], "  :END:\n");
        assert!(LOG[d.span.range()].starts_with("  :LOGBOOK:"));
        assert!(LOG[d.span.range()].ends_with(":END:\n"));
    }

    #[test]
    fn drawer_rules() {
        let none = |s: &str| find_drawers(s, ParserOptions::default()).is_empty();
        assert!(none("a\n  :LOGBOOK: \n  x\n  :END:\n"), "trailing space");
        assert!(none("a\n  :LOGBOOK:\n  x\n"), "unclosed");
        assert!(none("a\n  :PROPERTIES:\n  :a: b\n  :END:\n"), "properties");
        assert!(none("a\n```\n:LOGBOOK:\n:END:\n```\n"), "fenced");
        let d = find_drawers("a\n:logbook:\nx\n:end:", ParserOptions::default());
        assert_eq!(d.len(), 1);
        assert!(d[0].is_logbook());
        let d = find_drawers(
            "a\n  :MYDRAWER:\n  [[x]]\n  :END:",
            ParserOptions::default(),
        );
        assert_eq!(d[0].name, "MYDRAWER");
    }

    #[test]
    fn logbook_entries_parse() {
        let d = &find_drawers(LOG, ParserOptions::default())[0];
        let entries = parse_logbook(LOG, d);
        assert_eq!(entries.len(), 3);
        let LogEntry::Clock(open) = &entries[0] else {
            panic!("clock expected")
        };
        assert!(open.end.is_none());
        assert_eq!(open.start.time.map(|t| t.hour), Some(10));
        let LogEntry::Clock(closed) = &entries[1] else {
            panic!("clock expected")
        };
        assert_eq!(closed.duration.as_deref(), Some("01:00:00"));
        assert_eq!(
            &LOG[closed.span.range()],
            "CLOCK: [2024-01-01 Mon 10:00:00]--[2024-01-01 Mon 11:00:00] =>  01:00:00"
        );
        assert!(matches!(entries[2], LogEntry::Other(_)));
        assert_eq!(total_seconds(&entries), 3600);
    }

    #[test]
    fn formatter_emits_the_same_strings() {
        let open = "CLOCK: [2024-01-01 Mon 10:00:00]";
        let closed = "CLOCK: [2024-01-01 Mon 10:00:00]--[2024-01-01 Mon 11:30:15] =>  01:30:15";
        let c = parse_clock(open, 0, open.len()).expect("open clock");
        assert_eq!(format_clock(&c.start, None), open);
        let c = parse_clock(closed, 0, closed.len()).expect("closed clock");
        assert_eq!(format_clock(&c.start, c.end.as_ref()), closed);
    }

    #[test]
    fn durations_cross_midnight_and_month_ends() {
        let line = "CLOCK: [2024-01-31 Wed 23:30:00]--[2024-02-01 Thu 01:00:00] =>  01:30:00";
        let c = parse_clock(line, 0, line.len()).expect("clock");
        let end = c.end.as_ref().expect("end");
        assert_eq!(elapsed_seconds(&c.start, end), 5400);
        assert_eq!(format_duration(90_061), "25:01:01");
    }

    #[test]
    fn not_clock_lines() {
        for s in [
            "CLOCK:[2024-01-01 Mon 10:00:00]",
            "CLOCK: <2024-01-01 Mon 10:00:00>",
            "clock: [2024-01-01 Mon 10:00:00]",
            "CLOCK: [2024-01-01 Mon 10:00:00] junk",
            "x CLOCK: [2024-01-01 Mon 10:00:00]",
        ] {
            assert!(parse_clock(s, 0, s.len()).is_none(), "{s}");
        }
    }
}
