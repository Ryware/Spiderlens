//! Proxy auto-configuration (PAC) scripts (spec 025, system proxy).
//!
//! A PAC script is JavaScript with a `FindProxyForURL(url, host)` function
//! that names the proxy for each address. It is run by `boa_engine`, a
//! JavaScript engine written in Rust. Its context is not `Send`, so one
//! dedicated thread owns it; callers send it a request over a channel and
//! wait for the answer. The answer for each site (`scheme://host:port`) is
//! kept, so the script runs once per site however many requests follow.
//! A script that fails or takes longer than five seconds means a direct
//! connection for that site.

use std::collections::HashMap;
use std::fmt;
use std::net::{IpAddr, ToSocketAddrs, UdpSocket};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use boa_engine::{Context, JsResult, JsString, JsValue, NativeFunction, Source, js_string};
use spiderlens_config::ProxyKind;
use url::Url;

use super::proxy::ProxyProblem;

/// How long the script may take to answer for one site.
const EVALUATION_TIMEOUT: Duration = Duration::from_secs(5);
/// How long starting up (download and compile) may take.
const STARTUP_TIMEOUT: Duration = Duration::from_secs(15);
/// How long downloading the script may take.
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(10);

/// One entry of the script's answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PacChoice {
    Direct,
    Proxy { kind: ProxyKind, host: String, port: u16 },
}

impl PacChoice {
    /// The proxy address reqwest takes, or `None` for a direct connection.
    pub(crate) fn proxy_url(&self) -> Option<String> {
        match self {
            Self::Direct => None,
            Self::Proxy { kind, host, port } => {
                let host = if host.contains(':') && !host.starts_with('[') {
                    format!("[{host}]")
                } else {
                    host.clone()
                };
                Some(format!("{}://{host}:{port}", kind.scheme()))
            }
        }
    }
}

/// What a caller sends to the script thread: the URL given to the script,
/// its host, and where to send the answer (empty when the script failed).
type Request = (String, String, Sender<String>);

/// Where the script thread gets its script from.
enum Origin {
    Inline(String),
    Download(String),
}

/// A running PAC script.
pub(crate) struct Pac {
    source: String,
    requests: Mutex<Sender<Request>>,
    /// `scheme://host:port` to the proxy address, or `None` for direct.
    cache: Mutex<HashMap<String, Option<String>>>,
}

impl fmt::Debug for Pac {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Pac({})", self.source)
    }
}

/// Two handles are equal when they are the same script instance.
impl PartialEq for Pac {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self, other)
    }
}

impl Eq for Pac {}

impl Pac {
    /// Loads the script at `location`: `http`, `https` or `file`.
    pub(crate) fn load(location: &str) -> Result<Self, ProxyProblem> {
        let url = Url::parse(location.trim())
            .map_err(|e| ProxyProblem::PacUnreadable(format!("{location}: {e}")))?;
        match url.scheme() {
            "file" => {
                let path = url
                    .to_file_path()
                    .map_err(|()| ProxyProblem::PacUnreadable(format!("{location}: not a file path")))?;
                let script = std::fs::read_to_string(&path)
                    .map_err(|e| ProxyProblem::PacUnreadable(format!("{}: {e}", path.display())))?;
                Self::start(location, Origin::Inline(script))
            }
            "http" | "https" => Self::start(location, Origin::Download(url.to_string())),
            other => Err(ProxyProblem::PacUnreadable(format!(
                "{location}: the address starts with {other}, not http, https or file"
            ))),
        }
    }

    /// A script given as text.
    pub(crate) fn from_script(script: &str) -> Result<Self, ProxyProblem> {
        Self::start("inline script", Origin::Inline(script.to_string()))
    }

