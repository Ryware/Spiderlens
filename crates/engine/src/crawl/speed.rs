//! How many requests one site gets at once (spec 008, plan D4).
//!
//! Additive increase, multiplicative decrease, the rule TCP uses for its
//! congestion window: start small, add one request at a time while the site
//! answers steadily, cut by a fraction as soon as it slows down, times out or
//! says it is overloaded. A cut is followed by a cool-down of one "round" (as
//! many responses as were allowed at the time), so the burst of responses
//! already in flight when a site struggles counts as one signal, not many.
//!
//! The level at which a site last pushed back is remembered as its ceiling.
//! Growing back up to it is quick; trying it again takes many quiet rounds,
//! so a site with a hard limit sees a probe now and then, not a steady
//! stream of rejected requests (SC-004: under 2% after settling).

use std::time::{Duration, Instant};

/// Where every site starts (spec assumption "cautious starting level").
pub const START: u32 = 2;
/// Weight of the newest response time in the running average: about the
/// last 20 responses.
const ALPHA: f64 = 0.1;
/// Responses needed before the average means anything.
const WARM_UP: u32 = 5;
/// Typical response time above this multiple of the baseline is a slowdown.
const SLOW: f64 = 2.0;
/// Growth only while the typical response time stays below this multiple.
const STEADY: f64 = 1.5;
/// Quiet rounds before trying the level that failed last time again.
const PROBE_ROUNDS: u32 = 50;
/// Longest Retry-After honoured; anything longer is treated as this.
pub const MAX_RETRY_AFTER: Duration = Duration::from_secs(24 * 3600);

/// What one response says about the site's health.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Signal {
    /// An answer, whatever its status (except the overload ones).
    Answered,
    /// 429 or 503, with how long the site asked us to wait.
    Overloaded { retry_after: Option<Duration> },
    /// The request timed out.
    TimedOut,
    /// Failed for a reason that says nothing about load (DNS, TLS...).
    Other,
}

#[derive(Debug, Clone)]
pub struct SiteSpeed {
    allowance: u32,
    max: u32,
    adaptive: bool,
    /// Running average of response times, in milliseconds.
    typical_ms: Option<f64>,
    /// What "normal" is for this site: the lowest typical time seen, drifting
    /// up very slowly so a site that is simply slower on deep pages is not
    /// treated as struggling forever.
    baseline_ms: Option<f64>,
    samples: u32,
    steady_streak: u32,
    cool_down: u32,
    /// The allowance at which the site last pushed back.
    ceiling: Option<u32>,
}

impl SiteSpeed {
    pub fn new(max_per_host: u32, adaptive: bool) -> Self {
        let max = max_per_host.max(1);
        Self {
            // Adaptive off: the fixed cap, but overloads still cut (FR-018).
            allowance: if adaptive { START.min(max) } else { max },
            max,
            adaptive,
            typical_ms: None,
            baseline_ms: None,
            samples: 0,
            steady_streak: 0,
            cool_down: 0,
            ceiling: None,
        }
    }

    /// A new cap while the crawl runs (spec 004 plan D13): the allowance is
    /// clamped to it, and a fixed (non-adaptive) cap applies at once.
    pub fn set_max(&mut self, max_per_host: u32, adaptive: bool) {
        self.max = max_per_host.max(1);
        self.adaptive = adaptive;
        if !adaptive {
            self.allowance = self.max;
        }
        self.allowance = self.allowance.min(self.max);
    }

    /// Requests this site may have open now.
    pub fn allowance(&self) -> u32 {
        self.allowance
    }

    /// Learns from one finished request. Returns when the site may be asked
    /// again, if it asked for a pause.
    pub fn on_response(&mut self, took: Duration, signal: Signal, now: Instant) -> Option<Instant> {
        self.cool_down = self.cool_down.saturating_sub(1);
        match signal {
            Signal::Answered => {
                self.observe(took);
                self.adjust_for_latency();
                None
            }
            Signal::Overloaded { retry_after } => {
                self.cut(0.5);
                retry_after.map(|d| now + d.min(MAX_RETRY_AFTER))
            }
            Signal::TimedOut => {
                if self.adaptive {
                    self.cut(0.75);
                }
                None
            }
            Signal::Other => None,
        }
    }

