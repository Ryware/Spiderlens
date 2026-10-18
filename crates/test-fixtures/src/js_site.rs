//! A fixture site for JavaScript rendering tests (spec 014, task T005).
//!
//! One server on 127.0.0.1 answers for the site host [`JS_HOST`] and a
//! second, third-party host [`CDN_HOST`], chosen by the `Host` header like
//! [`crate::SiteBuilder`]. Pages can ship scripts, stylesheets, JSON
//! endpoints, images, fonts and media, each with its own `Cache-Control`.
//!
//! The server counts requests per host and path, the peak number of open
//! requests per host, and keeps a request log. `GET /__stats` on any host
//! returns them as JSON; `GET /__reset` clears them. In bodies and headers,
//! `{base:HOST}` becomes `http://HOST:PORT`.
//!
//! Requests to `127.0.0.1` or `localhost` are answered and counted as
//! [`JS_HOST`], so the CLI, which cannot map host names, can crawl it.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::net::{SocketAddr, TcpListener as StdListener};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use axum::Router;
use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{HeaderName, HeaderValue, StatusCode, header};
use axum::response::Response;

/// The crawled site's host name.
pub const JS_HOST: &str = "js.test";
/// A third-party host for CDN scripts, fonts and beacons.
pub const CDN_HOST: &str = "cdn.test";

/// A smallest valid PNG (1 x 1, transparent).
const PNG_1X1: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52, 0x00,
    0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4, 0x89, 0x00,
    0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x00, 0x01, 0x00, 0x00, 0x05, 0x00, 0x01,
    0x0d, 0x0a, 0x2d, 0xb4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

/// One answer the server gives for a host and path.
#[derive(Clone, Debug)]
pub struct Resource {
    pub status: u16,
    pub content_type: String,
    pub body: Vec<u8>,
    /// `Cache-Control` value, if any.
    pub cache_control: Option<String>,
    pub headers: Vec<(String, String)>,
    /// Wait this long before answering.
    pub delay: Duration,
}

impl Resource {
    pub fn new(content_type: &str, body: impl Into<Vec<u8>>) -> Self {
        Self {
            status: 200,
            content_type: content_type.into(),
            body: body.into(),
            cache_control: None,
            headers: Vec::new(),
            delay: Duration::ZERO,
        }
    }
}

/// Builds a [`JsSite`]. Both [`JS_HOST`] and [`CDN_HOST`] are always served.
pub struct JsSiteBuilder {
    resources: HashMap<(String, String), Resource>,
}

impl Default for JsSiteBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl JsSiteBuilder {
    pub fn new() -> Self {
        Self {
            resources: HashMap::new(),
        }
    }

    /// Any resource on any host.
    pub fn resource(mut self, host: &str, path: &str, resource: Resource) -> Self {
        self.resources.insert((host.into(), path.into()), resource);
        self
    }

    /// An HTML page on [`JS_HOST`].
    pub fn page(self, path: &str, html: &str) -> Self {
        self.resource(JS_HOST, path, Resource::new("text/html; charset=utf-8", html))
    }

    pub fn script(self, host: &str, path: &str, js: &str) -> Self {
        self.resource(host, path, Resource::new("text/javascript", js))
    }

    pub fn stylesheet(self, host: &str, path: &str, css: &str) -> Self {
        self.resource(host, path, Resource::new("text/css", css))
    }

    pub fn json(self, host: &str, path: &str, json: &str) -> Self {
        self.resource(host, path, Resource::new("application/json", json))
    }

    /// A 1 x 1 PNG.
    pub fn image(self, host: &str, path: &str) -> Self {
        self.resource(host, path, Resource::new("image/png", PNG_1X1))
    }

    /// A placeholder font file (not a valid font; the browser just fails to use it).
    pub fn font(self, host: &str, path: &str) -> Self {
        self.resource(host, path, Resource::new("font/woff2", b"wOF2-fixture".to_vec()))
    }

    /// A placeholder media file.
    pub fn media(self, host: &str, path: &str) -> Self {
        self.resource(host, path, Resource::new("video/mp4", b"fixture-media".to_vec()))
    }

    pub fn robots(self, host: &str, text: &str) -> Self {
        self.resource(host, "/robots.txt", Resource::new("text/plain", text))
    }

    /// Sets `Cache-Control` on a resource added earlier.
    pub fn cache_control(mut self, host: &str, path: &str, value: &str) -> Self {
        if let Some(r) = self.resources.get_mut(&(host.into(), path.into())) {
            r.cache_control = Some(value.into());
        }
        self
    }

    /// Adds a response header to a resource added earlier.
    pub fn header(mut self, host: &str, path: &str, name: &str, value: &str) -> Self {
        if let Some(r) = self.resources.get_mut(&(host.into(), path.into())) {
            r.headers.push((name.into(), value.into()));
        }
        self
    }

    /// Delays the answer of a resource added earlier.
    pub fn delay(mut self, host: &str, path: &str, delay: Duration) -> Self {
        if let Some(r) = self.resources.get_mut(&(host.into(), path.into())) {
            r.delay = delay;
        }
        self
    }