    fn start(source: &str, origin: Origin) -> Result<Self, ProxyProblem> {
        let (request_tx, request_rx) = mpsc::channel::<Request>();
        let (startup_tx, startup_rx) = mpsc::channel::<Result<(), String>>();
        std::thread::Builder::new()
            .name("spiderlens-pac".into())
            .spawn(move || worker(origin, &startup_tx, &request_rx))
            .map_err(|e| ProxyProblem::PacUnreadable(e.to_string()))?;
        match startup_rx.recv_timeout(STARTUP_TIMEOUT) {
            Ok(Ok(())) => Ok(Self {
                source: source.to_string(),
                requests: Mutex::new(request_tx),
                cache: Mutex::new(HashMap::new()),
            }),
            Ok(Err(detail)) => Err(ProxyProblem::PacUnreadable(detail)),
            Err(_) => Err(ProxyProblem::PacUnreadable(
                "the script did not load in time".into(),
            )),
        }
    }

    /// The script's answer for `url`, in order. Not cached. A failing or
    /// slow script gives `[Direct]`.
    pub(crate) fn find(&self, url: &Url) -> Vec<PacChoice> {
        let Some(host) = url.host_str() else {
            return vec![PacChoice::Direct];
        };
        let (reply_tx, reply_rx) = mpsc::channel::<String>();
        let sent = self
            .requests
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .send((script_url(url), host.to_string(), reply_tx));
        if sent.is_err() {
            return vec![PacChoice::Direct];
        }
        let choices = match reply_rx.recv_timeout(EVALUATION_TIMEOUT) {
            Ok(answer) => parse_result(&answer),
            Err(_) => Vec::new(),
        };
        if choices.is_empty() {
            vec![PacChoice::Direct]
        } else {
            choices
        }
    }

    /// The proxy address for `url`, or `None` for direct. Answered from
    /// the cache after the first request to a site.
    pub(crate) fn lookup(&self, url: &Url) -> Option<String> {
        let host = url.host_str()?;
        let key = format!(
            "{}://{host}:{}",
            url.scheme(),
            url.port_or_known_default().unwrap_or(0)
        );
        {
            let cache = self.cache.lock().unwrap_or_else(PoisonError::into_inner);
            if let Some(hit) = cache.get(&key) {
                return hit.clone();
            }
        }
        let answer = self.find(url).first().and_then(PacChoice::proxy_url);
        self.cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(key, answer.clone());
        answer
    }
}

/// The URL browsers give the script: HTTPS without path and query, others
/// without user name, password and fragment.
fn script_url(url: &Url) -> String {
    if url.scheme() == "https" {
        let host = url.host_str().unwrap_or_default();
        match url.port() {
            Some(p) => format!("https://{host}:{p}/"),
            None => format!("https://{host}/"),
        }
    } else {
        let mut stripped = url.clone();
        let _ = stripped.set_username("");
        let _ = stripped.set_password(None);
        stripped.set_fragment(None);
        stripped.to_string()
    }
}

/// `PROXY h:p; SOCKS5 h:p; SOCKS h:p; HTTPS h:p; DIRECT`.
pub(crate) fn parse_result(answer: &str) -> Vec<PacChoice> {
    let mut out: Vec<PacChoice> = Vec::new();
    for part in answer.split(';') {
        let mut words = part.split_whitespace();
        let Some(word) = words.next() else { continue };
        let (kind, default_port) = match word.to_ascii_uppercase().as_str() {
            "DIRECT" => {
                out.push(PacChoice::Direct);
                continue;
            }
            "PROXY" | "HTTP" => (ProxyKind::Http, 80u16),
            "HTTPS" => (ProxyKind::Https, 443u16),
            "SOCKS" | "SOCKS5" => (ProxyKind::Socks5, 1080u16),
            _ => continue,
        };
        let Some(address) = words.next() else { continue };
        let (host, port) = match address.rsplit_once(':') {
            Some((h, p)) if !h.ends_with(':') => match p.parse::<u16>() {
                Ok(port) => (h, port),
                Err(_) => continue,
            },
            _ => (address, default_port),
        };
        let host = host.trim_start_matches('[').trim_end_matches(']');
        if host.is_empty() {
            continue;
        }
        out.push(PacChoice::Proxy {
            kind,
            host: host.to_string(),
            port,
        });
    }
    out
}

