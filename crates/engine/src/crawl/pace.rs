//! Why a crawl is not going faster, and what each site is doing (spec 008
//! FR-019, FR-021, FR-028, FR-029).
//!
//! Pure state and rules: the runner hands in what it knows (its caps, each
//! site's speed and pause state, how busy its own loop is) and gets back
//! plain-language answers for the status bar, the per-site breakdown and the
//! crawl log.

use std::time::{Duration, Instant};

use serde::Serialize;

use super::speed::{Cut, SiteSpeed};

/// A cut or a slowdown this recent still explains a site's state.
const RECENT: Duration = Duration::from_secs(30);
/// A site counts towards "every site is slow" only while it is answering.
const ACTIVE: Duration = Duration::from_secs(5);
/// The crawl's own loop busier than this means the computer, not the
/// network, sets the pace.
const BUSY: f64 = 0.9;
/// How long one measurement of the loop's load lasts.
const LOAD_WINDOW: Duration = Duration::from_secs(2);

/// The main thing keeping the crawl from going faster (FR-028).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LimitKind {
    /// The user's "at once" cap is full.
    Concurrency,
    /// The user's URLs-per-second cap.
    Rate,
    /// The user's per-site cap.
    PerSite,
    /// A site got slower, so it gets fewer requests.
    SiteSlowdown,
    /// A site answered 429 or 503.
    SiteOverload,
    /// A site asked us to wait (Retry-After), with a countdown.
    RequestedWait,
    /// robots.txt asks for time between requests.
    CrawlDelay,
    /// Every site with work left is paused after repeated failures.
    SitePaused,
    /// A new site starts with a few requests at once and grows.
    StartingUp,
    /// Every site slowed down at once: the user's connection.
    Connection,
    /// The crawl's own processing on this computer.
    Computer,
    /// Writing results to the crawl file.
    Saving,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SpeedLimit {
    pub kind: LimitKind,
    /// The site it is about, when it is about one.
    pub host: Option<String>,
    /// For a requested wait or a pause: seconds until it ends.
    pub seconds_left: Option<u64>,
    /// In plain language, for the status bar and the CLI.
    pub message: String,
}

/// What a site is doing (FR-029).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SiteState {
    SpeedingUp,
    Steady,
    SlowedDown,
    Waiting,
    Paused,
}

/// One row of the per-site breakdown (FR-029).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SitePace {
    pub host: String,
    pub state: SiteState,
    /// Requests it may have open now, and the most it ever gets.
    pub allowance: u32,
    pub max: u32,
    pub in_flight: u32,
    /// Recent average response time.
    pub response_ms: Option<u32>,
    /// URLs finished this crawl.
    pub crawled: u64,
    /// URLs waiting for it. Counted in the part of the queue held in
    /// memory: with `waiting_more`, there are more on disk.
    pub waiting: u64,
    pub waiting_more: bool,
    /// Why it is in this state, in plain language.
    pub reason: String,
    pub seconds_left: Option<u64>,
}

/// What the runner knows about one site, borrowed for a moment.
pub struct SiteView<'a> {
    pub host: &'a str,
    pub speed: &'a SiteSpeed,
    pub in_flight: u32,
    /// Requests it may have open now, all rules applied.
    pub limit: u32,
    /// robots.txt crawl-delay being honoured.
    pub delay: Option<Duration>,
    /// No request before this.
    pub next_start: Instant,
    /// Paused after repeated failures: seconds left (0 while its test
    /// request is out), or None when not paused.
    pub paused: Option<u64>,
    pub crawled: u64,
    pub waiting: u64,
    /// Its robots.txt is still being read.
    pub robots_pending: bool,
}

fn secs(d: Duration) -> u64 {
    d.as_secs_f64().ceil() as u64
}

fn human_secs(s: f64) -> String {
    if s < 10.0 {
        format!("{s:.1} s").replace(".0 s", " s")
    } else {
        format!("{} s", s.round())
    }
}

