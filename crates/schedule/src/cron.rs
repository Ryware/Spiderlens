//! Five-field cron expressions and the next run time (plan D2).
//!
//! Supported: `*`, numbers, lists, ranges, steps (`*/n`, `a-b/n`, `a/n`),
//! three-letter month and weekday names, `L` (last day) in the day of month,
//! Sunday as 0 or 7, and the standard rule that when both day fields are
//! restricted a day matches either. The minute field must be one number, so
//! a schedule runs at most once an hour (FR-005).

use jiff::civil::{Date, DateTime};
use jiff::tz::{AmbiguousOffset, TimeZone};
use jiff::{Timestamp, ToSpan};

/// How far ahead to look before deciding an expression never runs.
const HORIZON_DAYS: i64 = 366 * 5;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cron {
    minute: u8,
    hours: u32,
    days: u32,
    last_day: bool,
    months: u16,
    weekdays: u8,
    days_restricted: bool,
    weekdays_restricted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CronError {
    #[error("A schedule needs five fields: minute, hour, day of month, month and day of week.")]
    FieldCount,
    #[error("The minute must be one number from 0 to 59, so the crawl runs at most once an hour.")]
    Minute,
    #[error("{field}: \"{text}\" is not valid ({hint}).")]
    Field {
        field: &'static str,
        text: String,
        hint: &'static str,
    },
    #[error("This schedule never runs (no matching date in the next five years).")]
    Never,
}

struct Spec {
    name: &'static str,
    min: u8,
    max: u8,
    names: &'static [&'static str],
    hint: &'static str,
}

const HOUR: Spec = Spec {
    name: "Hour",
    min: 0,
    max: 23,
    names: &[],
    hint: "0 to 23",
};
const DAY: Spec = Spec {
    name: "Day of month",
    min: 1,
    max: 31,
    names: &[],
    hint: "1 to 31 or L",
};
const MONTH: Spec = Spec {
    name: "Month",
    min: 1,
    max: 12,
    names: &[
        "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
    ],
    hint: "1 to 12 or jan to dec",
};
const WEEKDAY: Spec = Spec {
    name: "Day of week",
    min: 0,
    max: 7,
    names: &["sun", "mon", "tue", "wed", "thu", "fri", "sat"],
    hint: "0 to 7 or sun to sat",
};

impl Cron {
    pub fn parse(expr: &str) -> Result<Self, CronError> {
        let fields: Vec<&str> = expr.split_whitespace().collect();
        let [minute, hour, day, month, weekday] = fields[..] else {
            return Err(CronError::FieldCount);
        };
        let minute: u8 = minute.parse().ok().filter(|m| *m < 60).ok_or(CronError::Minute)?;
        let last_day = day.split(',').any(|i| i.eq_ignore_ascii_case("l"));
        let rest: Vec<&str> = day.split(',').filter(|i| !i.eq_ignore_ascii_case("l")).collect();
        let days = if rest.is_empty() {
            0
        } else {
            field(&rest.join(","), &DAY).map_err(|_| bad(&DAY, day))?
        };
        let weekdays = field(weekday, &WEEKDAY)?;
        Ok(Self {
            minute,
            hours: field(hour, &HOUR)? as u32,
            days: days as u32,
            last_day,
            months: field(month, &MONTH)? as u16,
            // Sunday is 0 and 7.
            weekdays: ((weekdays | (weekdays >> 7)) & 0x7f) as u8,
            days_restricted: day != "*",
            weekdays_restricted: weekday != "*",
        })
    }

    pub fn minute(&self) -> u8 {
        self.minute
    }

    fn matches(&self, d: Date) -> bool {
        if self.months & (1 << d.month()) == 0 {
            return false;
        }
        let by_day = self.days & (1 << d.day()) != 0 || (self.last_day && d.day() == d.days_in_month());
        let by_weekday = self.weekdays & (1 << d.weekday().to_sunday_zero_offset()) != 0;
        match (self.days_restricted, self.weekdays_restricted) {
            (true, true) => by_day || by_weekday,
            (true, false) => by_day,
            (false, true) => by_weekday,
            (false, false) => true,
        }
    }

