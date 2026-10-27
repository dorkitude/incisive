//! Timeframes, timezones and interval buckets.

use crate::event::parse_time;
use chrono::{DateTime, Datelike, Duration, FixedOffset, LocalResult, NaiveDate, NaiveDateTime, SecondsFormat, TimeZone, Timelike, Utc};
use once_cell::sync::Lazy;
use regex::Regex;
use serde_json::Value;

/// A query timezone: an IANA zone or a fixed offset.
#[derive(Clone, Copy, Debug)]
pub enum Zone {
    Named(chrono_tz::Tz),
    Fixed(FixedOffset),
}

impl Zone {
    pub fn utc() -> Zone {
        Zone::Fixed(FixedOffset::east_opt(0).unwrap())
    }

    fn to_local(self, t: DateTime<Utc>) -> NaiveDateTime {
        match self {
            Zone::Named(tz) => t.with_timezone(&tz).naive_local(),
            Zone::Fixed(o) => t.with_timezone(&o).naive_local(),
        }
    }

    /// Resolves a local time to UTC; in a DST gap, the first valid instant after it.
    fn from_local(self, n: NaiveDateTime) -> DateTime<Utc> {
        let resolve = |r: LocalResult<DateTime<Utc>>| match r {
            LocalResult::Single(t) | LocalResult::Ambiguous(t, _) => Some(t),
            LocalResult::None => None,
        };
        let conv = |n: NaiveDateTime| match self {
            Zone::Named(tz) => resolve(tz.from_local_datetime(&n).map(|t| t.with_timezone(&Utc))),
            Zone::Fixed(o) => resolve(o.from_local_datetime(&n).map(|t| t.with_timezone(&Utc))),
        };
        let mut n2 = n;
        for _ in 0..4 {
            if let Some(t) = conv(n2) {
                return t;
            }
            n2 += Duration::minutes(30);
        }
        Utc.from_utc_datetime(&n)
    }

    /// Formats an instant in this zone with millisecond precision (`Z` when the offset is zero).
    pub fn format(self, t: DateTime<Utc>) -> String {
        match self {
            Zone::Named(tz) => t.with_timezone(&tz).to_rfc3339_opts(SecondsFormat::Millis, true),
            Zone::Fixed(o) => t.with_timezone(&o).to_rfc3339_opts(SecondsFormat::Millis, true),
        }
    }
}

fn offset_zone(secs: i64) -> Result<Zone, String> {
    FixedOffset::east_opt(secs as i32).map(Zone::Fixed).ok_or_else(|| format!("invalid timezone offset {secs}"))
}

/// Accepts an IANA name, or a UTC offset in seconds (number or numeric string).
pub fn parse_timezone(v: Option<&Value>) -> Result<Zone, String> {
    match v {
        None | Some(Value::Null) => Ok(Zone::utc()),
        Some(Value::Number(n)) => offset_zone(n.as_f64().unwrap_or(0.0) as i64),
        Some(Value::String(s)) if s.is_empty() => Ok(Zone::utc()),
        Some(Value::String(s)) => {
            if let Ok(n) = s.parse::<i64>() {
                return offset_zone(n);
            }
            s.parse::<chrono_tz::Tz>().map(Zone::Named).map_err(|_| format!("unknown timezone {s:?}"))
        }
        Some(_) => Err("timezone must be a string or a number of seconds".into()),
    }
}

/// A half-open [start, end) window in UTC.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Range {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Unit {
    Minute,
    Hour,
    Day,
    Week,
    Month,
    Year,
}

fn unit(s: &str) -> Option<Unit> {
    Some(match s {
        "minute" => Unit::Minute,
        "hour" => Unit::Hour,
        "day" => Unit::Day,
        "week" => Unit::Week,
        "month" => Unit::Month,
        "year" => Unit::Year,
        _ => return None,
    })
}

