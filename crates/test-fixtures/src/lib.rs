//! A local fixture website for tests (constitution principle VII, FR-026).
//!
//! It binds to 127.0.0.1 on a random port, so tests run in parallel and never
//! touch a live site. Each fixture runs on its own thread with its own Tokio
//! runtime, so both async engine tests and sync CLI tests can use it.

pub mod a11y_site;
pub mod auth_site;
pub mod google_apis;
pub mod js_site;
pub mod links;
pub mod login_site;
pub mod near_dup;
pub mod pdf;
pub mod perf_providers;
pub mod providers;
pub mod proxy;
pub mod report;
pub mod sheets;
mod site;

pub use site::{Hit, Site, SiteBuilder};

use std::net::{SocketAddr, TcpListener as StdListener};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::extract::Path;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::get;

/// What `/robots.txt` returns.
#[derive(Clone, Copy, Debug, Default)]
pub enum RobotsMode {
    /// Rules that block `/private/` for SpiderLens.
    #[default]
    Rules,
    /// A plain status code with no body (for example 404 or 503).
    Status(u16),
}

pub const ROBOTS_TXT: &str =
    "User-agent: SpiderLens\nDisallow: /private/\n\nUser-agent: *\nDisallow: /admin/\n";

pub struct Fixture {
    addr: SocketAddr,
    scheme: &'static str,
}

impl Fixture {
    /// Plain HTTP fixture with the default robots.txt.
    pub fn start() -> Self {
        Self::start_with(RobotsMode::Rules)
    }

    pub fn start_with(robots: RobotsMode) -> Self {
        Self::spawn(robots, false)
    }

    /// HTTPS fixture with a self-signed certificate, which clients must reject.
    pub fn start_self_signed_tls() -> Self {
        Self::spawn(RobotsMode::Rules, true)
    }

    fn spawn(robots: RobotsMode, tls: bool) -> Self {
        let listener = StdListener::bind("127.0.0.1:0").expect("bind fixture port");
        listener.set_nonblocking(true).expect("nonblocking");
        let addr = listener.local_addr().expect("local addr");
        let (ready_tx, ready_rx) = mpsc::channel();
        thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .expect("fixture runtime");
            rt.block_on(async move {
                let app = routes(robots);
                if tls {
                    // reqwest and this server must agree on one crypto provider.
                    let _ = rustls::crypto::ring::default_provider().install_default();
                    let cert =
                        rcgen::generate_simple_self_signed(vec!["localhost".into(), "127.0.0.1".into()])
                            .expect("self-signed cert");
                    let config = axum_server::tls_rustls::RustlsConfig::from_pem(
                        cert.cert.pem().into_bytes(),
                        cert.key_pair.serialize_pem().into_bytes(),
                    )
                    .await
                    .expect("tls config");
                    ready_tx.send(()).ok();
                    axum_server::from_tcp_rustls(listener, config)
                        .serve(app.into_make_service())
                        .await
                        .ok();
                } else {
                    let listener = tokio::net::TcpListener::from_std(listener).expect("tokio listener");
                    ready_tx.send(()).ok();
                    axum::serve(listener, app).await.ok();
                }
            });
        });
        ready_rx.recv().expect("fixture started");
        Self {
            addr,
            scheme: if tls { "https" } else { "http" },
        }
    }

    pub fn base_url(&self) -> String {
        format!("{}://{}", self.scheme, self.addr)
    }

    pub fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url(), path)
    }
}

/// A local address with nothing listening on it (connection refused).
pub fn closed_port_url() -> String {
    let listener = StdListener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().expect("addr");
    drop(listener);
    format!("http://{addr}/")
}

/// A host under the reserved `.invalid` TLD (RFC 2606). It never resolves.
pub const UNRESOLVABLE_URL: &str = "http://spiderlens-fixture.invalid/";

/// Size of the `/huge` page: 2 MiB, larger than the 1 MiB limit tests use.
pub const HUGE_BYTES: usize = 2 * 1024 * 1024;

fn html(body: &str) -> Response {
    (
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        body.to_string(),
    )
        .into_response()
}

