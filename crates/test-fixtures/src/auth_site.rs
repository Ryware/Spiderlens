//! A fixture site that asks for HTTP sign-ins (spec 025 US1): Basic, Digest
//! (MD5 and SHA-256, `qop=auth`, stale nonces) and NTLM.
//!
//! It is a small blocking HTTP/1.1 server with keep-alive, because NTLM
//! signs in a connection, not a request. It answers for any `Host`; the
//! crawler is told to resolve [`AuthSite::host`] to it. Every request is
//! logged with the scheme of its `Authorization` header and the number of
//! the connection it came on.
//!
//! NTLM: the type 3 message is checked for the right `DOMAIN\user` name
//! only. The NTLMv2 proof is not verified, because the engine's functions
//! for it are `pub(crate)`. A wrong password therefore cannot be tested
//! with NTLM, only a wrong user name.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use sha2::Digest as _;

const REALM: &str = "SpiderLens test";

/// One request as the server read it.
pub(crate) struct Request {
    pub method: String,
    /// The request target as sent (origin-form or absolute-form).
    pub target: String,
    /// Names lowercase.
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Request {
    pub(crate) fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    /// The target without its query.
    pub(crate) fn path(&self) -> &str {
        self.target.split('?').next().unwrap_or("")
    }

    pub(crate) fn query(&self) -> Option<&str> {
        self.target.split_once('?').map(|(_, q)| q)
    }
}

/// One answer.
pub(crate) struct Reply {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    /// Close the connection after this answer.
    pub close: bool,
}

impl Reply {
    pub(crate) fn html(status: u16, body: &str) -> Self {
        Self {
            status,
            headers: vec![("Content-Type".into(), "text/html; charset=utf-8".into())],
            body: body.as_bytes().to_vec(),
            close: false,
        }
    }

    pub(crate) fn header(mut self, name: &str, value: impl Into<String>) -> Self {
        self.headers.push((name.to_string(), value.into()));
        self
    }

    pub(crate) fn redirect(status: u16, location: &str) -> Self {
        Self::html(status, "<title>Moved</title>").header("Location", location)
    }
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        302 => "Found",
        401 => "Unauthorized",
        404 => "Not Found",
        407 => "Proxy Authentication Required",
        502 => "Bad Gateway",
        _ => "Status",
    }
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// Reads one request (head and, with `Content-Length`, body) from the
/// stream. `buf` keeps bytes read past the end of the request. `None` when
/// the peer closed the connection.
pub(crate) fn read_request(stream: &mut TcpStream, buf: &mut Vec<u8>) -> Option<Request> {
    let head_end = loop {
        if let Some(i) = find(buf, b"\r\n\r\n") {
            break i;
        }
        let mut chunk = [0u8; 4096];
        let n = stream.read(&mut chunk).ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&chunk[..n]);
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).into_owned();
    buf.drain(..head_end + 4);
    let mut lines = head.split("\r\n");
    let mut first = lines.next()?.split_whitespace();
    let method = first.next()?.to_string();
    let target = first.next()?.to_string();
    let headers: Vec<(String, String)> = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(n, v)| (n.trim().to_ascii_lowercase(), v.trim().to_string()))
        .collect();
    let len = headers
        .iter()
        .find(|(n, _)| n == "content-length")
        .and_then(|(_, v)| v.parse::<usize>().ok())
        .unwrap_or(0);
    while buf.len() < len {
        let mut chunk = [0u8; 4096];
        let n = stream.read(&mut chunk).ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    let body: Vec<u8> = buf.drain(..len).collect();
    Some(Request {
        method,
        target,
        headers,
        body,
    })
}