    /// `pages` client-rendered pages `/app/0` .. `/app/{pages-1}`, each an
    /// application shell with three scripts, one stylesheet and two JSON
    /// requests (`/api/page/N.json`, `/api/links/N.json`). The scripts and
    /// stylesheet are shared and cacheable for an hour; the JSON is not
    /// cacheable. When rendering is done, `<body>` gets `data-ready="1"`,
    /// the title becomes `App page N` and the page holds an `<h1>`, three
    /// paragraphs and links to its ten children in a tree, its parent and
    /// two pseudo-random pages. The raw HTML has the title `Loading` and
    /// no links. Used by the render spike (T001) and the `js-site` binary.
    pub fn app_pages(mut self, pages: u64) -> Self {
        let hour = "public, max-age=3600";
        self = self
            .script(JS_HOST, "/static/vendor.js", &vendor_js())
            .cache_control(JS_HOST, "/static/vendor.js", hour)
            .script(JS_HOST, "/static/runtime.js", RUNTIME_JS)
            .cache_control(JS_HOST, "/static/runtime.js", hour)
            .script(JS_HOST, "/static/app.js", APP_JS)
            .cache_control(JS_HOST, "/static/app.js", hour)
            .stylesheet(JS_HOST, "/static/app.css", APP_CSS)
            .cache_control(JS_HOST, "/static/app.css", hour);
        for n in 0..pages {
            let shell = format!(
                "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><title>Loading</title>\
                 <link rel=\"stylesheet\" href=\"/static/app.css\">\
                 <script src=\"/static/vendor.js\" defer></script>\
                 <script src=\"/static/runtime.js\" defer></script>\
                 <script src=\"/static/app.js\" defer></script></head>\
                 <body data-page=\"{n}\"><div id=\"app\">Loading</div></body></html>"
            );
            self = self.page(&format!("/app/{n}"), &shell);
            let mut links = Vec::new();
            if n > 0 {
                links.push((n - 1) / 10);
            }
            links.extend(((n * 10 + 1)..=(n * 10 + 10)).filter(|c| *c < pages));
            links.extend((0..2).map(|k| mix(n * 2 + k) % pages));
            let links: Vec<String> = links.iter().map(|l| l.to_string()).collect();
            self = self
                .json(
                    JS_HOST,
                    &format!("/api/page/{n}.json"),
                    &format!(
                        "{{\"title\":\"App page {n}\",\"heading\":\"Page {n}\",\"paragraphs\":3,\"description\":\"Client rendered page {n}\"}}"
                    ),
                )
                .cache_control(JS_HOST, &format!("/api/page/{n}.json"), "no-store")
                .json(
                    JS_HOST,
                    &format!("/api/links/{n}.json"),
                    &format!("[{}]", links.join(",")),
                )
                .cache_control(JS_HOST, &format!("/api/links/{n}.json"), "no-store");
        }
        self
    }

    /// The pages of spec 014 user story 1 (task T020), linked from `/`:
    ///
    /// - `/title-changed`: raw title `Loading`; a script sets
    ///   `Red Shoes | Shop`.
    /// - `/js-nav`: every navigation link is inserted by a script and points
    ///   to `/only-js-1` and `/only-js-2`, which nothing else links to.
    /// - `/jsonld-added`: a script adds a JSON-LD `Product` block.
    /// - `/noindex-added`: a script adds `<meta name="robots"
    ///   content="noindex">`.
    /// - `/js-redirect`: `location.replace('/redirect-target')`; nothing
    ///   else links to the target.
    /// - `/server-rendered`: fully server-rendered, no script.
    pub fn us1_pages(self) -> Self {
        let page = |title: &str, body: &str, script: &str| {
            let script = if script.is_empty() {
                String::new()
            } else {
                format!("<script>{script}</script>")
            };
            format!(
                "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><title>{title}</title>\
                 {script}</head><body>{body}</body></html>"
            )
        };
        let on_ready = |js: &str| format!("document.addEventListener('DOMContentLoaded', () => {{ {js} }});");
        self.page(
            "/",
            &page(
                "Home",
                "<h1>Home</h1><p>The shop home page.</p>\
                 <a href=\"/title-changed\">Title</a> <a href=\"/js-nav\">Navigation</a> \
                 <a href=\"/jsonld-added\">Product</a> <a href=\"/noindex-added\">Hidden</a> \
                 <a href=\"/js-redirect\">Old page</a> <a href=\"/server-rendered\">About</a>",
                "",
            ),
        )
        .page(
            "/title-changed",
            &page(
                "Loading",
                "<h1>Red shoes</h1><p>Shoes for running and walking.</p>",
                &on_ready("document.title = 'Red Shoes | Shop';"),
            ),
        )
        .page(
            "/js-nav",
            &page(
                "Navigation",
                "<h1>Navigation</h1><nav id=\"nav\"></nav>",
                &on_ready(
                    "document.getElementById('nav').innerHTML = \
                     '<a href=\"/only-js-1\">First</a> <a href=\"/only-js-2\">Second</a>';",
                ),
            ),
        )
        .page(
            "/only-js-1",
            &page("Only JS 1", "<h1>Only JS 1</h1><p>Linked by a script.</p>", ""),
        )
        .page(
            "/only-js-2",
            &page("Only JS 2", "<h1>Only JS 2</h1><p>Linked by a script.</p>", ""),
        )
        .page(
            "/jsonld-added",
            &page(
                "Product",
                "<h1>Product</h1><p>A product page.</p>",
                &on_ready(
                    "const s = document.createElement('script'); s.type = 'application/ld+json'; \
                     s.textContent = JSON.stringify({'@context': 'https://schema.org', '@type': 'Product', \
                     name: 'Red Shoes'}); document.head.appendChild(s);",
                ),
            ),
        )
        .page(
            "/noindex-added",
            &page(
                "Hidden",
                "<h1>Hidden</h1><p>Indexable until a script runs.</p>",
                &on_ready(
                    "const m = document.createElement('meta'); m.name = 'robots'; m.content = 'noindex'; \
                     document.head.appendChild(m);",
                ),
            ),
        )
        .page(
            "/js-redirect",
            &page(
                "Old page",
                "<p>This page moved.</p>",
                "location.replace('/redirect-target');",
            ),
        )
        .page(
            "/redirect-target",
            &page(
                "New page",
                "<h1>New page</h1><p>Reached by a script redirect.</p>",
                "",
            ),
        )
        .page(
            "/server-rendered",
            &page(
                "About us",
                "<h1>About us</h1><p>Everything here is in the HTML.</p><a href=\"/\">Home</a>",
                "",
            ),
        )
    }

