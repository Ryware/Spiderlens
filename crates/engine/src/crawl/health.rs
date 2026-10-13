//! Per-host circuit breaker (spec 004 US4, plan D6).
//!
//! ```text
//! Active --(N transient failures in a row)--> Paused(first pause)
//! Paused --(time up, or "Resume now")--> Probing (one request allowed)
//! Probing --any answer--> Active (counters reset)
//! Probing --transient failure--> Paused(2x, up to the maximum)
//!                                or GivenUp after M failed probes
//! ```
//!
//! Pure state: the caller passes the time in and acts on the returned
//! [`Transition`]. Only transient failures count; a 404 or a TLS error is the
//! page's problem, not the host's.

use std::time::{Duration, Instant};

use spiderlens_config::RetrySettings;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthState {
    Active,
    Paused { until: Instant },
    /// The pause is over; one test request may go out.
    Probing { sent: bool },
    GivenUp,
}

/// Something the user (and the crawl log) should hear about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transition {
    Paused { pause: Duration, failures: u32 },
    Resumed,
    GivenUp,
}

#[derive(Debug, Clone)]
pub struct HostHealth {
    state: HealthState,
    /// Transient failures in a row while active.
    streak: u32,
    /// Pauses in a row without recovering.
    pauses: u32,
}

impl Default for HostHealth {
    fn default() -> Self {
        Self {
            state: HealthState::Active,
            streak: 0,
            pauses: 0,
        }
    }
}

impl HostHealth {
    pub fn state(&self) -> HealthState {
        self.state
    }

    /// Ends a finished pause: the host may send its probe.
    pub fn tick(&mut self, now: Instant) {
        if let HealthState::Paused { until } = self.state
            && now >= until
        {
            self.state = HealthState::Probing { sent: false };
        }
    }

    /// Requests the host may have open now: `None` for no limit beyond its
    /// usual one, `Some(0)` while paused or given up, `Some(1)` to probe.
    pub fn limit(&self) -> Option<u32> {
        match self.state {
            HealthState::Active => None,
            HealthState::Paused { .. } | HealthState::GivenUp => Some(0),
            HealthState::Probing { sent: false } => Some(1),
            HealthState::Probing { sent: true } => Some(0),
        }
    }

    /// When the pause ends, while paused.
    pub fn paused_until(&self) -> Option<Instant> {
        match self.state {
            HealthState::Paused { until } => Some(until),
            _ => None,
        }
    }

    pub fn is_given_up(&self) -> bool {
        self.state == HealthState::GivenUp
    }

    /// True for a host that can take no request now (paused, probe out, or
    /// given up).
    pub fn is_blocked(&self) -> bool {
        self.limit() == Some(0)
    }

    pub fn on_start(&mut self) {
        if let HealthState::Probing { sent } = &mut self.state {
            *sent = true;
        }
    }

    /// The host answered (any status) or failed for a non-transient reason.
    pub fn on_alive(&mut self) -> Option<Transition> {
        self.streak = 0;
        match self.state {
            HealthState::Probing { .. } => {
                self.pauses = 0;
                self.state = HealthState::Active;
                Some(Transition::Resumed)
            }
            _ => None,
        }
    }

    /// A transient failure (connection, timeout, DNS temporary, 5xx, 429,
    /// truncated body).
    pub fn on_transient_failure(&mut self, settings: &RetrySettings, now: Instant) -> Option<Transition> {
        match self.state {
            HealthState::Active => {
                self.streak += 1;
                if self.streak >= settings.host_pause_after {
                    Some(self.pause(settings, now))
                } else {
                    None
                }
            }
            HealthState::Probing { .. } => {
                if self.pauses >= settings.host_give_up_after {
                    self.state = HealthState::GivenUp;
                    Some(Transition::GivenUp)
                } else {
                    Some(self.pause(settings, now))
                }
            }
            // Requests that were already in flight when the host paused.
            HealthState::Paused { .. } | HealthState::GivenUp => None,
        }
    }

    fn pause(&mut self, settings: &RetrySettings, now: Instant) -> Transition {
        let secs = (settings.host_pause_secs * 2f64.powi(self.pauses.min(30) as i32))
            .min(settings.host_pause_max_secs.max(settings.host_pause_secs));
        let pause = Duration::from_secs_f64(secs);
        let failures = self.streak;
        self.pauses += 1;
        self.streak = 0;
        self.state = HealthState::Paused { until: now + pause };
        Transition::Paused { pause, failures }
    }

