//! A multi-host fixture site for crawl tests (spec 003 research R13).
//!
//! One server on 127.0.0.1 answers for several `.test` host names, chosen by
//! the `Host` header; the crawler is told to resolve those names locally.
//! Every request is logged with its time, so tests can check "fetched exactly
//! once" and crawl-delay gaps. In page bodies, `{base:HOST}` becomes
//! `http://HOST:PORT`.

use std::collections::HashMap;
use std::net::{SocketAddr, TcpListener as StdListener};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::Instant;

use axum::Router;
use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{HeaderName, HeaderValue, StatusCode, header};
use axum::response::Response;

#[derive(Clone, Debug)]
enum Reply {
    Body {
        status: u16,
        content_type: String,
        body: Vec<u8>,
        headers: Vec<(String, String)>,
        /// Gzip the body after placeholders are filled in.
        gzip: bool,
    },
    Redirect {
        status: u16,
        location: String,
    },
    /// The n-th request gets the n-th reply; the last one repeats (spec 004
    /// scripted failures).
    Sequence(Vec<Reply>),
    /// Headers never come.
    Hang,
    /// 200 with a body that stops part-way and the connection drops.
    Truncated { sent: Vec<u8> },
}

/// One logged request.
#[derive(Clone, Debug)]
pub struct Hit {
    pub host: String,
    /// Path and query, as requested.
    pub path: String,
    pub at: Instant,
    pub user_agent: String,
    /// Every request header, names lowercase.
    pub headers: Vec<(String, String)>,
}

#[derive(Default)]
pub struct SiteBuilder {
    replies: HashMap<(String, String), Reply>,
    hosts: Vec<String>,
}

impl SiteBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    fn put(mut self, host: &str, path: &str, reply: Reply) -> Self {
        if !self.hosts.iter().any(|h| h == host) {
            self.hosts.push(host.to_string());
        }
        self.replies.insert((host.to_string(), path.to_string()), reply);
        self
    }

    /// An HTML page. `links` become `<a href>` elements after `extra_html`.
    pub fn page(self, host: &str, path: &str, extra_html: &str, links: &[&str]) -> Self {
        let anchors: String = links
            .iter()
            .map(|l| format!("<a href=\"{l}\">{l}</a>\n"))
            .collect();
        let body = format!(
            "<!doctype html><html><head><title>{path}</title></head><body>{extra_html}{anchors}</body></html>"
        );
        self.html(host, path, &body)
    }

    /// A raw HTML document.
    pub fn html(self, host: &str, path: &str, html: &str) -> Self {
        self.raw(host, path, 200, "text/html; charset=utf-8", html.as_bytes())
    }

    pub fn raw(self, host: &str, path: &str, status: u16, content_type: &str, body: &[u8]) -> Self {
        self.put(
            host,
            path,
            Reply::Body {
                status,
                content_type: content_type.into(),
                body: body.to_vec(),
                headers: Vec::new(),
                gzip: false,
            },
        )
    }

    /// A gzip-compressed file (placeholders are filled in first).
    pub fn gzip(self, host: &str, path: &str, content_type: &str, text: &str) -> Self {
        self.put(
            host,
            path,
            Reply::Body {
                status: 200,
                content_type: content_type.into(),
                body: text.as_bytes().to_vec(),
                headers: Vec::new(),
                gzip: true,
            },
        )
    }

    /// Adds a response header to an existing page.
    pub fn header(mut self, host: &str, path: &str, name: &str, value: &str) -> Self {
        if let Some(Reply::Body { headers, .. }) = self.replies.get_mut(&(host.into(), path.into())) {
            headers.push((name.into(), value.into()));
        }
        self
    }

    pub fn redirect(self, host: &str, path: &str, status: u16, location: &str) -> Self {
        self.put(
            host,
            path,
            Reply::Redirect {
                status,
                location: location.into(),
            },
        )
    }

    /// A bare status code with a short HTML body.
    pub fn status(self, host: &str, path: &str, status: u16) -> Self {
        self.raw(host, path, status, "text/html", format!("<title>{status}</title>").as_bytes())
    }

    /// Answers `fail_status` the first `times` requests, then an HTML page
    /// linking to `links` (spec 004 US2.1).
    pub fn flaky(self, host: &str, path: &str, fail_status: u16, times: usize, links: &[&str]) -> Self {
        let fail = Reply::Body {
            status: fail_status,
            content_type: "text/html".into(),
            body: format!("<title>{fail_status}</title><a href=\"/should-not-be-followed\">x</a>").into_bytes(),
            headers: Vec::new(),
            gzip: false,
        };
        let anchors: String = links.iter().map(|l| format!("<a href=\"{l}\">{l}</a>\n")).collect();
        let ok = Reply::Body {
            status: 200,
            content_type: "text/html; charset=utf-8".into(),
            body: format!("<!doctype html><title>{path}</title><body>{anchors}</body>").into_bytes(),
            headers: Vec::new(),
            gzip: false,
        };
        let mut steps = vec![fail; times];
        steps.push(ok);
        self.put(host, path, Reply::Sequence(steps))
    }

    /// A status with a `Retry-After` header, every time.
    pub fn retry_after(self, host: &str, path: &str, status: u16, retry_after: &str) -> Self {
        self.status(host, path, status).header(host, path, "retry-after", retry_after)
    }

    /// Never answers (for timeouts).
    pub fn hang(self, host: &str, path: &str) -> Self {
        self.put(host, path, Reply::Hang)
    }

    /// Sends part of a page, then drops the connection.
    pub fn truncated(self, host: &str, path: &str) -> Self {
        self.put(
            host,
            path,
            Reply::Truncated {
                sent: b"<!doctype html><title>cut</title><p>Only the first part".to_vec(),
            },
        )
    }

    pub fn robots(self, host: &str, text: &str) -> Self {
        self.raw(host, "/robots.txt", 200, "text/plain", text.as_bytes())
    }

    pub fn robots_status(self, host: &str, status: u16) -> Self {
        self.raw(host, "/robots.txt", status, "text/plain", b"")
    }

    /// Registers a host with no pages (everything 404s).
    pub fn host(mut self, host: &str) -> Self {
        if !self.hosts.iter().any(|h| h == host) {
            self.hosts.push(host.to_string());
        }
        self
    }

    pub fn start(self) -> Site {
        let listener = StdListener::bind("127.0.0.1:0").expect("bind fixture port");
        listener.set_nonblocking(true).expect("nonblocking");
        let addr = listener.local_addr().expect("local addr");
        let log = Arc::new(Mutex::new(Vec::new()));
        let replies: HashMap<_, _> = self
            .replies
            .into_iter()
            .map(|(k, v)| (k, substitute(v, addr.port())))
            .collect();
        let down = Arc::new(Mutex::new(std::collections::HashSet::new()));
        let state = Arc::new(Shared {
            replies,
            log: log.clone(),
            served: Mutex::new(HashMap::new()),
            down: down.clone(),
        });
        let (ready_tx, ready_rx) = mpsc::channel();
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
        Site {
            addr,
            hosts: self.hosts,
            log,
            down,
        }
    }
}

