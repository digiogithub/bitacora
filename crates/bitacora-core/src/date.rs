//! Joda-style date pattern formatter and strict parser for the subset Logseq journals use.
//!
//! Supported pattern letters: `y`/`yyyy`/`yy`, `M`/`MM`/`MMM`/`MMMM`, `d`/`dd`/`do` (ordinal day),
//! `E`/`EE`/`EEE` (short weekday) and `EEEE` (full weekday). `'text'` quotes literals (`''` is a
//! quote). Any other non-letter character is a literal. Unknown letters make the pattern invalid.

use std::fmt;

/// Calendar date (proleptic Gregorian).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date {
    year: i32,
    month: u8,
    day: u8,
}

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const WEEKDAYS: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];

fn is_leap(y: i32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

fn days_in_month(y: i32, m: u8) -> u8 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ if is_leap(y) => 29,
        _ => 28,
    }
}

impl Date {
    /// Validated constructor.
    pub fn new(year: i32, month: u8, day: u8) -> Option<Self> {
        if !(0..=9999).contains(&year) || !(1..=12).contains(&month) {
            return None;
        }
        if day == 0 || day > days_in_month(year, month) {
            return None;
        }
        Some(Self { year, month, day })
    }

    /// From a journal day integer such as `20251114`.
    pub fn from_journal_day(n: u32) -> Option<Self> {
        Self::new((n / 10000) as i32, ((n / 100) % 100) as u8, (n % 100) as u8)
    }

    /// `yyyyMMdd` as an integer.
    pub fn journal_day(self) -> u32 {
        self.year as u32 * 10000 + u32::from(self.month) * 100 + u32::from(self.day)
    }

    /// Year.
    pub fn year(self) -> i32 {
        self.year
    }
    /// Month 1-12.
    pub fn month(self) -> u8 {
        self.month
    }
    /// Day of month 1-31.
    pub fn day(self) -> u8 {
        self.day
    }

    /// Weekday index, Monday = 0.
    pub fn weekday(self) -> usize {
        // Days since 1970-01-01 (Thursday).
        let y = i64::from(self.year) - i64::from(self.month <= 2);
        let era = y.div_euclid(400);
        let yoe = y.rem_euclid(400);
        let m = i64::from(self.month);
        let doy = (153 * ((m + 9) % 12) + 2) / 5 + i64::from(self.day) - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        let days = era * 146_097 + doe - 719_468;
        (days + 3).rem_euclid(7) as usize
    }
}

impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

/// Invalid date pattern.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid date pattern: {0}")]
pub struct PatternError(pub String);

#[derive(Debug, Clone, PartialEq, Eq)]
enum Tok {
    Year(usize),
    Month(usize),
    Day,
    DayPadded,
    DayOrdinal,
    Weekday { full: bool },
    Lit(String),
}

/// A compiled date pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DateFormat {
    toks: Vec<Tok>,
}

fn ordinal_suffix(d: u8) -> &'static str {
    match d {
        11..=13 => "th",
        _ => match d % 10 {
            1 => "st",
            2 => "nd",
            3 => "rd",
            _ => "th",
        },
    }
}

