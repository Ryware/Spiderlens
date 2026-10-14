//! How the renderer's requests reach the network (spec 014 plan D4,
//! research R6 and R7).
//!
//! Every request a rendered page makes, other than its main document and
//! the types blocked by the settings, comes here. [`EngineNet`] answers it
//! from the shared cache when it can; otherwise it asks the coordinator for
//! a permit (`NetMsg::Permit`), fetches with the crawl's client, and reports
//! the end (`NetMsg::Done`) so the host's allowance and Retry-After learn
//! from it. The coordinator keeps the accounting in [`Permits`]: an internal
//! resource counts against its host's allowance and the global caps exactly
//! like a page; a third-party resource has its own small caps instead.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures_util::future::BoxFuture;
use reqwest::Client;
use spiderlens_config::RobotsMode;
use spiderlens_render::{FailReason, Network, ResourceAnswer, ResourceRequest};
use tokio::sync::oneshot;
use url::Url;

use super::cache::{Cached, ResourceCache};
use crate::crawl::host_key;
use crate::crawl::hosts::{Host, RobotsState};
use crate::crawl::speed::{Signal, parse_retry_after};
use crate::fetch::fetch_resource;
use crate::robots::RobotsVerdict;
use crate::{ErrorClass, RequestHeader};

/// Third-party requests open at once, over all third-party hosts.
pub const THIRD_PARTY_OPEN: u32 = 8;
/// Third-party requests open at once to one host.
pub const THIRD_PARTY_PER_HOST: u32 = 2;

/// The coordinator's answer to a permit request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermitAnswer {
    /// Fetch now. `robots_rule` is the rule that would have blocked it
    /// under `ignore_report`.
    Granted { robots_rule: Option<String> },
    /// robots.txt disallows it and the crawl respects robots.txt.
    BlockedByRobots { rule: String },
    /// The host was given up by the circuit breaker (spec 004 FR-026):
    /// nothing more is sent to it.
    HostUnavailable,
}

#[derive(Debug)]
pub struct PermitRequest {
    pub url: Url,
    pub host_key: String,
    pub third_party: bool,
    pub reply: oneshot::Sender<PermitAnswer>,
}

/// What render tasks tell the coordinator.
#[derive(Debug)]
pub enum NetMsg {
    Permit(PermitRequest),
    Done {
        host_key: String,
        third_party: bool,
        took: Duration,
        signal: Signal,
    },
}

/// The coordinator's state a permit decision reads and changes. The same
/// counters the page dispatcher uses, so pages and resources share them.
pub(crate) struct Slots<'a> {
    pub hosts: &'a mut HashMap<String, Host>,
    /// Requests open over all internal hosts (pages, robots.txt, resources).
    pub in_flight: &'a mut u32,
    pub next_global: &'a mut Instant,
    pub max_concurrent: u32,
    /// `max_urls_per_sec`.
    pub rate: Option<f64>,
}

/// Result of one pass over the waiting permits.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Served {
    /// When a waiting permit may be granted, if it waits on time.
    pub wake: Option<Instant>,
    /// Hosts whose robots.txt must be read before their resources go out.
    pub need_robots: Vec<String>,
    /// A permit waits on the global caps: no new page fetch may take the
    /// slot first (plan D4: waiting permits are served before new pages).
    pub waiting_on_global: bool,
}

/// Permit accounting, owned by the coordinator.
#[derive(Debug, Default)]
pub(crate) struct Permits {
    waiting: VecDeque<PermitRequest>,
    third_open: u32,
    third_per_host: HashMap<String, u32>,
}

impl Permits {
    pub fn push(&mut self, r: PermitRequest) {
        self.waiting.push_back(r);
    }

    pub fn waiting(&self) -> usize {
        self.waiting.len()
    }

    /// Third-party requests open now.
    #[cfg(test)]
    pub fn third_party_open(&self) -> u32 {
        self.third_open
    }