pub(crate) fn write_reply(
    stream: &mut TcpStream,
    reply: &Reply,
    head_only: bool,
    close: bool,
) -> std::io::Result<()> {
    let mut out = format!("HTTP/1.1 {} {}\r\n", reply.status, reason(reply.status));
    out.push_str(&format!("Content-Length: {}\r\n", reply.body.len()));
    for (n, v) in &reply.headers {
        out.push_str(&format!("{n}: {v}\r\n"));
    }
    out.push_str(if close {
        "Connection: close\r\n\r\n"
    } else {
        "Connection: keep-alive\r\n\r\n"
    });
    let mut bytes = out.into_bytes();
    if !head_only {
        bytes.extend_from_slice(&reply.body);
    }
    stream.write_all(&bytes)?;
    stream.flush()
}

/// Serves on a random 127.0.0.1 port, one thread per connection. `S` is
/// the state of one connection. The handler gets the request, the
/// connection's number (from 1) and that state.
pub(crate) fn serve<S, F>(handler: F) -> SocketAddr
where
    S: Default + Send + 'static,
    F: Fn(&Request, usize, &mut S) -> Reply + Send + Sync + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind fixture port");
    let addr = listener.local_addr().expect("local addr");
    let handler = Arc::new(handler);
    let next = Arc::new(AtomicUsize::new(1));
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let handler = handler.clone();
            let id = next.fetch_add(1, Ordering::SeqCst);
            thread::spawn(move || {
                stream.set_nodelay(true).ok();
                let mut state = S::default();
                let mut buf = Vec::new();
                while let Some(req) = read_request(&mut stream, &mut buf) {
                    let reply = handler(&req, id, &mut state);
                    let close = reply.close
                        || req
                            .header("connection")
                            .is_some_and(|v| v.eq_ignore_ascii_case("close"));
                    if write_reply(&mut stream, &reply, req.method == "HEAD", close).is_err() || close {
                        break;
                    }
                }
            });
        }
    });
    addr
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// A value that is different every call and hard to guess: a hash of the
/// clock and a counter.
pub(crate) fn random_token(counter: &AtomicUsize) -> String {
    let n = counter.fetch_add(1, Ordering::SeqCst);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let digest = sha2::Sha256::digest(format!("spiderlens-{n}-{nanos}").as_bytes());
    hex(&digest[..16])
}

/// The hash a Digest challenge names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DigestAlgorithm {
    Md5,
    Sha256,
}

impl DigestAlgorithm {
    fn name(self) -> &'static str {
        match self {
            Self::Md5 => "MD5",
            Self::Sha256 => "SHA-256",
        }
    }

    fn hash(self, data: &str) -> String {
        match self {
            Self::Md5 => hex(&md5::Md5::digest(data.as_bytes())),
            Self::Sha256 => hex(&sha2::Sha256::digest(data.as_bytes())),
        }
    }
}

/// How a protected path asks for a sign-in.
#[derive(Clone, Debug)]
pub enum Scheme {
    Basic {
        user: String,
        password: String,
    },
    Digest {
        user: String,
        password: String,
        algorithm: DigestAlgorithm,
    },
    /// `user` is what the client sends, such as `CORP\ann`. Only the name
    /// is checked (see the module notes).
    Ntlm {
        user: String,
        password: String,
    },
}

impl Scheme {
    pub fn basic(user: &str, password: &str) -> Self {
        Self::Basic {
            user: user.into(),
            password: password.into(),
        }
    }

    pub fn digest(user: &str, password: &str, algorithm: DigestAlgorithm) -> Self {
        Self::Digest {
            user: user.into(),
            password: password.into(),
            algorithm,
        }
    }

    pub fn ntlm(user: &str, password: &str) -> Self {
        Self::Ntlm {
            user: user.into(),
            password: password.into(),
        }
    }
}

/// One logged request.
#[derive(Clone, Debug)]
pub struct AuthHit {
    /// Path without the query.
    pub path: String,
    /// Lowercase scheme of the `Authorization` header: `basic`, `digest`,
    /// `ntlm`; `None` when the request carried none.
    pub scheme: Option<String>,
    /// The whole `Authorization` header.
    pub authorization: Option<String>,
    /// Which connection (numbered from 1) the request came on.
    pub connection: usize,
}