impl DateFormat {
    /// Compile a pattern.
    pub fn new(pattern: &str) -> Result<Self, PatternError> {
        let chars: Vec<char> = pattern.chars().collect();
        let mut toks = Vec::new();
        let mut i = 0;
        let mut lit = String::new();
        let flush = |lit: &mut String, toks: &mut Vec<Tok>| {
            if !lit.is_empty() {
                toks.push(Tok::Lit(std::mem::take(lit)));
            }
        };
        while i < chars.len() {
            let c = chars[i];
            if c == '\'' {
                i += 1;
                if chars.get(i) == Some(&'\'') {
                    lit.push('\'');
                    i += 1;
                    continue;
                }
                loop {
                    match chars.get(i) {
                        None => {
                            return Err(PatternError(format!("unterminated quote in {pattern:?}")));
                        }
                        Some('\'') if chars.get(i + 1) == Some(&'\'') => {
                            lit.push('\'');
                            i += 2;
                        }
                        Some('\'') => {
                            i += 1;
                            break;
                        }
                        Some(&ch) => {
                            lit.push(ch);
                            i += 1;
                        }
                    }
                }
            } else if c.is_ascii_alphabetic() {
                let mut n = 1;
                while chars.get(i + n) == Some(&c) {
                    n += 1;
                }
                flush(&mut lit, &mut toks);
                match c {
                    'y' => toks.push(Tok::Year(n)),
                    'M' if n <= 4 => toks.push(Tok::Month(n)),
                    'd' => match n {
                        1 => {
                            if chars.get(i + 1) == Some(&'o') {
                                toks.push(Tok::DayOrdinal);
                                i += 1; // consume `o`
                            } else {
                                toks.push(Tok::Day);
                            }
                        }
                        2 => toks.push(Tok::DayPadded),
                        _ => return Err(PatternError(pattern.to_owned())),
                    },
                    'E' if n <= 4 => toks.push(Tok::Weekday { full: n >= 4 }),
                    _ => return Err(PatternError(pattern.to_owned())),
                }
                i += n;
            } else {
                lit.push(c);
                i += 1;
            }
        }
        flush(&mut lit, &mut toks);
        Ok(Self { toks })
    }

    /// Render a date.
    pub fn format(&self, d: Date) -> String {
        let mut out = String::new();
        for t in &self.toks {
            match t {
                Tok::Year(2) => out.push_str(&format!("{:02}", d.year % 100)),
                Tok::Year(n) => out.push_str(&format!("{:0w$}", d.year, w = *n)),
                Tok::Month(1) => out.push_str(&d.month.to_string()),
                Tok::Month(2) => out.push_str(&format!("{:02}", d.month)),
                Tok::Month(3) => out.push_str(&MONTHS[usize::from(d.month) - 1][..3]),
                Tok::Month(_) => out.push_str(MONTHS[usize::from(d.month) - 1]),
                Tok::Day => out.push_str(&d.day.to_string()),
                Tok::DayPadded => out.push_str(&format!("{:02}", d.day)),
                Tok::DayOrdinal => {
                    out.push_str(&d.day.to_string());
                    out.push_str(ordinal_suffix(d.day));
                }
                Tok::Weekday { full } => {
                    let w = WEEKDAYS[d.weekday()];
                    out.push_str(if *full { w } else { &w[..3] });
                }
                Tok::Lit(s) => out.push_str(s),
            }
        }
        out
    }

    /// Strictly parse a whole string; `None` unless every token matches and the date exists.
    pub fn parse(&self, input: &str) -> Option<Date> {
        let (mut year, mut month, mut day) = (None, None, None);
        let mut rest = input;
        for t in &self.toks {
            match t {
                Tok::Year(n) => {
                    let w = if *n == 2 { 2 } else { 4 };
                    let (v, r) = take_digits(rest, w, w)?;
                    year = Some(if *n == 2 { 2000 + v } else { v });
                    rest = r;
                }
                Tok::Month(1 | 2) => {
                    let (v, r) = take_digits(rest, 1, 2)?;
                    month = Some(v);
                    rest = r;
                }
                Tok::Month(n) => {
                    let (idx, r) = take_name(rest, &MONTHS, *n == 3)?;
                    month = Some(idx as i32 + 1);
                    rest = r;
                }
                Tok::Day | Tok::DayPadded => {
                    let (v, r) = take_digits(rest, 1, 2)?;
                    day = Some(v);
                    rest = r;
                }
                Tok::DayOrdinal => {
                    let (v, r) = take_digits(rest, 1, 2)?;
                    r.get(..2).filter(|s| *s == ordinal_suffix(v as u8))?;
                    day = Some(v);
                    rest = &r[2..];
                }
                Tok::Weekday { full } => {
                    let (_, r) = take_name(rest, &WEEKDAYS, !*full)?;
                    rest = r;
                }
                Tok::Lit(s) => rest = rest.strip_prefix(s.as_str())?,
            }
        }
        if !rest.is_empty() {
            return None;
        }
        Date::new(year?, u8::try_from(month?).ok()?, u8::try_from(day?).ok()?)
    }
}

