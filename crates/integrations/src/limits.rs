//! Per-provider limits (plan D5): a token bucket for the request rate, a
//! semaphore for concurrency, and a daily ledger shared by the app and the
//! CLI. Defaults sit at or below each provider's published limit
//! (research.md); the user may lower them, or raise them up to that limit
//! (FR-004).

use std::collections::BTreeMap;
use std::future::Future;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use jiff::{Timestamp, ToSpan, Zoned};
use serde::{Deserialize, Serialize};
use tokio::time::Instant;

use crate::Provider;
use crate::http::HttpError;
use crate::settings::{ProviderSettings, read_json, write_json};

/// A daily allowance and the time zone its day is counted in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Daily {
    pub allowance: u64,
    pub tz: &'static str,
}

/// Google resets daily quotas at midnight Pacific time.
pub const GOOGLE_TZ: &str = "America/Los_Angeles";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub per_minute: u32,
    pub concurrency: u32,
    pub daily: Option<Daily>,
    /// The provider's published per-minute limit: the most a user can set.
    pub published_per_minute: u32,
    pub published_concurrency: u32,
}

impl Limits {
    pub fn defaults(p: Provider) -> Self {
        let (per_minute, concurrency, daily, pub_pm, pub_c) = match p {
            Provider::SearchConsole => (600, 4, None, 1200, 10),
            Provider::UrlInspection => (
                300,
                4,
                Some(Daily {
                    allowance: 2000,
                    tz: GOOGLE_TZ,
                }),
                600,
                10,
            ),
            Provider::Analytics => (60, 4, None, 600, 10),
            Provider::FieldData => (140, 8, None, 150, 16),
            Provider::LabTests => (
                120,
                6,
                Some(Daily {
                    allowance: 25_000,
                    tz: GOOGLE_TZ,
                }),
                240,
                16,
            ),
            Provider::Ahrefs => (50, 2, None, 60, 4),
            Provider::Majestic => (30, 2, None, 60, 4),
            Provider::Moz => (30, 2, None, 60, 4),
            Provider::Sheets => (50, 1, None, 60, 1),
        };
        Self {
            per_minute,
            concurrency,
            daily,
            published_per_minute: pub_pm,
            published_concurrency: pub_c,
        }
    }

    /// Applies the user's choices, clamped to the published limits.
    pub fn with_settings(mut self, s: &ProviderSettings) -> Self {
        if let Some(pm) = s.per_minute {
            self.per_minute = pm.clamp(1, self.published_per_minute);
        }
        if let Some(c) = s.concurrency {
            self.concurrency = c.clamp(1, self.published_concurrency);
        }
        self
    }
}

/// A token bucket refilled continuously; holds at most one minute's worth.
pub struct Bucket {
    per_minute: f64,
    state: Mutex<(f64, Instant)>,
}

impl Bucket {
    pub fn new(per_minute: u32) -> Self {
        let per_minute = f64::from(per_minute.max(1));
        // Start with a small burst rather than a full minute, so a run does
        // not open with hundreds of requests at once.
        let start = per_minute.min(per_minute / 10.0 + 1.0);
        Self {
            per_minute,
            state: Mutex::new((start, Instant::now())),
        }
    }

    pub async fn acquire(&self) {
        loop {
            let wait = {
                let mut s = self.state.lock().expect("bucket lock");
                let now = Instant::now();
                let elapsed = now.duration_since(s.1).as_secs_f64();
                s.0 = (s.0 + elapsed * self.per_minute / 60.0).min(self.per_minute);
                s.1 = now;
                if s.0 >= 1.0 {
                    s.0 -= 1.0;
                    return;
                }
                Duration::from_secs_f64((1.0 - s.0) * 60.0 / self.per_minute)
            };
            tokio::time::sleep(wait).await;
        }
    }
}

/// The provider's current day, as a date string in its time zone.
pub fn provider_day(now: Timestamp, tz: &str) -> String {
    match now.in_tz(tz) {
        Ok(z) => z.date().to_string(),
        Err(_) => now.to_string()[..10].to_string(),
    }
}

