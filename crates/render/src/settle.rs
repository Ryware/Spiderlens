//! The settle rule (plan D1, FR-010) as a pure state machine.
//!
//! After `load`, a page is settled when no request that counts has been open
//! for [`QUIET`] and no DOM mutation was seen for [`QUIET`]. Requests of type
//! `Ping` and `CSPViolationReport`, and third-party beacons, never hold the
//! page open. The minimum wait is applied first; the cap ends rendering
//! whatever happens. Time is a [`Duration`] since navigation started, fed by
//! the caller, so the rule is tested with a fake clock and no browser.

use std::collections::HashSet;
use std::time::Duration;

use crate::types::ResourceKind;

/// How long the page must be quiet.
pub const QUIET: Duration = Duration::from_millis(500);

/// Something the page did, as seen by the render loop.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// The main frame fired `load`.
    Load,
    /// A request started. `beacon` marks a fire-and-forget request (for
    /// example a POST to an analytics host).
    RequestStart {
        id: String,
        kind: ResourceKind,
        third_party: bool,
        beacon: bool,
    },
    /// A request was answered or failed.
    RequestEnd { id: String },
    /// The injected `MutationObserver` saw a change.
    Mutation,
}

/// What the render loop should do now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// Keep waiting for events, and look again at `until` at the latest.
    Waiting {
        until: Duration,
    },
    Settled,
    /// The cap passed before the page settled.
    TimedOut,
}

#[derive(Clone, Debug)]
pub struct Settle {
    min_wait: Duration,
    max_time: Duration,
    loaded: bool,
    open: HashSet<String>,
    last_activity: Duration,
}

impl Settle {
    pub fn new(min_wait: Duration, max_time: Duration) -> Self {
        Self {
            min_wait,
            max_time,
            loaded: false,
            open: HashSet::new(),
            last_activity: Duration::ZERO,
        }
    }

    /// Whether a request of this kind holds the page open.
    pub fn counts(kind: ResourceKind, third_party: bool, beacon: bool) -> bool {
        !matches!(kind, ResourceKind::Ping | ResourceKind::CspViolationReport) && !(third_party && beacon)
    }

    pub fn on(&mut self, now: Duration, event: Event) {
        match event {
            Event::Load => {
                self.loaded = true;
                self.last_activity = now;
            }
            Event::RequestStart {
                id,
                kind,
                third_party,
                beacon,
            } => {
                if Self::counts(kind, third_party, beacon) {
                    self.open.insert(id);
                    self.last_activity = now;
                }
            }
            Event::RequestEnd { id } => {
                // Ends of requests that never counted change nothing.
                if self.open.remove(&id) {
                    self.last_activity = now;
                }
            }
            Event::Mutation => self.last_activity = now,
        }
    }

    /// Requests that count and are still open.
    #[cfg(test)]
    pub fn open_requests(&self) -> usize {
        self.open.len()
    }