fn take_digits(s: &str, min: usize, max: usize) -> Option<(i32, &str)> {
    let n = s.bytes().take(max).take_while(u8::is_ascii_digit).count();
    if n < min {
        return None;
    }
    Some((s[..n].parse().ok()?, &s[n..]))
}

/// Case-sensitive English name match (inputs are capitalised by the caller).
fn take_name<'a>(s: &'a str, names: &[&str], short: bool) -> Option<(usize, &'a str)> {
    for (i, name) in names.iter().enumerate() {
        let n = if short { &name[..3] } else { name };
        if let Some(r) = s.strip_prefix(n) {
            return Some((i, r));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u8, day: u8) -> Date {
        Date::new(y, m, day).expect("date")
    }

    #[test]
    fn formats() {
        let f = |p: &str, dt| DateFormat::new(p).expect(p).format(dt);
        assert_eq!(f("MMM do, yyyy", d(2025, 11, 14)), "Nov 14th, 2025");
        assert_eq!(f("MMM do, yyyy", d(2025, 11, 1)), "Nov 1st, 2025");
        assert_eq!(f("MMM do, yyyy", d(2025, 11, 2)), "Nov 2nd, 2025");
        assert_eq!(f("MMM do, yyyy", d(2025, 11, 3)), "Nov 3rd, 2025");
        assert_eq!(f("MMM do, yyyy", d(2025, 11, 11)), "Nov 11th, 2025");
        assert_eq!(f("MMM do, yyyy", d(2025, 11, 22)), "Nov 22nd, 2025");
        assert_eq!(f("yyyy_MM_dd", d(2025, 1, 4)), "2025_01_04");
        assert_eq!(f("yyyy-MM-dd", d(2025, 11, 14)), "2025-11-14");
        assert_eq!(
            f("EEEE, MMMM d, yyyy", d(2025, 11, 14)),
            "Friday, November 14, 2025"
        );
        assert_eq!(f("EEE d/M/yy", d(2024, 2, 29)), "Thu 29/2/24");
        assert_eq!(f("yyyy 'W' dd", d(2024, 2, 9)), "2024 W 09");
        assert_eq!(f("dd.MM.yyyy", d(2021, 12, 31)), "31.12.2021");
    }

    #[test]
    fn weekdays() {
        assert_eq!(d(1970, 1, 1).weekday(), 3); // Thursday
        assert_eq!(d(2000, 1, 1).weekday(), 5); // Saturday
        assert_eq!(d(2025, 11, 14).weekday(), 4); // Friday
    }

    #[test]
    fn parses_strictly() {
        let p = |pat: &str, s: &str| DateFormat::new(pat).expect(pat).parse(s);
        assert_eq!(p("MMM do, yyyy", "Nov 14th, 2025"), Some(d(2025, 11, 14)));
        assert_eq!(p("MMM do, yyyy", "Nov 14st, 2025"), None);
        assert_eq!(p("MMM do, yyyy", "nov 14th, 2025"), None);
        assert_eq!(p("yyyy_MM_dd", "2025_11_14"), Some(d(2025, 11, 14)));
        assert_eq!(p("yyyy_MM_dd", "2025_02_30"), None);
        assert_eq!(p("yyyy_MM_dd", "2025_11_14x"), None);
        assert_eq!(p("yyyy-MM-dd", "2024-02-29"), Some(d(2024, 2, 29)));
        assert_eq!(p("yyyy-MM-dd", "2023-02-29"), None);
        assert_eq!(p("yyyy-MM-dd", "25-11-14"), None);
        assert_eq!(
            p("EEEE, MMMM d, yyyy", "Friday, November 14, 2025"),
            Some(d(2025, 11, 14))
        );
    }

    #[test]
    fn bad_patterns() {
        assert!(DateFormat::new("yyyy-QQ").is_err());
        assert!(DateFormat::new("'oops").is_err());
        assert!(DateFormat::new("ddd").is_err());
    }

    #[test]
    fn journal_day_roundtrip() {
        assert_eq!(d(2025, 11, 14).journal_day(), 20251114);
        assert_eq!(Date::from_journal_day(20251114), Some(d(2025, 11, 14)));
        assert_eq!(Date::from_journal_day(20251314), None);
    }
}
