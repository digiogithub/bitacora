//! Org-style timestamps (`<2024-01-01 Mon .+1d>`) and the `SCHEDULED:` / `DEADLINE:` / `CLOSED:`
//! planning lines built on them.
//!
//! Syntax (mldoc 1.5.7 `inline.ml`, observed with `tools/mldoc-diff`,
//! `docs/analysis/logseq/02-markdown-block-syntax.md` §5.3):
//!
//! * `<...>` is active, `[...]` inactive; the closer must match the opener.
//! * `YYYY-MM-DD` (digit runs, not validated: `<2024-13-45 Mon>` parses) then exactly one space and
//!   a day name (any alphabetic word, `Mon`, `lun`, ...). A date without a day name is not a
//!   timestamp.
//! * Optional ` H:MM` (or `HH:MM`, optionally `-HH:MM` for a range, optionally `:SS`) right after
//!   the day name, then an optional repeater ` +Nu`, ` ++Nu` or ` .+Nu` with `u` in `h d w m y`.
//!   A warning (` -Nu`) and unknown words before the closer are tolerated and ignored; a space
//!   directly before the closer is not.

use crate::span::Span;

/// A calendar date as written (never validated).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Date {
    /// Year as written.
    pub year: u32,
    /// Month as written (1-12 for real dates).
    pub month: u32,
    /// Day as written.
    pub day: u32,
}

/// A time of day.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Time {
    /// Hour.
    pub hour: u32,
    /// Minute.
    pub min: u32,
    /// Seconds, when written (`CLOCK` entries carry them).
    pub sec: Option<u32>,
}

/// The repeater flavour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepeatKind {
    /// `+1d`: repeat from the original date.
    Plus,
    /// `++1d`: repeat until the date is in the future.
    DoublePlus,
    /// `.+1d`: repeat from today.
    Dotted,
}

/// The repeater unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepeatUnit {
    /// `h`.
    Hour,
    /// `d`.
    Day,
    /// `w`.
    Week,
    /// `m`.
    Month,
    /// `y`.
    Year,
}

/// A repeater such as `.+1d`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Repeater {
    /// `+`, `++` or `.+`.
    pub kind: RepeatKind,
    /// The interval.
    pub value: u32,
    /// The unit.
    pub unit: RepeatUnit,
}

impl Repeater {
    /// The repeater as written in a file (`.+1d`).
    #[must_use]
    pub fn format(&self) -> String {
        let kind = match self.kind {
            RepeatKind::Plus => "+",
            RepeatKind::DoublePlus => "++",
            RepeatKind::Dotted => ".+",
        };
        let unit = match self.unit {
            RepeatUnit::Hour => 'h',
            RepeatUnit::Day => 'd',
            RepeatUnit::Week => 'w',
            RepeatUnit::Month => 'm',
            RepeatUnit::Year => 'y',
        };
        format!("{kind}{}{unit}", self.value)
    }
}

/// A parsed timestamp.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Timestamp {
    /// The whole timestamp including its brackets, in the coordinates of the parsed text.
    pub span: Span,
    /// `<...>` (true) or `[...]` (false).
    pub active: bool,
    /// The date.
    pub date: Date,
    /// The day name as written.
    pub weekday: String,
    /// The start time.
    pub time: Option<Time>,
    /// The end time of a `10:00-11:00` range.
    pub end_time: Option<Time>,
    /// The repeater.
    pub repeater: Option<Repeater>,
}

impl Timestamp {
    /// The date as the integer Logseq stores (`20240101`).
    #[must_use]
    pub const fn yyyymmdd(&self) -> u32 {
        self.date.year * 10_000 + self.date.month * 100 + self.date.day
    }

    /// Canonical text: `<2024-01-01 Mon 10:00 .+1d>`.
    #[must_use]
    pub fn format(&self) -> String {
        let (open, close) = if self.active { ('<', '>') } else { ('[', ']') };
        let mut s = format!(
            "{open}{:04}-{:02}-{:02} {}",
            self.date.year, self.date.month, self.date.day, self.weekday
        );
        if let Some(t) = &self.time {
            s.push(' ');
            s.push_str(&format_time(t));
            if let Some(e) = &self.end_time {
                s.push('-');
                s.push_str(&format_time(e));
            }
        }
        if let Some(r) = &self.repeater {
            s.push(' ');
            s.push_str(&r.format());
        }
        s.push(close);
        s
    }
}