    /// The first run strictly after `after`, in `tz`.
    ///
    /// A time that does not exist that day (spring forward) runs at the first
    /// valid minute after the gap; a time that happens twice (fall back) runs
    /// once, at the earlier instant.
    pub fn next_after(&self, tz: &TimeZone, after: Timestamp) -> Option<Timestamp> {
        let start = after.to_zoned(tz.clone()).date();
        for offset in 0..HORIZON_DAYS {
            let Ok(date) = start.checked_add(offset.days()) else {
                return None;
            };
            if !self.matches(date) {
                continue;
            }
            for hour in 0..24i8 {
                if self.hours & (1 << hour) == 0 {
                    continue;
                }
                let at = resolve(tz, date.at(hour, self.minute as i8, 0, 0));
                if at > after {
                    return Some(at);
                }
            }
        }
        None
    }

    /// The next `n` runs after `after`.
    pub fn upcoming(&self, tz: &TimeZone, after: Timestamp, n: usize) -> Vec<Timestamp> {
        let mut out = Vec::with_capacity(n);
        let mut t = after;
        while out.len() < n {
            match self.next_after(tz, t) {
                Some(next) => {
                    out.push(next);
                    t = next;
                }
                None => break,
            }
        }
        out
    }

    /// Parses and checks that the expression runs at all.
    pub fn validate(expr: &str) -> Result<Self, CronError> {
        let cron = Self::parse(expr)?;
        // Any fixed reference works: an impossible date never matches.
        if cron.next_after(&TimeZone::UTC, Timestamp::UNIX_EPOCH).is_none() {
            return Err(CronError::Never);
        }
        Ok(cron)
    }
}

fn resolve(tz: &TimeZone, dt: DateTime) -> Timestamp {
    let amb = tz.to_ambiguous_timestamp(dt);
    match amb.offset() {
        AmbiguousOffset::Gap { .. } => {
            // The transition itself is the first valid instant after the gap.
            let later = amb.later().expect("a gap resolves");
            tz.preceding(later).next().map(|t| t.timestamp()).unwrap_or(later)
        }
        _ => amb.compatible().expect("compatible always resolves"),
    }
}

fn bad(spec: &Spec, text: &str) -> CronError {
    CronError::Field {
        field: spec.name,
        text: text.to_string(),
        hint: spec.hint,
    }
}

fn value(text: &str, spec: &Spec) -> Option<u8> {
    let v = match text.parse::<u8>() {
        Ok(v) => v,
        Err(_) => {
            let lower = text.to_ascii_lowercase();
            let i = spec.names.iter().position(|n| *n == lower)?;
            // Month names start at 1, weekday names at 0.
            i as u8 + u8::from(spec.min == 1)
        }
    };
    (spec.min..=spec.max).contains(&v).then_some(v)
}