struct Page {
    links: Vec<String>,
    delay_ms: u64,
}

#[derive(Default)]
struct ConnAuth {
    ntlm_ok: bool,
}

struct State {
    pages: HashMap<String, Page>,
    protected: Vec<(String, Scheme)>,
    log: Mutex<Vec<AuthHit>>,
    nonce: Mutex<String>,
    counter: AtomicUsize,
}

pub struct AuthSiteBuilder {
    host: String,
    pages: HashMap<String, Page>,
    protected: Vec<(String, Scheme)>,
}

impl AuthSiteBuilder {
    /// An HTML page whose `<a href>` links go to `links`.
    pub fn page(mut self, path: &str, links: &[&str]) -> Self {
        self.pages.insert(
            path.to_string(),
            Page {
                links: links.iter().map(|l| (*l).to_string()).collect(),
                delay_ms: 0,
            },
        );
        self
    }

    /// A page that answers after `delay_ms` (to keep a crawl running).
    pub fn slow_page(mut self, path: &str, delay_ms: u64) -> Self {
        self.pages.insert(
            path.to_string(),
            Page {
                links: Vec::new(),
                delay_ms,
            },
        );
        self
    }

    /// Every path starting with `prefix` needs `scheme`. The longest
    /// matching prefix wins. `/robots.txt` is never protected.
    pub fn protect(mut self, prefix: &str, scheme: Scheme) -> Self {
        self.protected.push((prefix.to_string(), scheme));
        self
    }

    pub fn start(self) -> AuthSite {
        let state = Arc::new(State {
            pages: self.pages,
            protected: self.protected,
            log: Mutex::new(Vec::new()),
            nonce: Mutex::new(String::new()),
            counter: AtomicUsize::new(0),
        });
        let first = random_token(&state.counter);
        *state.nonce.lock().expect("nonce lock") = first;
        let shared = state.clone();
        let addr =
            serve(move |req: &Request, conn: usize, auth: &mut ConnAuth| handle(&shared, req, conn, auth));
        AuthSite {
            addr,
            host: self.host,
            state,
        }
    }
}

pub struct AuthSite {
    addr: SocketAddr,
    host: String,
    state: Arc<State>,
}

impl AuthSite {
    pub fn builder(host: &str) -> AuthSiteBuilder {
        AuthSiteBuilder {
            host: host.to_string(),
            pages: HashMap::new(),
            protected: Vec::new(),
        }
    }

    pub fn host(&self) -> &str {
        &self.host
    }

    pub fn port(&self) -> u16 {
        self.addr.port()
    }

    /// `http://HOST:PORT`, the origin a sign-in belongs to.
    pub fn origin(&self) -> String {
        format!("http://{}:{}", self.host, self.addr.port())
    }

    pub fn url(&self, path: &str) -> String {
        format!("{}{path}", self.origin())
    }

    /// DNS overrides pointing the host at this server.
    pub fn resolve(&self) -> Vec<(String, SocketAddr)> {
        vec![(self.host.clone(), self.addr)]
    }

    pub fn hits(&self) -> Vec<AuthHit> {
        self.state.log.lock().expect("log lock").clone()
    }

    /// Requests for one path, whatever they carried.
    pub fn count(&self, path: &str) -> usize {
        self.hits().iter().filter(|h| h.path == path).count()
    }

    /// Makes the Digest nonce the clients hold stale: their next request
    /// gets `401` with `stale=true` and a new nonce.
    pub fn expire_nonce(&self) {
        let fresh = random_token(&self.state.counter);
        *self.state.nonce.lock().expect("nonce lock") = fresh;
    }
}

fn unauthorized(challenge: impl Into<String>) -> Reply {
    Reply::html(401, "<html><head><title>401</title></head></html>").header("WWW-Authenticate", challenge)
}