fn routes(robots: RobotsMode) -> Router {
    Router::new()
        .route(
            "/robots.txt",
            get(move || async move {
                match robots {
                    RobotsMode::Rules => ([(header::CONTENT_TYPE, "text/plain")], ROBOTS_TXT).into_response(),
                    RobotsMode::Status(code) => StatusCode::from_u16(code).unwrap().into_response(),
                }
            }),
        )
        .route(
            "/",
            get(|| async { html("<html><head><title>Home | Example</title></head><body>Hi</body></html>") }),
        )
        .route(
            "/home",
            get(|| async { html("<html><head><title>Home | Example</title></head><body>Hi</body></html>") }),
        )
        .route("/old", get(|| async { Redirect::permanent("/mid") }))
        .route(
            "/mid",
            get(|| async { (StatusCode::FOUND, [(header::LOCATION, "/home")]) }),
        )
        .route("/loop-a", get(|| async { Redirect::temporary("/loop-b") }))
        .route("/loop-b", get(|| async { Redirect::temporary("/loop-a") }))
        .route(
            "/chain/{n}",
            get(|Path(n): Path<u32>| async move {
                if n == 0 {
                    html("<title>End of chain</title>")
                } else {
                    Redirect::temporary(&format!("/chain/{}", n - 1)).into_response()
                }
            }),
        )
        .route(
            "/no-title",
            get(|| async { html("<html><head></head><body>No title here</body></html>") }),
        )
        .route(
            "/empty-title",
            get(|| async { html("<html><head><title>  \n </title></head></html>") }),
        )
        .route(
            "/entities",
            get(|| async { html("<title>Tom &amp; Jerry &mdash; &#x263A;</title>") }),
        )
        .route(
            "/whitespace",
            get(|| async { html("<title>\n   Lots   of\t space \n</title>") }),
        )
        .route(
            "/hebrew-1255",
            get(|| async {
                let (bytes, _, _) = encoding_rs::WINDOWS_1255.encode("<title>שלום עולם</title>");
                (
                    [(header::CONTENT_TYPE, "text/html; charset=windows-1255")],
                    bytes.into_owned(),
                )
                    .into_response()
            }),
        )
        .route(
            "/shift-jis-meta",
            get(|| async {
                let (bytes, _, _) =
                    encoding_rs::SHIFT_JIS.encode("<meta charset=\"Shift_JIS\"><title>こんにちは</title>");
                ([(header::CONTENT_TYPE, "text/html")], bytes.into_owned()).into_response()
            }),
        )
        .route(
            "/image.png",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "image/png")],
                    vec![0x89u8, b'P', b'N', b'G'],
                )
                    .into_response()
            }),
        )
        .route(
            "/doc.pdf",
            get(|| async {
                ([(header::CONTENT_TYPE, "application/pdf")], b"%PDF-1.7".to_vec()).into_response()
            }),
        )
        .route(
            "/echo",
            get(|headers: axum::http::HeaderMap| async move {
                // Echoes request headers in the title so tests can see them.
                let get = |n: &str| {
                    headers
                        .get(n)
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("-")
                        .to_string()
                };
                html(&format!(
                    "<title>UA={} X-Key={}</title>",
                    get("user-agent"),
                    get("x-key")
                ))
            }),
        )
        .route(
            "/redirect",
            get(
                |axum::extract::Query(q): axum::extract::Query<std::collections::HashMap<String, String>>| async move {
                    Redirect::temporary(q.get("to").map(String::as_str).unwrap_or("/")).into_response()
                },
            ),
        )
        .route("/private/page", get(|| async { html("<title>Secret</title>") }))
        .route(
            "/slow",
            get(|| async {
                tokio::time::sleep(Duration::from_secs(10)).await;
                html("<title>Too late</title>")
            }),
        )
        .route(
            "/huge",
            get(|| async {
                // No Content-Length, so the client must enforce the cap while streaming.
                let chunk = vec![b'a'; 64 * 1024];
                let chunks = HUGE_BYTES / chunk.len();
                let stream = futures_util::stream::iter(
                    (0..chunks).map(move |_| Ok::<_, std::io::Error>(chunk.clone())),
                );
                let mut resp = Response::new(Body::from_stream(stream));
                resp.headers_mut()
                    .insert(header::CONTENT_TYPE, HeaderValue::from_static("text/html"));
                resp
            }),
        )
        .fallback(|| async {
            (
                StatusCode::NOT_FOUND,
                [(header::CONTENT_TYPE, "text/html")],
                "<html><head><title>Not found</title></head></html>",
            )
        })
}
