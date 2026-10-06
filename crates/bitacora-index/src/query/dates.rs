//! Date arguments of the DSL (`today`, `-7d`, `[[Oct 5th, 2026]]`, ...) resolved to journal-day
//! integers (`yyyyMMdd`) or millisecond timestamps. Behaviour follows the documented DSL
//! (`docs/analysis/logseq/03-parsing-indexing-search.md` §8.1), written independently (ADR-015).

use bitacora_core::date::Date;
use bitacora_core::journal::parse_journal_title;

use super::{QueryContext, QueryError};

const MS_PER_DAY: i64 = 86_400_000;

/// Days since 1970-01-01 of a civil date (proleptic Gregorian).
#[must_use]
pub fn days_from_civil(d: Date) -> i64 {
    let y = i64::from(d.year()) - i64::from(d.month() <= 2);
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let m = i64::from(d.month());
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(d.day()) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Inverse of [`days_from_civil`].
#[must_use]
pub fn civil_from_days(z: i64) -> Option<Date> {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = y + i64::from(m <= 2);
    Date::new(
        i32::try_from(y).ok()?,
        u8::try_from(m).ok()?,
        u8::try_from(d).ok()?,
    )
}

fn add_days(d: Date, n: i64) -> Option<Date> {
    civil_from_days(days_from_civil(d).checked_add(n)?)
}

fn add_months(d: Date, n: i64) -> Option<Date> {
    let total = i64::from(d.year()) * 12 + i64::from(d.month()) - 1 + n;
    let y = i32::try_from(total.div_euclid(12)).ok()?;
    let m = u8::try_from(total.rem_euclid(12) + 1).ok()?;
    // Clamp the day to the length of the target month.
    (1..=d.day()).rev().find_map(|day| Date::new(y, m, day))
}

/// A parsed `[+-]N<unit>` offset.
struct Offset {
    n: i64,
    unit: char,
}

fn parse_offset(s: &str) -> Option<Offset> {
    let unit = s.chars().last()?;
    let digits = &s[..s.len() - unit.len_utf8()];
    let n: i64 = digits.trim_start_matches('+').parse().ok()?;
    Some(Offset { n, unit })
}

fn bad(arg: &str) -> QueryError {
    QueryError::Syntax(format!("invalid date argument `{arg}`"))
}

fn page_ref_title(arg: &str) -> Option<&str> {
    arg.strip_prefix("[[")?.strip_suffix("]]")
}

fn journal_title_date(title: &str, ctx: &QueryContext) -> Option<Date> {
    parse_journal_title(&title.replace(':', ""), &ctx.journal_formatters)
}

fn day_ms(d: Date, ctx: &QueryContext) -> i64 {
    ctx.today_start_ms + (days_from_civil(d) - days_from_civil(ctx.today)) * MS_PER_DAY
}

/// Resolves a date argument to a journal day (`yyyyMMdd`). Accepts `today`, `yesterday`,
/// `tomorrow`, `[+-]N(d|w|m|y)` and `[[Journal title]]`.
pub fn journal_day(arg: &str, ctx: &QueryContext) -> Result<i64, QueryError> {
    let lc = arg.trim().to_lowercase();
    let date = match lc.as_str() {
        "today" => Some(ctx.today),
        "yesterday" => add_days(ctx.today, -1),
        "tomorrow" => add_days(ctx.today, 1),
        _ if page_ref_title(arg.trim()).is_some() => {
            journal_title_date(page_ref_title(arg.trim()).unwrap_or_default(), ctx)
        }
        _ => parse_offset(&lc).and_then(|o| match o.unit {
            'd' => add_days(ctx.today, o.n),
            'w' => add_days(ctx.today, o.n.checked_mul(7)?),
            'm' => add_months(ctx.today, o.n),
            'y' => add_months(ctx.today, o.n.checked_mul(12)?),
            _ => None,
        }),
    };
    date.map(|d| i64::from(d.journal_day()))
        .ok_or_else(|| bad(arg))
}

/// Resolves a date argument to a millisecond timestamp. Besides the journal forms it accepts
/// `now` and the hour / minute offsets `[+-]Nh`, `[+-]Nmin` (counted from the start of today,
/// like the day offsets).
pub fn timestamp(arg: &str, ctx: &QueryContext) -> Result<i64, QueryError> {
    let lc = arg.trim().to_lowercase();
    let start = ctx.today_start_ms;
    let ms = match lc.as_str() {
        "now" => Some(ctx.now_ms),
        "today" => Some(start),
        "yesterday" => Some(start - MS_PER_DAY),
        "tomorrow" => Some(start + MS_PER_DAY),
        _ if page_ref_title(arg.trim()).is_some() => {
            journal_title_date(page_ref_title(arg.trim()).unwrap_or_default(), ctx)
                .map(|d| day_ms(d, ctx))
        }
        _ => {
            let lc2 = lc
                .strip_suffix("min")
                .map_or(lc.clone(), |s| format!("{s}n"));
            parse_offset(&lc2).and_then(|o| match o.unit {
                'd' => start.checked_add(o.n.checked_mul(MS_PER_DAY)?),
                'w' => start.checked_add(o.n.checked_mul(7 * MS_PER_DAY)?),
                'h' => start.checked_add(o.n.checked_mul(3_600_000)?),
                'n' => start.checked_add(o.n.checked_mul(60_000)?),
                'm' => add_months(ctx.today, o.n).map(|d| day_ms(d, ctx)),
                'y' => add_months(ctx.today, o.n.checked_mul(12)?).map(|d| day_ms(d, ctx)),
                _ => None,
            })
        }
    };
    ms.ok_or_else(|| bad(arg))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;

    fn ctx() -> QueryContext {
        QueryContext::new(Date::new(2026, 3, 31).expect("date"), 1_000)
    }

    #[test]
    fn civil_round_trip() {
        for d in [(1970, 1, 1), (2000, 2, 29), (2026, 10, 6), (1969, 12, 31)] {
            let date = Date::new(d.0, d.1, d.2).expect("date");
            assert_eq!(civil_from_days(days_from_civil(date)), Some(date));
        }
        assert_eq!(days_from_civil(Date::new(1970, 1, 2).expect("d")), 1);
    }

    #[test]
    fn journal_days() {
        let c = ctx();
        assert_eq!(journal_day("today", &c).unwrap(), 20260331);
        assert_eq!(journal_day("Yesterday", &c).unwrap(), 20260330);
        assert_eq!(journal_day("tomorrow", &c).unwrap(), 20260401);
        assert_eq!(journal_day("-7d", &c).unwrap(), 20260324);
        assert_eq!(journal_day("+1w", &c).unwrap(), 20260407);
        assert_eq!(journal_day("-1m", &c).unwrap(), 20260228);
        assert_eq!(journal_day("+1y", &c).unwrap(), 20270331);
        assert_eq!(journal_day("[[Oct 5th, 2026]]", &c).unwrap(), 20261005);
        assert!(journal_day("[[not a date]]", &c).is_err());
        assert!(journal_day("soon", &c).is_err());
        assert!(journal_day("1h", &c).is_err());
    }

    #[test]
    fn timestamps() {
        let c = ctx();
        let start = c.today_start_ms;
        assert_eq!(timestamp("now", &c).unwrap(), 1_000);
        assert_eq!(timestamp("today", &c).unwrap(), start);
        assert_eq!(timestamp("-1d", &c).unwrap(), start - MS_PER_DAY);
        assert_eq!(timestamp("+2h", &c).unwrap(), start + 7_200_000);
        assert_eq!(timestamp("-30min", &c).unwrap(), start - 1_800_000);
        assert_eq!(timestamp("-1w", &c).unwrap(), start - 7 * MS_PER_DAY);
        assert_eq!(
            timestamp("[[Mar 30th, 2026]]", &c).unwrap(),
            start - MS_PER_DAY
        );
    }
}