/// Start of the calendar unit containing t, in zone z. Weeks start on Sunday.
fn unit_start(t: DateTime<Utc>, u: Unit, z: Zone) -> DateTime<Utc> {
    let l = z.to_local(t);
    let d = l.date();
    let n = match u {
        Unit::Minute => d.and_hms_opt(l.hour(), l.minute(), 0).unwrap(),
        Unit::Hour => d.and_hms_opt(l.hour(), 0, 0).unwrap(),
        Unit::Day => d.and_hms_opt(0, 0, 0).unwrap(),
        Unit::Week => (d - Duration::days(d.weekday().num_days_from_sunday() as i64)).and_hms_opt(0, 0, 0).unwrap(),
        Unit::Month => NaiveDate::from_ymd_opt(d.year(), d.month(), 1).unwrap().and_hms_opt(0, 0, 0).unwrap(),
        Unit::Year => NaiveDate::from_ymd_opt(d.year(), 1, 1).unwrap().and_hms_opt(0, 0, 0).unwrap(),
    };
    z.from_local(n)
}

fn add_months(d: NaiveDate, n: i64) -> NaiveDate {
    let total = d.year() as i64 * 12 + d.month0() as i64 + n;
    let (y, m) = (total.div_euclid(12) as i32, total.rem_euclid(12) as u32 + 1);
    let mut day = d.day();
    loop {
        if let Some(x) = NaiveDate::from_ymd_opt(y, m, day) {
            return x;
        }
        day -= 1;
    }
}

/// Adds n calendar units in zone z (DST-correct for days and larger).
pub fn add_units(t: DateTime<Utc>, u: Unit, n: i64, z: Zone) -> DateTime<Utc> {
    match u {
        Unit::Minute => t + Duration::minutes(n),
        Unit::Hour => t + Duration::hours(n),
        _ => {
            let l = z.to_local(t);
            let d = match u {
                Unit::Day => l.date() + Duration::days(n),
                Unit::Week => l.date() + Duration::days(7 * n),
                Unit::Month => add_months(l.date(), n),
                _ => add_months(l.date(), 12 * n),
            };
            z.from_local(d.and_time(l.time()))
        }
    }
}

static REL: Lazy<Regex> = Lazy::new(|| Regex::new(r"^(this|previous)(?:_(\d+))?_(minute|hour|day|week|month|year)s?$").unwrap());
static EVERY: Lazy<Regex> = Lazy::new(|| Regex::new(r"^every_(\d+)_(minute|hour|day|week|month|year)s?$").unwrap());

/// Resolves a relative timeframe ("this_7_days", "previous_month") or {"start", "end"}.
/// "this_N_units" covers the current unit and the N-1 before it, ending at the start of the next
/// unit; "previous_N_units" is the N whole units before the current one. None means all time.
pub fn parse_timeframe(v: Option<&Value>, z: Zone, now: DateTime<Utc>) -> Result<Option<Range>, String> {
    match v {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) if s.is_empty() => Ok(None),
        Some(Value::String(s)) if s.trim_start().starts_with('{') => {
            let v: Value = serde_json::from_str(s).map_err(|_| "timeframe is not valid JSON".to_string())?;
            parse_timeframe(Some(&v), z, now)
        }
        Some(Value::String(s)) => {
            let c = REL.captures(s).ok_or_else(|| format!("invalid relative timeframe {s:?}"))?;
            let n: i64 = c.get(2).map(|m| m.as_str().parse().unwrap_or(0)).unwrap_or(1);
            if n < 1 {
                return Err(format!("invalid relative timeframe {s:?}"));
            }
            let u = unit(&c[3]).unwrap();
            let cur = unit_start(now, u, z);
            if &c[1] == "this" {
                Ok(Some(Range { start: add_units(cur, u, -(n - 1), z), end: add_units(cur, u, 1, z) }))
            } else {
                Ok(Some(Range { start: add_units(cur, u, -n, z), end: cur }))
            }
        }
        Some(Value::Object(m)) => {
            let get = |k: &str| m.get(k).and_then(Value::as_str).filter(|s| !s.is_empty());
            let (Some(s), Some(e)) = (get("start"), get("end")) else {
                return Err("absolute timeframe requires start and end".into());
            };
            let start = parse_time(s).ok_or_else(|| format!("invalid timeframe start {s:?}"))?;
            let end = parse_time(e).ok_or_else(|| format!("invalid timeframe end {e:?}"))?;
            if end <= start {
                return Err("timeframe end must be after start".into());
            }
            Ok(Some(Range { start, end }))
        }
        Some(_) => Err("timeframe must be a string or an object with start and end".into()),
    }
}