    fn observe(&mut self, took: Duration) {
        let ms = took.as_secs_f64() * 1000.0;
        let typical = match self.typical_ms {
            None => ms,
            Some(t) => t + ALPHA * (ms - t),
        };
        self.typical_ms = Some(typical);
        self.samples += 1;
        if self.samples >= WARM_UP {
            self.baseline_ms = Some(match self.baseline_ms {
                None => typical,
                Some(b) if typical < b => typical,
                Some(b) => b + 0.002 * (typical - b),
            });
        }
    }

    fn adjust_for_latency(&mut self) {
        if !self.adaptive {
            // Recover from an overload cut back to the fixed cap.
            self.grow();
            return;
        }
        let (Some(typical), Some(base)) = (self.typical_ms, self.baseline_ms) else {
            self.grow();
            return;
        };
        // Sub-millisecond noise on a local or very fast site is not a trend.
        let base = base.max(1.0);
        if typical > SLOW * base {
            self.cut(0.75);
        } else if typical <= STEADY * base {
            self.grow();
        } else {
            self.steady_streak = 0;
        }
    }

    /// One more request after a full round of steady responses.
    fn grow(&mut self) {
        if self.allowance >= self.max {
            return;
        }
        self.steady_streak += 1;
        let next = self.allowance + 1;
        let rounds = match self.ceiling {
            Some(c) if next >= c => PROBE_ROUNDS,
            _ => 1,
        };
        if self.steady_streak >= self.allowance * rounds {
            self.allowance = next;
            self.steady_streak = 0;
            if self.ceiling.is_some_and(|c| next > c) {
                // The old limit held for a while: the site has more room now.
                self.ceiling = None;
            }
        }
    }

    fn cut(&mut self, factor: f64) {
        self.steady_streak = 0;
        if self.cool_down > 0 {
            return;
        }
        let before = self.allowance;
        self.allowance = ((f64::from(before) * factor).floor() as u32).max(1);
        self.cool_down = before;
        self.ceiling = Some(before);
    }

    #[cfg(test)]
    pub fn max(&self) -> u32 {
        self.max
    }
}

