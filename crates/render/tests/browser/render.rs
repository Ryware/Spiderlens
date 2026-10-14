//! One-page rendering against the JavaScript fixture site (spec 014 T016).
//! The test `Network` proxies every request to the fixture, so the fixture's
//! own request log shows exactly what the browser asked for.

use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::future::BoxFuture;
use spiderlens_render::{
    BlockSet, Browser, BrowserInfo, FailReason, FailedReason, LaunchOptions, MainResponse, Network,
    RenderJob, RenderOutcome, RenderState, ResourceAnswer, ResourceKind, ResourceRequest, SeedCookie, Window,
};
use spiderlens_test_fixtures::js_site::{CDN_HOST, JS_HOST, JsSite, JsSiteBuilder, Resource};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use url::Url;

const DESKTOP: Window = Window {
    width: 1024,
    height: 768,
    mobile: false,
};

/// A minimal HTTP/1.1 client to the fixture (the render crate has no HTTP
/// client of its own).
async fn http(site: &JsSite, method: &str, url: &Url) -> Option<(u16, Vec<(String, String)>, Vec<u8>)> {
    let mut s = tokio::net::TcpStream::connect(site.addr()).await.ok()?;
    let host = url.host_str()?;
    let path = &url[url::Position::BeforePath..];
    let req =
        format!("{method} {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\nContent-Length: 0\r\n\r\n");
    s.write_all(req.as_bytes()).await.ok()?;
    let mut buf = Vec::new();
    s.read_to_end(&mut buf).await.ok()?;
    let split = buf.windows(4).position(|w| w == b"\r\n\r\n")?;
    let head = String::from_utf8_lossy(&buf[..split]).into_owned();
    let body = buf[split + 4..].to_vec();
    let mut lines = head.lines();
    let status = lines.next()?.get(9..12)?.parse().ok()?;
    let headers = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(n, v)| (n.trim().to_string(), v.trim().to_string()))
        .collect();
    Some((status, headers, body))
}

struct FixtureNet(Arc<JsSite>);

impl Network for FixtureNet {
    fn fetch(&self, req: ResourceRequest) -> BoxFuture<'_, ResourceAnswer> {
        Box::pin(async move {
            match http(&self.0, &req.method, &req.url).await {
                Some((status, headers, body)) => ResourceAnswer::Response {
                    status,
                    headers,
                    body,
                },
                None => ResourceAnswer::Fail {
                    reason: FailReason::Failed,
                },
            }
        })
    }
}

/// One render case.
type Case = for<'a> fn(&'a Ctx) -> BoxFuture<'a, Result<(), String>>;

struct Ctx {
    site: Arc<JsSite>,
    browser: Browser,
}

impl Ctx {
    async fn job(&self, path: &str) -> RenderJob {
        let url = Url::parse(&self.site.url(JS_HOST, path)).expect("url");
        let (status, headers, body) = http(&self.site, "GET", &url).await.expect("main document");
        let main = MainResponse {
            status,
            headers,
            body,
        };
        RenderJob::new(url, main, "SpiderLens-test".into(), DESKTOP)
    }

    async fn render(&self, job: RenderJob) -> RenderOutcome {
        self.browser
            .render(job, Arc::new(FixtureNet(self.site.clone())))
            .await
    }
}

fn check(ok: bool, what: impl FnOnce() -> String) -> Result<(), String> {
    if ok { Ok(()) } else { Err(what()) }
}

fn html(o: &RenderOutcome) -> &str {
    o.html.as_deref().unwrap_or("")
}