/// The script thread: gets the script, compiles it, reports once, then
/// answers requests until the `Pac` is dropped.
fn worker(origin: Origin, startup: &Sender<Result<(), String>>, requests: &Receiver<Request>) {
    let script = match origin {
        Origin::Inline(s) => s,
        Origin::Download(url) => match download(&url) {
            Ok(s) => s,
            Err(detail) => {
                let _ = startup.send(Err(detail));
                return;
            }
        },
    };
    let mut ctx = match build_context(&script) {
        Ok(c) => c,
        Err(detail) => {
            let _ = startup.send(Err(detail));
            return;
        }
    };
    let _ = startup.send(Ok(()));
    while let Ok((url, host, reply)) = requests.recv() {
        let answer = evaluate(&mut ctx, &url, &host).unwrap_or_default();
        let _ = reply.send(answer);
    }
}

/// Downloads the script on a small runtime of this thread, without a
/// proxy of its own.
fn download(url: &str) -> Result<String, String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    runtime.block_on(fetch(url))
}

async fn fetch(url: &str) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(DOWNLOAD_TIMEOUT)
        .build()
        .map_err(|e| error_detail(&e))?;
    let response = client.get(url).send().await.map_err(|e| error_detail(&e))?;
    if !response.status().is_success() {
        return Err(format!("{url}: the server answered {}", response.status()));
    }
    response.text().await.map_err(|e| error_detail(&e))
}

fn error_detail(e: &reqwest::Error) -> String {
    let mut detail = e.to_string();
    let mut source = std::error::Error::source(e);
    while let Some(s) = source {
        detail = s.to_string();
        source = s.source();
    }
    detail
}

/// A context with the helper functions and the script loaded.
fn build_context(script: &str) -> Result<Context, String> {
    let mut ctx = Context::default();
    ctx.register_global_callable(js_string!("dnsResolve"), 1, NativeFunction::from_fn_ptr(dns_resolve))
        .map_err(|e| e.to_string())?;
    ctx.register_global_callable(js_string!("myIpAddress"), 0, NativeFunction::from_fn_ptr(my_ip_address))
        .map_err(|e| e.to_string())?;
    ctx.eval(Source::from_bytes(PRELUDE)).map_err(|e| e.to_string())?;
    ctx.eval(Source::from_bytes(script)).map_err(|e| e.to_string())?;
    let kind = ctx
        .eval(Source::from_bytes("typeof FindProxyForURL"))
        .map_err(|e| e.to_string())?;
    let kind = kind.to_string(&mut ctx).map_err(|e| e.to_string())?;
    if kind.to_std_string_escaped() != "function" {
        return Err("the script has no FindProxyForURL function".into());
    }
    Ok(ctx)
}

fn evaluate(ctx: &mut Context, url: &str, host: &str) -> Result<String, String> {
    let call = format!("String(FindProxyForURL({}, {}))", quote(url), quote(host));
    let value = ctx.eval(Source::from_bytes(&call)).map_err(|e| e.to_string())?;
    let text = value.to_string(ctx).map_err(|e| e.to_string())?;
    Ok(text.to_std_string_escaped())
}

/// A JavaScript string literal.
fn quote(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".to_string())
}

/// The first argument as text; empty when missing.
fn text_arg(args: &[JsValue], ctx: &mut Context) -> JsResult<String> {
    match args.first() {
        Some(v) => Ok(v.to_string(ctx)?.to_std_string_escaped()),
        None => Ok(String::new()),
    }
}

/// `dnsResolve(host)`: the first IPv4 address, or null.
fn dns_resolve(_this: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    let host = text_arg(args, ctx)?;
    let found: Option<String> = (host.as_str(), 0u16).to_socket_addrs().ok().and_then(|mut addrs| {
        addrs.find_map(|a| match a.ip() {
            IpAddr::V4(v4) => Some(v4.to_string()),
            IpAddr::V6(_) => None,
        })
    });
    match found {
        Some(ip) => Ok(JsValue::from(JsString::from(ip.as_str()))),
        None => Ok(JsValue::null()),
    }
}