impl State {
    fn scheme_for(&self, path: &str) -> Option<&Scheme> {
        if path == "/robots.txt" {
            return None;
        }
        self.protected
            .iter()
            .filter(|(p, _)| path.starts_with(p.as_str()))
            .max_by_key(|(p, _)| p.len())
            .map(|(_, s)| s)
    }

    fn page(&self, path: &str) -> Reply {
        match self.pages.get(path) {
            Some(p) => {
                if p.delay_ms > 0 {
                    thread::sleep(Duration::from_millis(p.delay_ms));
                }
                let anchors: String = p
                    .links
                    .iter()
                    .map(|l| format!("<a href=\"{l}\">{l}</a>\n"))
                    .collect();
                Reply::html(
                    200,
                    &format!(
                        "<!doctype html><html><head><title>{path}</title></head><body>{anchors}</body></html>"
                    ),
                )
            }
            None => Reply::html(404, "<html><head><title>Not found</title></head></html>"),
        }
    }

    /// `None` when the request is allowed, else the 401 to send.
    fn check(
        &self,
        scheme: &Scheme,
        method: &str,
        authorization: Option<&str>,
        conn: &mut ConnAuth,
    ) -> Option<Reply> {
        match scheme {
            Scheme::Basic { user, password } => {
                let expected = format!("Basic {}", B64.encode(format!("{user}:{password}")));
                if authorization == Some(expected.as_str()) {
                    None
                } else {
                    Some(unauthorized(format!("Basic realm=\"{REALM}\"")))
                }
            }
            Scheme::Digest {
                user,
                password,
                algorithm,
            } => {
                let challenge = |stale: bool| {
                    let nonce = self.nonce.lock().expect("nonce lock").clone();
                    format!(
                        "Digest realm=\"{REALM}\", qop=\"auth\", algorithm={}, nonce=\"{nonce}\", opaque=\"spiderlens-opaque\"{}",
                        algorithm.name(),
                        if stale { ", stale=true" } else { "" }
                    )
                };
                let Some(rest) = authorization.and_then(|a| a.strip_prefix("Digest ")) else {
                    return Some(unauthorized(challenge(false)));
                };
                let p = parse_params(rest);
                let get = |k: &str| p.get(k).map(String::as_str).unwrap_or("");
                let current = self.nonce.lock().expect("nonce lock").clone();
                if get("username") != user {
                    return Some(unauthorized(challenge(false)));
                }
                if get("nonce") != current {
                    // An old nonce is not a wrong password (RFC 7616 3.3).
                    let fresh = random_token(&self.counter);
                    *self.nonce.lock().expect("nonce lock") = fresh;
                    return Some(unauthorized(challenge(true)));
                }
                let ha1 = algorithm.hash(&format!("{user}:{REALM}:{password}"));
                let ha2 = algorithm.hash(&format!("{method}:{}", get("uri")));
                let expected = algorithm.hash(&format!(
                    "{ha1}:{current}:{}:{}:auth:{ha2}",
                    get("nc"),
                    get("cnonce")
                ));
                if expected == get("response") {
                    None
                } else {
                    Some(unauthorized(challenge(false)))
                }
            }
            Scheme::Ntlm { user, .. } => {
                if conn.ntlm_ok {
                    return None;
                }
                let message = authorization
                    .and_then(|a| a.strip_prefix("NTLM "))
                    .and_then(|t| B64.decode(t.trim()).ok());
                match message.as_deref().and_then(ntlm_type) {
                    Some(1) => Some(unauthorized(format!("NTLM {}", B64.encode(ntlm_challenge())))),
                    Some(3) => {
                        let sent = message.as_deref().and_then(ntlm_user);
                        if sent.is_some_and(|s| s.eq_ignore_ascii_case(user)) {
                            conn.ntlm_ok = true;
                            None
                        } else {
                            Some(unauthorized("NTLM"))
                        }
                    }
                    _ => Some(unauthorized("NTLM")),
                }
            }
        }
    }
}

