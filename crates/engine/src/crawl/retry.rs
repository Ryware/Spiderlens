//! The retry decision (spec 004 US2, plan D1, D3).
//!
//! A pure function: the runner hands it the facts of one attempt and the
//! settings, and carries out what it returns. No clock, no I/O; the random
//! jitter factor is passed in, so every wait is testable.

use std::time::Duration;

use spiderlens_config::RetrySettings;

use crate::{ErrorClass, ErrorReason, FinalClass};

/// Random variation of each wait: plus or minus this fraction (FR-017).
pub const JITTER: f64 = 0.2;

/// What one attempt produced, as far as retrying is concerned.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Attempt {
    /// Status code, when there was an answer.
    pub status: Option<u16>,
    /// Failure with no usable answer (or a body problem on one).
    pub error: Option<(ErrorClass, Option<ErrorReason>)>,
    /// The server's `Retry-After`, already parsed (429 and 503 only).
    pub retry_after: Option<Duration>,
    /// Attempts made in this round, this one included.
    pub round_attempts: u32,
}

impl Attempt {
    pub fn class(&self) -> FinalClass {
        FinalClass::of(self.status, self.error.map(|(c, _)| c))
    }

    /// Whether trying again can help, whatever the settings say.
    pub fn is_transient(&self) -> bool {
        self.class()
            .is_transient(self.status, self.error.and_then(|(_, r)| r))
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Decision {
    /// This attempt is the URL's result.
    Record,
    /// Try again after `wait`.
    Retry {
        wait: Duration,
        /// The server's Retry-After set the wait (it was longer than the
        /// backoff).
        retry_after_used: bool,
        /// The server asked for longer than the maximum wait (FR-018).
        capped_from: Option<Duration>,
    },
}

/// Exponential backoff before attempt `round_attempts + 1`: base, 2x base,
/// 4x base... times `1 + jitter` (jitter in [-0.2, 0.2]), capped (FR-017).
pub fn backoff(settings: &RetrySettings, round_attempts: u32, jitter: f64) -> Duration {
    let exp = round_attempts.saturating_sub(1).min(30) as i32;
    let secs = settings.base_wait_secs * 2f64.powi(exp) * (1.0 + jitter.clamp(-JITTER, JITTER));
    Duration::from_secs_f64(secs.min(settings.max_wait_secs).max(0.0))
}

/// Decides whether a finished attempt is retried and after how long.
pub fn decide(settings: &RetrySettings, a: &Attempt, jitter: f64) -> Decision {
    if !a.is_transient() || a.round_attempts >= settings.attempts {
        return Decision::Record;
    }
    match a.class() {
        FinalClass::Http429 if !settings.retry_429 => return Decision::Record,
        FinalClass::Http5xx if !settings.retry_5xx => return Decision::Record,
        _ => {}
    }
    let max = Duration::from_secs_f64(settings.max_wait_secs);
    let mut wait = backoff(settings, a.round_attempts, jitter);
    let mut retry_after_used = false;
    let mut capped_from = None;
    if let Some(ra) = a.retry_after {
        if ra > max {
            capped_from = Some(ra);
        }
        let ra = ra.min(max);
        if ra > wait {
            wait = ra;
            retry_after_used = true;
        }
    }
    Decision::Retry {
        wait,
        retry_after_used,
        capped_from,
    }
}

/// A jitter factor in [-JITTER, JITTER].
pub fn jitter(rng: &mut fastrand::Rng) -> f64 {
    (rng.f64() * 2.0 - 1.0) * JITTER
}

#[cfg(test)]
mod tests {
    use super::*;

    fn http(status: u16, round: u32) -> Attempt {
        Attempt {
            status: Some(status),
            error: None,
            retry_after: None,
            round_attempts: round,
        }
    }

    fn failed(class: ErrorClass, reason: Option<ErrorReason>, round: u32) -> Attempt {
        Attempt {
            status: None,
            error: Some((class, reason)),
            retry_after: None,
            round_attempts: round,
        }
    }

    fn wait(d: Decision) -> Duration {
        match d {
            Decision::Retry { wait, .. } => wait,
            Decision::Record => panic!("expected a retry"),
        }
    }

    const S: f64 = 1.0;

    #[test]
    fn us2_1_waits_about_one_then_two_seconds() {
        let s = RetrySettings::default();
        assert_eq!(wait(decide(&s, &http(503, 1), 0.0)), Duration::from_secs_f64(S));
        assert_eq!(wait(decide(&s, &http(503, 2), 0.0)), Duration::from_secs_f64(2.0 * S));
        // Jitter stays within 20% either way.
        let lo = wait(decide(&s, &http(503, 2), -0.2));
        let hi = wait(decide(&s, &http(503, 2), 0.2));
        assert_eq!(lo, Duration::from_secs_f64(1.6));
        assert_eq!(hi, Duration::from_secs_f64(2.4));
        assert_eq!(wait(decide(&s, &http(503, 2), 5.0)), hi, "jitter is clamped");
    }

    #[test]
    fn us2_2_three_attempts_in_total() {
        let s = RetrySettings::default();
        assert!(matches!(decide(&s, &http(500, 2), 0.0), Decision::Retry { .. }));
        assert_eq!(decide(&s, &http(500, 3), 0.0), Decision::Record);
    }

    #[test]
    fn us2_3_retry_after_sets_the_wait() {
        let s = RetrySettings::default();
        let mut a = http(429, 1);
        a.retry_after = Some(Duration::from_secs(30));
        assert_eq!(
            decide(&s, &a, 0.0),
            Decision::Retry {
                wait: Duration::from_secs(30),
                retry_after_used: true,
                capped_from: None
            }
        );
        // A Retry-After shorter than the backoff does not shorten it.
        a.retry_after = Some(Duration::ZERO);
        assert_eq!(wait(decide(&s, &a, 0.0)), Duration::from_secs(1));
    }

    #[test]
    fn us2_4_retry_after_is_capped_at_the_maximum_wait() {
        let s = RetrySettings::default();
        let mut a = http(503, 1);
        a.retry_after = Some(Duration::from_secs(600));
        assert_eq!(
            decide(&s, &a, 0.0),
            Decision::Retry {
                wait: Duration::from_secs(60),
                retry_after_used: true,
                capped_from: Some(Duration::from_secs(600))
            }
        );
    }

    #[test]
    fn us2_5_network_failures_retry_like_5xx() {
        let s = RetrySettings::default();
        for a in [
            failed(ErrorClass::Connection, Some(ErrorReason::Reset), 1),
            failed(ErrorClass::DnsTemporary, None, 1),
            failed(ErrorClass::Timeout, Some(ErrorReason::Read), 1),
            failed(ErrorClass::ContentProblem, Some(ErrorReason::Truncated), 1),
        ] {
            assert_eq!(wait(decide(&s, &a, 0.0)), Duration::from_secs(1), "{a:?}");
        }
    }

    #[test]
    fn us2_7_permanent_classes_are_never_retried() {
        let s = RetrySettings::default();
        for a in [
            failed(ErrorClass::Dns, None, 1),
            failed(ErrorClass::TlsInvalid, Some(ErrorReason::Expired), 1),
            failed(ErrorClass::RedirectProblem, Some(ErrorReason::InvalidTarget), 1),
            failed(ErrorClass::ContentProblem, Some(ErrorReason::OverSize), 1),
            failed(ErrorClass::ContentProblem, Some(ErrorReason::Decompression), 1),
            failed(ErrorClass::MalformedUrl, None, 1),
            http(404, 1),
            http(403, 1),
            http(501, 1),
            http(505, 1),
            http(999, 1),
            http(200, 1),
        ] {
            assert_eq!(decide(&s, &a, 0.0), Decision::Record, "{a:?}");
        }
    }

    #[test]
    fn one_attempt_means_no_retry_but_still_transient() {
        let s = RetrySettings {
            attempts: 1,
            ..RetrySettings::default()
        };
        let a = http(503, 1);
        assert_eq!(decide(&s, &a, 0.0), Decision::Record);
        assert!(a.is_transient(), "the manual retry still picks it up");
    }

    #[test]
    fn switches_turn_off_429_and_5xx_retries() {
        let s = RetrySettings {
            retry_429: false,
            retry_5xx: false,
            ..RetrySettings::default()
        };
        assert_eq!(decide(&s, &http(429, 1), 0.0), Decision::Record);
        assert_eq!(decide(&s, &http(503, 1), 0.0), Decision::Record);
        assert!(matches!(
            decide(&s, &failed(ErrorClass::Timeout, None, 1), 0.0),
            Decision::Retry { .. }
        ));
    }

    #[test]
    fn backoff_grows_and_is_capped() {
        let s = RetrySettings {
            attempts: 10,
            ..RetrySettings::default()
        };
        let waits: Vec<f64> = (1..=8).map(|n| backoff(&s, n, 0.0).as_secs_f64()).collect();
        assert_eq!(waits, [1.0, 2.0, 4.0, 8.0, 16.0, 32.0, 60.0, 60.0]);
    }

    #[test]
    fn jitter_stays_in_range() {
        let mut rng = fastrand::Rng::with_seed(7);
        for _ in 0..10_000 {
            let j = jitter(&mut rng);
            assert!((-JITTER..=JITTER).contains(&j));
        }
    }
}
