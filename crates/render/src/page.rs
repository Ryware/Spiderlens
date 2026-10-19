//! Rendering one page (spec 014 plan D1 "Per page", D3, D4; FR-007 to
//! FR-014, FR-042, FR-043; SC-007).
//!
//! A fresh browser context per page. `Fetch` pauses every request: the main
//! document is fulfilled from [`RenderJob::main`], blocked types fail with
//! `BlockedByClient`, and every other request is answered by the caller's
//! [`Network`]. After `load`, the settle rule decides when the page is done;
//! the DOM is then serialised with open shadow roots and same-origin iframes
//! flattened. Every problem becomes a [`RenderState`], never a panic.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use base64::Engine as _;
use chromiumoxide::Page;
use chromiumoxide::cdp::browser_protocol::browser::{
    BrowserContextId, SetDownloadBehaviorBehavior, SetDownloadBehaviorParams,
};
use chromiumoxide::cdp::browser_protocol::emulation::{
    SetDeviceMetricsOverrideParams, SetLocaleOverrideParams, SetTimezoneOverrideParams,
    SetTouchEmulationEnabledParams, SetUserAgentOverrideParams,
};
use chromiumoxide::cdp::browser_protocol::fetch::{
    EventRequestPaused, FailRequestParams, FulfillRequestParams, HeaderEntry,
};
use chromiumoxide::cdp::browser_protocol::inspector::{EnableParams as InspectorEnable, EventTargetCrashed};
use chromiumoxide::cdp::browser_protocol::network::{CookieParam, ErrorReason, ResourceType, TimeSinceEpoch};
use chromiumoxide::cdp::browser_protocol::page::{
    AddScriptToEvaluateOnNewDocumentParams, CaptureScreenshotFormat, CaptureScreenshotParams,
    CreateIsolatedWorldParams, EventJavascriptDialogOpening, EventLoadEventFired,
    HandleJavaScriptDialogParams, NavigateParams, Viewport,
};
use chromiumoxide::cdp::browser_protocol::storage::SetCookiesParams;
use chromiumoxide::cdp::browser_protocol::target::{
    CreateBrowserContextParams, CreateTargetParams, DisposeBrowserContextParams,
};
use chromiumoxide::cdp::js_protocol::runtime::{
    AddBindingParams, ConsoleApiCalledType, EvaluateParams, EventBindingCalled, EventConsoleApiCalled,
    EventExceptionThrown, GetHeapUsageParams, RemoteObject,
};
use futures_util::StreamExt;
use tokio::sync::mpsc;
use tokio::task::JoinSet;
use url::Url;

use crate::settle::{Event, Settle, Status};
use crate::types::*;

const FLATTEN_JS: &str = include_str!("flatten.js");
const INIT_JS: &str = include_str!("init.js");
const MUTATION_BINDING: &str = "__spiderlensMutation";

/// The window height grows to the document height up to this (FR-009).
pub const MAX_WINDOW_HEIGHT: u32 = 12_000;
/// Console entries kept per page (FR-043); totals count all.
pub const MAX_CONSOLE: usize = 50;
/// Longest console message kept, in characters.
pub const MAX_CONSOLE_MESSAGE: usize = 2_000;
/// Device scale factor of the Googlebot smartphone window.
const MOBILE_SCALE: f64 = 2.625;
/// Extra time after the cap before `render` gives up on the browser (SC-007).
pub const HARD_STOP_MARGIN: Duration = Duration::from_secs(2);
/// A crash with the heap above this share of its limit is "page too large".
const HEAP_NEAR_LIMIT: f64 = 0.8;

/// The resource type the browser reported.
pub(crate) fn kind_of(t: &ResourceType) -> ResourceKind {
    match t {
        ResourceType::Document => ResourceKind::Document,
        ResourceType::Stylesheet => ResourceKind::Stylesheet,
        ResourceType::Image => ResourceKind::Image,
        ResourceType::Media | ResourceType::TextTrack => ResourceKind::Media,
        ResourceType::Font => ResourceKind::Font,
        ResourceType::Script => ResourceKind::Script,
        ResourceType::Xhr => ResourceKind::Xhr,
        ResourceType::Fetch | ResourceType::EventSource => ResourceKind::Fetch,
        ResourceType::Ping => ResourceKind::Ping,
        ResourceType::CspViolationReport => ResourceKind::CspViolationReport,
        _ => ResourceKind::Other,
    }
}