fn site() -> JsSite {
    JsSiteBuilder::new()
        .app_pages(3)
        .page(
            "/blocking",
            "<!DOCTYPE html><html><head><link rel=stylesheet href=/s.css>\
             <script src='{base:cdn.test}/lib.js'></script></head>\
             <body><img src=/a.png><p>blocking</p></body></html>",
        )
        .image(JS_HOST, "/a.png")
        .stylesheet(JS_HOST, "/s.css", "p{color:red}")
        .script(CDN_HOST, "/lib.js", "document.documentElement.dataset.cdn = '1';")
        .page(
            "/forever",
            "<html><body><p id=t>0</p><script>let n = 0; \
             setInterval(() => { document.getElementById('t').textContent = ++n; }, 50);</script></body></html>",
        )
        .page(
            "/redirect",
            "<html><head><title>Before</title></head><body><p>stay</p>\
             <script>setTimeout(() => { location.href = '/next'; }, 10);</script></body></html>",
        )
        .page("/next", "<html><body>next</body></html>")
        .page(
            "/redirect-now",
            "<html><head><title>Moved</title><script>location.replace('/next');</script></head>\
             <body><p>stay</p></body></html>",
        )
        .page(
            "/console",
            "<html><body><script>console.error('bad', 'thing'); console.warn('careful'); console.log('fine');\
             </script><script src=/broken.js></script></body></html>",
        )
        .script(JS_HOST, "/broken.js", "\n\nnull.boom();")
        .page(
            "/shadow",
            "<!DOCTYPE html><html><body><div id=host><p>Light child</p></div>\
             <iframe src=/frame></iframe>\
             <script>const r = document.getElementById('host').attachShadow({mode: 'open'});\
             r.innerHTML = '<h2>Shadow heading</h2><slot></slot>';</script></body></html>",
        )
        .page("/frame", "<html><body><p>Frame text</p></body></html>")
        .page(
            "/dialogs",
            "<html><head><title>x</title></head><body><script>alert('hi'); \
             document.title = confirm('sure?') ? 'yes' : 'no';\
             window.open('/next');</script></body></html>",
        )
        .page("/quick", "<html><body>quick</body></html>")
        .page("/busy", "<html><body><p>busy</p><script>while (true) {}</script></body></html>")
        .page(
            "/window",
            "<html><head><title>w</title><meta name=viewport content='width=device-width'></head><body style='margin:0'><div style='height:3000px'>tall</div>\
             <p id=o></p><script>\
             const show = () => { document.getElementById('o').textContent = \
               'w=' + innerWidth + ' h=' + innerHeight + ' tz=' + Intl.DateTimeFormat().resolvedOptions().timeZone \
               + ' lang=' + navigator.language + ' cookie=' + document.cookie + ' ua=' + navigator.userAgent; };\
             show(); addEventListener('resize', show);</script></body></html>",
        )
        .resource(
            JS_HOST,
            "/gone",
            Resource {
                status: 404,
                ..Resource::new("text/html", "<html><body><h1>Not here</h1></body></html>")
            },
        )
        .start()
}

pub fn render_pages_of_the_js_fixture(info: &BrowserInfo) -> Result<(), String> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    rt.block_on(async {
        let site = Arc::new(site());
        let browser = Browser::launch(info, LaunchOptions::default())
            .await
            .map_err(|e| format!("launch: {e:?}"))?;
        let mut ctx = Ctx { site, browser };
        let cases: &[(&str, Case)] = &[
            ("client_rendered_page", |c| Box::pin(client_rendered_page(c))),
            ("blocked_types_and_third_party", |c| {
                Box::pin(blocked_types_and_third_party(c))
            }),
            ("a_page_that_never_settles_times_out", |c| {
                Box::pin(never_settles(c))
            }),
            ("a_script_navigation_is_recorded_not_followed", |c| {
                Box::pin(js_redirect(c))
            }),
            ("console_errors_and_exceptions", |c| Box::pin(console(c))),
            ("shadow_roots_and_iframes_are_flattened", |c| {
                Box::pin(shadow_and_iframe(c))
            }),
            ("dialogs_and_popups_do_not_block", |c| Box::pin(dialogs(c))),
            ("the_minimum_wait_comes_first", |c| Box::pin(min_wait(c))),
            ("window_timezone_locale_cookies", |c| {
                Box::pin(window_and_context(c))
            }),
            ("error_pages_and_encoded_main_documents", |c| {
                Box::pin(error_page(c))
            }),
            ("contexts_share_nothing", |c| Box::pin(fresh_context(c))),
            ("screenshot", |c| Box::pin(screenshot(c))),
        ];
        for (name, case) in cases {
            let t = Instant::now();
            case(&ctx).await.map_err(|e| format!("{name}: {e}"))?;
            println!("  render case {name} ... ok ({:.2} s)", t.elapsed().as_secs_f64());
        }
        // Last, because it leaves the browser stuck.
        hard_stop(&mut ctx).await.map_err(|e| format!("hard_stop: {e}"))?;
        println!("  render case hard_stop ... ok");
        ctx.browser.close().await;
        Ok(())
    })
}