/// Days since 1970-01-01 for a proleptic Gregorian date.
pub(crate) fn days_from_civil(y: i32, m: u32, d: u32) -> i64 {
    let y = i64::from(if m <= 2 { y - 1 } else { y });
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let m = i64::from(m);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// English three-letter weekday name (`Mon`) of a calendar date, as Logseq writes it.
pub(crate) fn weekday_name(y: i32, m: u32, d: u32) -> &'static str {
    const NAMES: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
    // 1970-01-01 was a Thursday (index 3).
    let idx = (days_from_civil(y, m, d) + 3).rem_euclid(7);
    NAMES[usize::try_from(idx).unwrap_or(0)]
}

fn format_time(t: &Time) -> String {
    match t.sec {
        Some(s) => format!("{:02}:{:02}:{s:02}", t.hour, t.min),
        None => format!("{:02}:{:02}", t.hour, t.min),
    }
}

fn digits(b: &[u8], i: usize, min: usize, max: usize) -> Option<(u32, usize)> {
    let n = b[i..]
        .iter()
        .take(max)
        .take_while(|c| c.is_ascii_digit())
        .count();
    if n < min {
        return None;
    }
    let mut v: u32 = 0;
    for &c in &b[i..i + n] {
        v = v.checked_mul(10)?.checked_add(u32::from(c - b'0'))?;
    }
    Some((v, i + n))
}

fn parse_time(tok: &str) -> Option<(Time, Option<Time>)> {
    let b = tok.as_bytes();
    let (hour, i) = digits(b, 0, 1, 2)?;
    if b.get(i) != Some(&b':') {
        return None;
    }
    let (min, mut i) = digits(b, i + 1, 2, 2)?;
    let mut time = Time {
        hour,
        min,
        sec: None,
    };
    let mut end = None;
    if b.get(i) == Some(&b'-') {
        let (h2, j) = digits(b, i + 1, 1, 2)?;
        if b.get(j) != Some(&b':') {
            return None;
        }
        let (m2, _) = digits(b, j + 1, 2, 2)?;
        end = Some(Time {
            hour: h2,
            min: m2,
            sec: None,
        });
        i = b.len();
    } else if b.get(i) == Some(&b':') {
        let (s, j) = digits(b, i + 1, 2, 2)?;
        time.sec = Some(s);
        i = j;
    }
    (i == b.len() || end.is_some()).then_some((time, end))
}

pub(crate) fn parse_repeater(tok: &str) -> Option<Repeater> {
    let (kind, rest) = if let Some(r) = tok.strip_prefix("++") {
        (RepeatKind::DoublePlus, r)
    } else if let Some(r) = tok.strip_prefix(".+") {
        (RepeatKind::Dotted, r)
    } else {
        (RepeatKind::Plus, tok.strip_prefix('+')?)
    };
    let (value, i) = digits(rest.as_bytes(), 0, 1, 9)?;
    let unit = match &rest[i..] {
        "h" => RepeatUnit::Hour,
        "d" => RepeatUnit::Day,
        "w" => RepeatUnit::Week,
        "m" => RepeatUnit::Month,
        "y" => RepeatUnit::Year,
        _ => return None,
    };
    Some(Repeater { kind, value, unit })
}

/// Parses the timestamp that starts at byte `at` of `text` (which must be on one line).
#[must_use]
pub fn parse_timestamp(text: &str, at: usize) -> Option<Timestamp> {
    let b = text.as_bytes();
    let (active, closer) = match b.get(at)? {
        b'<' => (true, b'>'),
        b'[' => (false, b']'),
        _ => return None,
    };
    let (year, i) = digits(b, at + 1, 1, 9)?;
    if b.get(i) != Some(&b'-') {
        return None;
    }
    let (month, i) = digits(b, i + 1, 1, 2)?;
    if b.get(i) != Some(&b'-') {
        return None;
    }
    let (day, i) = digits(b, i + 1, 1, 2)?;
    if b.get(i) != Some(&b' ') {
        return None;
    }
    let day_start = i + 1;
    let wd_len = text[day_start..]
        .chars()
        .take_while(|c| c.is_alphabetic())
        .map(char::len_utf8)
        .sum::<usize>();
    if wd_len == 0 {
        return None;
    }
    let weekday = text[day_start..day_start + wd_len].to_owned();
    let mut i = day_start + wd_len;

    let mut time = None;
    let mut end_time = None;
    let mut repeater = None;
    let mut index = 0;
    loop {
        match b.get(i)? {
            c if *c == closer => break,
            b' ' => {
                let tok_start = i + 1;
                let tok_len = b[tok_start..]
                    .iter()
                    .take_while(|&&c| c != b' ' && c != closer && c != b'\n' && c != b'\r')
                    .count();
                if tok_len == 0 {
                    return None;
                }
                let tok = text.get(tok_start..tok_start + tok_len)?;
                if index == 0
                    && let Some((t, e)) = parse_time(tok)
                {
                    time = Some(t);
                    end_time = e;
                } else if repeater.is_none()
                    && index <= usize::from(time.is_some())
                    && let Some(r) = parse_repeater(tok)
                {
                    repeater = Some(r);
                }
                index += 1;
                i = tok_start + tok_len;
            }
            _ => return None,
        }
    }
    Some(Timestamp {
        span: Span::new(at, i + 1),
        active,
        date: Date { year, month, day },
        weekday,
        time,
        end_time,
        repeater,
    })
}