    pub fn poll(&self, now: Duration) -> Status {
        let quiet_at = (self.last_activity + QUIET).max(self.min_wait);
        if self.loaded && self.open.is_empty() && now >= quiet_at {
            return Status::Settled;
        }
        if now >= self.max_time {
            return Status::TimedOut;
        }
        let until = if self.loaded && self.open.is_empty() {
            quiet_at.min(self.max_time)
        } else {
            self.max_time
        };
        Status::Waiting { until }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    fn start(id: &str, kind: ResourceKind, third_party: bool, beacon: bool) -> Event {
        Event::RequestStart {
            id: id.into(),
            kind,
            third_party,
            beacon,
        }
    }

    fn end(id: &str) -> Event {
        Event::RequestEnd { id: id.into() }
    }

    fn rule() -> Settle {
        Settle::new(Duration::ZERO, Duration::from_secs(10))
    }

    #[test]
    fn nothing_settles_before_load() {
        let s = rule();
        assert_eq!(s.poll(ms(2_000)), Status::Waiting { until: ms(10_000) });
        assert_eq!(s.poll(ms(10_000)), Status::TimedOut);
    }

    #[test]
    fn settles_500_ms_after_load_when_quiet() {
        let mut s = rule();
        s.on(ms(100), Event::Load);
        assert_eq!(s.poll(ms(599)), Status::Waiting { until: ms(600) });
        assert_eq!(s.poll(ms(600)), Status::Settled);
    }

    #[test]
    fn an_open_request_holds_the_page_and_its_end_restarts_the_quiet_time() {
        let mut s = rule();
        s.on(ms(0), start("a", ResourceKind::Xhr, false, false));
        s.on(ms(50), Event::Load);
        assert_eq!(s.open_requests(), 1);
        assert_eq!(s.poll(ms(3_000)), Status::Waiting { until: ms(10_000) });
        s.on(ms(3_000), end("a"));
        assert_eq!(s.poll(ms(3_499)), Status::Waiting { until: ms(3_500) });
        assert_eq!(s.poll(ms(3_500)), Status::Settled);
    }

    #[test]
    fn a_mutation_restarts_the_quiet_time() {
        let mut s = rule();
        s.on(ms(0), Event::Load);
        s.on(ms(400), Event::Mutation);
        assert_eq!(s.poll(ms(500)), Status::Waiting { until: ms(900) });
        s.on(ms(800), Event::Mutation);
        assert_eq!(s.poll(ms(900)), Status::Waiting { until: ms(1_300) });
        assert_eq!(s.poll(ms(1_300)), Status::Settled);
    }

    #[test]
    fn pings_csp_reports_and_third_party_beacons_are_ignored() {
        let mut s = rule();
        s.on(ms(0), Event::Load);
        s.on(ms(10), start("p", ResourceKind::Ping, false, false));
        s.on(ms(20), start("c", ResourceKind::CspViolationReport, false, false));
        s.on(ms(30), start("b", ResourceKind::Xhr, true, true));
        assert_eq!(s.open_requests(), 0);
        // They do not count as activity either: settled 500 ms after load.
        assert_eq!(s.poll(ms(500)), Status::Settled);
        // Their ends are ignored.
        s.on(ms(600), end("b"));
        assert_eq!(s.poll(ms(600)), Status::Settled);
    }

    #[test]
    fn a_first_party_beacon_and_a_third_party_script_count() {
        assert!(Settle::counts(ResourceKind::Xhr, false, true));
        assert!(Settle::counts(ResourceKind::Script, true, false));
        assert!(!Settle::counts(ResourceKind::Other, true, true));
    }

    #[test]
    fn the_minimum_wait_comes_first() {
        let mut s = Settle::new(Duration::from_secs(2), Duration::from_secs(10));
        s.on(ms(100), Event::Load);
        assert_eq!(s.poll(ms(600)), Status::Waiting { until: ms(2_000) });
        assert_eq!(s.poll(ms(2_000)), Status::Settled);
    }

    #[test]
    fn the_cap_stops_a_page_that_never_settles() {
        let mut s = Settle::new(Duration::ZERO, Duration::from_secs(1));
        s.on(ms(0), Event::Load);
        let mut t = 0;
        while t < 1_000 {
            s.on(ms(t), Event::Mutation);
            assert!(matches!(s.poll(ms(t)), Status::Waiting { .. }));
            t += 100;
        }
        assert_eq!(s.poll(ms(1_000)), Status::TimedOut);
        // The wake-up time never passes the cap.
        s.on(ms(900), Event::Mutation);
        assert_eq!(s.poll(ms(950)), Status::Waiting { until: ms(1_000) });
    }

    #[test]
    fn a_page_quiet_at_the_cap_is_settled_not_timed_out() {
        let mut s = Settle::new(Duration::ZERO, Duration::from_secs(1));
        s.on(ms(0), Event::Load);
        assert_eq!(s.poll(ms(1_000)), Status::Settled);
    }
}