/// The site's state, why, and seconds left on a wait or pause.
pub fn site_state(v: &SiteView, now: Instant) -> (SiteState, String, Option<u64>) {
    let s = v.speed;
    if let Some(left) = v.paused {
        let why = "Paused after repeated failures; it gets one test request when the pause ends.";
        return (SiteState::Paused, why.into(), Some(left));
    }
    if v.robots_pending {
        return (
            SiteState::SpeedingUp,
            "Reading its robots.txt first.".into(),
            None,
        );
    }
    if v.next_start > now {
        let left = v.next_start - now;
        // A crawl-delay spaces every request; a longer gap is a wait the
        // site asked for.
        if v.delay.is_none_or(|d| left > d) {
            let why = format!(
                "Asked us to wait (Retry-After): {} left.",
                human_secs(left.as_secs_f64())
            );
            return (SiteState::Waiting, why, Some(secs(left)));
        }
    }
    if let Some(d) = v.delay {
        let why = format!(
            "robots.txt asks for {} between requests, so one at a time.",
            human_secs(d.as_secs_f64())
        );
        return (SiteState::Steady, why, None);
    }
    if let Some((at, cut)) = s.last_cut()
        && now.duration_since(at) < RECENT
        && s.allowance() < s.max()
    {
        let why = match cut {
            Cut::Overloaded => format!(
                "Answered 429 or 503 (overloaded), so cut to {} at once.",
                s.allowance()
            ),
            Cut::TimedOut => format!("A request timed out, so cut to {} at once.", s.allowance()),
            Cut::Slower => format!(
                "Responses got slower ({}, usually {}), so cut to {} at once.",
                ms(s.typical_ms()),
                ms(s.baseline_ms()),
                s.allowance()
            ),
        };
        return (SiteState::SlowedDown, why, None);
    }
    if s.allowance() < s.max() {
        let why = if s.adaptive() {
            format!(
                "{} of {} at once, one more each time a round of answers comes back steady.",
                s.allowance(),
                s.max()
            )
        } else {
            format!("Recovering to the fixed {} at once.", s.max())
        };
        return (SiteState::SpeedingUp, why, None);
    }
    let why = format!("At the per-site cap of {} at once.", s.max());
    (SiteState::Steady, why, None)
}

fn ms(v: Option<f64>) -> String {
    v.map_or_else(|| "unknown".into(), |m| format!("{} ms", m.round()))
}

pub fn site_pace(v: &SiteView, now: Instant, waiting_more: bool) -> SitePace {
    let (state, reason, seconds_left) = site_state(v, now);
    SitePace {
        host: v.host.to_string(),
        state,
        allowance: v.limit,
        max: v.speed.max(),
        in_flight: v.in_flight,
        response_ms: v.speed.typical_ms().map(|m| m.round() as u32),
        crawled: v.crawled,
        waiting: v.waiting,
        waiting_more,
        reason,
        seconds_left,
    }
}

/// The crawl-wide picture the limit is chosen from.
pub struct CrawlView {
    pub in_flight: u32,
    pub max_concurrent: u32,
    pub max_urls_per_sec: Option<f64>,
    pub urls_per_sec: f64,
    /// Every site is slow at once (FR-019).
    pub connection_slow: bool,
    pub load: Load,
}