fn field(text: &str, spec: &Spec) -> Result<u64, CronError> {
    let mut bits = 0u64;
    for item in text.split(',') {
        if item.eq_ignore_ascii_case("l") {
            // L is only valid in the day of month, handled in Cron::parse.
            return Err(bad(spec, text));
        }
        let (range, step) = match item.split_once('/') {
            Some((r, s)) => (
                r,
                s.parse::<u8>()
                    .ok()
                    .filter(|s| *s > 0)
                    .ok_or_else(|| bad(spec, text))?,
            ),
            None => (item, 1),
        };
        let (lo, hi) = if range == "*" {
            (spec.min, spec.max)
        } else if let Some((a, b)) = range.split_once('-') {
            let (a, b) = (value(a, spec), value(b, spec));
            match (a, b) {
                (Some(a), Some(b)) if a <= b => (a, b),
                _ => return Err(bad(spec, text)),
            }
        } else {
            let v = value(range, spec).ok_or_else(|| bad(spec, text))?;
            if item.contains('/') { (v, spec.max) } else { (v, v) }
        };
        let mut v = lo;
        while v <= hi {
            bits |= 1 << v;
            v = match v.checked_add(step) {
                Some(n) => n,
                None => break,
            };
        }
    }
    Ok(bits)
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::date;

    fn at(tz: &str, dt: DateTime) -> Timestamp {
        dt.to_zoned(TimeZone::get(tz).unwrap()).unwrap().timestamp()
    }

    fn local(tz: &TimeZone, t: Timestamp) -> String {
        t.to_zoned(tz.clone()).strftime("%Y-%m-%d %H:%M %a").to_string()
    }

    #[test]
    fn daily_at_seven() {
        let c = Cron::parse("0 7 * * *").unwrap();
        let tz = TimeZone::UTC;
        let next = c
            .next_after(&tz, at("UTC", date(2026, 10, 14).at(8, 0, 0, 0)))
            .unwrap();
        assert_eq!(local(&tz, next), "2026-10-15 07:00 Thu");
        // Strictly after: exactly 07:00 gives the next day.
        let same = c.next_after(&tz, next).unwrap();
        assert_eq!(local(&tz, same), "2026-10-16 07:00 Fri");
    }

    #[test]
    fn weekly_names_and_sunday_seven() {
        let tz = TimeZone::UTC;
        let a = Cron::parse("30 6 * * mon,THU").unwrap();
        let from = at("UTC", date(2026, 10, 14).at(0, 0, 0, 0)); // Wednesday
        let runs: Vec<_> = a
            .upcoming(&tz, from, 3)
            .into_iter()
            .map(|t| local(&tz, t))
            .collect();
        assert_eq!(
            runs,
            [
                "2026-10-15 06:30 Thu",
                "2026-10-19 06:30 Mon",
                "2026-10-22 06:30 Thu"
            ]
        );
        assert_eq!(
            Cron::parse("0 0 * * 7").unwrap().weekdays,
            Cron::parse("0 0 * * 0").unwrap().weekdays
        );
    }

    #[test]
    fn every_six_hours_and_ranges() {
        let tz = TimeZone::UTC;
        let c = Cron::parse("15 */6 * * 1-5").unwrap();
        let from = at("UTC", date(2026, 10, 16).at(13, 0, 0, 0)); // Friday
        let runs: Vec<_> = c
            .upcoming(&tz, from, 3)
            .into_iter()
            .map(|t| local(&tz, t))
            .collect();
        assert_eq!(
            runs,
            [
                "2026-10-16 18:15 Fri",
                "2026-10-19 00:15 Mon",
                "2026-10-19 06:15 Mon"
            ]
        );
    }

    #[test]
    fn last_day_of_month() {
        let tz = TimeZone::UTC;
        let c = Cron::parse("0 9 L * *").unwrap();
        let from = at("UTC", date(2027, 1, 31).at(10, 0, 0, 0));
        let runs: Vec<_> = c
            .upcoming(&tz, from, 2)
            .into_iter()
            .map(|t| local(&tz, t))
            .collect();
        assert_eq!(runs, ["2027-02-28 09:00 Sun", "2027-03-31 09:00 Wed"]);
        let mixed = Cron::parse("0 9 1,L * *").unwrap();
        let runs: Vec<_> = mixed
            .upcoming(&tz, from, 2)
            .into_iter()
            .map(|t| local(&tz, t))
            .collect();
        assert_eq!(runs, ["2027-02-01 09:00 Mon", "2027-02-28 09:00 Sun"]);
    }

    #[test]
    fn both_day_fields_mean_either() {
        let tz = TimeZone::UTC;
        // The 13th, or any Friday.
        let c = Cron::parse("0 0 13 * fri").unwrap();
        let from = at("UTC", date(2026, 10, 10).at(0, 0, 0, 0));
        let runs: Vec<_> = c
            .upcoming(&tz, from, 3)
            .into_iter()
            .map(|t| local(&tz, t))
            .collect();
        assert_eq!(
            runs,
            [
                "2026-10-13 00:00 Tue",
                "2026-10-16 00:00 Fri",
                "2026-10-23 00:00 Fri"
            ]
        );
    }

    #[test]
    fn spring_forward_runs_at_the_first_valid_minute() {
        // 2026-03-08 02:30 does not exist in New York; the clocks jump to 03:00.
        let tz = TimeZone::get("America/New_York").unwrap();
        let c = Cron::parse("30 2 * * *").unwrap();
        let from = at("America/New_York", date(2026, 3, 7).at(12, 0, 0, 0));
        let runs: Vec<_> = c
            .upcoming(&tz, from, 2)
            .into_iter()
            .map(|t| local(&tz, t))
            .collect();
        assert_eq!(runs, ["2026-03-08 03:00 Sun", "2026-03-09 02:30 Mon"]);
    }

    #[test]
    fn fall_back_runs_once() {
        // 2026-10-25 01:30 happens twice in London.
        let tz = TimeZone::get("Europe/London").unwrap();
        let c = Cron::parse("30 * * * *").unwrap();
        let from = at("Europe/London", date(2026, 10, 25).at(0, 0, 0, 0));
        let runs: Vec<_> = c
            .upcoming(&tz, from, 3)
            .into_iter()
            .map(|t| local(&tz, t))
            .collect();
        assert_eq!(
            runs,
            [
                "2026-10-25 00:30 Sun",
                "2026-10-25 01:30 Sun",
                "2026-10-25 02:30 Sun"
            ]
        );
        let gap = c.upcoming(&tz, from, 3);
        assert_eq!(
            gap[2].duration_since(gap[1]).as_hours(),
            2,
            "the repeated 01:30 is skipped"
        );
    }

    #[test]
    fn errors_name_the_field() {
        assert_eq!(Cron::parse("0 7 * *"), Err(CronError::FieldCount));
        assert_eq!(Cron::parse("*/5 * * * *"), Err(CronError::Minute));
        assert_eq!(Cron::parse("0,30 * * * *"), Err(CronError::Minute));
        let e = Cron::parse("0 25 * * *").unwrap_err().to_string();
        assert!(e.starts_with("Hour: \"25\""), "{e}");
        assert!(
            Cron::parse("0 0 * 13 *")
                .unwrap_err()
                .to_string()
                .starts_with("Month")
        );
        assert!(
            Cron::parse("0 0 * * L")
                .unwrap_err()
                .to_string()
                .starts_with("Day of week")
        );
        assert!(
            Cron::parse("0 0 5-1 * *")
                .unwrap_err()
                .to_string()
                .starts_with("Day of month")
        );
        assert!(Cron::parse("0 0 */0 * *").is_err());
        assert_eq!(Cron::validate("0 0 31 2 *"), Err(CronError::Never));
        assert!(
            Cron::validate("0 0 29 2 *").is_ok(),
            "leap years exist within five years"
        );
    }

    proptest::proptest! {
        #[test]
        fn runs_increase_and_match(minute in 0u8..60, hour in 0u8..24, wd in 0u8..7, start in 0i64..2_000_000_000) {
            let expr = format!("{minute} {hour},{} * * {wd}", (hour + 7) % 24);
            let c = Cron::parse(&expr).unwrap();
            let tz = TimeZone::get("Asia/Jerusalem").unwrap();
            let runs = c.upcoming(&tz, Timestamp::from_second(start).unwrap(), 4);
            proptest::prop_assert_eq!(runs.len(), 4);
            for w in runs.windows(2) {
                proptest::prop_assert!(w[0] < w[1]);
            }
            for r in runs {
                let z = r.to_zoned(tz.clone());
                proptest::prop_assert_eq!(z.weekday().to_sunday_zero_offset() as u8, wd);
            }
        }
    }
}
