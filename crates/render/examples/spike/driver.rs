// One CDP client driving the spike. Included twice by `spike.rs`, once with
// `cdp` = `chromiumoxide` 0.9 and once with `cdp` = `chromey`; both crates
// share the chromiumoxide API for everything used here.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use cdp::browser::{Browser, BrowserConfig};
use cdp::cdp::browser_protocol::fetch::{
    ContinueRequestParams, EventRequestPaused, FailRequestParams, FulfillRequestParams, HeaderEntry,
};
use cdp::cdp::browser_protocol::network::{ErrorReason, EventLoadingFailed};
use cdp::cdp::browser_protocol::target::{
    CreateBrowserContextParams, CreateTargetParams, DisposeBrowserContextParams,
};
use cdp::page::Page;
use futures_util::StreamExt;

use super::{Answer, MainDoc, Shared};

pub async fn run(
    shared: Arc<Shared>,
    browser_path: &Path,
    shell: bool,
    tabs: usize,
    args: Vec<String>,
) -> Result<(), String> {
    let mut builder = BrowserConfig::builder()
        .chrome_executable(browser_path)
        // The handler enables `Fetch` on every page and leaves every paused
        // request to us.
        .enable_request_intercept()
        .request_timeout(Duration::from_secs(30))
        .viewport(None)
        .args(args.into_iter().map(browser_arg));
    if !shell {
        builder = builder.new_headless_mode();
    }
    let config = builder.build()?;
    let (mut browser, mut handler) = Browser::launch(config).await.map_err(|e| e.to_string())?;
    let handler_task = tokio::spawn(async move {
        while let Some(event) = handler.next().await {
            if event.is_err() {
                // Keep going: one bad message must not stop the spike.
                continue;
            }
        }
    });
    if let Some(pid) = browser.get_mut_child().and_then(|c| c.inner.id()) {
        shared.browser_pid.store(pid, Ordering::Relaxed);
    }
    let browser = Arc::new(browser);
    let workers: Vec<_> = (0..tabs)
        .map(|_| {
            let (browser, shared) = (browser.clone(), shared.clone());
            tokio::spawn(async move {
                while let Some(n) = shared.next_page() {
                    let started = Instant::now();
                    match tokio::time::timeout(Duration::from_secs(30), render(&browser, &shared, n)).await {
                        Ok(Ok(html)) => shared.page_done(n, &html, started),
                        Ok(Err(e)) => shared.page_failed(n, e),
                        Err(_) => shared.page_failed(n, "no result after 30 s".into()),
                    }
                }
            })
        })
        .collect();
    for w in workers {
        w.await.map_err(|e| e.to_string())?;
    }
    let mut browser = Arc::try_unwrap(browser).map_err(|_| "browser still shared".to_string())?;
    browser.close().await.ok();
    browser.wait().await.ok();
    handler_task.abort();
    Ok(())
}

async fn render(browser: &Browser, shared: &Arc<Shared>, n: u64) -> Result<String, String> {
    let main = Arc::new(shared.main_doc(n).await?);
    let t = Instant::now();
    let context = browser
        .execute(CreateBrowserContextParams::default())
        .await
        .map_err(|e| format!("createBrowserContext: {e}"))?
        .result
        .browser_context_id;
    let result = render_in(browser, shared, n, main, context.clone(), t).await;
    let t = Instant::now();
    browser
        .execute(DisposeBrowserContextParams::new(context))
        .await
        .map_err(|e| format!("disposeBrowserContext: {e}"))?;
    shared.add_phase(4, t);
    result
}

async fn render_in(
    browser: &Browser,
    shared: &Arc<Shared>,
    n: u64,
    main: Arc<MainDoc>,
    context: cdp::cdp::browser_protocol::browser::BrowserContextId,
    setup: Instant,
) -> Result<String, String> {
    let mut target = CreateTargetParams::new("about:blank");
    target.browser_context_id = Some(context);
    let page = browser
        .new_page(target)
        .await
        .map_err(|e| format!("new_page: {e}"))?;
    let mut paused = page
        .event_listener::<EventRequestPaused>()
        .await
        .map_err(|e| format!("listener: {e}"))?;
    let url = shared.page_url(n);
    // Requests the browser itself failed (for example in `--mode continue`).
    let failures = {
        let mut failed = page
            .event_listener::<EventLoadingFailed>()
            .await
            .map_err(|e| format!("listener: {e}"))?;
        let shared = shared.clone();
        tokio::spawn(async move {
            while let Some(event) = failed.next().await {
                shared.note_error(format!("loading failed: {}", event.error_text));
            }
        })
    };
    let interceptor = {
        let (page, shared, url) = (page.clone(), shared.clone(), url.clone());
        tokio::spawn(async move {
            while let Some(event) = paused.next().await {
                let (page, shared, url, main) = (page.clone(), shared.clone(), url.clone(), main.clone());
                tokio::spawn(async move {
                    let _wait = shared.pause_begin();
                    let answer = if event.request.url == url && shared.mode == super::Mode::Fulfil {
                        Answer::Fulfil {
                            status: main.status,
                            headers: main.headers.clone(),
                            body_b64: shared.encode(&main.body),
                        }
                    } else {
                        shared.answer(&event.request.url).await
                    };
                    if let Err(e) = send_answer(&page, &event, answer).await {
                        shared.note_error(format!("answer {}: {e}", event.request.url));
                    }
                });
            }
        })
    };
    shared.add_phase(0, setup);
    let outcome = async {
        let t = Instant::now();
        page.goto(url.as_str()).await.map_err(|e| format!("goto: {e}"))?;
        shared.add_phase(1, t);
        let t = Instant::now();
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let ready = page
                .evaluate("document.body !== null && document.body.dataset.ready === '1'")
                .await
                .map_err(|e| format!("evaluate: {e}"))?
                .into_value::<bool>()
                .unwrap_or(false);
            if ready {
                break;
            }
            if Instant::now() > deadline {
                return Err("not ready after 20 s".to_string());
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        shared.add_phase(2, t);
        let t = Instant::now();
        let html = page.content().await.map_err(|e| format!("content: {e}"));
        shared.add_phase(3, t);
        html
    }
    .await;
    let t = Instant::now();
    interceptor.abort();
    failures.abort();
    page.close().await.ok();
    shared.add_phase(4, t);
    outcome
}

async fn send_answer(page: &Page, event: &EventRequestPaused, answer: Answer) -> Result<(), String> {
    let id = event.request_id.clone();
    match answer {
        Answer::Fulfil {
            status,
            headers,
            body_b64,
        } => {
            let mut p = FulfillRequestParams::new(id, status as i64);
            p.response_headers = Some(headers.into_iter().map(|(n, v)| HeaderEntry::new(n, v)).collect());
            p.body = Some(body_b64.into());
            page.execute(p).await.map(|_| ()).map_err(|e| e.to_string())
        }
        Answer::Continue => page
            .execute(ContinueRequestParams::new(id))
            .await
            .map(|_| ())
            .map_err(|e| e.to_string()),
        Answer::Fail => page
            .execute(FailRequestParams::new(id, ErrorReason::Failed))
            .await
            .map(|_| ())
            .map_err(|e| e.to_string()),
    }
}