    /// Pages for failure handling (spec 014 US5, T056). The home page
    /// links to all of them:
    ///
    /// - `/throws`: an uncaught exception ("boom from the fixture").
    /// - `/logs`: `console.error` and `console.warn` calls.
    /// - `/missing-script`: a script that answers 404.
    /// - `/heavy`: allocates until the page's JavaScript heap limit is hit.
    /// - `/challenge`: a bot challenge page ("Just a moment..." with a
    ///   challenge form) built by a script, over a raw page with content.
    /// - `/steady`: a plain page that renders without trouble.
    ///
    /// The renderer-crash page is not a fixture page: the tests kill the
    /// browser process (CDP `Page.crash` needs the test to hold the
    /// connection, which the engine owns).
    pub fn us5_pages(self) -> Self {
        let page = |title: &str, body: &str, script: &str| {
            let script = if script.is_empty() {
                String::new()
            } else {
                format!("<script>{script}</script>")
            };
            format!(
                "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><title>{title}</title>\
                 {script}</head><body>{body}</body></html>"
            )
        };
        let on_ready = |js: &str| format!("document.addEventListener('DOMContentLoaded', () => {{ {js} }});");
        self.page(
            "/",
            &page(
                "Failures home",
                "<h1>Failures home</h1><p>Pages that go wrong in different ways.</p>\
                 <a href=\"/throws\">Throws</a> <a href=\"/logs\">Logs</a> \
                 <a href=\"/missing-script\">Missing script</a> <a href=\"/heavy\">Heavy</a> \
                 <a href=\"/challenge\">Challenge</a> <a href=\"/steady\">Steady</a>",
                "",
            ),
        )
        .page(
            "/throws",
            &page(
                "Throws",
                "<h1>Throws</h1><p>A script fails after the page loads.</p>",
                &on_ready("throw new Error('boom from the fixture');"),
            ),
        )
        .page(
            "/logs",
            &page(
                "Logs",
                "<h1>Logs</h1><p>A script writes to the console.</p>",
                &on_ready("console.error('fixture error message'); console.warn('fixture warning message');"),
            ),
        )
        .page(
            "/missing-script",
            "<!doctype html><html><head><meta charset=\"utf-8\"><title>Missing script</title>\
             <script src=\"/missing.js\"></script></head>\
             <body><h1>Missing script</h1><p>Its script is gone.</p></body></html>",
        )
        .page(
            "/heavy",
            &page(
                "Heavy",
                "<h1>Heavy</h1><p>A script allocates without end.</p>",
                &on_ready("const keep = []; for (;;) { keep.push(new Array(1000000).fill(1.5)); }"),
            ),
        )
        .page(
            "/challenge",
            &page(
                "Welcome",
                "<h1>Welcome</h1><p>The real content is here for people who get past the check.</p>",
                &on_ready(
                    "document.title = 'Just a moment...'; \
                     document.body.innerHTML = '<form id=\"challenge-form\" action=\"/cdn-cgi/challenge-platform/h/b\">\
                     <p>Checking your browser before accessing the site.</p></form>';",
                ),
            ),
        )
        .page(
            "/steady",
            &page("Steady", "<h1>Steady</h1><p>Nothing goes wrong here.</p>", ""),
        )
    }

