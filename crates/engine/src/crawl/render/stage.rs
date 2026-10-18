//! The render stage of a running crawl (spec 014 plan D2, D8).
//!
//! The coordinator owns one [`RenderStage`] when the crawl renders. Pages
//! wait in the `renders` table (the queue lives in the file, not in
//! memory); the stage keeps a small window of the next ones, starts up to
//! `limit` render tasks, and each task renders, extracts both versions and
//! compares them off the coordinator, then reports a [`Rendered`].

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use spiderlens_config::{CrawlConfig, UaPreset, UrlFilter, WindowPreset};
use spiderlens_render::{
    BlockSet, Browser, BrowserInfo, InternalHosts, LaunchError, LaunchOptions, MainResponse, Network,
    RenderJob, RenderOutcome, ScreenshotOptions, Window,
};
use tokio::sync::{RwLock, Semaphore};
use url::Url;

use super::guard::{MemoryGuard, MemorySource, SystemMemory, TICK_EVERY};
use super::net::{EngineNet, NetShared, Note};
use crate::crawl::store::{CrawlStore, RenderEntry, Result as StoreResult};
use crate::extract::compare::{Comparison, compare};
use crate::extract::{self, HeaderFacts, PageData, PageInput, body, challenge};

/// Queued renders held in memory at once.
const WINDOW: usize = 64;
/// Tries per page: the first and one retry after a crash (FR-042).
pub(crate) const MAX_ATTEMPTS: u32 = 2;

/// What one render task reports to the coordinator.
pub(crate) struct Rendered {
    /// `attempts` counts this try.
    pub entry: RenderEntry,
    pub outcome: RenderOutcome,
    /// What the engine alone knows about each request, in request order.
    pub notes: Vec<(Url, Note)>,
    pub raw: Option<PageData>,
    /// None when the render produced no HTML.
    pub rendered: Option<Result<PageData, String>>,
    pub comparison: Option<Comparison>,
    pub bot_challenge: bool,
    /// From the stored response headers; the same for both versions.
    pub header_facts: HeaderFacts,
    /// Compressed serialised DOM and its size, when HTML is kept.
    pub html: Option<(Vec<u8>, usize)>,
}

/// The parts of every page's job that come from the settings.
struct JobTemplate {
    user_agent: String,
    window: Window,
    max_time: Duration,
    min_wait: Duration,
    block: BlockSet,
    internal: InternalHosts,
    screenshot: bool,
}

pub(crate) struct RenderStage {
    browser: Arc<RwLock<Browser>>,
    restarting: Arc<AtomicBool>,
    net: Arc<NetShared>,
    template: JobTemplate,
    queue: VecDeque<RenderEntry>,
    last_seq: Option<i64>,
    /// The last refill found every queued row: refill again only after
    /// [`Self::on_enqueued`].
    exhausted: bool,
    /// Render tasks running.
    pub open: u32,
    /// Renders allowed at once.
    pub limit: u32,
    /// Rendering include and exclude patterns, compiled once (FR-005).
    filter: UrlFilter,
    /// Also render internal 4xx and 5xx HTML pages (FR-004).
    error_pages: bool,
    /// Lowers and raises `limit` with memory pressure (FR-037, FR-038).
    guard: MemoryGuard,
    memory: Box<dyn MemorySource>,
    last_tick: Option<Instant>,
    /// The guard asked for a restart between pages (over budget at a limit
    /// of 1, or 1,000 pages since the last one).
    restart_asked: AtomicBool,
    /// A render failed in a way that may mean the browser died: check it
    /// before the next page and start a new one if it is gone (FR-042).
    suspect: Arc<AtomicBool>,
    /// The browser died and could not be started again: the rest of the
    /// queue becomes `browser_unavailable` (FR-046).
    unavailable: Arc<AtomicBool>,
}

/// What happens to a fetched page when the crawl renders (plan D2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Eligibility {
    /// Not a page rendering applies to: no `renders` row.
    No,
    /// Eligible but outside the rendering patterns: `not_rendered`.
    OutsideRules,
    Queue,
}