fn handle(state: &State, req: &Request, conn: usize, conn_auth: &mut ConnAuth) -> Reply {
    let path = req.path().to_string();
    let authorization = req.header("authorization").map(str::to_string);
    state.log.lock().expect("log lock").push(AuthHit {
        path: path.clone(),
        scheme: authorization
            .as_deref()
            .and_then(|a| a.split_whitespace().next())
            .map(str::to_ascii_lowercase),
        authorization: authorization.clone(),
        connection: conn,
    });
    if let Some(scheme) = state.scheme_for(&path)
        && let Some(refusal) = state.check(scheme, &req.method, authorization.as_deref(), conn_auth)
    {
        return refusal;
    }
    state.page(&path)
}

/// `k=v, k="v"` pairs of a Digest `Authorization` header, names lowercase.
fn parse_params(s: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let mut chars = s.chars().peekable();
    loop {
        while matches!(chars.peek(), Some(c) if c.is_whitespace() || *c == ',') {
            chars.next();
        }
        let mut key = String::new();
        while let Some(&c) = chars.peek() {
            if c == '=' {
                break;
            }
            key.push(c);
            chars.next();
        }
        if chars.next().is_none() {
            break;
        }
        let mut value = String::new();
        if chars.peek() == Some(&'"') {
            chars.next();
            while let Some(c) = chars.next() {
                match c {
                    '\\' => {
                        if let Some(n) = chars.next() {
                            value.push(n);
                        }
                    }
                    '"' => break,
                    _ => value.push(c),
                }
            }
        } else {
            while let Some(&c) = chars.peek() {
                if c == ',' {
                    break;
                }
                value.push(c);
                chars.next();
            }
        }
        out.insert(key.trim().to_ascii_lowercase(), value.trim().to_string());
    }
    out
}

const NTLM_SIGNATURE: &[u8; 8] = b"NTLMSSP\0";

fn ntlm_type(m: &[u8]) -> Option<u32> {
    if m.get(..8)? != NTLM_SIGNATURE {
        return None;
    }
    Some(u32::from_le_bytes(m.get(8..12)?.try_into().ok()?))
}

/// A type 2 message: a fixed challenge, Unicode, no target information.
fn ntlm_challenge() -> Vec<u8> {
    let mut m = Vec::with_capacity(48);
    m.extend_from_slice(NTLM_SIGNATURE);
    m.extend_from_slice(&2u32.to_le_bytes());
    // Target name: length, capacity, offset.
    m.extend_from_slice(&[0, 0, 0, 0]);
    m.extend_from_slice(&48u32.to_le_bytes());
    // Unicode, request target, NTLM, extended session security.
    m.extend_from_slice(&0x0008_0205u32.to_le_bytes());
    m.extend_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
    m.extend_from_slice(&[0u8; 8]);
    // Target information: empty.
    m.extend_from_slice(&[0, 0, 0, 0]);
    m.extend_from_slice(&48u32.to_le_bytes());
    m
}

/// `DOMAIN\user` (or `user`) from a type 3 message.
fn ntlm_user(m: &[u8]) -> Option<String> {
    let flags = u32::from_le_bytes(m.get(60..64)?.try_into().ok()?);
    let field = |at: usize| -> Option<String> {
        let len = u16::from_le_bytes(m.get(at..at + 2)?.try_into().ok()?) as usize;
        let offset = u32::from_le_bytes(m.get(at + 4..at + 8)?.try_into().ok()?) as usize;
        let raw = m.get(offset..offset + len)?;
        Some(if flags & 1 != 0 {
            let units: Vec<u16> = raw
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect();
            String::from_utf16_lossy(&units)
        } else {
            String::from_utf8_lossy(raw).into_owned()
        })
    };
    let domain = field(28)?;
    let user = field(36)?;
    Some(if domain.is_empty() {
        user
    } else {
        format!("{domain}\\{user}")
    })
}
