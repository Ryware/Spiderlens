//! Editor presets for how often a schedule runs (FR-004). Every preset is
//! one cron expression underneath (plan D2).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::cron::{Cron, CronError};

/// A time of day in the computer's local time zone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TimeOfDay {
    pub hour: u8,
    pub minute: u8,
}

impl TimeOfDay {
    /// `07:30` or `7:30`.
    pub fn parse(text: &str) -> Option<Self> {
        let (h, m) = text.split_once(':')?;
        let (hour, minute) = (h.trim().parse().ok()?, m.trim().parse().ok()?);
        (hour < 24 && minute < 60 && m.trim().len() == 2).then_some(Self { hour, minute })
    }
}

impl std::fmt::Display for TimeOfDay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:02}:{:02}", self.hour, self.minute)
    }
}

/// Day of the month for the monthly preset: 1 to 28, or the last day.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum MonthDay {
    Day(u8),
    Last,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum Frequency {
    /// Every N hours (1 to 24) at a minute past the hour, counted from midnight.
    Hours {
        every: u8,
        minute: u8,
    },
    Daily {
        time: TimeOfDay,
    },
    /// Days are 0 (Sunday) to 6 (Saturday).
    Weekly {
        days: Vec<u8>,
        time: TimeOfDay,
    },
    Monthly {
        day: MonthDay,
        time: TimeOfDay,
    },
    Advanced {
        cron: String,
    },
}

const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

impl Frequency {
    pub fn cron_expr(&self) -> String {
        match self {
            Self::Hours { every, minute } => match every {
                24 => format!("{minute} 0 * * *"),
                1 => format!("{minute} * * * *"),
                n => format!("{minute} */{n} * * *"),
            },
            Self::Daily { time } => format!("{} {} * * *", time.minute, time.hour),
            Self::Weekly { days, time } => {
                let mut d = days.clone();
                d.sort_unstable();
                d.dedup();
                let list: Vec<String> = d.iter().map(u8::to_string).collect();
                format!("{} {} * * {}", time.minute, time.hour, list.join(","))
            }
            Self::Monthly { day, time } => {
                let d = match day {
                    MonthDay::Day(n) => n.to_string(),
                    MonthDay::Last => "L".into(),
                };
                format!("{} {} {d} * *", time.minute, time.hour)
            }
            Self::Advanced { cron } => cron.trim().to_string(),
        }
    }

    /// Checks the preset's own limits, then that the expression runs.
    pub fn cron(&self) -> Result<Cron, CronError> {
        let field = |field, text: String, hint| CronError::Field { field, text, hint };
        match self {
            Self::Hours { every, .. } if !(1..=24).contains(every) => {
                return Err(field("Every", every.to_string(), "1 to 24 hours"));
            }
            Self::Weekly { days, .. } if days.is_empty() || days.iter().any(|d| *d > 6) => {
                return Err(field("Days", format!("{days:?}"), "choose at least one day"));
            }
            Self::Monthly {
                day: MonthDay::Day(d),
                ..
            } if !(1..=28).contains(d) => {
                return Err(field("Day of month", d.to_string(), "1 to 28, or the last day"));
            }
            _ => {}
        }
        Cron::validate(&self.cron_expr())
    }

    /// "Every Monday and Thursday at 07:00".
    pub fn describe(&self) -> String {
        match self {
            Self::Hours { every: 1, minute } => format!("Every hour at :{minute:02}"),
            Self::Hours { every, minute } => format!("Every {every} hours at :{minute:02}"),
            Self::Daily { time } => format!("Every day at {time}"),
            Self::Weekly { days, time } => {
                let mut d = days.clone();
                d.sort_unstable();
                d.dedup();
                let names: Vec<&str> = d
                    .iter()
                    .filter_map(|i| WEEKDAYS.get(*i as usize).copied())
                    .collect();
                format!("Every {} at {time}", join_and(&names))
            }
            Self::Monthly {
                day: MonthDay::Last,
                time,
            } => format!("Last day of every month at {time}"),
            Self::Monthly {
                day: MonthDay::Day(d),
                time,
            } => format!("Day {d} of every month at {time}"),
            Self::Advanced { cron } => format!("Custom: {}", cron.trim()),
        }
    }
}

fn join_and(items: &[&str]) -> String {
    match items {
        [] => String::new(),
        [one] => (*one).to_string(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEVEN: TimeOfDay = TimeOfDay { hour: 7, minute: 0 };

    #[test]
    fn presets_make_cron() {
        let cases = [
            (
                Frequency::Hours { every: 1, minute: 5 },
                "5 * * * *",
                "Every hour at :05",
            ),
            (
                Frequency::Hours { every: 6, minute: 0 },
                "0 */6 * * *",
                "Every 6 hours at :00",
            ),
            (
                Frequency::Hours {
                    every: 24,
                    minute: 30,
                },
                "30 0 * * *",
                "Every 24 hours at :30",
            ),
            (
                Frequency::Daily { time: SEVEN },
                "0 7 * * *",
                "Every day at 07:00",
            ),
            (
                Frequency::Weekly {
                    days: vec![4, 1, 1],
                    time: SEVEN,
                },
                "0 7 * * 1,4",
                "Every Mon and Thu at 07:00",
            ),
            (
                Frequency::Monthly {
                    day: MonthDay::Last,
                    time: SEVEN,
                },
                "0 7 L * *",
                "Last day of every month at 07:00",
            ),
            (
                Frequency::Monthly {
                    day: MonthDay::Day(15),
                    time: SEVEN,
                },
                "0 7 15 * *",
                "Day 15 of every month at 07:00",
            ),
        ];
        for (f, cron, text) in cases {
            assert_eq!(f.cron_expr(), cron);
            assert_eq!(f.describe(), text);
            f.cron().unwrap();
        }
    }

    #[test]
    fn preset_limits() {
        assert!(Frequency::Hours { every: 0, minute: 0 }.cron().is_err());
        assert!(Frequency::Hours { every: 25, minute: 0 }.cron().is_err());
        assert!(
            Frequency::Weekly {
                days: vec![],
                time: SEVEN
            }
            .cron()
            .is_err()
        );
        assert!(
            Frequency::Monthly {
                day: MonthDay::Day(31),
                time: SEVEN
            }
            .cron()
            .is_err()
        );
        assert!(
            Frequency::Advanced {
                cron: "*/10 * * * *".into()
            }
            .cron()
            .is_err()
        );
    }

    #[test]
    fn time_of_day() {
        assert_eq!(TimeOfDay::parse("7:05"), Some(TimeOfDay { hour: 7, minute: 5 }));
        assert_eq!(TimeOfDay::parse("24:00"), None);
        assert_eq!(TimeOfDay::parse("07:5"), None);
        assert_eq!(TimeOfDay { hour: 7, minute: 5 }.to_string(), "07:05");
    }

    #[test]
    fn serde_shape() {
        let f = Frequency::Weekly {
            days: vec![1],
            time: SEVEN,
        };
        let v = serde_json::to_value(&f).unwrap();
        assert_eq!(
            v,
            serde_json::json!({"kind": "weekly", "days": [1], "time": {"hour": 7, "minute": 0}})
        );
    }
}