async fn client_rendered_page(c: &Ctx) -> Result<(), String> {
    c.site.reset();
    let job = c.job("/app/0").await;
    let out = c.render(job).await;
    check(out.state == RenderState::Rendered, || {
        format!("state {:?}", out.state)
    })?;
    let h = html(&out);
    check(h.starts_with("<!DOCTYPE html>"), || {
        format!("no doctype: {}", &h[..h.len().min(80)])
    })?;
    check(h.contains("<title>App page 0</title>"), || {
        "rendered title missing".into()
    })?;
    check(h.contains("data-ready=\"1\""), || "not ready".into())?;
    // One request for the main document: the test's own (FR-007, SC-006).
    check(c.site.count(JS_HOST, "/app/0") == 1, || {
        format!(
            "main document requested {} times",
            c.site.count(JS_HOST, "/app/0")
        )
    })?;
    for p in [
        "/static/vendor.js",
        "/static/runtime.js",
        "/static/app.js",
        "/static/app.css",
        "/api/page/0.json",
    ] {
        check(c.site.count(JS_HOST, p) == 1, || {
            format!("{p} requested {} times", c.site.count(JS_HOST, p))
        })?;
    }
    let loaded: Vec<_> = out.resources.iter().filter(|r| r.status == Some(200)).collect();
    check(loaded.len() == 6, || format!("resources: {:?}", out.resources))?;
    check(
        out.resources
            .iter()
            .any(|r| r.kind == ResourceKind::Script && r.bytes > 100_000),
        || "vendor.js size".into(),
    )?;
    check(out.render_ms > 0 && out.window == DESKTOP, || {
        "render_ms or window".into()
    })
}

async fn blocked_types_and_third_party(c: &Ctx) -> Result<(), String> {
    c.site.reset();
    let out = c.render(c.job("/blocking").await).await;
    check(out.state == RenderState::Rendered, || {
        format!("state {:?}", out.state)
    })?;
    let img = out
        .resources
        .iter()
        .find(|r| r.kind == ResourceKind::Image)
        .ok_or("no image record")?;
    check(img.failed == Some(FailReason::BlockedByClient), || {
        format!("image: {img:?}")
    })?;
    check(c.site.count(JS_HOST, "/a.png") == 0, || {
        "blocked image was fetched".into()
    })?;
    check(c.site.count(JS_HOST, "/s.css") == 1, || {
        "stylesheet not loaded".into()
    })?;
    check(html(&out).contains("data-cdn=\"1\""), || {
        "third-party script did not run".into()
    })?;
    let cdn = out
        .resources
        .iter()
        .find(|r| r.url.host_str() == Some(CDN_HOST))
        .ok_or("no cdn record")?;
    check(cdn.third_party, || "cdn not third party".into())?;

    // Third party blocked, everything else loaded.
    c.site.reset();
    let mut job = c.job("/blocking").await;
    job.block = BlockSet {
        images: false,
        media: false,
        fonts: false,
        stylesheets: true,
        third_party: true,
    };
    let out = c.render(job).await;
    check(c.site.count(CDN_HOST, "/lib.js") == 0, || {
        "third party fetched".into()
    })?;
    check(c.site.count(JS_HOST, "/a.png") == 1, || "image not loaded".into())?;
    check(c.site.count(JS_HOST, "/s.css") == 0, || {
        "stylesheet fetched".into()
    })?;
    check(!html(&out).contains("data-cdn"), || "blocked script ran".into())
}