    /// Grants every waiting permit that may go now, in arrival order.
    pub fn serve(&mut self, s: &mut Slots<'_>, robots_mode: RobotsMode, now: Instant) -> Served {
        let mut out = Served::default();
        let mut still = VecDeque::with_capacity(self.waiting.len());
        let soonest = |t: Instant, wake: &mut Option<Instant>| *wake = Some(wake.map_or(t, |w| w.min(t)));
        while let Some(r) = self.waiting.pop_front() {
            // The render that asked has ended.
            if r.reply.is_closed() {
                continue;
            }
            let Some(host) = s.hosts.get(&r.host_key) else {
                if !out.need_robots.contains(&r.host_key) {
                    out.need_robots.push(r.host_key.clone());
                }
                still.push_back(r);
                continue;
            };
            let policy = match &host.robots {
                RobotsState::Known(p, _) => p.clone(),
                RobotsState::Unknown => {
                    if !out.need_robots.contains(&r.host_key) {
                        out.need_robots.push(r.host_key.clone());
                    }
                    still.push_back(r);
                    continue;
                }
                RobotsState::Fetching => {
                    still.push_back(r);
                    continue;
                }
            };
            let mut robots_rule = None;
            if let RobotsVerdict::Disallowed(rule) = policy.check(&r.url) {
                match robots_mode {
                    RobotsMode::Respect => {
                        let _ = r.reply.send(PermitAnswer::BlockedByRobots { rule });
                        continue;
                    }
                    RobotsMode::IgnoreReport => robots_rule = Some(rule),
                    RobotsMode::Ignore => {}
                }
            }
            if !r.third_party && host.health.is_given_up() {
                let _ = r.reply.send(PermitAnswer::HostUnavailable);
                continue;
            }
            // A paused host takes no request until its pause ends.
            if !r.third_party
                && let Some(until) = host.health.paused_until()
            {
                soonest(until, &mut out.wake);
                still.push_back(r);
                continue;
            }
            // Crawl-delay and Retry-After hold every request to the host.
            if host.next_start > now {
                soonest(host.next_start, &mut out.wake);
                still.push_back(r);
                continue;
            }
            if r.third_party {
                let per_host = self.third_per_host.get(&r.host_key).copied().unwrap_or(0);
                if self.third_open >= THIRD_PARTY_OPEN || per_host >= THIRD_PARTY_PER_HOST {
                    still.push_back(r);
                    continue;
                }
                self.third_open += 1;
                *self.third_per_host.entry(r.host_key.clone()).or_default() += 1;
            } else {
                if host.in_flight >= host.limit() {
                    still.push_back(r);
                    continue;
                }
                if *s.in_flight >= s.max_concurrent {
                    out.waiting_on_global = true;
                    still.push_back(r);
                    continue;
                }
                if *s.next_global > now {
                    out.waiting_on_global = true;
                    soonest(*s.next_global, &mut out.wake);
                    still.push_back(r);
                    continue;
                }
                *s.in_flight += 1;
                if let Some(rate) = s.rate {
                    *s.next_global = now + Duration::from_secs_f64(1.0 / rate);
                }
            }
            let host = s.hosts.get_mut(&r.host_key).expect("checked above");
            if !r.third_party {
                host.in_flight += 1;
            }
            if let Some(d) = host.delay {
                host.next_start = now + d;
            }
            if r.reply.send(PermitAnswer::Granted { robots_rule }).is_err() {
                // Asked and gone in the meantime: give the slot back.
                self.release(s, &r.host_key, r.third_party);
            }
        }
        self.waiting = still;
        out
    }

    fn release(&mut self, s: &mut Slots<'_>, host_key: &str, third_party: bool) {
        if third_party {
            self.third_open = self.third_open.saturating_sub(1);
            if let Some(n) = self.third_per_host.get_mut(host_key) {
                *n = n.saturating_sub(1);
            }
        } else {
            *s.in_flight = s.in_flight.saturating_sub(1);
            if let Some(h) = s.hosts.get_mut(host_key) {
                h.in_flight = h.in_flight.saturating_sub(1);
            }
        }
    }