/// Splits r into interval buckets aligned to r.start (the last one clipped to r.end).
pub fn buckets(r: Range, interval: &str, z: Zone, max: usize) -> Result<Vec<Range>, String> {
    let (u, n) = match interval {
        "minutely" => (Unit::Minute, 1),
        "hourly" => (Unit::Hour, 1),
        "daily" => (Unit::Day, 1),
        "weekly" => (Unit::Week, 1),
        "monthly" => (Unit::Month, 1),
        "yearly" => (Unit::Year, 1),
        other => {
            let c = EVERY.captures(other).ok_or_else(|| format!("invalid interval {other:?}"))?;
            let n: i64 = c[1].parse().unwrap_or(0);
            if n < 1 {
                return Err(format!("invalid interval {other:?}"));
            }
            (unit(&c[2]).unwrap(), n)
        }
    };
    let mut out = vec![];
    let mut s = r.start;
    let mut i = 1;
    while s < r.end {
        let mut e = add_units(r.start, u, n * i, z);
        if e > r.end {
            e = r.end;
        }
        if e <= s {
            return Err("interval did not advance".into());
        }
        out.push(Range { start: s, end: e });
        if out.len() > max {
            return Err(format!("interval produces more than {max} buckets"));
        }
        s = e;
        i += 1;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fmt(r: Range, z: Zone) -> (String, String) {
        (z.format(r.start), z.format(r.end))
    }

    #[test]
    fn relative_timeframes() {
        let ny = Zone::Named("America/New_York".parse().unwrap());
        let utc = Zone::utc();
        let now = Utc.with_ymd_and_hms(2026, 3, 11, 15, 30, 0).unwrap();
        let cases = [
            ("this_day", utc, "2026-03-11T00:00:00.000Z", "2026-03-12T00:00:00.000Z"),
            ("this_7_days", utc, "2026-03-05T00:00:00.000Z", "2026-03-12T00:00:00.000Z"),
            ("previous_7_days", utc, "2026-03-04T00:00:00.000Z", "2026-03-11T00:00:00.000Z"),
            ("previous_day", ny, "2026-03-10T00:00:00.000-04:00", "2026-03-11T00:00:00.000-04:00"),
            ("previous_4_days", ny, "2026-03-07T00:00:00.000-05:00", "2026-03-11T00:00:00.000-04:00"),
            ("this_week", utc, "2026-03-08T00:00:00.000Z", "2026-03-15T00:00:00.000Z"),
            ("previous_month", utc, "2026-02-01T00:00:00.000Z", "2026-03-01T00:00:00.000Z"),
            ("this_2_hours", utc, "2026-03-11T14:00:00.000Z", "2026-03-11T16:00:00.000Z"),
            ("previous_minute", utc, "2026-03-11T15:29:00.000Z", "2026-03-11T15:30:00.000Z"),
            ("this_year", utc, "2026-01-01T00:00:00.000Z", "2027-01-01T00:00:00.000Z"),
        ];
        for (tf, z, s, e) in cases {
            let r = parse_timeframe(Some(&json!(tf)), z, now).unwrap().unwrap();
            assert_eq!(fmt(r, z), (s.to_string(), e.to_string()), "{tf}");
        }
        for bad in ["last_week", "this_0_days", "today"] {
            assert!(parse_timeframe(Some(&json!(bad)), utc, now).is_err(), "{bad}");
        }
    }

    #[test]
    fn monthly_buckets_do_not_drift() {
        let r = Range { start: Utc.with_ymd_and_hms(2026, 1, 31, 0, 0, 0).unwrap(), end: Utc.with_ymd_and_hms(2026, 5, 1, 0, 0, 0).unwrap() };
        let b = buckets(r, "monthly", Zone::utc(), 100).unwrap();
        assert_eq!(b.len(), 4);
        assert_eq!(b[1].start, Utc.with_ymd_and_hms(2026, 2, 28, 0, 0, 0).unwrap());
        assert_eq!(b[2].start, Utc.with_ymd_and_hms(2026, 3, 31, 0, 0, 0).unwrap());
        assert!(buckets(r, "daily", Zone::utc(), 10).is_err());
        assert!(buckets(r, "fortnightly", Zone::utc(), 10).is_err());
    }
}