    /// The pages of spec 014 user story 4 (task T046), linked from `/`. Pass
    /// a rendering include pattern to render only the one under test.
    ///
    /// - `/settles`: a heading appears after 2 s.
    /// - `/polls`: asks `/api/poll.json` every 200 ms and rewrites the
    ///   page, forever.
    /// - `/resources`: an image, a font, a video, a stylesheet, a script of
    ///   its own, a third-party script on `cdn.test` and `/private/app.js`,
    ///   which the fixture's robots.txt disallows.
    /// - `/products/`, `/products/a`, `/products/b`: a section.
    /// - `/width`: writes `window.innerWidth` into its `<h1>` as
    ///   `innerWidth=N` (raw text `raw`).
    /// - `/retry`: its first script answers 429 with `Retry-After: 2`; when
    ///   that fails, a second script is asked for.
    /// - `/many`: eight scripts of `/many/N.js`, each answered after 300 ms.
    /// - `/plain`, `/other`: nothing happens.
    pub fn us4_pages(mut self) -> Self {
        let page = |title: &str, head: &str, body: &str| {
            format!(
                "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">\
                 <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
                 <title>{title}</title>{head}</head><body>{body}</body></html>"
            )
        };
        let many: String = (0..8)
            .map(|n| format!("<script src=\"/many/{n}.js\"></script>"))
            .collect();
        let mut retry = Resource::new("text/javascript", "");
        retry.status = 429;
        self = self
            .robots(JS_HOST, "User-agent: *\nDisallow: /private/\n")
            .page(
                "/",
                &page(
                    "Rendering settings",
                    "",
                    "<h1>Rendering settings</h1><p>Pages for the rendering settings.</p>\
                     <a href=\"/settles\">Settles</a> <a href=\"/polls\">Polls</a> \
                     <a href=\"/resources\">Resources</a> <a href=\"/products/\">Products</a> \
                     <a href=\"/products/a\">A</a> <a href=\"/products/b\">B</a> \
                     <a href=\"/width\">Width</a> <a href=\"/retry\">Retry</a> \
                     <a href=\"/many\">Many</a> <a href=\"/plain\">Plain</a> <a href=\"/other\">Other</a>",
                ),
            )
            .page(
                "/settles",
                &page(
                    "Settles",
                    "<script>setTimeout(function () { document.body.insertAdjacentHTML('beforeend', \
                     '<h2>Settled after two seconds</h2>'); }, 2000);</script>",
                    "<h1>Settles</h1><p>A heading is added after two seconds.</p>",
                ),
            )
            .page(
                "/polls",
                &page(
                    "Polls",
                    "<script>var n = 0; setInterval(function () { n++; \
                     fetch('/api/poll.json?n=' + n).then(function (r) { return r.text(); }).then(function (t) { \
                     document.getElementById('t').textContent = t + n; }); }, 200);</script>",
                    "<h1>Polls</h1><p id=\"t\">0</p>",
                ),
            )
            .json(JS_HOST, "/api/poll.json", "{}")
            .cache_control(JS_HOST, "/api/poll.json", "no-store")
            .page(
                "/resources",
                &page(
                    "Resources",
                    "<link rel=\"stylesheet\" href=\"/css/site.css\">\
                     <script src=\"/js/own.js\"></script>\
                     <script src=\"{base:cdn.test}/cdn/lib.js\"></script>\
                     <script src=\"/private/app.js\"></script>\
                     <script>new FontFace('Fixture', 'url(/fonts/a.woff2)').load().catch(function () {});</script>",
                    "<h1>Resources</h1><img src=\"/img/a.png\" alt=\"a\">\
                     <video src=\"/media/a.mp4\" preload=\"auto\" muted></video>",
                ),
            )
            .stylesheet(JS_HOST, "/css/site.css", "h1 { color: green; }")
            .script(JS_HOST, "/js/own.js", "window.own = 1;")
            .script(CDN_HOST, "/cdn/lib.js", "window.cdn = 1;")
            .script(JS_HOST, "/private/app.js", "window.privateApp = 1;")
            .image(JS_HOST, "/img/a.png")
            .font(JS_HOST, "/fonts/a.woff2")
            .media(JS_HOST, "/media/a.mp4")
            .page(
                "/products/",
                &page("Products", "", "<h1>Products</h1><p>The products section.</p>"),
            )
            .page(
                "/products/a",
                &page("Product A", "", "<h1>Product A</h1><p>The first product.</p>"),
            )
            .page(
                "/products/b",
                &page("Product B", "", "<h1>Product B</h1><p>The second product.</p>"),
            )
            .page(
                "/width",
                &page(
                    "Width",
                    "<script>document.addEventListener('DOMContentLoaded', function () { \
                     document.getElementById('w').textContent = 'innerWidth=' + window.innerWidth; });</script>",
                    "<h1 id=\"w\">raw</h1>",
                ),
            )
            .page(
                "/retry",
                &page(
                    "Retry",
                    "<script>function loadB() { var s = document.createElement('script'); \
                     s.src = '/retry/b.js'; document.head.appendChild(s); }</script>\
                     <script src=\"/retry/a.js\" onerror=\"loadB()\"></script>",
                    "<h1>Retry</h1><p>A script is asked to wait.</p>",
                ),
            )
            .resource(JS_HOST, "/retry/a.js", retry)
            .header(JS_HOST, "/retry/a.js", "Retry-After", "2")
            .script(JS_HOST, "/retry/b.js", "window.retryB = 1;")
            .page(
                "/many",
                &page("Many", &many, "<h1>Many</h1><p>Eight scripts at once.</p>"),
            )
            .page("/plain", &page("Plain", "", "<h1>Plain</h1><p>Nothing happens.</p>"))
            .page("/other", &page("Other", "", "<h1>Other</h1><p>Nothing happens.</p>"));
        for n in 0..8 {
            self = self
                .script(JS_HOST, &format!("/many/{n}.js"), "window.many = 1;")
                .delay(JS_HOST, &format!("/many/{n}.js"), Duration::from_millis(300));
        }
        self
    }