    /// A granted request ended: frees its slot and lets the host's
    /// allowance and Retry-After learn from it (spec 008 D4).
    pub fn done(
        &mut self,
        s: &mut Slots<'_>,
        host_key: &str,
        third_party: bool,
        took: Duration,
        signal: Signal,
    ) {
        self.release(s, host_key, third_party);
        if let Some(h) = s.hosts.get_mut(host_key)
            && let Some(until) = h.speed.on_response(took, signal, Instant::now())
        {
            h.next_start = h.next_start.max(until);
        }
    }
}

/// What the engine alone knows about one request (plan D4 step 6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Note {
    /// Answered from the resource cache.
    Cached,
    /// Not fetched: robots.txt disallows it.
    BlockedByRobots(String),
    /// Fetched although robots.txt disallows it (`ignore_report`).
    RobotsIgnored(String),
    /// The fetch failed: the error class, for the resource record.
    Error(String),
}

/// What every page's network shares for one crawl.
pub struct NetShared {
    pub client: Client,
    pub headers: Arc<Vec<RequestHeader>>,
    pub max_body: u64,
    pub cache: Mutex<ResourceCache>,
    pub send: Box<dyn Fn(NetMsg) -> bool + Send + Sync>,
}

/// The [`Network`] of one render: shared parts plus that page's notes.
pub struct EngineNet {
    shared: Arc<NetShared>,
    notes: Mutex<Vec<(Url, Note)>>,
}

impl EngineNet {
    pub fn new(shared: Arc<NetShared>) -> Self {
        Self {
            shared,
            notes: Mutex::new(Vec::new()),
        }
    }

    /// The notes of this page, in request order.
    pub fn take_notes(&self) -> Vec<(Url, Note)> {
        std::mem::take(&mut *self.notes.lock().expect("notes lock"))
    }

    fn note(&self, url: &Url, note: Note) {
        self.notes.lock().expect("notes lock").push((url.clone(), note));
    }