async fn never_settles(c: &Ctx) -> Result<(), String> {
    let mut job = c.job("/forever").await;
    job.max_time = Duration::from_secs(1);
    let t = Instant::now();
    let out = c.render(job).await;
    let took = t.elapsed();
    check(out.state == RenderState::TimedOut, || {
        format!("state {:?}", out.state)
    })?;
    check(took < Duration::from_secs(3), || format!("took {took:?}"))?;
    check(took >= Duration::from_secs(1), || {
        format!("stopped early: {took:?}")
    })?;
    check(html(&out).contains("<p id=\"t\">"), || {
        "DOM at the cap not kept".into()
    })
}

async fn js_redirect(c: &Ctx) -> Result<(), String> {
    c.site.reset();
    let out = c.render(c.job("/redirect").await).await;
    let target = out.js_redirect.as_ref().map(|u| u.path().to_string());
    check(target.as_deref() == Some("/next"), || {
        format!("js_redirect {:?}", out.js_redirect)
    })?;
    check(c.site.count(JS_HOST, "/next") == 0, || {
        "the redirect was followed".into()
    })?;
    check(html(&out).contains("stay"), || "page content lost".into())?;
    // A navigation before `load` still settles, well before the cap.
    let out = c.render(c.job("/redirect-now").await).await;
    check(out.state == RenderState::Rendered, || {
        format!("state {:?} after {} ms", out.state, out.render_ms)
    })?;
    check(
        out.js_redirect.as_ref().map(|u| u.path()) == Some("/next"),
        || format!("js_redirect {:?}", out.js_redirect),
    )?;
    check(c.site.count(JS_HOST, "/next") == 0, || {
        "the redirect was followed".into()
    })
}

async fn console(c: &Ctx) -> Result<(), String> {
    let out = c.render(c.job("/console").await).await;
    check(out.console_totals == (2, 1), || {
        format!("totals {:?}: {:?}", out.console_totals, out.console)
    })?;
    let err = &out.console[0];
    check(err.message == "bad thing", || format!("{err:?}"))?;
    let exc = out
        .console
        .iter()
        .find(|e| e.level == spiderlens_render::ConsoleLevel::Exception)
        .ok_or("no exception")?;
    let (src, line, _) = exc.source.clone().ok_or("no source")?;
    check(src.ends_with("/broken.js") && line == 3, || format!("{exc:?}"))
}

async fn shadow_and_iframe(c: &Ctx) -> Result<(), String> {
    c.site.reset();
    let out = c.render(c.job("/shadow").await).await;
    let h = html(&out);
    for want in [
        "<h2>Shadow heading</h2>",
        "<p>Light child</p>",
        "Frame text",
        "data-spiderlens-frame=\"/frame\"",
    ] {
        check(h.contains(want), || format!("missing {want}: {h}"))?;
    }
    check(c.site.count(JS_HOST, "/frame") == 1, || {
        "frame not fetched once".into()
    })
}

async fn dialogs(c: &Ctx) -> Result<(), String> {
    c.site.reset();
    let out = c.render(c.job("/dialogs").await).await;
    check(out.state == RenderState::Rendered, || {
        format!("state {:?}", out.state)
    })?;
    check(html(&out).contains("<title>no</title>"), || {
        "confirm was not dismissed".into()
    })?;
    check(c.site.count(JS_HOST, "/next") == 0, || "pop-up opened".into())
}