    /// `pages` pages `/slow/0` .. `/slow/{pages-1}`, linked from `/`, each
    /// with its own script answered after 600 ms (the concurrency test, T051).
    pub fn us4_slow_pages(mut self, pages: u64) -> Self {
        let links: String = (0..pages)
            .map(|n| format!("<a href=\"/slow/{n}\">Slow {n}</a> "))
            .collect();
        self = self.page(
            "/",
            &format!(
                "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><title>Slow</title></head>\
                 <body><h1>Slow</h1><p>Slow pages.</p>{links}</body></html>"
            ),
        );
        for n in 0..pages {
            self = self
                .page(
                    &format!("/slow/{n}"),
                    &format!(
                        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><title>Slow {n}</title>\
                         <script src=\"/slow/{n}.js\"></script></head>\
                         <body><h1>Slow {n}</h1><p>Waits for its script.</p></body></html>"
                    ),
                )
                .script(JS_HOST, &format!("/slow/{n}.js"), "window.slow = 1;")
                .delay(JS_HOST, &format!("/slow/{n}.js"), Duration::from_millis(600));
        }
        self
    }

    /// `pages` pages `/c/0` .. `/c/{pages-1}`, linked from `/`, all loading
    /// `/shared/max-age.js` (`Cache-Control: max-age=3600`) and
    /// `/shared/no-store.js` (`no-store`) (the cache test, T055).
    pub fn us4_cache_pages(mut self, pages: u64) -> Self {
        let links: String = (0..pages)
            .map(|n| format!("<a href=\"/c/{n}\">Page {n}</a> "))
            .collect();
        self = self
            .page(
                "/",
                &format!(
                    "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><title>Cache</title></head>\
                     <body><h1>Cache</h1><p>Pages sharing scripts.</p>{links}</body></html>"
                ),
            )
            .script(JS_HOST, "/shared/max-age.js", "window.cached = 1;")
            .cache_control(JS_HOST, "/shared/max-age.js", "max-age=3600")
            .script(JS_HOST, "/shared/no-store.js", "window.fresh = 1;")
            .cache_control(JS_HOST, "/shared/no-store.js", "no-store");
        for n in 0..pages {
            self = self.page(
                &format!("/c/{n}"),
                &format!(
                    "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><title>Page {n}</title>\
                     <script src=\"/shared/max-age.js\"></script>\
                     <script src=\"/shared/no-store.js\"></script></head>\
                     <body><h1>Page {n}</h1><p>Two shared scripts.</p></body></html>"
                ),
            );
        }
        self
    }

    /// Starts on a random loopback port.
    pub fn start(self) -> JsSite {
        let listener = StdListener::bind("127.0.0.1:0").expect("bind fixture port");
        self.start_on(listener)
    }

    /// Starts on a listener the caller bound (the `js-site` binary uses a fixed port).
    pub fn start_on(self, listener: StdListener) -> JsSite {
        listener.set_nonblocking(true).expect("nonblocking");
        let addr = listener.local_addr().expect("local addr");
        let port = addr.port();
        let resources = self
            .resources
            .into_iter()
            .map(|(k, mut r)| {
                if let Ok(text) = String::from_utf8(r.body.clone()) {
                    r.body = fill(&text, port).into_bytes();
                }
                r.headers = r.headers.into_iter().map(|(n, v)| (n, fill(&v, port))).collect();
                (k, r)
            })
            .collect();
        let shared = Arc::new(Shared {
            resources,
            started: Instant::now(),
            stats: Mutex::new(Stats::default()),
        });
        let (ready_tx, ready_rx) = mpsc::channel();
        let state = shared.clone();
        thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .expect("fixture runtime");
            rt.block_on(async move {
                let app = Router::new().fallback(serve).with_state(state);
                let listener = tokio::net::TcpListener::from_std(listener).expect("tokio listener");
                ready_tx.send(()).ok();
                axum::serve(listener, app).await.ok();
            });
        });
        ready_rx.recv().expect("fixture started");
        JsSite { addr, shared }
    }
}

