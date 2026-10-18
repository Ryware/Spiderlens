//! A local proxy for tests (spec 025 US4): HTTP forward proxy (absolute-form
//! requests), HTTP `CONNECT` and SOCKS5, on one port. The kind is told from
//! the first byte of a connection.
//!
//! The proxy resolves target names itself, from the routes it was given
//! (usually `Site::resolve()` or `AuthSite::resolve()`), and otherwise with
//! the system. A crawl that goes through it therefore needs no DNS
//! overrides of its own, and a request that bypassed the proxy would fail
//! to resolve the `.test` name: that is how a test shows that every request
//! went through.
//!
//! With [`ProxyBuilder::require_credentials`] the HTTP forms want
//! `Proxy-Authorization: Basic` (else `407`) and SOCKS5 wants user name and
//! password authentication (else the handshake fails).
//!
//! The log has one entry per forwarded HTTP request, and one per tunnel
//! (`CONNECT` or SOCKS5). Requests seen inside a tunnel are logged too
//! (their `GET` lines are read from the bytes passing through), so
//! [`ProxyServer::requests_to`] counts requests whatever the kind.

use std::io::{Read, Write};
use std::net::{Ipv4Addr, Ipv6Addr, Shutdown, SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;

use crate::auth_site::{Reply, read_request, write_reply};

/// How a logged entry reached the proxy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Via {
    /// A forwarded absolute-form request.
    Http,
    /// A `CONNECT` tunnel.
    Connect,
    /// A SOCKS5 tunnel.
    Socks5,
}

/// One logged entry.
#[derive(Clone, Debug)]
pub struct ProxyHit {
    pub via: Via,
    /// Target host name as the client sent it, lowercase.
    pub host: String,
    pub port: u16,
    /// Path and query of a request. `None` for the opening of a tunnel.
    pub path: Option<String>,
}

struct State {
    routes: Vec<(String, SocketAddr)>,
    credentials: Option<(String, String)>,
    log: Mutex<Vec<ProxyHit>>,
    denied: AtomicUsize,
}

impl State {
    fn record(&self, via: Via, host: &str, port: u16, path: Option<String>) {
        self.log.lock().expect("log lock").push(ProxyHit {
            via,
            host: host.to_ascii_lowercase(),
            port,
            path,
        });
    }

    fn upstream(&self, host: &str, port: u16) -> std::io::Result<TcpStream> {
        let addr = match self.routes.iter().find(|(h, _)| h.eq_ignore_ascii_case(host)) {
            Some((_, addr)) => *addr,
            None => (host, port).to_socket_addrs()?.next().ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::NotFound, "target does not resolve")
            })?,
        };
        TcpStream::connect(addr)
    }
}

#[derive(Default)]
pub struct ProxyBuilder {
    routes: Vec<(String, SocketAddr)>,
    credentials: Option<(String, String)>,
}

impl ProxyBuilder {
    /// Names that resolve to a local server, such as `site.resolve()`.
    pub fn routes(mut self, routes: Vec<(String, SocketAddr)>) -> Self {
        self.routes.extend(routes);
        self
    }

    pub fn route(mut self, host: &str, addr: SocketAddr) -> Self {
        self.routes.push((host.to_string(), addr));
        self
    }

    /// Refuses clients that do not present this user name and password.
    pub fn require_credentials(mut self, user: &str, password: &str) -> Self {
        self.credentials = Some((user.to_string(), password.to_string()));
        self
    }

    pub fn start(self) -> ProxyServer {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind proxy port");
        let addr = listener.local_addr().expect("local addr");
        let state = Arc::new(State {
            routes: self.routes,
            credentials: self.credentials,
            log: Mutex::new(Vec::new()),
            denied: AtomicUsize::new(0),
        });
        let shared = state.clone();
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                let state = shared.clone();
                thread::spawn(move || {
                    let _ = handle(stream, &state);
                });
            }
        });
        ProxyServer { addr, state }
    }
}

pub struct ProxyServer {
    addr: SocketAddr,
    state: Arc<State>,
}

impl ProxyServer {
    pub fn builder() -> ProxyBuilder {
        ProxyBuilder::default()
    }

    /// A proxy with no credentials that resolves `routes`.
    pub fn start(routes: Vec<(String, SocketAddr)>) -> Self {
        Self::builder().routes(routes).start()
    }