async fn min_wait(c: &Ctx) -> Result<(), String> {
    let mut job = c.job("/quick").await;
    job.min_wait = Duration::from_millis(1_500);
    let out = c.render(job).await;
    check(out.state == RenderState::Rendered, || {
        format!("state {:?}", out.state)
    })?;
    check(out.render_ms >= 1_500, || format!("render_ms {}", out.render_ms))
}

async fn window_and_context(c: &Ctx) -> Result<(), String> {
    let mut job = c.job("/window").await;
    job.window = Window {
        width: 411,
        height: 731,
        mobile: true,
    };
    job.cookies = vec![SeedCookie {
        name: "seed".into(),
        value: "1".into(),
        domain: JS_HOST.into(),
        path: "/".into(),
        secure: false,
        http_only: false,
        expires: None,
    }];
    let out = c.render(job).await;
    let h = html(&out);
    let text = h
        .split("<p id=\"o\">")
        .nth(1)
        .and_then(|r| r.split("</p>").next())
        .unwrap_or("");
    check(text.contains("w=411 "), || format!("width: {text}"))?;
    let height: u32 = text
        .split("h=")
        .nth(1)
        .and_then(|r| r.split(' ').next())
        .and_then(|n| n.parse().ok())
        .unwrap_or(0);
    check(height >= 3_000, || format!("window not extended: {text}"))?;
    check(text.contains("tz=UTC"), || format!("timezone: {text}"))?;
    check(text.contains("lang=en-US"), || format!("locale: {text}"))?;
    check(text.contains("cookie=seed=1"), || format!("cookie: {text}"))?;
    check(text.contains("ua=SpiderLens-test"), || {
        format!("user agent: {text}")
    })
}

async fn error_page(c: &Ctx) -> Result<(), String> {
    let mut job = c.job("/gone").await;
    // The stored body is decoded: an encoding header must not reach Chrome.
    job.main.headers.push(("Content-Encoding".into(), "gzip".into()));
    let out = c.render(job).await;
    check(out.state == RenderState::Rendered, || {
        format!("state {:?}", out.state)
    })?;
    check(html(&out).contains("<h1>Not here</h1>"), || {
        html(&out).to_string()
    })
}

async fn fresh_context(c: &Ctx) -> Result<(), String> {
    // A cookie set by one page is not seen by the next (FR-011).
    let out = c.render(c.job("/window").await).await;
    check(html(&out).contains("cookie= "), || {
        "cookie leaked between pages".into()
    })
}

async fn screenshot(c: &Ctx) -> Result<(), String> {
    let mut job = c.job("/quick").await;
    job.screenshot = Some(spiderlens_render::ScreenshotOptions::default());
    let out = c.render(job).await;
    let shot = out.screenshot.ok_or("no screenshot")?;
    check(
        shot.webp.starts_with(b"RIFF") && &shot.webp[8..12] == b"WEBP",
        || "not WebP".into(),
    )?;
    check(shot.width == 1024 && shot.height >= 1, || {
        format!("{}x{}", shot.width, shot.height)
    })
}

async fn hard_stop(c: &mut Ctx) -> Result<(), String> {
    let mut job = c.job("/busy").await;
    job.max_time = Duration::from_secs(1);
    let t = Instant::now();
    let out = c.render(job).await;
    let took = t.elapsed();
    check(took < Duration::from_millis(3_500), || format!("took {took:?}"))?;
    check(
        matches!(
            out.state,
            RenderState::Failed {
                reason: FailedReason::RendererError,
                retryable: true
            }
        ),
        || format!("state {:?}", out.state),
    )?;
    check(c.browser.needs_restart(), || "not marked for restart".into())?;
    c.browser.restart().await.map_err(|e| format!("restart: {e:?}"))?;
    check(!c.browser.needs_restart(), || "still marked".into())?;
    let out = c.render(c.job("/quick").await).await;
    check(out.state == RenderState::Rendered, || {
        format!("after restart {:?}", out.state)
    })
}