/// `{base:HOST}` to `http://HOST:PORT`.
fn fill(s: &str, port: u16) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find("{base:") {
        out.push_str(&rest[..i]);
        let after = &rest[i + 6..];
        let end = after.find('}').expect("closing brace");
        let _ = write!(out, "http://{}:{port}", &after[..end]);
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

/// A cheap deterministic mix (splitmix64), as in `bench-site`.
fn mix(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9e37_79b9_7f4a_7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

/// About 150 KB of script, so moving script bytes costs something.
fn vendor_js() -> String {
    let mut s = String::from("window.vendor = {};\n");
    let mut i = 0;
    while s.len() < 150_000 {
        let _ = writeln!(
            s,
            "window.vendor.f{i} = function (a, b) {{ return (a * {i} + b) % 9973; /* padding padding padding */ }};"
        );
        i += 1;
    }
    s
}

const RUNTIME_JS: &str = "window.runtime = { el: function (tag, text) { \
    var e = document.createElement(tag); if (text) { e.textContent = text; } return e; } };\n";

const APP_JS: &str = r#"(function () {
  var n = document.body.getAttribute('data-page');
  Promise.all([
    fetch('/api/page/' + n + '.json').then(function (r) { return r.json(); }),
    fetch('/api/links/' + n + '.json').then(function (r) { return r.json(); })
  ]).then(function (res) {
    var page = res[0], links = res[1], el = window.runtime.el;
    document.title = page.title;
    var meta = document.createElement('meta');
    meta.name = 'description';
    meta.content = page.description;
    document.head.appendChild(meta);
    var app = document.getElementById('app');
    app.textContent = '';
    app.appendChild(el('h1', page.heading));
    for (var i = 0; i < page.paragraphs; i++) {
      app.appendChild(el('p', 'Paragraph ' + i + ' of ' + page.heading + '. ' + window.vendor.f1(i, 7)));
    }
    var ul = el('ul');
    links.forEach(function (l) {
      var li = el('li'), a = el('a', 'Page ' + l);
      a.href = '/app/' + l;
      li.appendChild(a);
      ul.appendChild(li);
    });
    app.appendChild(ul);
    document.body.setAttribute('data-ready', '1');
  });
})();
"#;

const APP_CSS: &str = "body { font-family: sans-serif; margin: 2em; } h1 { font-size: 2em; } \
    #app ul { list-style: none; } #app a { color: #0645ad; }\n";

#[derive(Default)]
struct Stats {
    requests: u64,
    per_path: HashMap<(String, String), u64>,
    open: HashMap<String, u64>,
    peak_open: HashMap<String, u64>,
    log: Vec<LogEntry>,
}

/// One logged request.
#[derive(Clone, Debug)]
pub struct LogEntry {
    pub host: String,
    pub path: String,
    /// Milliseconds since the server started.
    pub at_ms: u64,
}

struct Shared {
    resources: HashMap<(String, String), Resource>,
    started: Instant,
    stats: Mutex<Stats>,
}

/// Decrements the open count of a host when the answer is done.
struct Open {
    shared: Arc<Shared>,
    host: String,
}

impl Drop for Open {
    fn drop(&mut self) {
        let mut s = self.shared.stats.lock().expect("stats lock");
        if let Some(n) = s.open.get_mut(&self.host) {
            *n = n.saturating_sub(1);
        }
    }
}

async fn serve(State(shared): State<Arc<Shared>>, req: Request) -> Response {
    let host = req
        .headers()
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("")
        .split(':')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    // A client without a host override (the CLI) reaches the site host by
    // its address.
    let host = if host == "127.0.0.1" || host == "localhost" {
        JS_HOST.to_string()
    } else {
        host
    };
    let path = req
        .uri()
        .path_and_query()
        .map(|p| p.as_str().to_string())
        .unwrap_or_else(|| "/".into());
    match path.as_str() {
        "/__stats" => return stats_response(&shared),
        "/__reset" => {
            *shared.stats.lock().expect("stats lock") = Stats::default();
            return Response::new(Body::from("ok\n"));
        }
        _ => {}
    }
    let _open = {
        let mut s = shared.stats.lock().expect("stats lock");
        s.requests += 1;
        *s.per_path.entry((host.clone(), path.clone())).or_default() += 1;
        let open = s.open.entry(host.clone()).or_default();
        *open += 1;
        let open = *open;
        let peak = s.peak_open.entry(host.clone()).or_default();
        *peak = (*peak).max(open);
        let at_ms = shared.started.elapsed().as_millis() as u64;
        s.log.push(LogEntry {
            host: host.clone(),
            path: path.clone(),
            at_ms,
        });
        Open {
            shared: shared.clone(),
            host: host.clone(),
        }
    };
    let Some(r) = shared.resources.get(&(host, path)) else {
        return Response::builder()
            .status(StatusCode::NOT_FOUND)
            .header(header::CONTENT_TYPE, "text/html")
            .body(Body::from("<title>Not found</title>"))
            .expect("response");
    };
    if !r.delay.is_zero() {
        tokio::time::sleep(r.delay).await;
    }
    let mut b = Response::builder()
        .status(r.status)
        .header(header::CONTENT_TYPE, r.content_type.as_str());
    if let Some(cc) = &r.cache_control {
        b = b.header(header::CACHE_CONTROL, cc.as_str());
    }
    for (n, v) in &r.headers {
        b = b.header(
            HeaderName::from_bytes(n.as_bytes()).expect("header name"),
            HeaderValue::from_str(v).expect("header value"),
        );
    }
    b.body(Body::from(r.body.clone())).expect("response")
}