/// The rendering window the settings ask for (FR-008): `auto` follows the
/// user agent, mobile for a mobile one.
pub(crate) fn window_for(config: &CrawlConfig) -> Window {
    const MOBILE: Window = Window {
        width: 411,
        height: 731,
        mobile: true,
    };
    const DESKTOP: Window = Window {
        width: 1024,
        height: 768,
        mobile: false,
    };
    let w = &config.rendering.window;
    match w.preset {
        WindowPreset::Mobile => MOBILE,
        WindowPreset::Desktop => DESKTOP,
        WindowPreset::Custom => Window {
            width: w.width.unwrap_or(DESKTOP.width),
            height: w.height.unwrap_or(DESKTOP.height),
            mobile: false,
        },
        WindowPreset::Auto => {
            let ua = &config.identity.user_agent;
            let mobile = match ua.preset {
                UaPreset::GooglebotSmartphone | UaPreset::ChromeMobile => true,
                UaPreset::Custom => ua.string().is_some_and(|s| s.contains("Mobile")),
                _ => false,
            };
            if mobile { MOBILE } else { DESKTOP }
        }
    }
}

/// The charset label of a stored `Content-Type` header.
fn charset_of(headers: &[(String, String)]) -> Option<String> {
    let ct = headers
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case("content-type"))?;
    ct.1.split(';').skip(1).find_map(|p| {
        let (k, v) = p.split_once('=')?;
        k.trim()
            .eq_ignore_ascii_case("charset")
            .then(|| v.trim().trim_matches('"').to_string())
    })
}

fn header_values(headers: &[(String, String)], name: &str) -> Vec<String> {
    headers
        .iter()
        .filter(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.clone())
        .collect()
}