/// `Retry-After`: delay in seconds, or an HTTP date. A date in the past means
/// no wait; anything unreadable is ignored (spec 008 edge cases).
pub fn parse_retry_after(value: &str, now: jiff::Timestamp) -> Option<Duration> {
    let v = value.trim();
    if let Ok(secs) = v.parse::<u64>() {
        return Some(Duration::from_secs(secs).min(MAX_RETRY_AFTER));
    }
    let at = jiff::fmt::rfc2822::parse(v).ok()?.timestamp();
    let wait = at.duration_since(now);
    Some(if wait.is_negative() {
        Duration::ZERO
    } else {
        Duration::try_from(wait)
            .unwrap_or(MAX_RETRY_AFTER)
            .min(MAX_RETRY_AFTER)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: Duration = Duration::from_millis(1);

    fn steady(s: &mut SiteSpeed, n: u32, took: Duration) {
        let now = Instant::now();
        for _ in 0..n {
            s.on_response(took, Signal::Answered, now);
        }
    }

    #[test]
    fn starts_cautious_and_grows_one_step_per_round() {
        let mut s = SiteSpeed::new(32, true);
        assert_eq!(s.allowance(), 2);
        steady(&mut s, 2, 50 * MS);
        assert_eq!(s.allowance(), 3, "one round of 2 steady answers adds one");
        steady(&mut s, 3, 50 * MS);
        assert_eq!(s.allowance(), 4);
    }

    #[test]
    fn reaches_the_cap_on_a_steady_site_and_stays_there() {
        let mut s = SiteSpeed::new(32, true);
        // Sum of 2..=31: about 500 responses (plan D4).
        steady(&mut s, 600, 50 * MS);
        assert_eq!(s.allowance(), 32);
        steady(&mut s, 100, 50 * MS);
        assert_eq!(s.allowance(), s.max());
    }

    #[test]
    fn overload_halves_once_per_round_never_below_one() {
        let mut s = SiteSpeed::new(32, true);
        steady(&mut s, 600, 50 * MS);
        let now = Instant::now();
        // A burst of 429s from requests that were already in flight.
        for _ in 0..10 {
            s.on_response(50 * MS, Signal::Overloaded { retry_after: None }, now);
        }
        assert_eq!(s.allowance(), 16, "the burst counts as one signal");
        for _ in 0..200 {
            s.on_response(50 * MS, Signal::Overloaded { retry_after: None }, now);
        }
        assert_eq!(s.allowance(), 1);
    }

    #[test]
    fn a_hard_limit_is_probed_rarely() {
        // A site that rejects a 5th simultaneous request (SC-004).
        let mut s = SiteSpeed::new(32, true);
        let now = Instant::now();
        let (mut sent, mut rejected) = (0u32, 0u32);
        for _ in 0..5000 {
            sent += 1;
            if s.allowance() > 4 {
                rejected += 1;
                s.on_response(MS * 20, Signal::Overloaded { retry_after: None }, now);
            } else {
                s.on_response(MS * 20, Signal::Answered, now);
            }
        }
        assert!(
            f64::from(rejected) / f64::from(sent) < 0.02,
            "{rejected} of {sent} rejected"
        );
    }

    #[test]
    fn retry_after_returns_the_time_to_wait_until() {
        let mut s = SiteSpeed::new(4, true);
        let now = Instant::now();
        let until = s.on_response(
            MS,
            Signal::Overloaded {
                retry_after: Some(Duration::from_secs(30)),
            },
            now,
        );
        assert_eq!(until, Some(now + Duration::from_secs(30)));
    }

    #[test]
    fn slowdown_cuts_and_recovery_is_gradual() {
        let mut s = SiteSpeed::new(32, true);
        steady(&mut s, 600, 20 * MS);
        assert_eq!(s.allowance(), 32);
        // Response times triple (SC-006).
        steady(&mut s, 40, 60 * MS);
        assert!(s.allowance() < 32, "slowed down: {}", s.allowance());
        let low = s.allowance();
        steady(&mut s, 200, 20 * MS);
        assert!(s.allowance() > low, "recovers once steady again");
        assert!(s.allowance() < 32, "but one step at a time, not straight back");
    }

    #[test]
    fn noise_on_a_fast_site_does_not_swing_the_allowance() {
        let mut s = SiteSpeed::new(32, true);
        let now = Instant::now();
        for i in 0..1000u64 {
            let took = Duration::from_micros(200 + (i * 7919) % 600);
            s.on_response(took, Signal::Answered, now);
        }
        assert_eq!(s.allowance(), 32);
    }

    #[test]
    fn adaptive_off_uses_the_cap_but_still_backs_off_on_overload() {
        let mut s = SiteSpeed::new(10, false);
        assert_eq!(s.allowance(), 10);
        steady(&mut s, 50, 500 * MS);
        assert_eq!(s.allowance(), 10, "slow answers do not cut");
        s.on_response(MS, Signal::TimedOut, Instant::now());
        assert_eq!(s.allowance(), 10, "timeouts do not cut");
        s.on_response(MS, Signal::Overloaded { retry_after: None }, Instant::now());
        assert_eq!(s.allowance(), 5);
        steady(&mut s, 1000, MS);
        assert_eq!(s.allowance(), 10, "back to the fixed cap, the last step slowly");
    }

    #[test]
    fn retry_after_forms() {
        let now: jiff::Timestamp = "2026-10-12T12:00:00Z".parse().unwrap();
        assert_eq!(parse_retry_after("120", now), Some(Duration::from_secs(120)));
        assert_eq!(
            parse_retry_after("Mon, 12 Oct 2026 12:01:00 GMT", now),
            Some(Duration::from_secs(60))
        );
        assert_eq!(
            parse_retry_after("Mon, 12 Oct 2026 11:00:00 GMT", now),
            Some(Duration::ZERO),
            "a past date means no wait"
        );
        assert_eq!(parse_retry_after("soon", now), None);
        assert_eq!(parse_retry_after("99999999", now), Some(MAX_RETRY_AFTER));
    }
}