/// Which planning keyword introduced a timestamp.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanningKind {
    /// `SCHEDULED:`.
    Scheduled,
    /// `DEADLINE:`.
    Deadline,
    /// `CLOSED:`.
    Closed,
}

impl PlanningKind {
    /// The keyword as Logseq writes it (upper case, with the colon).
    #[must_use]
    pub const fn keyword(self) -> &'static str {
        match self {
            PlanningKind::Scheduled => "SCHEDULED:",
            PlanningKind::Deadline => "DEADLINE:",
            PlanningKind::Closed => "CLOSED:",
        }
    }
}

/// One `KEYWORD: timestamp` entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Planning {
    /// The keyword.
    pub kind: PlanningKind,
    /// From the keyword to the end of the timestamp (or of the range).
    pub span: Span,
    /// The timestamp.
    pub timestamp: Timestamp,
    /// The end of a `<a>--<b>` range. Logseq lifts neither SCHEDULED nor DEADLINE ranges.
    pub range_end: Option<Timestamp>,
}

impl Planning {
    /// The `yyyymmdd` value Logseq stores for `:block/scheduled` / `:block/deadline`; `None` for
    /// `CLOSED` and for ranges.
    #[must_use]
    pub const fn lifted_date(&self) -> Option<u32> {
        match (self.kind, &self.range_end) {
            (PlanningKind::Scheduled | PlanningKind::Deadline, None) => {
                Some(self.timestamp.yyyymmdd())
            }
            _ => None,
        }
    }
}

fn keyword_at(text: &str, i: usize) -> Option<(PlanningKind, usize)> {
    for kind in [
        PlanningKind::Scheduled,
        PlanningKind::Deadline,
        PlanningKind::Closed,
    ] {
        let kw = kind.keyword();
        if let Some(head) = text.get(i..i + kw.len())
            && head.eq_ignore_ascii_case(kw)
        {
            return Some((kind, i + kw.len()));
        }
    }
    None
}