/// Whether a request is failed with `BlockedByClient` without a network
/// call (FR-014, `RenderJob::load_types`).
pub(crate) fn is_blocked(job: &RenderJob, kind: ResourceKind, third_party: bool) -> bool {
    if third_party && job.block.third_party {
        return true;
    }
    if let Some(types) = &job.load_types {
        return !types.contains(&kind);
    }
    match kind {
        ResourceKind::Image => job.block.images,
        ResourceKind::Media => job.block.media,
        ResourceKind::Font => job.block.fonts,
        ResourceKind::Stylesheet => job.block.stylesheets,
        _ => false,
    }
}

/// The browser's own favicon request (resource type `Other`, never
/// initiated by the page's scripts or markup loads we track).
pub(crate) fn is_browser_favicon(url: &Url, kind: ResourceKind) -> bool {
    kind == ResourceKind::Other && url.path().eq_ignore_ascii_case("/favicon.ico")
}

/// A fire-and-forget request (analytics hits): it never holds a page open
/// when it goes to a third party (FR-010).
pub(crate) fn is_beacon(method: &str, kind: ResourceKind) -> bool {
    method.eq_ignore_ascii_case("POST")
        && !matches!(
            kind,
            ResourceKind::Document | ResourceKind::Script | ResourceKind::Stylesheet
        )
}

/// Response headers as the browser gets them: the body is already decoded
/// and complete, so encoding and length headers would be wrong.
pub(crate) fn fulfil_headers(headers: &[(String, String)]) -> Vec<HeaderEntry> {
    headers
        .iter()
        .filter(|(n, _)| {
            !n.eq_ignore_ascii_case("content-encoding")
                && !n.eq_ignore_ascii_case("content-length")
                && !n.eq_ignore_ascii_case("transfer-encoding")
        })
        .map(|(n, v)| HeaderEntry::new(n.clone(), v.clone()))
        .collect()
}

fn error_reason(f: FailReason) -> ErrorReason {
    match f {
        FailReason::BlockedByClient => ErrorReason::BlockedByClient,
        FailReason::Failed => ErrorReason::Failed,
        FailReason::TimedOut => ErrorReason::TimedOut,
        FailReason::ConnectionRefused => ErrorReason::ConnectionRefused,
        FailReason::NameNotResolved => ErrorReason::NameNotResolved,
        FailReason::Aborted => ErrorReason::Aborted,
    }
}

/// Console entries of one page: the first [`MAX_CONSOLE`] kept, all counted.
#[derive(Default)]
pub(crate) struct Console {
    pub entries: Vec<ConsoleEntry>,
    pub errors: u32,
    pub warnings: u32,
}

impl Console {
    pub fn push(&mut self, level: ConsoleLevel, message: String, source: Option<(String, u32, u32)>) {
        match level {
            ConsoleLevel::Warning => self.warnings += 1,
            ConsoleLevel::Error | ConsoleLevel::Exception => self.errors += 1,
        }
        if self.entries.len() < MAX_CONSOLE {
            let message = match message.char_indices().nth(MAX_CONSOLE_MESSAGE) {
                Some((at, _)) => message[..at].to_owned(),
                None => message,
            };
            self.entries.push(ConsoleEntry {
                level,
                message,
                source,
            });
        }
    }
}

fn describe(o: &RemoteObject) -> String {
    match &o.value {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(v) => v.to_string(),
        None => o.description.clone().unwrap_or_default(),
    }
}

/// What an answering task reports back to the render loop.
struct Answered {
    id: String,
    record: Option<ResourceRecord>,
}