/// `myIpAddress()`: the address this machine uses to reach the internet.
fn my_ip_address(_this: &JsValue, _args: &[JsValue], _ctx: &mut Context) -> JsResult<JsValue> {
    let ip = local_ipv4().unwrap_or_else(|| "127.0.0.1".to_string());
    Ok(JsValue::from(JsString::from(ip.as_str())))
}

/// Connecting a UDP socket sends nothing but picks the local address.
fn local_ipv4() -> Option<String> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    match socket.local_addr().ok()?.ip() {
        IpAddr::V4(v4) => Some(v4.to_string()),
        IpAddr::V6(_) => None,
    }
}

/// The helper functions a PAC script may call, besides the two native
/// ones (`dnsResolve`, `myIpAddress`).
const PRELUDE: &str = r#"
function alert(message) {}

function isPlainHostName(host) {
  return String(host).indexOf('.') === -1;
}

function dnsDomainIs(host, domain) {
  host = String(host).toLowerCase();
  domain = String(domain).toLowerCase();
  return host.length >= domain.length &&
    host.substring(host.length - domain.length) === domain;
}

function localHostOrDomainIs(host, hostdom) {
  host = String(host).toLowerCase();
  hostdom = String(hostdom).toLowerCase();
  if (host === hostdom) { return true; }
  if (host.indexOf('.') !== -1) { return false; }
  return hostdom.indexOf(host + '.') === 0;
}

function isResolvable(host) {
  return dnsResolve(host) !== null;
}

function _octets(address) {
  var parts = String(address).split('.');
  if (parts.length !== 4) { return null; }
  var out = [];
  for (var i = 0; i < 4; i++) {
    var n = Number(parts[i]);
    if (parts[i] === '' || isNaN(n) || n < 0 || n > 255) { return null; }
    out.push(n);
  }
  return out;
}

function convert_addr(ipchars) {
  var o = _octets(ipchars);
  if (o === null) { return 0; }
  return o[0] * 16777216 + o[1] * 65536 + o[2] * 256 + o[3];
}

function isInNet(ipaddr, pattern, maskstr) {
  var address = _octets(ipaddr) !== null ? String(ipaddr) : dnsResolve(ipaddr);
  if (address === null) { return false; }
  var h = _octets(address);
  var p = _octets(pattern);
  var m = _octets(maskstr);
  if (h === null || p === null || m === null) { return false; }
  for (var i = 0; i < 4; i++) {
    if ((h[i] & m[i]) !== (p[i] & m[i])) { return false; }
  }
  return true;
}

function dnsDomainLevels(host) {
  return String(host).split('.').length - 1;
}

function shExpMatch(str, shexp) {
  var source = String(shexp);
  var re = '^';
  for (var i = 0; i < source.length; i++) {
    var c = source.charAt(i);
    if (c === '*') { re += '.*'; }
    else if (c === '?') { re += '.'; }
    else if ('\\^$.|+()[]{}/'.indexOf(c) !== -1) { re += '\\' + c; }
    else { re += c; }
  }
  re += '$';
  return new RegExp(re).test(String(str));
}

function _inRange(value, start, end) {
  if (start <= end) { return value >= start && value <= end; }
  return value >= start || value <= end;
}

function weekdayRange(wd1, wd2, gmt) {
  var days = ['SUN', 'MON', 'TUE', 'WED', 'THU', 'FRI', 'SAT'];
  var utc = false;
  if (wd2 === 'GMT') { utc = true; wd2 = wd1; }
  else if (gmt === 'GMT') { utc = true; }
  if (wd2 === undefined) { wd2 = wd1; }
  var now = new Date();
  var today = utc ? now.getUTCDay() : now.getDay();
  var a = days.indexOf(String(wd1).toUpperCase());
  var b = days.indexOf(String(wd2).toUpperCase());
  if (a < 0 || b < 0) { return false; }
  return _inRange(today, a, b);
}