    async fn answer(&self, req: ResourceRequest) -> ResourceAnswer {
        let get = req.method.eq_ignore_ascii_case("GET");
        let key = req.url.as_str().to_owned();
        if get {
            let hit = self
                .shared
                .cache
                .lock()
                .expect("cache lock")
                .get(&key, Instant::now());
            if let Some(hit) = hit {
                self.note(&req.url, Note::Cached);
                return ResourceAnswer::Response {
                    status: hit.status,
                    headers: hit.headers.clone(),
                    body: hit.body.clone(),
                };
            }
        }
        let host = host_key(&req.url);
        let (tx, rx) = oneshot::channel();
        let asked = (self.shared.send)(NetMsg::Permit(PermitRequest {
            url: req.url.clone(),
            host_key: host.clone(),
            third_party: req.third_party,
            reply: tx,
        }));
        if !asked {
            return ResourceAnswer::Fail {
                reason: FailReason::Aborted,
            };
        }
        match rx.await {
            Ok(PermitAnswer::Granted { robots_rule }) => {
                if let Some(rule) = robots_rule {
                    self.note(&req.url, Note::RobotsIgnored(rule));
                }
            }
            Ok(PermitAnswer::BlockedByRobots { rule }) => {
                self.note(&req.url, Note::BlockedByRobots(rule));
                return ResourceAnswer::Fail {
                    reason: FailReason::BlockedByClient,
                };
            }
            Ok(PermitAnswer::HostUnavailable) => {
                self.note(&req.url, Note::Error("host_unavailable".into()));
                return ResourceAnswer::Fail {
                    reason: FailReason::Failed,
                };
            }
            Err(_) => {
                return ResourceAnswer::Fail {
                    reason: FailReason::Aborted,
                };
            }
        }
        let started = Instant::now();
        let fetched = fetch_resource(
            &self.shared.client,
            &req.url,
            &req.method,
            &req.headers,
            req.body,
            &self.shared.headers,
            !req.third_party,
            self.shared.max_body,
        )
        .await;
        let took = started.elapsed();
        let signal = match &fetched {
            Ok(r) if r.status == 429 || r.status == 503 => Signal::Overloaded {
                retry_after: r
                    .retry_after
                    .as_deref()
                    .and_then(|v| parse_retry_after(v, jiff::Timestamp::now())),
            },
            Ok(_) => Signal::Answered,
            Err(e) if e.class == ErrorClass::Timeout => Signal::TimedOut,
            Err(_) => Signal::Other,
        };
        (self.shared.send)(NetMsg::Done {
            host_key: host,
            third_party: req.third_party,
            took,
            signal,
        });
        match fetched {
            Ok(r) => {
                // The body is decoded: the cached copy must not claim otherwise.
                let headers: Vec<(String, String)> = r
                    .headers
                    .into_iter()
                    .filter(|(n, _)| n != "content-encoding" && n != "content-length")
                    .collect();
                if get {
                    let cached = Cached {
                        status: r.status,
                        headers: headers.clone(),
                        body: r.body.clone(),
                    };
                    self.shared.cache.lock().expect("cache lock").put(
                        &key,
                        cached,
                        Instant::now(),
                        jiff::Timestamp::now(),
                    );
                }
                ResourceAnswer::Response {
                    status: r.status,
                    headers,
                    body: r.body,
                }
            }
            Err(e) => {
                self.note(&req.url, Note::Error(e.class.as_str().to_owned()));
                ResourceAnswer::Fail {
                    reason: match e.class {
                        ErrorClass::Timeout => FailReason::TimedOut,
                        ErrorClass::Dns | ErrorClass::DnsTemporary => FailReason::NameNotResolved,
                        ErrorClass::Connection => FailReason::ConnectionRefused,
                        _ => FailReason::Failed,
                    },
                }
            }
        }
    }
}

impl Network for EngineNet {
    fn fetch(&self, req: ResourceRequest) -> BoxFuture<'_, ResourceAnswer> {
        Box::pin(self.answer(req))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::robots::{RobotsPolicy, RobotsTxt};
    use spiderlens_config::SpeedSettings;

    const SITE: &str = "http://a.test:80";
    const CDN: &str = "http://cdn.test:80";

    struct World {
        hosts: HashMap<String, Host>,
        in_flight: u32,
        next_global: Instant,
        max_concurrent: u32,
        rate: Option<f64>,
        permits: Permits,
    }

    impl World {
        fn new(per_host: u32) -> Self {
            let speed = SpeedSettings {
                max_concurrent: 8,
                max_per_host: per_host,
                max_urls_per_sec: None,
                adaptive: false,
            };
            let mut hosts = HashMap::new();
            for key in [SITE, CDN] {
                let mut h = Host::new(&speed);
                h.robots = RobotsState::Known(Arc::new(RobotsPolicy::allow_all()), None);
                hosts.insert(key.to_string(), h);
            }
            Self {
                hosts,
                in_flight: 0,
                next_global: Instant::now(),
                max_concurrent: 8,
                rate: None,
                permits: Permits::default(),
            }
        }

        fn serve(&mut self, mode: RobotsMode) -> Served {
            let mut s = Slots {
                hosts: &mut self.hosts,
                in_flight: &mut self.in_flight,
                next_global: &mut self.next_global,
                max_concurrent: self.max_concurrent,
                rate: self.rate,
            };
            self.permits.serve(&mut s, mode, Instant::now())
        }

        fn done(&mut self, host: &str, third_party: bool, signal: Signal) {
            let mut s = Slots {
                hosts: &mut self.hosts,
                in_flight: &mut self.in_flight,
                next_global: &mut self.next_global,
                max_concurrent: self.max_concurrent,
                rate: self.rate,
            };
            self.permits
                .done(&mut s, host, third_party, Duration::from_millis(20), signal);
        }