/// Renders one page. Returns within `job.max_time` plus [`HARD_STOP_MARGIN`]
/// even when the browser hangs; then `stuck` is set and the caller should
/// restart the browser.
pub(crate) async fn render(
    cdp: &chromiumoxide::Browser,
    oom: &AtomicU64,
    job: RenderJob,
    net: Arc<dyn Network>,
    heap_limit_mb: u32,
) -> (RenderOutcome, bool) {
    let started = Instant::now();
    let oom_at_start = oom.load(Ordering::Relaxed);
    let deadline = tokio::time::Instant::now() + job.max_time + HARD_STOP_MARGIN;
    let window = job.window;
    let created = tokio::time::timeout_at(
        deadline,
        cdp.execute(CreateBrowserContextParams {
            dispose_on_detach: Some(true),
            proxy_server: job.proxy.clone(),
            ..Default::default()
        }),
    )
    .await;
    let context = match created {
        Ok(Ok(r)) => r.result.browser_context_id,
        Ok(Err(_)) => {
            return (
                RenderOutcome::failed(FailedReason::RendererError, true, window),
                false,
            );
        }
        Err(_) => {
            return (
                RenderOutcome::failed(FailedReason::RendererError, true, window),
                true,
            );
        }
    };
    let result = tokio::time::timeout_at(
        deadline,
        render_in(cdp, oom, &job, net, context.clone(), heap_limit_mb, started),
    )
    .await;
    // Closing the context closes its page and every request it holds. When
    // the hard stop already passed, give it a moment only.
    let grace = deadline
        .saturating_duration_since(tokio::time::Instant::now())
        .max(Duration::from_millis(300));
    let disposed = tokio::time::timeout(grace, cdp.execute(DisposeBrowserContextParams::new(context)))
        .await
        .is_ok_and(|r| r.is_ok());
    match result {
        Ok(mut out) => {
            out.render_ms = started.elapsed().as_millis().min(u128::from(u32::MAX)) as u32;
            (out, !disposed)
        }
        Err(_) => {
            // A renderer out of heap may hang instead of crashing; V8's
            // report on stderr still tells what happened.
            let mut out = if oom.load(Ordering::Relaxed) != oom_at_start {
                RenderOutcome::failed(FailedReason::PageTooLarge, false, window)
            } else {
                RenderOutcome::failed(FailedReason::RendererError, true, window)
            };
            out.render_ms = started.elapsed().as_millis().min(u128::from(u32::MAX)) as u32;
            (out, true)
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn render_in(
    cdp: &chromiumoxide::Browser,
    oom: &AtomicU64,
    job: &RenderJob,
    net: Arc<dyn Network>,
    context: BrowserContextId,
    heap_limit_mb: u32,
    started: Instant,
) -> RenderOutcome {
    let window = job.window;
    let fail = |reason, retryable| RenderOutcome::failed(reason, retryable, window);
    let mut target = CreateTargetParams::new("about:blank");
    target.browser_context_id = Some(context.clone());
    let Ok(page) = cdp.new_page(target).await else {
        return fail(FailedReason::RendererError, true);
    };
    macro_rules! listen {
        ($t:ty) => {
            match page.event_listener::<$t>().await {
                Ok(l) => l,
                Err(_) => return fail(FailedReason::RendererError, true),
            }
        };
    }
    let mut paused = listen!(EventRequestPaused);
    let mut loaded = listen!(EventLoadEventFired);
    let mut mutations = listen!(EventBindingCalled);
    let mut console_events = listen!(EventConsoleApiCalled);
    let mut exceptions = listen!(EventExceptionThrown);
    let mut dialogs = listen!(EventJavascriptDialogOpening);
    let mut crashed = listen!(EventTargetCrashed);

    if setup(&page, cdp, job, &context).await.is_err() {
        return fail(FailedReason::RendererError, true);
    }
    let main_frame = page
        .mainframe()
        .await
        .ok()
        .flatten()
        .map(|f| f.as_ref().to_owned());

    // Heap use, polled in the background, tells a crash from a page that
    // ran out of memory.
    let heap_used = Arc::new(AtomicU64::new(0));
    let oom_before = oom.load(Ordering::Relaxed);
    let heap_poll = {
        let (page, heap_used) = (page.clone(), heap_used.clone());
        tokio::spawn(async move {
            loop {
                if let Ok(r) = page.execute(GetHeapUsageParams::default()).await {
                    heap_used.store(r.result.used_size as u64, Ordering::Relaxed);
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        })
    };

    let nav_start = Instant::now();
    let mut settle = Settle::new(job.min_wait, job.max_time);
    let mut console = Console::default();
    let mut records: Vec<(String, ResourceRecord)> = Vec::new();
    let mut js_redirect: Option<Url> = None;
    let mut main_answered = false;
    let mut nav_error: Option<String> = None;
    let mut crash = false;
    let mut tasks: JoinSet<()> = JoinSet::new();
    let (tx, mut rx) = mpsc::unbounded_channel::<Answered>();
    let navigate = page.execute(NavigateParams::new(job.url.as_str()));
    tokio::pin!(navigate);
    let mut navigated = false;
    let status = loop {
        let now = nav_start.elapsed();
        let wake = match settle.poll(now) {
            Status::Waiting { until } => until,
            done => break done,
        };
        let sleep = tokio::time::sleep(wake.saturating_sub(now));
        tokio::select! {
            r = &mut navigate, if !navigated => {
                navigated = true;
                match r {
                    Ok(r) => nav_error = r.result.error_text.clone(),
                    Err(e) => nav_error = Some(e.to_string()),
                }
                // A main document that never loads cannot settle.
                if nav_error.is_some() && js_redirect.is_none() {
                    break Status::TimedOut;
                }
            }
            Some(ev) = paused.next() => {
                let now = nav_start.elapsed();
                let id = ev.request_id.as_ref().to_owned();
                let kind = kind_of(&ev.resource_type);
                let is_main_frame = main_frame.as_deref().is_none_or(|f| f == ev.frame_id.as_ref());
                if kind == ResourceKind::Document && is_main_frame {
                    if !main_answered {
                        main_answered = true;
                        settle.on(now, Event::RequestStart { id: id.clone(), kind, third_party: false, beacon: false });
                        let (page, tx, main) = (page.clone(), tx.clone(), job.main.clone());
                        tasks.spawn(async move {
                            let mut p = FulfillRequestParams::new(ev.request_id.clone(), i64::from(main.status));
                            p.response_headers = Some(fulfil_headers(&main.headers));
                            p.body = Some(base64::engine::general_purpose::STANDARD.encode(&main.body).into());
                            let _ = page.execute(p).await;
                            let _ = tx.send(Answered { id, record: None });
                        });
                    } else {
                        // A script navigated the main frame: recorded, not
                        // followed (FR-013). The crawl discovers the target.
                        if js_redirect.is_none() {
                            js_redirect = Url::parse(&ev.request.url).ok();
                            // A navigation started before `load` (a
                            // `location.replace` in the head) means `load`
                            // never comes: the page settles from here.
                            settle.on(now, Event::Load);
                        }
                        let page = page.clone();
                        tasks.spawn(async move {
                            let _ = page.execute(FailRequestParams::new(ev.request_id.clone(), ErrorReason::Aborted)).await;
                        });
                    }
                    continue;
                }
                let url = match Url::parse(&ev.request.url) {
                    Ok(u) if matches!(u.scheme(), "http" | "https") => u,
                    _ => {
                        let page = page.clone();
                        tasks.spawn(async move {
                            let _ = page.execute(FailRequestParams::new(ev.request_id.clone(), ErrorReason::Failed)).await;
                        });
                        continue;
                    }
                };
                // The browser fetches the favicon on its own, at a time of its
                // choosing; a search renderer never does, so it is neither
                // fetched nor recorded (keeps the resource list deterministic).
                if is_browser_favicon(&url, kind) {
                    let page = page.clone();
                    tasks.spawn(async move {
                        let _ = page.execute(FailRequestParams::new(ev.request_id.clone(), ErrorReason::BlockedByClient)).await;
                    });
                    continue;
                }
                let third_party = !(job.internal_hosts)(&url);
                let method = ev.request.method.clone();
                let beacon = is_beacon(&method, kind);
                settle.on(now, Event::RequestStart { id: id.clone(), kind, third_party, beacon });
                let blocked = is_blocked(job, kind, third_party);
                let record = ResourceRecord {
                    url: url.clone(),
                    kind,
                    third_party,
                    status: None,
                    failed: None,
                    bytes: 0,
                };
                // In request order; filled in when answered.
                records.push((id.clone(), record.clone()));
                let (page, tx, net) = (page.clone(), tx.clone(), net.clone());
                tasks.spawn(async move {
                    let mut record = record;
                    let answer = if blocked {
                        ResourceAnswer::Fail { reason: FailReason::BlockedByClient }
                    } else {
                        let headers = ev
                            .request
                            .headers
                            .inner()
                            .as_object()
                            .map(|m| {
                                m.iter()
                                    .map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_owned()))
                                    .collect()
                            })
                            .unwrap_or_default();
                        let body = ev.request.post_data_entries.as_ref().map(|entries| {
                            entries
                                .iter()
                                .filter_map(|e| e.bytes.as_ref())
                                .filter_map(|b| {
                                    let s: &str = b.as_ref();
                                    base64::engine::general_purpose::STANDARD.decode(s).ok()
                                })
                                .flatten()
                                .collect()
                        });
                        net.fetch(ResourceRequest { url, method, headers, body, kind, third_party }).await
                    };
                    match answer {
                        ResourceAnswer::Response { status, headers, body } => {
                            record.status = Some(status);
                            record.bytes = body.len() as u64;
                            let mut p = FulfillRequestParams::new(ev.request_id.clone(), i64::from(status));
                            p.response_headers = Some(fulfil_headers(&headers));
                            p.body = Some(base64::engine::general_purpose::STANDARD.encode(&body).into());
                            let _ = page.execute(p).await;
                        }
                        ResourceAnswer::Fail { reason } => {
                            record.failed = Some(reason);
                            let _ = page
                                .execute(FailRequestParams::new(ev.request_id.clone(), error_reason(reason)))
                                .await;
                        }
                    }
                    let _ = tx.send(Answered { id, record: Some(record) });
                });
            }
            Some(done) = rx.recv() => {
                settle.on(nav_start.elapsed(), Event::RequestEnd { id: done.id.clone() });
                if let Some(record) = done.record
                    && let Some(slot) = records.iter_mut().rev().find(|(id, _)| *id == done.id)
                {
                    slot.1 = record;
                }
            }
            Some(_) = loaded.next() => {
                settle.on(nav_start.elapsed(), Event::Load);
                // Grow the window to the document so lazy content in view
                // loads (FR-009). In the background: a busy page must not
                // stall the answering of its requests.
                let page = page.clone();
                tasks.spawn(async move { extend_height(&page, window).await; });
            }
            Some(_) = mutations.next() => settle.on(nav_start.elapsed(), Event::Mutation),
            Some(ev) = console_events.next() => {
                let level = match ev.r#type {
                    ConsoleApiCalledType::Error | ConsoleApiCalledType::Assert => Some(ConsoleLevel::Error),
                    ConsoleApiCalledType::Warning => Some(ConsoleLevel::Warning),
                    _ => None,
                };
                if let Some(level) = level {
                    let message = ev.args.iter().map(describe).collect::<Vec<_>>().join(" ");
                    let source = ev
                        .stack_trace
                        .as_ref()
                        .and_then(|s| s.call_frames.first())
                        .map(|f| (f.url.clone(), f.line_number as u32 + 1, f.column_number as u32 + 1));
                    console.push(level, message, source);
                }
            }
            Some(ev) = exceptions.next() => {
                let d = &ev.exception_details;
                let message = d
                    .exception
                    .as_ref()
                    .and_then(|e| e.description.clone())
                    .unwrap_or_else(|| d.text.clone());
                let source = d.url.clone().map(|u| (u, d.line_number as u32 + 1, d.column_number as u32 + 1));
                console.push(ConsoleLevel::Exception, message, source);
            }
            Some(_) = dialogs.next() => {
                let page = page.clone();
                tasks.spawn(async move {
                    let _ = page.execute(HandleJavaScriptDialogParams::new(false)).await;
                });
            }
            Some(_) = crashed.next() => {
                crash = true;
                break Status::TimedOut;
            }
            _ = sleep => {}
        }
    };
    heap_poll.abort();

    if crash {
        let used = heap_used.load(Ordering::Relaxed) as f64;
        let limit = f64::from(heap_limit_mb) * 1024.0 * 1024.0;
        tasks.abort_all();
        // A script that allocates fast fills the heap between two polls (and
        // blocks them); V8's own report on stderr comes just before the crash.
        let reported = tokio::time::timeout(Duration::from_millis(300), async {
            while oom.load(Ordering::Relaxed) == oom_before {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .is_ok();
        return if reported || used >= limit * HEAP_NEAR_LIMIT {
            fail(FailedReason::PageTooLarge, false)
        } else {
            fail(FailedReason::Crash, true)
        };
    }
    if !main_answered || (nav_error.is_some() && js_redirect.is_none()) {
        tasks.abort_all();
        let mut out = fail(FailedReason::NavigationError, false);
        out.console = console.entries;
        out.console_totals = (console.errors, console.warnings);
        return out;
    }
    let html = match page.evaluate(FLATTEN_JS).await.map(|r| r.into_value::<String>()) {
        Ok(Ok(html)) => html,
        _ => {
            tasks.abort_all();
            return fail(FailedReason::RendererError, true);
        }
    };
    let screenshot = match job.screenshot {
        Some(opts) => screenshot(&page, window, opts).await,
        None => None,
    };
    // Probes run last, so they can never change `html` or `screenshot`, and
    // while the page is still open.
    let probes = run_probes(&page, job).await;
    tasks.abort_all();
    let _ = started;
    RenderOutcome {
        state: if status == Status::Settled {
            RenderState::Rendered
        } else {
            RenderState::TimedOut
        },
        html: Some(html),
        js_redirect,
        console: console.entries,
        console_totals: (console.errors, console.warnings),
        resources: records.into_iter().map(|(_, r)| r).collect(),
        render_ms: 0,
        window,
        screenshot,
        probes,
    }
}

/// Runs the job's probes in order, each in its own isolated world on the main
/// frame. Each probe is bounded by its own budget and by what is left of
/// `probes_max_total`; once that is spent the rest report `TimedOut { spent: 0 }`.
async fn run_probes(page: &Page, job: &RenderJob) -> Vec<(String, ProbeOutcome)> {
    let mut out = Vec::with_capacity(job.probes.len());
    if job.probes.is_empty() {
        return out;
    }
    let begun = Instant::now();
    for probe in &job.probes {
        let remaining = job.probes_max_total.saturating_sub(begun.elapsed());
        if remaining.is_zero() {
            out.push((
                probe.name.clone(),
                ProbeOutcome::TimedOut {
                    spent: Duration::ZERO,
                },
            ));
            continue;
        }
        let budget = probe.budget.min(remaining);
        let started = Instant::now();
        let outcome = match tokio::time::timeout(budget, run_probe(page, probe)).await {
            Ok(outcome) => outcome,
            Err(_) => ProbeOutcome::TimedOut {
                spent: started.elapsed(),
            },
        };
        out.push((probe.name.clone(), outcome));
    }
    out
}

async fn run_probe(page: &Page, probe: &PageProbe) -> ProbeOutcome {
    let Some(frame) = page.mainframe().await.ok().flatten() else {
        return ProbeOutcome::Failed("no main frame".into());
    };
    let mut world = CreateIsolatedWorldParams::new(frame);
    world.world_name = Some(format!("spiderlens-probe-{}", probe.name));
    let context_id = match page.execute(world).await {
        Ok(r) => r.result.execution_context_id,
        Err(e) => return ProbeOutcome::Failed(format!("isolated world: {e}")),
    };
    let expression = format!(
        "(async () => JSON.stringify(await ({})({})))()",
        probe.script, probe.args_json
    );
    let mut eval = EvaluateParams::new(expression);
    eval.context_id = Some(context_id);
    eval.await_promise = Some(true);
    eval.return_by_value = Some(true);
    let r = match page.execute(eval).await {
        Ok(r) => r,
        Err(e) => return ProbeOutcome::Failed(e.to_string()),
    };
    if let Some(d) = &r.result.exception_details {
        let message = d
            .exception
            .as_ref()
            .and_then(|e| e.description.clone())
            .unwrap_or_else(|| d.text.clone());
        return ProbeOutcome::Failed(message);
    }
    match r.result.result.value.as_ref().and_then(|v| v.as_str()) {
        Some(json) if json.len() > PROBE_RESULT_MAX_BYTES => ProbeOutcome::TooLarge,
        Some(json) => ProbeOutcome::Ok(json.to_owned()),
        None => ProbeOutcome::Failed("result is not JSON".into()),
    }
}

/// Everything set on the page before it navigates.
async fn setup(
    page: &Page,
    cdp: &chromiumoxide::Browser,
    job: &RenderJob,
    context: &BrowserContextId,
) -> chromiumoxide::error::Result<()> {
    let w = job.window;
    page.execute(AddBindingParams::new(MUTATION_BINDING)).await?;
    page.execute(AddScriptToEvaluateOnNewDocumentParams::new(INIT_JS))
        .await?;
    page.execute(InspectorEnable::default()).await?;
    page.execute(metrics(w, w.height)).await?;
    if w.mobile {
        page.execute(SetTouchEmulationEnabledParams::new(true)).await?;
    }
    let mut ua = SetUserAgentOverrideParams::new(job.user_agent.clone());
    ua.accept_language = Some("en-US".into());
    page.execute(ua).await?;
    page.execute(SetTimezoneOverrideParams::new("UTC")).await?;
    page.execute(SetLocaleOverrideParams {
        locale: Some("en-US".into()),
    })
    .await?;
    let mut downloads = SetDownloadBehaviorParams::new(SetDownloadBehaviorBehavior::Deny);
    downloads.browser_context_id = Some(context.clone());
    cdp.execute(downloads).await?;
    if !job.cookies.is_empty() {
        let cookies = job
            .cookies
            .iter()
            .map(|c| {
                let mut p = CookieParam::new(c.name.clone(), c.value.clone());
                p.domain = Some(c.domain.clone());
                p.path = Some(c.path.clone());
                p.secure = Some(c.secure);
                p.http_only = Some(c.http_only);
                p.expires = c.expires.map(TimeSinceEpoch::new);
                p
            })
            .collect();
        let mut set = SetCookiesParams::new(cookies);
        set.browser_context_id = Some(context.clone());
        cdp.execute(set).await?;
    }
    Ok(())
}

fn metrics(w: Window, height: u32) -> SetDeviceMetricsOverrideParams {
    let scale = if w.mobile { MOBILE_SCALE } else { 1.0 };
    SetDeviceMetricsOverrideParams::new(i64::from(w.width), i64::from(height), scale, w.mobile)
}

const DOC_HEIGHT_JS: &str = "Math.max(document.documentElement ? document.documentElement.scrollHeight : 0, \
                             document.body ? document.body.scrollHeight : 0)";

async fn document_height(page: &Page) -> Option<u32> {
    let h = page
        .evaluate(DOC_HEIGHT_JS)
        .await
        .ok()?
        .into_value::<f64>()
        .ok()?;
    Some(h.max(0.0).min(f64::from(u32::MAX)) as u32)
}

async fn extend_height(page: &Page, w: Window) {
    if let Some(h) = document_height(page).await {
        let h = h.min(MAX_WINDOW_HEIGHT);
        if h > w.height {
            let _ = page.execute(metrics(w, h)).await;
        }
    }
}

async fn screenshot(page: &Page, w: Window, opts: ScreenshotOptions) -> Option<Screenshot> {
    let height = document_height(page)
        .await
        .unwrap_or(w.height)
        .clamp(1, opts.max_height);
    let p = CaptureScreenshotParams {
        format: Some(CaptureScreenshotFormat::Webp),
        quality: Some(i64::from(opts.quality)),
        capture_beyond_viewport: Some(true),
        clip: Some(Viewport {
            x: 0.0,
            y: 0.0,
            width: f64::from(w.width),
            height: f64::from(height),
            scale: 1.0,
        }),
        ..Default::default()
    };
    let r = page.execute(p).await.ok()?;
    let data: &str = r.result.data.as_ref();
    let webp = base64::engine::general_purpose::STANDARD.decode(data).ok()?;
    let scale = if w.mobile { MOBILE_SCALE } else { 1.0 };
    Some(Screenshot {
        webp,
        width: (f64::from(w.width) * scale) as u32,
        height: (f64::from(height) * scale) as u32,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job() -> RenderJob {
        let main = MainResponse {
            status: 200,
            headers: Vec::new(),
            body: Vec::new(),
        };
        let w = Window {
            width: 1024,
            height: 768,
            mobile: false,
        };
        RenderJob::new(Url::parse("http://js.test/").unwrap(), main, "UA".into(), w)
    }

    #[test]
    fn only_the_browser_favicon_request_is_skipped() {
        let u = |s: &str| Url::parse(s).unwrap();
        assert!(is_browser_favicon(
            &u("http://js.test/favicon.ico"),
            ResourceKind::Other
        ));
        assert!(!is_browser_favicon(
            &u("http://js.test/favicon.ico"),
            ResourceKind::Image
        ));
        assert!(!is_browser_favicon(
            &u("http://js.test/img/favicon.ico"),
            ResourceKind::Other
        ));
        assert!(!is_browser_favicon(
            &u("http://js.test/data.json"),
            ResourceKind::Other
        ));
    }

    #[test]
    fn default_blocking_is_images_media_and_fonts() {
        let j = job();
        for k in [ResourceKind::Image, ResourceKind::Media, ResourceKind::Font] {
            assert!(is_blocked(&j, k, false), "{k:?}");
        }
        for k in [
            ResourceKind::Script,
            ResourceKind::Stylesheet,
            ResourceKind::Xhr,
            ResourceKind::Fetch,
            ResourceKind::Document,
        ] {
            assert!(!is_blocked(&j, k, false), "{k:?}");
            assert!(!is_blocked(&j, k, true), "{k:?} third party");
        }
    }

    #[test]
    fn third_party_and_load_types_block() {
        let mut j = job();
        j.block.third_party = true;
        assert!(is_blocked(&j, ResourceKind::Script, true));
        assert!(!is_blocked(&j, ResourceKind::Script, false));
        j.load_types = Some(vec![ResourceKind::Script, ResourceKind::Image]);
        assert!(!is_blocked(&j, ResourceKind::Image, false));
        assert!(is_blocked(&j, ResourceKind::Xhr, false));
        // block.third_party still applies.
        assert!(is_blocked(&j, ResourceKind::Script, true));
        j.block.stylesheets = true;
        j.load_types = None;
        assert!(is_blocked(&j, ResourceKind::Stylesheet, false));
    }

    #[test]
    fn beacons_are_posts_of_non_document_types() {
        assert!(is_beacon("POST", ResourceKind::Ping));
        assert!(is_beacon("post", ResourceKind::Xhr));
        assert!(!is_beacon("GET", ResourceKind::Xhr));
        assert!(!is_beacon("POST", ResourceKind::Document));
    }

    #[test]
    fn decoded_bodies_drop_encoding_and_length_headers() {
        let h: Vec<(String, String)> = [
            ("Content-Type", "text/html"),
            ("Content-Encoding", "gzip"),
            ("content-length", "10"),
            ("Transfer-Encoding", "chunked"),
            ("Set-Cookie", "a=1"),
        ]
        .iter()
        .map(|(n, v)| (n.to_string(), v.to_string()))
        .collect();
        let names: Vec<String> = fulfil_headers(&h).into_iter().map(|e| e.name).collect();
        assert_eq!(names, ["Content-Type", "Set-Cookie"]);
    }

    #[test]
    fn console_keeps_fifty_and_counts_all() {
        let mut c = Console::default();
        for i in 0..70 {
            c.push(ConsoleLevel::Error, format!("e{i}"), None);
        }
        c.push(ConsoleLevel::Warning, "x".repeat(3_000), None);
        c.push(ConsoleLevel::Exception, "boom".into(), None);
        assert_eq!(c.entries.len(), MAX_CONSOLE);
        assert_eq!((c.errors, c.warnings), (71, 1));
        let mut c = Console::default();
        c.push(ConsoleLevel::Warning, "é".repeat(3_000), None);
        assert_eq!(c.entries[0].message.chars().count(), MAX_CONSOLE_MESSAGE);
    }

    #[test]
    fn resource_types_map_to_kinds() {
        assert_eq!(kind_of(&ResourceType::Xhr), ResourceKind::Xhr);
        assert_eq!(kind_of(&ResourceType::TextTrack), ResourceKind::Media);
        assert_eq!(
            kind_of(&ResourceType::CspViolationReport),
            ResourceKind::CspViolationReport
        );
        assert_eq!(kind_of(&ResourceType::Manifest), ResourceKind::Other);
    }
}