fn substitute(reply: Reply, port: u16) -> Reply {
    let fix = |s: &str| -> String {
        let mut out = String::new();
        let mut rest = s;
        while let Some(i) = rest.find("{base:") {
            out.push_str(&rest[..i]);
            let after = &rest[i + 6..];
            let end = after.find('}').expect("closing brace");
            out.push_str(&format!("http://{}:{port}", &after[..end]));
            rest = &after[end + 1..];
        }
        out.push_str(rest);
        out.replace("{port}", &port.to_string())
    };
    match reply {
        Reply::Body {
            status,
            content_type,
            body,
            headers,
            gzip,
        } => {
            let body = match String::from_utf8(body) {
                Ok(text) => fix(&text).into_bytes(),
                Err(e) => e.into_bytes(),
            };
            let body = if gzip {
                use std::io::Write;
                let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
                gz.write_all(&body).expect("gzip");
                gz.finish().expect("gzip")
            } else {
                body
            };
            Reply::Body {
                status,
                content_type,
                body,
                headers: headers.into_iter().map(|(n, v)| (n, fix(&v))).collect(),
                gzip: false,
            }
        }
        Reply::Redirect { status, location } => Reply::Redirect {
            status,
            location: fix(&location),
        },
        Reply::Sequence(steps) => Reply::Sequence(steps.into_iter().map(|r| substitute(r, port)).collect()),
        other @ (Reply::Hang | Reply::Truncated { .. }) => other,
    }
}

struct Shared {
    replies: HashMap<(String, String), Reply>,
    log: Arc<Mutex<Vec<Hit>>>,
    /// Requests served per (host, path), for sequences.
    served: Mutex<HashMap<(String, String), usize>>,
    /// Hosts that answer 503 to everything (an outage).
    down: Arc<Mutex<std::collections::HashSet<String>>>,
}