    /// "Resume now" (US4.5): end the pause and send a probe at once.
    pub fn resume_now(&mut self) -> bool {
        match self.state {
            HealthState::Paused { .. } | HealthState::GivenUp => {
                self.state = HealthState::Probing { sent: false };
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fail(h: &mut HostHealth, s: &RetrySettings, now: Instant, n: u32) -> Vec<Transition> {
        (0..n).filter_map(|_| h.on_transient_failure(s, now)).collect()
    }

    #[test]
    fn us4_1_five_failures_in_a_row_pause_for_a_minute() {
        let s = RetrySettings::default();
        let now = Instant::now();
        let mut h = HostHealth::default();
        assert!(fail(&mut h, &s, now, 4).is_empty());
        assert_eq!(h.limit(), None);
        assert_eq!(
            h.on_transient_failure(&s, now),
            Some(Transition::Paused {
                pause: Duration::from_secs(60),
                failures: 5
            })
        );
        assert_eq!(h.limit(), Some(0));
        assert_eq!(h.paused_until(), Some(now + Duration::from_secs(60)));
    }

    #[test]
    fn a_success_resets_the_streak() {
        let s = RetrySettings::default();
        let now = Instant::now();
        let mut h = HostHealth::default();
        fail(&mut h, &s, now, 4);
        assert_eq!(h.on_alive(), None);
        assert!(fail(&mut h, &s, now, 4).is_empty(), "the count starts over");
    }

    #[test]
    fn us4_2_one_probe_then_resume_or_double() {
        let s = RetrySettings::default();
        let t0 = Instant::now();
        let mut h = HostHealth::default();
        fail(&mut h, &s, t0, 5);
        h.tick(t0 + Duration::from_secs(59));
        assert_eq!(h.limit(), Some(0), "still paused");
        let t1 = t0 + Duration::from_secs(60);
        h.tick(t1);
        assert_eq!(h.limit(), Some(1), "exactly one test request");
        h.on_start();
        assert_eq!(h.limit(), Some(0), "no second request while the probe is out");
        assert_eq!(
            h.on_transient_failure(&s, t1),
            Some(Transition::Paused {
                pause: Duration::from_secs(120),
                failures: 0
            })
        );
        let t2 = t1 + Duration::from_secs(120);
        h.tick(t2);
        h.on_start();
        assert_eq!(h.on_alive(), Some(Transition::Resumed));
        assert_eq!(h.limit(), None);
        // Recovered: the next outage starts from the first pause again.
        assert_eq!(
            fail(&mut h, &s, t2, 5),
            [Transition::Paused {
                pause: Duration::from_secs(60),
                failures: 5
            }]
        );
    }

    #[test]
    fn pauses_double_up_to_ten_minutes() {
        let s = RetrySettings {
            host_give_up_after: 50,
            ..RetrySettings::default()
        };
        let mut now = Instant::now();
        let mut h = HostHealth::default();
        let mut pauses = Vec::new();
        fail(&mut h, &s, now, 4);
        for _ in 0..6 {
            if let Some(Transition::Paused { pause, .. }) = h.on_transient_failure(&s, now) {
                pauses.push(pause.as_secs());
                now += pause;
                h.tick(now);
                h.on_start();
            }
        }
        assert_eq!(pauses, [60, 120, 240, 480, 600, 600]);
    }

    #[test]
    fn us4_3_given_up_after_five_failed_probes() {
        let s = RetrySettings::default();
        let mut now = Instant::now();
        let mut h = HostHealth::default();
        let mut events = fail(&mut h, &s, now, 5);
        while !h.is_given_up() {
            now += Duration::from_secs(3600);
            h.tick(now);
            h.on_start();
            events.extend(h.on_transient_failure(&s, now));
        }
        let paused = events
            .iter()
            .filter(|e| matches!(e, Transition::Paused { .. }))
            .count();
        assert_eq!(paused, 5);
        assert_eq!(events.last(), Some(&Transition::GivenUp));
        assert_eq!(h.limit(), Some(0));
    }

    #[test]
    fn us4_5_resume_now_sends_a_probe() {
        let s = RetrySettings::default();
        let now = Instant::now();
        let mut h = HostHealth::default();
        fail(&mut h, &s, now, 5);
        assert!(h.resume_now());
        assert_eq!(h.limit(), Some(1));
        assert!(!h.resume_now(), "already probing");
    }

    #[test]
    fn failures_in_flight_while_paused_do_not_count() {
        let s = RetrySettings::default();
        let now = Instant::now();
        let mut h = HostHealth::default();
        fail(&mut h, &s, now, 5);
        assert!(fail(&mut h, &s, now, 10).is_empty());
        assert_eq!(h.paused_until(), Some(now + Duration::from_secs(60)));
    }
}