fn json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn stats_response(shared: &Shared) -> Response {
    let s = shared.stats.lock().expect("stats lock");
    let mut body = format!("{{\"requests\":{},\"per_path\":{{", s.requests);
    let mut keys: Vec<_> = s.per_path.iter().collect();
    keys.sort();
    for (i, ((host, path), n)) in keys.into_iter().enumerate() {
        let sep = if i == 0 { "" } else { "," };
        let _ = write!(body, "{sep}{}:{n}", json_str(&format!("{host}{path}")));
    }
    body.push_str("},\"peak_open\":{");
    let mut peaks: Vec<_> = s.peak_open.iter().collect();
    peaks.sort();
    for (i, (host, n)) in peaks.into_iter().enumerate() {
        let sep = if i == 0 { "" } else { "," };
        let _ = write!(body, "{sep}{}:{n}", json_str(host));
    }
    body.push_str("},\"log\":[");
    for (i, e) in s.log.iter().enumerate() {
        let sep = if i == 0 { "" } else { "," };
        let _ = write!(
            body,
            "{sep}{{\"host\":{},\"path\":{},\"at_ms\":{}}}",
            json_str(&e.host),
            json_str(&e.path),
            e.at_ms
        );
    }
    body.push_str("]}\n");
    Response::builder()
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body))
        .expect("response")
}

/// A running JavaScript fixture site.
pub struct JsSite {
    addr: SocketAddr,
    shared: Arc<Shared>,
}

impl JsSite {
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    pub fn port(&self) -> u16 {
        self.addr.port()
    }

    /// `http://HOST:PORT/PATH`
    /// The site's address without a host override: `http://127.0.0.1:PORT`
    /// plus `path`, answered as [`JS_HOST`].
    pub fn local_url(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{path}", self.port())
    }

    pub fn url(&self, host: &str, path: &str) -> String {
        format!("http://{host}:{}{path}", self.addr.port())
    }

    /// DNS overrides pointing both hosts at this server.
    pub fn resolve(&self) -> Vec<(String, SocketAddr)> {
        vec![(JS_HOST.into(), self.addr), (CDN_HOST.into(), self.addr)]
    }

    /// Requests for one path on one host.
    pub fn count(&self, host: &str, path: &str) -> u64 {
        let s = self.shared.stats.lock().expect("stats lock");
        s.per_path.get(&(host.into(), path.into())).copied().unwrap_or(0)
    }

    /// All requests since the start or the last reset.
    pub fn requests(&self) -> u64 {
        self.shared.stats.lock().expect("stats lock").requests
    }

    /// The most requests to one host that were open at the same time.
    pub fn peak_open(&self, host: &str) -> u64 {
        let s = self.shared.stats.lock().expect("stats lock");
        s.peak_open.get(host).copied().unwrap_or(0)
    }

    pub fn log(&self) -> Vec<LogEntry> {
        self.shared.stats.lock().expect("stats lock").log.clone()
    }