        /// A page fetch started by the dispatcher, as `Run::start` does.
        fn start_page(&mut self, host: &str) -> bool {
            let h = self.hosts.get_mut(host).unwrap();
            if h.in_flight >= h.limit() || self.in_flight >= self.max_concurrent {
                return false;
            }
            h.in_flight += 1;
            self.in_flight += 1;
            true
        }

        fn ask(&mut self, host: &str, path: &str, third_party: bool) -> oneshot::Receiver<PermitAnswer> {
            let (tx, rx) = oneshot::channel();
            self.permits.push(PermitRequest {
                url: Url::parse(&format!("{host}{path}")).unwrap(),
                host_key: host.to_string(),
                third_party,
                reply: tx,
            });
            rx
        }
    }

    fn granted(rx: &mut oneshot::Receiver<PermitAnswer>) -> bool {
        matches!(rx.try_recv(), Ok(PermitAnswer::Granted { .. }))
    }

    #[test]
    fn a_host_never_exceeds_its_allowance_counting_pages_and_resources() {
        let mut w = World::new(4);
        assert!(w.start_page(SITE));
        assert!(w.start_page(SITE));
        assert!(w.start_page(SITE));
        let mut rx: Vec<_> = (0..3).map(|i| w.ask(SITE, &format!("/r{i}.js"), false)).collect();
        w.serve(RobotsMode::Respect);
        let got: Vec<bool> = rx.iter_mut().map(granted).collect();
        assert_eq!(got, [true, false, false], "only one slot was left");
        assert_eq!(w.hosts[SITE].in_flight, 4);
        assert_eq!(w.permits.waiting(), 2);
        // Pages cannot take the slots resources hold.
        assert!(!w.start_page(SITE));
        // A resource ends: the next waiting one gets its slot, not a page.
        w.done(SITE, false, Signal::Answered);
        w.serve(RobotsMode::Respect);
        assert!(granted(&mut rx[1]));
        assert_eq!(w.hosts[SITE].in_flight, 4);
        assert_eq!(w.in_flight, 4);
        assert!(!w.start_page(SITE));
    }

    #[test]
    fn retry_after_holds_resources_too() {
        let mut w = World::new(4);
        let mut first = w.ask(SITE, "/a.js", false);
        w.serve(RobotsMode::Respect);
        assert!(granted(&mut first));
        w.done(
            SITE,
            false,
            Signal::Overloaded {
                retry_after: Some(Duration::from_secs(30)),
            },
        );
        let mut second = w.ask(SITE, "/b.js", false);
        let served = w.serve(RobotsMode::Respect);
        assert!(!granted(&mut second));
        let wake = served.wake.expect("waits on time");
        assert!(wake > Instant::now() + Duration::from_secs(25));
        // The allowance was cut too (spec 008 AIMD).
        assert!(w.hosts[SITE].speed.allowance() < 4);
    }

    #[test]
    fn third_party_has_its_own_caps_and_does_not_count_against_the_site() {
        let mut w = World::new(4);
        let mut rx: Vec<_> = (0..3).map(|i| w.ask(CDN, &format!("/{i}.js"), true)).collect();
        w.serve(RobotsMode::Respect);
        let got: Vec<bool> = rx.iter_mut().map(granted).collect();
        assert_eq!(got, [true, true, false], "two per third-party host");
        assert_eq!(w.in_flight, 0, "the crawled site's slots are untouched");
        assert_eq!(w.hosts[CDN].in_flight, 0);
        assert_eq!(w.permits.third_party_open(), 2);
        // Eight in total over all third-party hosts.
        let mut many = Vec::new();
        for i in 0..10 {
            let host = format!("http://cdn{i}.test:80");
            let mut h = Host::new(&SpeedSettings::POLITE);
            h.robots = RobotsState::Known(Arc::new(RobotsPolicy::allow_all()), None);
            w.hosts.insert(host.clone(), h);
            many.push(w.ask(&host, "/x.js", true));
        }
        w.serve(RobotsMode::Respect);
        let granted_now = many.iter_mut().map(granted).filter(|g| *g).count();
        assert_eq!(granted_now, 6);
        assert_eq!(w.permits.third_party_open(), THIRD_PARTY_OPEN);
        w.done(CDN, true, Signal::Answered);
        w.serve(RobotsMode::Respect);
        assert!(granted(&mut rx[2]));
    }