impl RenderStage {
    /// Launches the browser. Rendering off never gets here, so a raw crawl
    /// starts no browser.
    pub(crate) async fn start(
        config: &CrawlConfig,
        user_agent: String,
        info: &BrowserInfo,
        launch: LaunchOptions,
        net: Arc<NetShared>,
        internal: InternalHosts,
    ) -> Result<Self, LaunchError> {
        let r = &config.rendering;
        // Validation reports bad patterns before a crawl starts; a file
        // edited by hand falls back to rendering every eligible page.
        let filter = UrlFilter::new(&r.include, &r.exclude)
            .unwrap_or_else(|_| UrlFilter::new(&[], &[]).expect("no patterns"));
        let browser = Browser::launch(info, launch).await?;
        let limit = r
            .concurrency
            .unwrap_or_else(spiderlens_render::default_concurrency)
            .max(1);
        Ok(Self {
            browser: Arc::new(RwLock::new(browser)),
            restarting: Arc::new(AtomicBool::new(false)),
            net,
            template: JobTemplate {
                user_agent,
                window: window_for(config),
                max_time: Duration::from_secs_f64(r.max_render_secs),
                min_wait: Duration::from_secs_f64(r.min_wait_secs),
                block: BlockSet {
                    images: r.block.images,
                    media: r.block.media,
                    fonts: r.block.fonts,
                    stylesheets: r.block.stylesheets,
                    third_party: r.block.third_party,
                },
                internal,
                screenshot: r.screenshots,
            },
            queue: VecDeque::new(),
            last_seq: None,
            exhausted: false,
            open: 0,
            limit,
            filter,
            error_pages: r.render_error_pages,
            guard: MemoryGuard::new(limit, r.memory_budget_mb),
            memory: Box::new(SystemMemory),
            last_tick: None,
            restart_asked: AtomicBool::new(false),
            suspect: Arc::new(AtomicBool::new(false)),
            unavailable: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Whether a fetched page is rendered: internal, not a check, HTML,
    /// 2xx (or 4xx and 5xx when error pages are rendered), and inside the
    /// rendering patterns (FR-004, FR-005).
    pub(crate) fn eligibility(&self, url: &str, internal_page: bool, html: bool, status: u16) -> Eligibility {
        let status_ok = (200..300).contains(&status) || (self.error_pages && status >= 400);
        if !(internal_page && html && status_ok) {
            Eligibility::No
        } else if self.filter.decide(url).crawls() {
            Eligibility::Queue
        } else {
            Eligibility::OutsideRules
        }
    }

    /// A page was queued for rendering after the last refill.
    pub(crate) fn on_enqueued(&mut self) {
        self.exhausted = false;
    }

    /// The next queued render, in queue order.
    pub(crate) fn next(&mut self, store: &CrawlStore) -> StoreResult<Option<RenderEntry>> {
        if self.queue.is_empty() && !self.exhausted {
            let batch = store.renders_after(self.last_seq, WINDOW)?;
            self.exhausted = batch.len() < WINDOW;
            if let Some(last) = batch.last() {
                self.last_seq = Some(last.seq);
            }
            self.queue.extend(batch);
        }
        Ok(self.queue.pop_front())
    }

    /// Whether any render is waiting or running.
    pub(crate) fn busy(&mut self, store: &CrawlStore) -> StoreResult<bool> {
        if self.open > 0 || !self.queue.is_empty() {
            return Ok(true);
        }
        match self.next(store)? {
            Some(e) => {
                self.queue.push_front(e);
                Ok(true)
            }
            None => Ok(false),
        }
    }

    /// A render to try again goes back to the head of the window (its queue
    /// place is earlier than anything refilled since).
    pub(crate) fn retry(&mut self, entry: RenderEntry) {
        self.queue.push_front(entry);
    }

    /// Restarts the browser between pages when a render had to be abandoned.
    /// Renders started meanwhile wait for the new process.
    pub(crate) fn restart_if_needed(&self) {
        let asked = self.restart_asked.swap(false, Ordering::AcqRel);
        let stuck = self.browser.try_read().is_ok_and(|b| b.needs_restart());
        let suspect = self.suspect.swap(false, Ordering::AcqRel);
        if (asked || stuck || suspect) && !self.restarting.swap(true, Ordering::AcqRel) {
            let browser = self.browser.clone();
            let flag = self.restarting.clone();
            let unavailable = self.unavailable.clone();
            // A suspect browser is replaced only if it is gone; one that
            // was asked for (memory, 1,000 pages) or is stuck always is.
            let always = asked || stuck;
            tokio::spawn(async move {
                // Waits for the renders in progress to end.
                let mut b = browser.write().await;
                if always || !b.is_alive().await {
                    // One more try: a first launch can fail while the old
                    // process is still being cleaned up.
                    let mut relaunched = b.restart().await.is_ok();
                    if !relaunched {
                        tokio::time::sleep(Duration::from_millis(500)).await;
                        relaunched = b.restart().await.is_ok();
                    }
                    if !relaunched {
                        unavailable.store(true, Ordering::Release);
                    }
                }
                drop(b);
                flag.store(false, Ordering::Release);
            });
        }
    }

    /// A render failed in a way that may mean the browser is gone.
    pub(crate) fn browser_suspect(&self) {
        self.suspect.store(true, Ordering::Release);
    }

    /// The browser is gone and could not be started again.
    pub(crate) fn unavailable(&self) -> bool {
        self.unavailable.load(Ordering::Acquire)
    }

    /// Once a second: reads memory and lowers or raises the render limit
    /// (plan D10). Called from the coordinator's loop; cheap between ticks.
    pub(crate) fn guard_tick(&mut self) {
        let now = Instant::now();
        if self.last_tick.is_some_and(|t| now.duration_since(t) < TICK_EVERY) {
            return;
        }
        self.last_tick = Some(now);
        let browser_bytes = self.browser.try_read().map_or(0, |b| b.memory_bytes());
        let reading = self.memory.read(browser_bytes);
        let decision = self.guard.tick(reading, now);
        self.limit = decision.limit;
        if decision.restart {
            self.restart_asked.store(true, Ordering::Release);
        }
    }

    /// Memory holds the render limit below what the settings allow: the
    /// limiting reason `memory_guard` (plan D13).
    #[allow(dead_code)] // read by the progress line (batch A)
    pub(crate) fn memory_limited(&self) -> bool {
        self.guard.limited()
    }

    /// A render ended: every 1,000 pages the browser restarts to cap leaks.
    pub(crate) fn page_done(&mut self) {
        if self.guard.page_done() {
            self.restart_asked.store(true, Ordering::Release);
        }
    }

    /// Starts one render task.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn spawn(
        &mut self,
        entry: RenderEntry,
        status: u16,
        headers: Vec<(String, String)>,
        raw_body: Vec<u8>,
        token: String,
        store_html: bool,
        budget: Arc<Semaphore>,
        report: impl FnOnce(Rendered) + Send + 'static,
    ) {
        let Ok(url) = Url::parse(&entry.url) else {
            return;
        };
        let t = &self.template;
        let main = MainResponse {
            status,
            headers: headers.clone(),
            body: raw_body.clone(),
        };
        let mut job = RenderJob::new(url.clone(), main, t.user_agent.clone(), t.window);
        job.max_time = t.max_time;
        job.min_wait = t.min_wait;
        job.block = t.block;
        job.internal_hosts = t.internal.clone();
        job.screenshot = t.screenshot.then(ScreenshotOptions::default);
        let internal = t.internal.clone();
        let net = Arc::new(EngineNet::new(self.net.clone()));
        let browser = self.browser.clone();
        self.open += 1;
        tokio::spawn(async move {
            let outcome = {
                let b = browser.read().await;
                b.render(job, net.clone() as Arc<dyn Network>).await
            };
            let notes = net.take_notes();
            let header_facts = extract::header_facts(
                &header_values(&headers, "x-robots-tag"),
                &header_values(&headers, "link"),
            );
            let html = outcome.html.clone();
            let kib =
                ((raw_body.len() + html.as_ref().map_or(0, String::len)) / 1024 + 1).min(64 * 1024) as u32;
            let permit = budget.acquire_many(kib).await;
            let charset = charset_of(&headers);
            let page_url = url.clone();
            let work = tokio::task::spawn_blocking(move || {
                let input = |body: &[u8], charset: Option<&str>| {
                    extract::extract_page_caught(
                        &PageInput {
                            url: &page_url,
                            charset,
                            body,
                            truncated: false,
                            robots_token: &token,
                            content: None,
                            keep_text: true,
                            a11y: false,
                            hook: None,
                        },
                        None,
                    )
                };
                let raw = input(&raw_body, charset.as_deref()).ok();
                let rendered = html.as_deref().map(|h| input(h.as_bytes(), Some("utf-8")));
                let comparison = match (&raw, &rendered) {
                    (Some(raw), Some(Ok(r))) => Some(compare(raw, r, &token, &*internal)),
                    _ => None,
                };
                let bot_challenge = match (&raw, &rendered, &html) {
                    (Some(raw), Some(Ok(r)), Some(h)) => challenge::is_bot_challenge(h, r, raw),
                    _ => false,
                };
                let packed = if store_html {
                    html.as_deref()
                        .and_then(|h| body::compress(h.as_bytes()).ok().map(|p| (p, h.len())))
                } else {
                    None
                };
                (raw, rendered, comparison, bot_challenge, packed)
            });
            let (raw, rendered, comparison, bot_challenge, html) = work.await.unwrap_or_else(|e| {
                (
                    None,
                    Some(Err(format!("Page extraction failed: {e}"))),
                    None,
                    false,
                    None,
                )
            });
            drop(permit);
            report(Rendered {
                entry,
                outcome,
                notes,
                raw,
                rendered,
                comparison,
                bot_challenge,
                header_facts,
                html,
            });
        });
    }

    /// Closes the browser and deletes its profile. Open tasks (none at the
    /// end of a run) would keep it alive until they end; it is then closed
    /// on drop.
    pub(crate) async fn close(self) {
        if let Ok(lock) = Arc::try_unwrap(self.browser) {
            lock.into_inner().close().await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spiderlens_config::RenderWindow;

    #[test]
    fn the_window_follows_the_settings_and_the_user_agent() {
        let mut c = CrawlConfig::default();
        assert!(!window_for(&c).mobile, "SpiderLens is a desktop agent");
        c.identity.user_agent.preset = UaPreset::GooglebotSmartphone;
        assert_eq!(window_for(&c).width, 411);
        assert!(window_for(&c).mobile);
        c.rendering.window = RenderWindow {
            preset: WindowPreset::Custom,
            width: Some(1440),
            height: Some(900),
        };
        let w = window_for(&c);
        assert_eq!((w.width, w.height, w.mobile), (1440, 900, false));
        c.rendering.window.preset = WindowPreset::Desktop;
        assert_eq!(window_for(&c).width, 1024);
    }

    #[test]
    fn charset_and_header_values_come_from_the_stored_headers() {
        let h = vec![
            (
                "Content-Type".to_string(),
                "text/html; charset=\"ISO-8859-1\"".to_string(),
            ),
            ("link".to_string(), "<a>; rel=canonical".to_string()),
            ("Link".to_string(), "<b>; rel=next".to_string()),
        ];
        assert_eq!(charset_of(&h).as_deref(), Some("ISO-8859-1"));
        assert_eq!(header_values(&h, "link").len(), 2);
        assert!(charset_of(&[]).is_none());
    }
}