function timeRange() {
  var a = Array.prototype.slice.call(arguments);
  var utc = false;
  if (a.length > 0 && a[a.length - 1] === 'GMT') { utc = true; a.pop(); }
  var now = new Date();
  var h = utc ? now.getUTCHours() : now.getHours();
  var m = utc ? now.getUTCMinutes() : now.getMinutes();
  var s = utc ? now.getUTCSeconds() : now.getSeconds();
  var t = h * 3600 + m * 60 + s;
  if (a.length === 1) { return h === a[0]; }
  var start, end;
  if (a.length === 2) {
    start = a[0] * 3600; end = a[1] * 3600 + 3599;
  } else if (a.length === 4) {
    start = a[0] * 3600 + a[1] * 60; end = a[2] * 3600 + a[3] * 60 + 59;
  } else if (a.length === 6) {
    start = a[0] * 3600 + a[1] * 60 + a[2]; end = a[3] * 3600 + a[4] * 60 + a[5];
  } else {
    return false;
  }
  return _inRange(t, start, end);
}

function dateRange() {
  var a = Array.prototype.slice.call(arguments);
  var utc = false;
  if (a.length > 0 && a[a.length - 1] === 'GMT') { utc = true; a.pop(); }
  if (a.length === 0) { return true; }
  var months = ['JAN', 'FEB', 'MAR', 'APR', 'MAY', 'JUN', 'JUL', 'AUG', 'SEP', 'OCT', 'NOV', 'DEC'];
  var now = new Date();
  var cur = {
    d: utc ? now.getUTCDate() : now.getDate(),
    m: utc ? now.getUTCMonth() : now.getMonth(),
    y: utc ? now.getUTCFullYear() : now.getFullYear()
  };
  function parse(list) {
    var r = { d: null, m: null, y: null };
    for (var i = 0; i < list.length; i++) {
      var v = list[i];
      if (typeof v === 'string') { r.m = months.indexOf(v.toUpperCase()); }
      else if (v > 31) { r.y = v; }
      else { r.d = v; }
    }
    return r;
  }
  var first, second;
  if (a.length === 1) { first = parse(a); second = first; }
  else {
    var half = a.length / 2;
    first = parse(a.slice(0, half));
    second = parse(a.slice(half));
  }
  function key(r, c) {
    var y = first.y !== null ? (r.y !== null ? r.y : c.y) : 0;
    var m = first.m !== null ? (r.m !== null ? r.m : c.m) : 0;
    var d = first.d !== null ? (r.d !== null ? r.d : c.d) : 0;
    return y * 10000 + m * 100 + d;
  }
  return _inRange(key(cur, cur), key(first, cur), key(second, cur));
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    const SCRIPT: &str = r#"
function FindProxyForURL(url, host) {
  if (isPlainHostName(host)) return "DIRECT";
  if (dnsDomainIs(host, ".corp.example")) return "PROXY inner.corp:8080; DIRECT";
  if (shExpMatch(url, "*/blocked/*")) return "SOCKS5 sock:1080";
  if (isInNet(host, "127.0.0.0", "255.0.0.0")) return "DIRECT";
  return "PROXY gw.example:3128";
}
"#;

    fn url(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    #[test]
    fn result_strings_are_parsed_in_order() {
        let got = parse_result("PROXY a:8080; SOCKS5 b:1081; SOCKS c; HTTPS d:443 ; DIRECT; BOGUS x");
        assert_eq!(
            got,
            vec![
                PacChoice::Proxy { kind: ProxyKind::Http, host: "a".into(), port: 8080 },
                PacChoice::Proxy { kind: ProxyKind::Socks5, host: "b".into(), port: 1081 },
                PacChoice::Proxy { kind: ProxyKind::Socks5, host: "c".into(), port: 1080 },
                PacChoice::Proxy { kind: ProxyKind::Https, host: "d".into(), port: 443 },
                PacChoice::Direct,
            ]
        );
        assert!(parse_result("").is_empty());
        assert_eq!(
            parse_result("PROXY [::1]:3128")[0].proxy_url().as_deref(),
            Some("http://[::1]:3128")
        );
        assert_eq!(
            parse_result("SOCKS h:9")[0].proxy_url().as_deref(),
            Some("socks5h://h:9")
        );
        assert_eq!(PacChoice::Direct.proxy_url(), None);
    }

    #[test]
    fn script_gets_the_url_browsers_give() {
        assert_eq!(script_url(&url("https://u:p@a.example/x?y=1#z")), "https://a.example/");
        assert_eq!(script_url(&url("http://u:p@a.example:81/x?y=1#z")), "http://a.example:81/x?y=1");
    }

    #[test]
    fn script_decides_per_address() {
        let pac = Pac::from_script(SCRIPT).unwrap();
        assert_eq!(pac.find(&url("http://intranet/")), vec![PacChoice::Direct]);
        assert_eq!(
            pac.find(&url("http://app.corp.example/")),
            vec![
                PacChoice::Proxy { kind: ProxyKind::Http, host: "inner.corp".into(), port: 8080 },
                PacChoice::Direct,
            ]
        );
        assert_eq!(
            pac.find(&url("http://site.example/blocked/page")),
            vec![PacChoice::Proxy { kind: ProxyKind::Socks5, host: "sock".into(), port: 1080 }]
        );
        assert_eq!(pac.find(&url("http://127.0.0.1/")), vec![PacChoice::Direct]);
        assert_eq!(
            pac.lookup(&url("http://site.example/")).as_deref(),
            Some("http://gw.example:3128")
        );
    }

    #[test]
    fn answers_are_cached_per_site() {
        let pac = Pac::from_script(SCRIPT).unwrap();
        let first = pac.lookup(&url("http://site.example/a"));
        let second = pac.lookup(&url("http://site.example/b"));
        assert_eq!(first, second);
        assert_eq!(pac.cache.lock().unwrap().len(), 1);
        assert_eq!(pac.lookup(&url("http://intranet/")), None);
        assert_eq!(pac.cache.lock().unwrap().len(), 2);
    }

    #[test]
    fn a_script_without_an_answer_means_direct() {
        let pac = Pac::from_script("function FindProxyForURL(u, h) { throw new Error('no'); }").unwrap();
        assert_eq!(pac.lookup(&url("http://site.example/")), None);
        let pac = Pac::from_script("function FindProxyForURL(u, h) { return ''; }").unwrap();
        assert_eq!(pac.find(&url("http://site.example/")), vec![PacChoice::Direct]);
    }

    #[test]
    fn a_broken_script_is_unreadable() {
        assert!(matches!(
            Pac::from_script("function FindProxyForURL( {"),
            Err(ProxyProblem::PacUnreadable(_))
        ));
        assert!(matches!(
            Pac::from_script("var x = 1;"),
            Err(ProxyProblem::PacUnreadable(_))
        ));
        assert!(matches!(
            Pac::load("ftp://example.com/proxy.pac"),
            Err(ProxyProblem::PacUnreadable(_))
        ));
    }

    #[test]
    fn helper_functions() {
        let pac = Pac::from_script(
            r#"function FindProxyForURL(url, host) {
  var ok = shExpMatch("www.a.com", "*.a.c?m")
    && !shExpMatch("www.a.com", "*.b.com")
    && dnsDomainLevels("a.b.c") === 2
    && localHostOrDomainIs("www", "www.example.com")
    && !localHostOrDomainIs("www.other.com", "www.example.com")
    && isInNet("10.1.2.3", "10.0.0.0", "255.0.0.0")
    && !isInNet("11.1.2.3", "10.0.0.0", "255.0.0.0")
    && convert_addr("0.0.1.0") === 256
    && weekdayRange("SUN", "SAT")
    && timeRange(0, 23)
    && dateRange(2000, 2999)
    && typeof myIpAddress() === "string";
  return ok ? "DIRECT" : "PROXY failed:1";
}"#,
        )
        .unwrap();
        assert_eq!(pac.lookup(&url("http://x.example/")), None);
    }
}