    pub fn reset(&self) {
        *self.shared.stats.lock().expect("stats lock") = Stats::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpStream;

    /// A minimal HTTP/1.1 GET, so the test needs no client crate.
    fn get(site: &JsSite, host: &str, path: &str) -> (u16, String, String) {
        let mut s = TcpStream::connect(site.addr()).expect("connect");
        write!(
            s,
            "GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n"
        )
        .expect("write");
        let mut buf = String::new();
        s.read_to_string(&mut buf).expect("read");
        let (head, body) = buf.split_once("\r\n\r\n").expect("head");
        let status = head[9..12].parse().expect("status");
        (status, head.to_ascii_lowercase(), body.to_string())
    }

    #[test]
    fn serves_resources_per_host_and_counts_them() {
        let site = JsSiteBuilder::new()
            .page("/", "<script src=\"{base:cdn.test}/lib.js\"></script>")
            .script(CDN_HOST, "/lib.js", "1")
            .cache_control(CDN_HOST, "/lib.js", "max-age=60")
            .image(JS_HOST, "/a.png")
            .start();
        let (status, _, body) = get(&site, JS_HOST, "/");
        assert_eq!(status, 200);
        assert!(body.contains(&format!("http://cdn.test:{}/lib.js", site.port())));
        let (status, head, _) = get(&site, CDN_HOST, "/lib.js");
        assert_eq!(status, 200);
        assert!(head.contains("cache-control: max-age=60"));
        // The same path on the other host does not exist.
        assert_eq!(get(&site, JS_HOST, "/lib.js").0, 404);
        assert_eq!(site.count(CDN_HOST, "/lib.js"), 1);
        assert_eq!(site.count(JS_HOST, "/"), 1);
        assert_eq!(site.requests(), 3);
        assert_eq!(site.peak_open(JS_HOST), 1);
        let (_, _, stats) = get(&site, JS_HOST, "/__stats");
        assert!(stats.contains("\"cdn.test/lib.js\":1"), "{stats}");
        assert!(
            stats.contains("\"log\":[{\"host\":\"js.test\",\"path\":\"/\""),
            "{stats}"
        );
        site.reset();
        assert_eq!(site.requests(), 0);
    }

    #[test]
    fn peak_open_counts_overlapping_requests() {
        let site = JsSiteBuilder::new()
            .json(JS_HOST, "/slow.json", "{}")
            .delay(JS_HOST, "/slow.json", Duration::from_millis(300))
            .start();
        let site = Arc::new(site);
        let handles: Vec<_> = (0..3)
            .map(|_| {
                let site = site.clone();
                thread::spawn(move || get(&site, JS_HOST, "/slow.json").0)
            })
            .collect();
        for h in handles {
            assert_eq!(h.join().expect("join"), 200);
        }
        assert_eq!(site.peak_open(JS_HOST), 3);
        assert_eq!(site.peak_open(CDN_HOST), 0);
    }

    #[test]
    fn us1_pages_keep_script_only_content_out_of_the_raw_html() {
        let site = JsSiteBuilder::new().us1_pages().start();
        let (_, _, home) = get(&site, JS_HOST, "/");
        for p in [
            "/title-changed",
            "/js-nav",
            "/jsonld-added",
            "/noindex-added",
            "/js-redirect",
            "/server-rendered",
        ] {
            assert!(home.contains(&format!("href=\"{p}\"")), "{p}");
            assert_eq!(get(&site, JS_HOST, p).0, 200, "{p}");
        }
        assert!(!home.contains("only-js") && !home.contains("redirect-target"));
        let (_, _, title) = get(&site, JS_HOST, "/title-changed");
        assert!(title.contains("<title>Loading</title>") && title.contains("Red Shoes | Shop"));
        let (_, _, nav) = get(&site, JS_HOST, "/js-nav");
        let body = nav.split("</head>").nth(1).unwrap();
        assert!(!body.contains("<a "), "links only through the script");
        let (_, _, ld) = get(&site, JS_HOST, "/jsonld-added");
        assert!(!ld.contains("application/ld+json\""));
        let (_, _, ni) = get(&site, JS_HOST, "/noindex-added");
        assert!(!ni.contains("<meta name=\"robots\""));
        assert_eq!(get(&site, JS_HOST, "/redirect-target").0, 200);
        let (_, _, server) = get(&site, JS_HOST, "/server-rendered");
        assert!(!server.contains("<script"));
    }

    #[test]
    fn us5_pages_are_linked_from_the_home_page_and_the_missing_script_is_404() {
        let site = JsSiteBuilder::new().us5_pages().start();
        let (_, _, home) = get(&site, JS_HOST, "/");
        for p in [
            "/throws",
            "/logs",
            "/missing-script",
            "/heavy",
            "/challenge",
            "/steady",
        ] {
            assert!(home.contains(&format!("href=\"{p}\"")), "{p}");
            assert_eq!(get(&site, JS_HOST, p).0, 200, "{p}");
        }
        assert_eq!(get(&site, JS_HOST, "/missing.js").0, 404);
        let (_, _, challenge) = get(&site, JS_HOST, "/challenge");
        assert!(
            !challenge.contains("challenge-form\" action"),
            "the form comes from the script"
        );
        let (_, _, throws) = get(&site, JS_HOST, "/throws");
        assert!(throws.contains("boom from the fixture"));
    }

    #[test]
    fn app_pages_ship_three_scripts_a_stylesheet_and_two_json_requests() {
        let site = JsSiteBuilder::new().app_pages(30).start();
        let (_, _, shell) = get(&site, JS_HOST, "/app/2");
        assert!(shell.contains("<title>Loading</title>"));
        assert_eq!(shell.matches("<script src=").count(), 3);
        assert_eq!(shell.matches("rel=\"stylesheet\"").count(), 1);
        let (_, _, links) = get(&site, JS_HOST, "/api/links/2.json");
        assert!(links.starts_with("[0,21,22,"), "{links}");
        let (_, head, vendor) = get(&site, JS_HOST, "/static/vendor.js");
        assert!(vendor.len() > 140_000);
        assert!(head.contains("max-age=3600"));
        let (_, head, _) = get(&site, JS_HOST, "/api/page/2.json");
        assert!(head.contains("no-store"));
    }
}