    #[test]
    fn global_cap_and_rate_make_pages_wait_behind_resources() {
        let mut w = World::new(16);
        w.max_concurrent = 2;
        assert!(w.start_page(SITE));
        assert!(w.start_page(SITE));
        let mut rx = w.ask(SITE, "/a.js", false);
        let served = w.serve(RobotsMode::Respect);
        assert!(served.waiting_on_global);
        assert!(!granted(&mut rx));
        w.in_flight = 1;
        w.hosts.get_mut(SITE).unwrap().in_flight = 1;
        w.rate = Some(2.0);
        let served = w.serve(RobotsMode::Respect);
        assert!(!served.waiting_on_global);
        assert!(granted(&mut rx));
        // The rate cap now spaces the next request by 0.5 s.
        w.max_concurrent = 8;
        let mut next = w.ask(SITE, "/b.js", false);
        let served = w.serve(RobotsMode::Respect);
        assert!(served.waiting_on_global && served.wake.is_some());
        assert!(!granted(&mut next));
    }

    #[test]
    fn robots_modes_apply_to_resources() {
        let mut w = World::new(4);
        let policy = RobotsTxt::parse(b"User-agent: *\nDisallow: /private/").for_agent("SpiderLens");
        w.hosts.get_mut(SITE).unwrap().robots = RobotsState::Known(Arc::new(policy), None);
        let mut rx = w.ask(SITE, "/private/app.js", false);
        w.serve(RobotsMode::Respect);
        assert!(matches!(rx.try_recv(), Ok(PermitAnswer::BlockedByRobots { .. })));
        assert_eq!(w.in_flight, 0, "a blocked resource takes no slot");
        let mut rx = w.ask(SITE, "/private/app.js", false);
        w.serve(RobotsMode::IgnoreReport);
        assert!(matches!(
            rx.try_recv(),
            Ok(PermitAnswer::Granted { robots_rule: Some(_) })
        ));
    }

    #[test]
    fn paused_and_given_up_hosts_hold_or_refuse_resources() {
        let mut w = World::new(4);
        let retry = spiderlens_config::RetrySettings {
            host_pause_after: 1,
            host_give_up_after: 1,
            ..spiderlens_config::RetrySettings::default()
        };
        let now = Instant::now();
        let health = &mut w.hosts.get_mut(SITE).unwrap().health;
        health.on_transient_failure(&retry, now);
        let until = health.paused_until().expect("paused");
        // Paused (spec 004): the resource waits for the pause to end.
        let mut rx = w.ask(SITE, "/app.js", false);
        let served = w.serve(RobotsMode::Respect);
        assert!(!granted(&mut rx));
        assert_eq!(served.wake, Some(until));
        assert_eq!(w.in_flight, 0);
        // Third-party hosts are not under the circuit breaker.
        let mut cdn = w.ask(CDN, "/lib.js", true);
        w.serve(RobotsMode::Respect);
        assert!(granted(&mut cdn));
        // A failed probe gives the host up: the waiting resource is refused.
        let health = &mut w.hosts.get_mut(SITE).unwrap().health;
        health.resume_now();
        health.on_transient_failure(&retry, now);
        assert!(health.is_given_up());
        w.serve(RobotsMode::Respect);
        assert!(matches!(rx.try_recv(), Ok(PermitAnswer::HostUnavailable)));
        assert_eq!(w.in_flight, 0, "a refused resource takes no slot");
    }