/// The main limit (FR-028), or None when the crawl runs as fast as its caps
/// and the sites allow, or has nothing left to request. `blocking` is the
/// site holding up the most queued URLs when no site can start a request.
pub fn crawl_limit(c: &CrawlView, blocking: Option<&SiteView>, now: Instant) -> Option<SpeedLimit> {
    let limit = |kind, host: Option<&str>, seconds_left, message: String| SpeedLimit {
        kind,
        host: host.map(str::to_string),
        seconds_left,
        message,
    };
    if c.connection_slow {
        return Some(limit(
            LimitKind::Connection,
            None,
            None,
            "Every site got slower at once, so your connection is probably busy. No new requests are added until it recovers.".into(),
        ));
    }
    if c.load.busy >= BUSY {
        return Some(if c.load.saving * 2.0 >= c.load.busy {
            limit(
                LimitKind::Saving,
                None,
                None,
                "Saving results to the crawl file takes most of the time; a faster disk would help.".into(),
            )
        } else {
            limit(
                LimitKind::Computer,
                None,
                None,
                "Processing pages on this computer takes most of the time.".into(),
            )
        });
    }
    if c.in_flight >= c.max_concurrent {
        return Some(limit(
            LimitKind::Concurrency,
            None,
            None,
            format!("At your cap of {} requests at once.", c.max_concurrent),
        ));
    }
    if let Some(rate) = c.max_urls_per_sec
        && c.urls_per_sec >= rate * 0.9
    {
        return Some(limit(
            LimitKind::Rate,
            None,
            None,
            format!("At your cap of {} URLs per second.", rate.round()),
        ));
    }
    let v = blocking?;
    let site = v.host;
    let (state, _, left) = site_state(v, now);
    let s = v.speed;
    Some(match state {
        SiteState::Paused => limit(
            LimitKind::SitePaused,
            Some(site),
            left,
            format!("{site} is paused after repeated failures."),
        ),
        SiteState::Waiting => limit(
            LimitKind::RequestedWait,
            Some(site),
            left,
            format!("{site} asked us to wait (Retry-After)."),
        ),
        SiteState::Steady if v.delay.is_some() => limit(
            LimitKind::CrawlDelay,
            Some(site),
            None,
            format!(
                "{site}'s robots.txt asks for {} between requests.",
                human_secs(v.delay.unwrap_or_default().as_secs_f64())
            ),
        ),
        SiteState::SlowedDown => match s.last_cut().map(|(_, c)| c) {
            Some(Cut::Overloaded) => limit(
                LimitKind::SiteOverload,
                Some(site),
                None,
                format!(
                    "{site} answered 429 or 503 (overloaded), so it gets {} at once.",
                    s.allowance()
                ),
            ),
            _ => limit(
                LimitKind::SiteSlowdown,
                Some(site),
                None,
                format!("{site} got slower, so it gets {} at once.", s.allowance()),
            ),
        },
        SiteState::SpeedingUp => limit(
            LimitKind::StartingUp,
            Some(site),
            None,
            format!(
                "{site} gets {} of {} at once, growing while it answers steadily.",
                s.allowance(),
                s.max()
            ),
        ),
        SiteState::Steady => limit(
            LimitKind::PerSite,
            Some(site),
            None,
            format!("At your cap of {} requests at once per site ({site}).", s.max()),
        ),
    })
}

/// FR-019: most sites that are answering got slow at the same moment. One
/// site alone says nothing about the connection.
pub fn connection_slow<'a>(speeds: impl Iterator<Item = (&'a SiteSpeed, Instant)>, now: Instant) -> bool {
    let (mut active, mut slow) = (0, 0);
    for (s, last) in speeds {
        if now.duration_since(last) > ACTIVE || s.baseline_ms().is_none() {
            continue;
        }
        active += 1;
        if s.is_slow() {
            slow += 1;
        }
    }
    active >= 2 && slow * 4 >= active * 3
}

/// How busy the crawl's own loop is (FR-028: computer or saving).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Load {
    /// Share of time the loop was working rather than waiting.
    pub busy: f64,
    /// Share of time spent committing to the crawl file.
    pub saving: f64,
}

/// Measures [`Load`] over consecutive windows.
#[derive(Debug)]
pub struct LoadMeter {
    since: Instant,
    idle: Duration,
    saving: Duration,
    last: Load,
}

impl LoadMeter {
    pub fn new(now: Instant) -> Self {
        Self {
            since: now,
            idle: Duration::ZERO,
            saving: Duration::ZERO,
            last: Load::default(),
        }
    }

    pub fn idle(&mut self, d: Duration) {
        self.idle += d;
    }

    pub fn saving(&mut self, d: Duration) {
        self.saving += d;
    }

    /// The last complete window's load.
    pub fn load(&mut self, now: Instant) -> Load {
        let span = now.duration_since(self.since);
        if span >= LOAD_WINDOW {
            let total = span.as_secs_f64();
            self.last = Load {
                busy: (1.0 - self.idle.as_secs_f64() / total).clamp(0.0, 1.0),
                saving: (self.saving.as_secs_f64() / total).clamp(0.0, 1.0),
            };
            self.since = now;
            self.idle = Duration::ZERO;
            self.saving = Duration::ZERO;
        }
        self.last
    }
}

/// Speed events of one site during the crawl, for the crawl log (FR-021).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SiteEvents {
    /// 429 and 503 answers that cut its allowance.
    pub overloads: u32,
    /// Cuts for slow responses or timeouts.
    pub slowdowns: u32,
    /// Waits it asked for (Retry-After), their total and the longest.
    pub waits: u32,
    pub wait_ms: u64,
    pub longest_wait_ms: u64,
    /// Pauses after repeated failures.
    pub pauses: u32,
}

