//! `yyyyMMdd` integer dates (the index representation) and `yyyy-mm-dd` strings.

use jiff::civil::Date;

/// `yyyyMMdd` of a civil date.
pub(crate) fn to_int(d: Date) -> i64 {
    i64::from(d.year()) * 10_000 + i64::from(d.month()) * 100 + i64::from(d.day())
}

/// Civil date of a `yyyyMMdd` integer.
pub(crate) fn from_int(v: i64) -> Option<Date> {
    let y = i16::try_from(v / 10_000).ok()?;
    let m = i8::try_from((v / 100) % 100).ok()?;
    let d = i8::try_from(v % 100).ok()?;
    Date::new(y, m, d).ok()
}

/// `yyyy-mm-dd` of a `yyyyMMdd` integer (the integer printed as-is when invalid).
pub(crate) fn to_iso(v: i64) -> String {
    match from_int(v) {
        Some(d) => format!("{:04}-{:02}-{:02}", d.year(), d.month(), d.day()),
        None => v.to_string(),
    }
}

/// Parse `yyyy-mm-dd` (or `yyyyMMdd`) into a `yyyyMMdd` integer.
pub(crate) fn parse(s: &str) -> Option<i64> {
    let s = s.trim();
    if s.len() == 8 && s.bytes().all(|b| b.is_ascii_digit()) {
        return from_int(s.parse().ok()?).map(to_int);
    }
    let d: Date = s.parse().ok()?;
    Some(to_int(d))
}

/// Today in the local time zone as `yyyyMMdd`.
pub(crate) fn today_int() -> i64 {
    to_int(jiff::Zoned::now().date())
}

/// Unix milliseconds of local midnight at the start of a `yyyyMMdd` day.
pub(crate) fn day_start_ms(v: i64) -> Option<i64> {
    let d = from_int(v)?;
    let z = d
        .at(0, 0, 0, 0)
        .to_zoned(jiff::tz::TimeZone::system())
        .ok()?;
    Some(z.timestamp().as_millisecond())
}

/// `days` days before the `yyyyMMdd` date.
pub(crate) fn minus_days(v: i64, days: i64) -> Option<i64> {
    let d = from_int(v)?;
    let span = jiff::Span::new().try_days(days).ok()?;
    d.checked_sub(span).ok().map(to_int)
}