    #[test]
    fn a_host_seen_first_through_a_resource_needs_robots_first() {
        let mut w = World::new(4);
        let host = "http://new.test:80";
        let mut rx = w.ask(host, "/lib.js", true);
        let served = w.serve(RobotsMode::Respect);
        assert_eq!(served.need_robots, [host.to_string()]);
        assert!(!granted(&mut rx));
        let mut h = Host::new(&SpeedSettings::POLITE);
        h.robots = RobotsState::Fetching;
        w.hosts.insert(host.into(), h);
        assert!(w.serve(RobotsMode::Respect).need_robots.is_empty());
        w.hosts.get_mut(host).unwrap().robots = RobotsState::Known(Arc::new(RobotsPolicy::allow_all()), None);
        w.serve(RobotsMode::Respect);
        assert!(granted(&mut rx));
    }

    #[test]
    fn crawl_delay_spaces_resources_and_abandoned_requests_free_nothing() {
        let mut w = World::new(4);
        w.hosts.get_mut(SITE).unwrap().delay = Some(Duration::from_secs(5));
        let mut a = w.ask(SITE, "/a.js", false);
        let mut b = w.ask(SITE, "/b.js", false);
        w.serve(RobotsMode::Respect);
        assert!(granted(&mut a));
        assert!(!granted(&mut b), "crawl-delay: one at a time, 5 s apart");
        // A render that ended: its waiting request is dropped.
        drop(b);
        w.serve(RobotsMode::Respect);
        assert_eq!(w.permits.waiting(), 0);
        assert_eq!(w.in_flight, 1);
    }

    /// The engine `Network` end to end: a permit is asked and answered, the
    /// end is reported, and a cacheable answer is served again from the
    /// cache without a second request.
    #[tokio::test]
    async fn engine_net_asks_for_a_permit_reports_the_end_and_caches() {
        use spiderlens_render::ResourceKind;
        use spiderlens_test_fixtures::js_site::{JS_HOST, JsSiteBuilder};
        let site = JsSiteBuilder::new()
            .script(JS_HOST, "/a.js", "1")
            .cache_control(JS_HOST, "/a.js", "max-age=60")
            .start();
        let mut client = Client::builder().no_proxy();
        for (host, addr) in site.resolve() {
            client = client.resolve(&host, addr);
        }
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let shared = Arc::new(NetShared {
            client: client.build().unwrap(),
            headers: Arc::new(Vec::new()),
            max_body: 1 << 20,
            cache: Mutex::new(ResourceCache::new(super::super::cache::DEFAULT_CAPACITY)),
            send: Box::new(move |m| tx.send(m).is_ok()),
        });
        let coordinator = tokio::spawn(async move {
            let mut done = Vec::new();
            while let Some(m) = rx.recv().await {
                match m {
                    NetMsg::Permit(r) => {
                        let _ = r.reply.send(PermitAnswer::Granted { robots_rule: None });
                    }
                    NetMsg::Done { host_key, signal, .. } => done.push((host_key, signal)),
                }
            }
            done
        });
        let net = EngineNet::new(shared.clone());
        let url = Url::parse(&site.url(JS_HOST, "/a.js")).unwrap();
        let ask = || ResourceRequest {
            url: url.clone(),
            method: "GET".into(),
            headers: vec![("Accept".into(), "*/*".into())],
            body: None,
            kind: ResourceKind::Script,
            third_party: false,
        };
        for _ in 0..2 {
            match net.fetch(ask()).await {
                ResourceAnswer::Response { status, body, .. } => {
                    assert_eq!(status, 200);
                    assert_eq!(body, b"1");
                }
                other => panic!("{other:?}"),
            }
        }
        assert_eq!(
            site.count(JS_HOST, "/a.js"),
            1,
            "the second answer came from the cache"
        );
        assert_eq!(net.take_notes(), [(url.clone(), Note::Cached)]);
        drop(net);
        drop(shared);
        let done = coordinator.await.unwrap();
        assert_eq!(done, [(host_key(&url), Signal::Answered)]);
    }
}