/// Parses the planning entries of one line, `text[lo..hi]` (no line break): the line must start
/// (after blanks) with a keyword; `SCHEDULED: <..> DEADLINE: <..>` yields two entries.
#[must_use]
pub fn parse_planning_line(text: &str, lo: usize, hi: usize) -> Vec<Planning> {
    let line = &text[..hi];
    let b = line.as_bytes();
    let mut out = Vec::new();
    let mut i = lo;
    loop {
        while i < hi && matches!(b[i], b' ' | b'\t') {
            i += 1;
        }
        let Some((kind, after)) = keyword_at(line, i) else {
            break;
        };
        let mut j = after;
        while j < hi && b[j] == b' ' {
            j += 1;
        }
        if j == after {
            break;
        }
        let Some(ts) = parse_timestamp(line, j) else {
            break;
        };
        let mut end = ts.span.end;
        let mut range_end = None;
        if line[end..].starts_with("--")
            && let Some(ts2) = parse_timestamp(line, end + 2)
        {
            end = ts2.span.end;
            range_end = Some(ts2);
        }
        out.push(Planning {
            kind,
            span: Span::new(i, end),
            timestamp: ts,
            range_end,
        });
        i = end;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ts(s: &str) -> Timestamp {
        parse_timestamp(s, 0).unwrap_or_else(|| panic!("no timestamp in {s:?}"))
    }

    #[test]
    fn repeaters_units_and_activity() {
        let t = ts("<2024-01-01 Mon .+1d>");
        assert_eq!(t.yyyymmdd(), 20_240_101);
        assert_eq!(
            t.repeater,
            Some(Repeater {
                kind: RepeatKind::Dotted,
                value: 1,
                unit: RepeatUnit::Day
            })
        );
        assert!(t.active);
        for (src, kind, unit) in [
            ("<2024-01-01 Mon +1w>", RepeatKind::Plus, RepeatUnit::Week),
            (
                "<2024-01-01 Mon ++2m>",
                RepeatKind::DoublePlus,
                RepeatUnit::Month,
            ),
            ("<2024-01-01 Mon +3y>", RepeatKind::Plus, RepeatUnit::Year),
            (
                "<2024-01-01 Mon .+4h>",
                RepeatKind::Dotted,
                RepeatUnit::Hour,
            ),
        ] {
            let r = ts(src).repeater.unwrap();
            assert_eq!((r.kind, r.unit), (kind, unit), "{src}");
        }
        assert!(!ts("[2024-01-01 Mon]").active);
    }

    #[test]
    fn times_and_ranges() {
        let t = ts("<2024-01-01 Mon 10:30 +1y>");
        assert_eq!(t.time.map(|t| (t.hour, t.min)), Some((10, 30)));
        assert_eq!(t.repeater.map(|r| r.value), Some(1));
        let t = ts("<2024-01-01 Mon 9:05>");
        assert_eq!(t.time.map(|t| (t.hour, t.min)), Some((9, 5)));
        let t = ts("<2024-01-01 Mon 10:30-11:45>");
        assert_eq!(t.end_time.map(|t| (t.hour, t.min)), Some((11, 45)));
        let t = ts("[2024-01-01 Mon 10:00:07]");
        assert_eq!(t.time.and_then(|t| t.sec), Some(7));
    }

    #[test]
    fn leniency_matches_mldoc() {
        // Junk and a bare warning before the closer are ignored.
        assert_eq!(ts("<2024-01-01 Mon foo>").repeater, None);
        assert_eq!(ts("<2024-01-01 Mon -2d>").repeater, None);
        assert_eq!(ts("<2024-01-01 Mon +1x>").repeater, None);
        assert_eq!(ts("<2024-13-45 Mon>").yyyymmdd(), 20_241_345);
        assert_eq!(ts("<2024-01-01 lun>").weekday, "lun");
        assert_eq!(ts("<24-01-01 Mon>").date.year, 24);
    }

    #[test]
    fn invalid_timestamps() {
        for s in [
            "<2024-01-01>",
            "<2024-01-01 Mon >",
            "<2024-01-01 Mon]",
            "[2024-01-01 Mon>",
            "<2024-01-01   Mon>",
            "2024-01-01 Mon",
            "<2024-01-01 Mon",
        ] {
            assert!(parse_timestamp(s, 0).is_none(), "{s}");
        }
    }

    #[test]
    fn formatter_emits_canonical_text() {
        for s in [
            "<2024-01-01 Mon>",
            "[2024-01-01 Mon]",
            "<2024-01-01 Mon .+1d>",
            "<2024-01-01 Mon 10:30 ++2m>",
            "<2024-01-01 Mon 10:30-11:45>",
            "[2024-01-01 Mon 10:00:00]",
        ] {
            assert_eq!(ts(s).format(), s);
        }
    }

    fn planning(line: &str) -> Vec<Planning> {
        parse_planning_line(line, 0, line.len())
    }

    #[test]
    fn scheduled_and_deadline_lines() {
        let p = planning("  SCHEDULED: <2024-01-01 Mon .+1d>");
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].kind, PlanningKind::Scheduled);
        assert_eq!(p[0].lifted_date(), Some(20_240_101));
        assert!(p[0].timestamp.repeater.is_some());

        let p = planning("SCHEDULED: <2024-01-01 Mon> DEADLINE: [2024-02-01 Thu]");
        assert_eq!(p.len(), 2);
        assert_eq!(p[1].kind, PlanningKind::Deadline);
        assert!(!p[1].timestamp.active);

        let p = planning("scheduled: <2024-01-01 Mon>");
        assert_eq!(p[0].kind, PlanningKind::Scheduled);

        let p = planning("DEADLINE: <2024-01-01 Mon>--<2024-01-05 Fri>");
        assert!(p[0].range_end.is_some());
        assert_eq!(p[0].lifted_date(), None);

        let p = planning("CLOSED: [2024-01-01 Mon 10:00]");
        assert_eq!(p[0].lifted_date(), None);

        assert!(planning("SCHEDULED:<2024-01-01 Mon>").is_empty());
        assert!(planning("text SCHEDULED: <2024-01-01 Mon>").is_empty());
        assert!(planning("SCHEDULED: <2024-01-01>").is_empty());
    }
}