async fn serve(State(state): State<Arc<Shared>>, req: Request) -> Response {
    let host = req
        .headers()
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("")
        .split(':')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    let path = req
        .uri()
        .path_and_query()
        .map(|p| p.as_str().to_string())
        .unwrap_or_else(|| "/".into());
    let user_agent = req
        .headers()
        .get(header::USER_AGENT)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("")
        .to_string();
    let headers = req
        .headers()
        .iter()
        .map(|(k, v)| (k.as_str().to_string(), String::from_utf8_lossy(v.as_bytes()).into_owned()))
        .collect();
    state.log.lock().expect("log lock").push(Hit {
        host: host.clone(),
        path: path.clone(),
        at: Instant::now(),
        user_agent,
        headers,
    });
    if state.down.lock().expect("down lock").contains(&host) {
        return Response::builder()
            .status(StatusCode::SERVICE_UNAVAILABLE)
            .body(Body::from("down"))
            .expect("response");
    }
    let key = (host, path);
    let mut reply = state.replies.get(&key);
    if let Some(Reply::Sequence(steps)) = reply {
        let mut served = state.served.lock().expect("served lock");
        let n = served.entry(key.clone()).or_insert(0);
        reply = steps.get(*n).or(steps.last());
        *n += 1;
    }
    let mut builder = Response::builder();
    match reply {
        Some(Reply::Hang) => {
            tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
            builder.status(StatusCode::OK).body(Body::empty()).expect("response")
        }
        Some(Reply::Truncated { sent }) => {
            let first: Result<axum::body::Bytes, std::io::Error> = Ok(axum::body::Bytes::from(sent.clone()));
            let cut: Result<axum::body::Bytes, std::io::Error> =
                Err(std::io::Error::new(std::io::ErrorKind::ConnectionReset, "cut"));
            // Pause after the first chunk so the headers and the start of the
            // body reach the client before the connection drops.
            use futures_util::StreamExt;
            let stream = futures_util::stream::iter(vec![first]).chain(futures_util::stream::once(async move {
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                cut
            }));
            builder
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "text/html")
                .header(header::CONTENT_LENGTH, (sent.len() * 10).to_string())
                .body(Body::from_stream(stream))
                .expect("response")
        }
        Some(Reply::Sequence(_)) => unreachable!("sequences are resolved above"),
        Some(Reply::Body {
            status,
            content_type,
            body,
            headers,
            ..
        }) => {
            builder = builder
                .status(*status)
                .header(header::CONTENT_TYPE, content_type.as_str());
            for (n, v) in headers {
                builder = builder.header(
                    HeaderName::from_bytes(n.as_bytes()).expect("header name"),
                    HeaderValue::from_str(v).expect("header value"),
                );
            }
            builder.body(Body::from(body.clone())).expect("response")
        }
        Some(Reply::Redirect { status, location }) => builder
            .status(*status)
            .header(header::LOCATION, location.as_str())
            .body(Body::empty())
            .expect("response"),
        None => builder
            .status(StatusCode::NOT_FOUND)
            .header(header::CONTENT_TYPE, "text/html")
            .body(Body::from("<title>Not found</title>"))
            .expect("response"),
    }
}

pub struct Site {
    addr: SocketAddr,
    hosts: Vec<String>,
    log: Arc<Mutex<Vec<Hit>>>,
    down: Arc<Mutex<std::collections::HashSet<String>>>,
}

impl Site {
    pub fn port(&self) -> u16 {
        self.addr.port()
    }

    /// `http://HOST:PORT/PATH`
    pub fn url(&self, host: &str, path: &str) -> String {
        format!("http://{host}:{}{path}", self.addr.port())
    }

    /// DNS overrides pointing every registered host at this server.
    pub fn resolve(&self) -> Vec<(String, SocketAddr)> {
        self.hosts.iter().map(|h| (h.clone(), self.addr)).collect()
    }

    /// Starts or ends an outage: while down, the host answers 503 to every
    /// request (spec 004 US4).
    pub fn set_down(&self, host: &str, down: bool) {
        let mut d = self.down.lock().expect("down lock");
        if down {
            d.insert(host.to_string());
        } else {
            d.remove(host);
        }
    }

    pub fn hits(&self) -> Vec<Hit> {
        self.log.lock().expect("log lock").clone()
    }

    /// Requests for one path on one host, robots.txt included if asked.
    pub fn count(&self, host: &str, path: &str) -> usize {
        self.hits()
            .iter()
            .filter(|h| h.host == host && h.path == path)
            .count()
    }

    /// Requests other than robots.txt.
    pub fn page_hits(&self) -> Vec<Hit> {
        self.hits()
            .into_iter()
            .filter(|h| h.path != "/robots.txt")
            .collect()
    }
}