    pub fn host(&self) -> &'static str {
        "127.0.0.1"
    }

    pub fn port(&self) -> u16 {
        self.addr.port()
    }

    pub fn hits(&self) -> Vec<ProxyHit> {
        self.state.log.lock().expect("log lock").clone()
    }

    /// Requests (not tunnel openings) for `host`, all kinds.
    pub fn requests_to(&self, host: &str) -> usize {
        self.paths_to(host).len()
    }

    /// Paths of the requests for `host`, in order.
    pub fn paths_to(&self, host: &str) -> Vec<String> {
        self.hits()
            .into_iter()
            .filter(|h| h.host.eq_ignore_ascii_case(host))
            .filter_map(|h| h.path)
            .collect()
    }

    /// Tunnels (`CONNECT` or SOCKS5) opened to `host`.
    pub fn tunnels_to(&self, host: &str) -> usize {
        self.hits()
            .iter()
            .filter(|h| h.host.eq_ignore_ascii_case(host) && h.path.is_none())
            .count()
    }

    /// Clients turned away for missing or wrong credentials.
    pub fn denied(&self) -> usize {
        self.state.denied.load(Ordering::SeqCst)
    }
}

/// A local port with nothing listening on it.
pub fn closed_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    drop(listener);
    port
}

fn handle(mut client: TcpStream, state: &Arc<State>) -> std::io::Result<()> {
    client.set_nodelay(true).ok();
    let mut first = [0u8; 1];
    if client.peek(&mut first)? == 0 {
        return Ok(());
    }
    if first[0] == 5 {
        socks5(client, state)
    } else {
        http(client, state)
    }
}

fn http_authorized(state: &State, header: Option<&str>) -> bool {
    match &state.credentials {
        None => true,
        Some((user, password)) => {
            header == Some(format!("Basic {}", B64.encode(format!("{user}:{password}"))).as_str())
        }
    }
}

/// `host:port` or `host`, with the default port.
fn split_authority(authority: &str, default_port: u16) -> (String, u16) {
    match authority.rsplit_once(':') {
        Some((h, p)) if p.chars().all(|c| c.is_ascii_digit()) && !p.is_empty() => {
            (h.to_string(), p.parse().unwrap_or(default_port))
        }
        _ => (authority.to_string(), default_port),
    }
}

fn http(mut client: TcpStream, state: &Arc<State>) -> std::io::Result<()> {
    let mut buf = Vec::new();
    let Some(req) = read_request(&mut client, &mut buf) else {
        return Ok(());
    };
    if !http_authorized(state, req.header("proxy-authorization")) {
        state.denied.fetch_add(1, Ordering::SeqCst);
        let mut refusal = Reply::html(407, "<title>407</title>")
            .header("Proxy-Authenticate", "Basic realm=\"SpiderLens test proxy\"");
        refusal.close = true;
        return write_reply(&mut client, &refusal, false, true);
    }
    if req.method == "CONNECT" {
        let (host, port) = split_authority(&req.target, 443);
        let Ok(upstream) = state.upstream(&host, port) else {
            let mut bad = Reply::html(502, "<title>502</title>");
            bad.close = true;
            return write_reply(&mut client, &bad, false, true);
        };
        state.record(Via::Connect, &host, port, None);
        client.write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")?;
        relay(client, upstream, buf, sniffer(state, Via::Connect, host, port));
        return Ok(());
    }
    // Absolute-form: `GET http://host:port/path HTTP/1.1`.
    let Some(rest) = req.target.strip_prefix("http://") else {
        let mut bad = Reply::html(502, "<title>502</title>");
        bad.close = true;
        return write_reply(&mut client, &bad, false, true);
    };
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], rest[i..].to_string()),
        None => (rest, "/".to_string()),
    };
    let (host, port) = split_authority(authority, 80);
    let Ok(mut upstream) = state.upstream(&host, port) else {
        let mut bad = Reply::html(502, "<title>502</title>");
        bad.close = true;
        return write_reply(&mut client, &bad, false, true);
    };
    state.record(Via::Http, &host, port, Some(path.clone()));
    let mut out = format!("{} {path} HTTP/1.1\r\n", req.method);
    for (n, v) in &req.headers {
        if n.starts_with("proxy-") || n == "connection" {
            continue;
        }
        out.push_str(&format!("{n}: {v}\r\n"));
    }
    out.push_str("Connection: close\r\n\r\n");
    upstream.write_all(out.as_bytes())?;
    upstream.write_all(&req.body)?;
    let mut answer = Vec::new();
    upstream.read_to_end(&mut answer)?;
    client.write_all(&answer)?;
    client.flush()?;
    let _ = client.shutdown(Shutdown::Both);
    Ok(())
}

