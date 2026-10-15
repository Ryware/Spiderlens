//! Which run, if any, a tick should start (plan D3).

use jiff::tz::TimeZone;
use jiff::{SignedDuration, Timestamp};

use crate::cron::Cron;
use crate::model::Trigger;

/// A slot this close to now counts as on time rather than caught up.
pub const ON_TIME: SignedDuration = SignedDuration::from_mins(5);
/// Older slots are not searched; a schedule idle this long catches up once.
const LOOKBACK: SignedDuration = SignedDuration::from_hours(24 * 400);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Due {
    Nothing,
    Run { slot: Timestamp, trigger: Trigger },
    Missed { slot: Timestamp },
}

/// `last_slot` is the scheduled time of the newest recorded run, or when the
/// schedule was created. Only the newest slot in `(last_slot, now]` counts,
/// so any number of missed times gives one run or one Missed record (FR-012).
pub fn due(cron: &Cron, tz: &TimeZone, last_slot: Timestamp, now: Timestamp, catch_up: bool) -> Due {
    let floor = now.checked_sub(LOOKBACK).unwrap_or(Timestamp::MIN);
    let mut t = last_slot.max(floor);
    let mut newest = None;
    while let Some(next) = cron.next_after(tz, t) {
        if next > now {
            break;
        }
        newest = Some(next);
        t = next;
    }
    match newest {
        None => Due::Nothing,
        Some(slot) if now.duration_since(slot) <= ON_TIME => Due::Run {
            slot,
            trigger: Trigger::Scheduled,
        },
        Some(slot) if catch_up => Due::Run {
            slot,
            trigger: Trigger::CaughtUp,
        },
        Some(slot) => Due::Missed { slot },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::date;

    fn t(d: u8, h: i8, m: i8) -> Timestamp {
        date(2026, 10, d as i8)
            .at(h, m, 0, 0)
            .to_zoned(TimeZone::UTC)
            .unwrap()
            .timestamp()
    }

    #[test]
    fn on_time_late_caught_up_missed() {
        let c = Cron::parse("0 7 * * *").unwrap();
        let tz = TimeZone::UTC;
        let created = t(13, 12, 0);
        assert_eq!(due(&c, &tz, created, t(14, 6, 59), true), Due::Nothing);
        assert_eq!(
            due(&c, &tz, created, t(14, 7, 0), true),
            Due::Run {
                slot: t(14, 7, 0),
                trigger: Trigger::Scheduled
            }
        );
        assert_eq!(
            due(&c, &tz, created, t(14, 7, 5), true),
            Due::Run {
                slot: t(14, 7, 0),
                trigger: Trigger::Scheduled
            }
        );
        assert_eq!(
            due(&c, &tz, created, t(14, 9, 0), true),
            Due::Run {
                slot: t(14, 7, 0),
                trigger: Trigger::CaughtUp
            }
        );
        assert_eq!(
            due(&c, &tz, created, t(14, 9, 0), false),
            Due::Missed { slot: t(14, 7, 0) }
        );
        // Once recorded, the same slot is not due again.
        assert_eq!(due(&c, &tz, t(14, 7, 0), t(14, 9, 0), true), Due::Nothing);
    }

    #[test]
    fn many_missed_slots_give_one_result() {
        let c = Cron::parse("0 * * * *").unwrap();
        let tz = TimeZone::UTC;
        // Off for three days: one catch-up run for the newest slot.
        assert_eq!(
            due(&c, &tz, t(10, 0, 0), t(13, 8, 30), true),
            Due::Run {
                slot: t(13, 8, 0),
                trigger: Trigger::CaughtUp
            }
        );
    }

    #[test]
    fn very_old_last_slot_is_bounded() {
        let c = Cron::parse("0 7 * * *").unwrap();
        let old = Timestamp::from_second(0).unwrap();
        assert_eq!(
            due(&c, &TimeZone::UTC, old, t(14, 8, 0), false),
            Due::Missed { slot: t(14, 7, 0) }
        );
    }
}