impl SiteEvents {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    pub fn wait(&mut self, d: Duration) {
        let ms = d.as_millis() as u64;
        self.waits += 1;
        self.wait_ms += ms;
        self.longest_wait_ms = self.longest_wait_ms.max(ms);
    }

    /// One line for the crawl log, e.g. "cut back 3 times after 429 or 503
    /// answers; asked to wait twice (45 s in total, longest 30 s)".
    pub fn summary(&self) -> String {
        let times = |n: u32| match n {
            1 => "once".to_string(),
            2 => "twice".to_string(),
            n => format!("{n} times"),
        };
        let mut parts = Vec::new();
        if self.overloads > 0 {
            parts.push(format!(
                "cut back {} after 429 or 503 answers",
                times(self.overloads)
            ));
        }
        if self.slowdowns > 0 {
            parts.push(format!(
                "cut back {} for slow responses or timeouts",
                times(self.slowdowns)
            ));
        }
        if self.waits > 0 {
            parts.push(format!(
                "asked to wait {} ({} in total, longest {})",
                times(self.waits),
                human_secs(self.wait_ms as f64 / 1000.0),
                human_secs(self.longest_wait_ms as f64 / 1000.0)
            ));
        }
        if self.pauses > 0 {
            parts.push(format!("paused {} after repeated failures", times(self.pauses)));
        }
        parts.join("; ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crawl::speed::Signal;

    const MS: Duration = Duration::from_millis(1);

    fn view<'a>(speed: &'a SiteSpeed, now: Instant) -> SiteView<'a> {
        SiteView {
            host: "https://a.test:443",
            speed,
            in_flight: speed.allowance(),
            limit: speed.allowance(),
            delay: None,
            next_start: now,
            paused: None,
            crawled: 10,
            waiting: 5,
            robots_pending: false,
        }
    }

    fn crawl(in_flight: u32) -> CrawlView {
        CrawlView {
            in_flight,
            max_concurrent: 64,
            max_urls_per_sec: None,
            urls_per_sec: 100.0,
            connection_slow: false,
            load: Load::default(),
        }
    }

    fn steady(s: &mut SiteSpeed, n: u32, took: Duration, now: Instant) {
        for _ in 0..n {
            s.on_response(took, Signal::Answered, now);
        }
    }

    #[test]
    fn a_new_site_is_speeding_up_then_at_the_per_site_cap() {
        let now = Instant::now();
        let mut s = SiteSpeed::new(4, true);
        let (state, _, _) = site_state(&view(&s, now), now);
        assert_eq!(state, SiteState::SpeedingUp);
        let l = crawl_limit(&crawl(2), Some(&view(&s, now)), now).unwrap();
        assert_eq!(l.kind, LimitKind::StartingUp);
        assert!(l.message.contains("2 of 4"), "{}", l.message);

        steady(&mut s, 200, 10 * MS, now);
        assert_eq!(s.allowance(), 4);
        let l = crawl_limit(&crawl(4), Some(&view(&s, now)), now).unwrap();
        assert_eq!(l.kind, LimitKind::PerSite);
    }

    #[test]
    fn overload_and_retry_after_are_named_with_a_countdown() {
        let now = Instant::now();
        let mut s = SiteSpeed::new(8, false);
        let until = s
            .on_response(
                MS,
                Signal::Overloaded {
                    retry_after: Some(Duration::from_secs(20)),
                },
                now,
            )
            .unwrap();
        let mut v = view(&s, now);
        v.next_start = until;
        let (state, reason, left) = site_state(&v, now);
        assert_eq!((state, left), (SiteState::Waiting, Some(20)));
        assert!(reason.contains("20 s"), "{reason}");
        let l = crawl_limit(&crawl(0), Some(&v), now).unwrap();
        assert_eq!((l.kind, l.seconds_left), (LimitKind::RequestedWait, Some(20)));

        // Once the wait is over, the cut still explains the lower allowance.
        let later = now + Duration::from_secs(21);
        let v = view(&s, later);
        assert_eq!(site_state(&v, later).0, SiteState::SlowedDown);
        assert_eq!(
            crawl_limit(&crawl(4), Some(&v), later).unwrap().kind,
            LimitKind::SiteOverload
        );
    }