/// When the provider's day ends (its daily allowance resets).
pub fn next_reset(now: Timestamp, tz: &str) -> Timestamp {
    let Ok(z) = now.in_tz(tz) else {
        return now + 24.hours();
    };
    let next: Option<Zoned> = z
        .date()
        .tomorrow()
        .ok()
        .and_then(|d| d.to_zoned(z.time_zone().clone()).ok());
    next.map(|n| n.timestamp()).unwrap_or(now + 24.hours())
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct LedgerFile {
    /// key → (provider day, used)
    #[serde(default)]
    used: BTreeMap<String, (String, u64)>,
}

/// Usage per provider, credential and property per provider day, in
/// `usage.json` next to the settings.
pub struct Ledger {
    path: PathBuf,
}

impl Ledger {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn key(p: Provider, credential: &str, property: &str) -> String {
        format!("{}|{credential}|{property}", p.id())
    }

    pub fn used(&self, key: &str, day: &str) -> u64 {
        let f: LedgerFile = read_json(&self.path);
        match f.used.get(key) {
            Some((d, n)) if d == day => *n,
            _ => 0,
        }
    }

    pub fn add(&self, key: &str, day: &str, n: u64) -> crate::Result<u64> {
        let mut f: LedgerFile = read_json(&self.path);
        let entry = f.used.entry(key.to_string()).or_insert((day.to_string(), 0));
        if entry.0 != day {
            *entry = (day.to_string(), 0);
        }
        entry.1 += n;
        let total = entry.1;
        // Old days are dropped so the file stays small.
        f.used.retain(|_, (d, _)| d == day);
        write_json(&self.path, &f)?;
        Ok(total)
    }
}

/// Spec 004's retry rules: up to three attempts, exponential backoff with
/// jitter, and at least the wait a provider asks for. Returns the result and
/// the attempts used.
pub async fn with_retry<T, F, Fut>(mut f: F) -> (Result<T, HttpError>, u32)
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, HttpError>>,
{
    const ATTEMPTS: u32 = 3;
    let mut attempt = 0;
    loop {
        attempt += 1;
        match f().await {
            Ok(v) => return (Ok(v), attempt),
            Err(e) if e.is_transient() && attempt < ATTEMPTS => {
                let base = Duration::from_millis(500 * 2u64.pow(attempt));
                let asked = match &e {
                    HttpError::RateLimited { retry_after, .. } => *retry_after,
                    _ => None,
                };
                tokio::time::sleep(asked.unwrap_or_default().max(base + jitter(base))).await;
            }
            Err(e) => return (Err(e), attempt),
        }
    }
}

fn jitter(base: Duration) -> Duration {
    let mut b = [0u8; 2];
    let _ = getrandom::fill(&mut b);
    let frac = f64::from(u16::from_le_bytes(b)) / f64::from(u16::MAX);
    base.mul_f64(frac / 2.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_settings_are_clamped_to_published_limits() {
        let l = Limits::defaults(Provider::FieldData).with_settings(&ProviderSettings {
            enabled: true,
            per_minute: Some(10_000),
            concurrency: Some(0),
        });
        assert_eq!(l.per_minute, 150);
        assert_eq!(l.concurrency, 1);
    }

    #[test]
    fn every_default_is_within_its_published_limit() {
        for p in Provider::ALL {
            let l = Limits::defaults(p);
            assert!(l.per_minute <= l.published_per_minute, "{p:?}");
            assert!(l.concurrency <= l.published_concurrency, "{p:?}");
        }
    }

    #[tokio::test(start_paused = true)]
    async fn bucket_never_exceeds_its_rate() {
        let b = Bucket::new(60);
        let start = Instant::now();
        for _ in 0..30 {
            b.acquire().await;
        }
        // 60/min with a burst of 7: 30 requests need at least 23 seconds.
        assert!(
            start.elapsed() >= Duration::from_secs(22),
            "{:?}",
            start.elapsed()
        );
        assert!(
            start.elapsed() <= Duration::from_secs(25),
            "{:?}",
            start.elapsed()
        );
    }

    #[test]
    fn google_day_and_reset_follow_pacific_time() {
        // 2026-10-14 06:30 UTC is 23:30 on the 13th in Los Angeles (PDT).
        let now: Timestamp = "2026-10-14T06:30:00Z".parse().unwrap();
        assert_eq!(provider_day(now, GOOGLE_TZ), "2026-10-13");
        assert_eq!(next_reset(now, GOOGLE_TZ).to_string(), "2026-10-14T07:00:00Z");
    }

    #[test]
    fn ledger_counts_per_day_and_resets() {
        let dir = tempfile::tempdir().unwrap();
        let l = Ledger::new(dir.path().join("usage.json"));
        let k = Ledger::key(Provider::UrlInspection, "a@b.c", "sc-domain:a.test");
        assert_eq!(l.add(&k, "2026-10-13", 5).unwrap(), 5);
        assert_eq!(l.add(&k, "2026-10-13", 2).unwrap(), 7);
        assert_eq!(l.used(&k, "2026-10-13"), 7);
        assert_eq!(l.used(&k, "2026-10-14"), 0);
        assert_eq!(l.add(&k, "2026-10-14", 1).unwrap(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn retry_honours_retry_after_and_stops_on_permanent_errors() {
        let mut calls = 0;
        let start = Instant::now();
        let (r, attempts) = with_retry(|| {
            calls += 1;
            let n = calls;
            async move {
                if n == 1 {
                    Err(HttpError::RateLimited {
                        provider: "T".into(),
                        retry_after: Some(Duration::from_secs(30)),
                        quota: false,
                        message: String::new(),
                    })
                } else {
                    Ok(n)
                }
            }
        })
        .await;
        assert_eq!(r.unwrap(), 2);
        assert_eq!(attempts, 2);
        assert!(start.elapsed() >= Duration::from_secs(30));

        let (r, attempts) = with_retry(|| async {
            Err::<(), _>(HttpError::Forbidden {
                provider: "T".into(),
                message: String::new(),
            })
        })
        .await;
        assert!(r.is_err());
        assert_eq!(attempts, 1);
    }
}