/// Logs the requests seen inside a tunnel to `host:port`.
fn sniffer(state: &Arc<State>, via: Via, host: String, port: u16) -> impl Fn(String) + Send + 'static {
    let state = Arc::clone(state);
    move |path: String| state.record(via, &host, port, Some(path))
}

fn relay(client: TcpStream, upstream: TcpStream, early: Vec<u8>, sniff: impl Fn(String) + Send + 'static) {
    let (Ok(mut client_in), Ok(mut upstream_out)) = (client.try_clone(), upstream.try_clone()) else {
        return;
    };
    thread::spawn(move || {
        let mut pending = early;
        let mut chunk = [0u8; 8192];
        loop {
            if !pending.is_empty() {
                for line in String::from_utf8_lossy(&pending).split("\r\n") {
                    if let Some(rest) = line.strip_prefix("GET ").or_else(|| line.strip_prefix("HEAD "))
                        && let Some(path) = rest.split_whitespace().next()
                    {
                        sniff(path.to_string());
                    }
                }
                if upstream_out.write_all(&pending).is_err() {
                    break;
                }
                pending.clear();
            }
            match client_in.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(n) => pending.extend_from_slice(&chunk[..n]),
            }
        }
        let _ = upstream_out.shutdown(Shutdown::Write);
    });
    let (mut upstream_in, mut client_out) = (upstream, client);
    let _ = std::io::copy(&mut upstream_in, &mut client_out);
    let _ = client_out.shutdown(Shutdown::Write);
}

fn socks5(mut client: TcpStream, state: &Arc<State>) -> std::io::Result<()> {
    let mut head = [0u8; 2];
    client.read_exact(&mut head)?;
    let mut methods = vec![0u8; head[1] as usize];
    client.read_exact(&mut methods)?;
    match &state.credentials {
        Some((user, password)) => {
            if !methods.contains(&2) {
                state.denied.fetch_add(1, Ordering::SeqCst);
                client.write_all(&[5, 0xFF])?;
                return Ok(());
            }
            client.write_all(&[5, 2])?;
            let mut version = [0u8; 1];
            client.read_exact(&mut version)?;
            let sent_user = read_len_prefixed(&mut client)?;
            let sent_password = read_len_prefixed(&mut client)?;
            if sent_user == user.as_bytes() && sent_password == password.as_bytes() {
                client.write_all(&[1, 0])?;
            } else {
                state.denied.fetch_add(1, Ordering::SeqCst);
                client.write_all(&[1, 1])?;
                return Ok(());
            }
        }
        None => client.write_all(&[5, 0])?,
    }
    let mut request = [0u8; 4];
    client.read_exact(&mut request)?;
    if request[1] != 1 {
        // Command not supported.
        client.write_all(&[5, 7, 0, 1, 0, 0, 0, 0, 0, 0])?;
        return Ok(());
    }
    let host = match request[3] {
        1 => {
            let mut a = [0u8; 4];
            client.read_exact(&mut a)?;
            Ipv4Addr::from(a).to_string()
        }
        3 => String::from_utf8_lossy(&read_len_prefixed(&mut client)?).into_owned(),
        4 => {
            let mut a = [0u8; 16];
            client.read_exact(&mut a)?;
            Ipv6Addr::from(a).to_string()
        }
        _ => return Ok(()),
    };
    let mut port = [0u8; 2];
    client.read_exact(&mut port)?;
    let port = u16::from_be_bytes(port);
    let Ok(upstream) = state.upstream(&host, port) else {
        // Host unreachable.
        client.write_all(&[5, 4, 0, 1, 0, 0, 0, 0, 0, 0])?;
        return Ok(());
    };
    client.write_all(&[5, 0, 0, 1, 0, 0, 0, 0, 0, 0])?;
    state.record(Via::Socks5, &host, port, None);
    relay(
        client,
        upstream,
        Vec::new(),
        sniffer(state, Via::Socks5, host, port),
    );
    Ok(())
}

fn read_len_prefixed(client: &mut TcpStream) -> std::io::Result<Vec<u8>> {
    let mut len = [0u8; 1];
    client.read_exact(&mut len)?;
    let mut data = vec![0u8; len[0] as usize];
    client.read_exact(&mut data)?;
    Ok(data)
}