    #[test]
    fn crawl_delay_is_steady_with_its_reason() {
        let now = Instant::now();
        let s = SiteSpeed::new(8, true);
        let mut v = view(&s, now);
        v.delay = Some(Duration::from_secs(5));
        v.next_start = now + Duration::from_secs(3);
        let (state, reason, _) = site_state(&v, now);
        assert_eq!(state, SiteState::Steady);
        assert!(reason.contains("5 s between requests"), "{reason}");
        assert_eq!(
            crawl_limit(&crawl(1), Some(&v), now).unwrap().kind,
            LimitKind::CrawlDelay
        );
    }

    #[test]
    fn user_caps_come_before_sites() {
        let now = Instant::now();
        let s = SiteSpeed::new(8, true);
        let v = view(&s, now);
        assert_eq!(
            crawl_limit(&crawl(64), Some(&v), now).unwrap().kind,
            LimitKind::Concurrency
        );
        let mut c = crawl(3);
        c.max_urls_per_sec = Some(50.0);
        c.urls_per_sec = 48.0;
        assert_eq!(crawl_limit(&c, Some(&v), now).unwrap().kind, LimitKind::Rate);
        c.urls_per_sec = 20.0;
        assert_eq!(
            crawl_limit(&c, Some(&v), now).unwrap().kind,
            LimitKind::StartingUp
        );
        // Nothing blocked and no cap reached: no limit to name.
        assert_eq!(crawl_limit(&crawl(3), None, now), None);
    }

    #[test]
    fn a_busy_loop_names_saving_or_the_computer() {
        let now = Instant::now();
        let mut c = crawl(10);
        c.load = Load {
            busy: 0.95,
            saving: 0.6,
        };
        assert_eq!(crawl_limit(&c, None, now).unwrap().kind, LimitKind::Saving);
        c.load = Load {
            busy: 0.95,
            saving: 0.1,
        };
        assert_eq!(crawl_limit(&c, None, now).unwrap().kind, LimitKind::Computer);
    }

    #[test]
    fn every_site_slow_at_once_is_the_connection() {
        let now = Instant::now();
        let mut a = SiteSpeed::new(8, true);
        let mut b = SiteSpeed::new(8, true);
        steady(&mut a, 20, 10 * MS, now);
        steady(&mut b, 20, 10 * MS, now);
        assert!(!connection_slow([(&a, now), (&b, now)].into_iter(), now));
        // While held, slowness neither cuts nor grows the allowance.
        a.set_hold(true);
        b.set_hold(true);
        let before = a.allowance();
        steady(&mut a, 20, 100 * MS, now);
        steady(&mut b, 20, 100 * MS, now);
        assert_eq!(a.allowance(), before);
        assert!(connection_slow([(&a, now), (&b, now)].into_iter(), now));
        // One site alone says nothing about the connection.
        assert!(!connection_slow([(&a, now)].into_iter(), now));
        // A site that stopped answering a while ago does not count.
        let later = now + Duration::from_secs(10);
        assert!(!connection_slow([(&a, later), (&b, now)].into_iter(), later));
    }

    #[test]
    fn load_meter_reports_complete_windows() {
        let t0 = Instant::now();
        let mut m = LoadMeter::new(t0);
        m.idle(Duration::from_millis(500));
        m.saving(Duration::from_millis(1000));
        assert_eq!(m.load(t0 + Duration::from_secs(1)), Load::default());
        let l = m.load(t0 + Duration::from_secs(2));
        assert!(
            (l.busy - 0.75).abs() < 1e-9 && (l.saving - 0.5).abs() < 1e-9,
            "{l:?}"
        );
    }

    #[test]
    fn event_summary_reads_as_a_sentence() {
        let mut e = SiteEvents::default();
        assert!(e.is_empty());
        e.overloads = 3;
        e.wait(Duration::from_secs(30));
        e.wait(Duration::from_secs(15));
        e.pauses = 1;
        assert_eq!(
            e.summary(),
            "cut back 3 times after 429 or 503 answers; asked to wait twice (45 s in total, longest 30 s); \
             paused once after repeated failures"
        );
    }
}
