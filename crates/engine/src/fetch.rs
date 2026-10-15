//! HTTP fetching with manual redirect following and a capped, streamed body
//! (FR-012, FR-013, FR-018, FR-019; research R3, R4).

use std::collections::HashSet;
use std::error::Error as _;

use reqwest::{Client, ClientBuilder, Response, StatusCode, header};
use url::Url;

use crate::net::ntlm::NtlmPool;
use crate::net::{AuthNote, AuthOutcome, NetSession};
use crate::{CheckError, CheckOptions, ErrorClass, ErrorReason, RedirectHop, RequestHeader};

pub(crate) fn build_client(opts: &CheckOptions) -> Result<Client, CheckError> {
    build_client_with(opts, &[])
}

/// `resolve` pins host names to addresses; tests use it to serve several
/// `.test` host names from 127.0.0.1 (spec 003 research R13).
pub(crate) fn build_client_with(
    opts: &CheckOptions,
    resolve: &[(String, std::net::SocketAddr)],
) -> Result<Client, CheckError> {
    finish(client_builder(opts, resolve))
}

/// The client settings every request shares; `net::NetSession` adds the
/// cookie jar when a crawl needs one.
pub(crate) fn client_builder(
    opts: &CheckOptions,
    resolve: &[(String, std::net::SocketAddr)],
) -> ClientBuilder {
    let mut builder = Client::builder();
    for (host, addr) in resolve {
        builder = builder.resolve(host, *addr);
    }
    builder
        .user_agent(&opts.user_agent)
        // We follow redirects ourselves so every hop is recorded.
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(opts.connect_timeout)
        .read_timeout(opts.read_timeout)
        // Proxy support arrives with spec 025's proxy settings. Until
        // then, requests go direct so results do not depend on the shell's
        // environment.
        .no_proxy()
}

pub(crate) fn finish(builder: ClientBuilder) -> Result<Client, CheckError> {
    builder.build().map_err(|e| {
        CheckError::new(
            ErrorClass::Connection,
            format!("Could not start the HTTP client: {e}"),
        )
    })
}

/// What to do when a body reaches the size limit.
#[derive(Clone, Copy)]
pub(crate) enum OnLimit {
    /// Report a content problem (pages).
    Fail,
    /// Keep what was read so far (robots.txt).
    Truncate,
}

pub(crate) struct BodyPolicy<'a> {
    pub max_bytes: u64,
    pub on_limit: OnLimit,
    /// Decides from the content type whether the bytes are kept. Bytes that
    /// are not kept are still read and counted, but dropped chunk by chunk.
    pub keep: &'a (dyn Fn(Option<&str>) -> bool + Sync),
}

pub(crate) struct Fetched {
    pub status: StatusCode,
    pub final_url: Url,
    pub content_type: Option<String>,
    pub charset: Option<String>,
    pub body: Option<Vec<u8>>,
}

/// The redirect hops are returned even when the fetch fails part-way.
pub(crate) struct FollowOutcome {
    pub hops: Vec<RedirectHop>,
    pub result: Result<Fetched, CheckError>,
}

pub(crate) async fn get_following(
    client: &Client,
    start: &Url,
    max_redirects: usize,
    body: &BodyPolicy<'_>,
    headers: &[RequestHeader],
) -> FollowOutcome {
    let mut hops = Vec::new();
    let result = follow(client, start, max_redirects, body, headers, &mut hops).await;
    FollowOutcome { hops, result }
}

async fn follow(
    client: &Client,
    start: &Url,
    max_redirects: usize,
    body: &BodyPolicy<'_>,
    headers: &[RequestHeader],
    hops: &mut Vec<RedirectHop>,
) -> Result<Fetched, CheckError> {
    let mut current = start.clone();
    let mut visited: HashSet<Url> = HashSet::from([current.clone()]);

    loop {
        let mut request = client.get(current.clone());
        // Internal-only headers go to the requested page's host, never to a
        // host a redirect leads to (spec 002, FR-019).
        for h in headers
            .iter()
            .filter(|h| h.all_hosts || same_host(&current, start))
        {
            request = request.header(&h.name, &h.value);
        }
        let response = request.send().await.map_err(|e| classify(&e))?;
        let status = response.status();

        if let Some(location) = redirect_location(&response) {
            let mut target = current.join(location).map_err(|_| {
                CheckError::new(
                    ErrorClass::RedirectProblem,
                    format!("The server redirected to an invalid address: {location}"),
                )
                .with_reason(ErrorReason::InvalidTarget)
            })?;
            target.set_fragment(None);
            if !matches!(target.scheme(), "http" | "https") {
                return Err(CheckError::new(
                    ErrorClass::RedirectProblem,
                    format!("The server redirected to an unsupported address: {target}"),
                )
                .with_reason(ErrorReason::InvalidTarget));
            }
            hops.push(RedirectHop {
                url: current.to_string(),
                status: status.as_u16(),
                location: target.to_string(),
            });
            if hops.len() > max_redirects {
                return Err(CheckError::new(
                    ErrorClass::RedirectProblem,
                    format!("The redirect chain is longer than the limit of {max_redirects} hops."),
                )
                .with_reason(ErrorReason::OverLimit));
            }
            if !visited.insert(target.clone()) {
                return Err(CheckError::new(
                    ErrorClass::RedirectProblem,
                    format!("The redirects loop back to {target}, which was already visited."),
                )
                .with_reason(ErrorReason::Loop));
            }
            current = target;
            continue;
        }

        let (content_type, charset) = parse_content_type(&response);
        let keep = (body.keep)(content_type.as_deref());
        let bytes = read_body(response, body.max_bytes, body.on_limit, keep).await?;
        return Ok(Fetched {
            status,
            final_url: current,
            content_type,
            charset,
            body: bytes,
        });
    }
}

/// One response without following redirects, for the crawler (spec 003
/// research R8): a redirect is recorded and its target becomes a discovery.
pub(crate) struct Once {
    pub status: u16,
    /// Raw `Location` of a 3xx response.
    pub location: Option<String>,
    pub content_type: Option<String>,
    pub charset: Option<String>,
    pub x_robots_tag: Vec<String>,
    pub link_header: Vec<String>,
    /// Raw `Retry-After`, read on 429 and 503 (spec 008 FR-015).
    pub retry_after: Option<String>,
    /// HTML bodies only, and only when `read_body` was set.
    pub body: Option<Vec<u8>>,
    /// The body was cut off (size limit or dropped connection); `body`
    /// holds the part received and this says why (spec 005 plan D7).
    pub body_problem: Option<CheckError>,
    /// Every response header in received order, names lowercase.
    pub headers: Vec<(String, String)>,
    /// `Content-Length` as sent, when reqwest did not decompress the body
    /// (it removes the header when it does).
    pub content_length: Option<u64>,
    /// A PDF body, when PDFs are read (spec 027 part C).
    pub pdf: Option<PdfBody>,
    /// A 401 no sign-in answered, or that refused the credentials (spec
    /// 025 FR-004, FR-005).
    pub auth: Option<AuthNote>,
}

/// A downloaded PDF. Over the PDF size limit nothing is kept: a partial
/// PDF cannot be read (its index is at the end).
pub(crate) struct PdfBody {
    pub bytes: Vec<u8>,
    pub over_limit: bool,
}

/// PDFs by type, or by a `.pdf` path when the server sends a generic type.
pub(crate) fn is_pdf(url: &Url, content_type: Option<&str>) -> bool {
    match content_type {
        Some("application/pdf" | "application/x-pdf") => true,
        None | Some("application/octet-stream" | "binary/octet-stream" | "application/download") => {
            url.path().to_ascii_lowercase().ends_with(".pdf")
        }
        _ => false,
    }
}

/// One request through the crawl's session: its proxy and cookies, and
/// for internal URLs its sign-ins (spec 025 plan D3). A 401 that a sign-in
/// can answer is sent once more with credentials; the 401s that remain are
/// noted for the URL.
pub(crate) async fn fetch_once(
    net: &NetSession,
    url: &Url,
    headers: &[RequestHeader],
    internal: bool,
    read_body: bool,
    max_bytes: u64,
    pdf_max_bytes: Option<u64>,
) -> Result<Once, CheckError> {
    let build = |client: &Client| {
        let mut request = client.get(url.clone());
        for h in headers.iter().filter(|h| h.all_hosts || internal) {
            request = request.header(&h.name, &h.value);
        }
        request
    };
    // NTLM origins answer on their own connections (plan D4).
    if let Some(pool) = net.ntlm_for(url, internal) {
        return via_ntlm(&pool, &build, url, read_body, max_bytes, pdf_max_bytes).await;
    }
    let (request, sent) = net.authorize(build(net.client()), url, internal);
    let mut response = request.send().await.map_err(|e| classify(&e))?;
    let mut auth = None;
    if internal && response.status() == StatusCode::UNAUTHORIZED {
        match net.on_unauthorized(url, response.headers(), sent) {
            AuthOutcome::RetryWithCredentials => {
                drop(response);
                let (request, sent) = net.authorize(build(net.client()), url, internal);
                response = request.send().await.map_err(|e| classify(&e))?;
                if response.status() == StatusCode::UNAUTHORIZED {
                    auth = Some(match net.on_unauthorized(url, response.headers(), sent) {
                        AuthOutcome::Required { schemes, supported } => {
                            AuthNote::Required { schemes, supported }
                        }
                        AuthOutcome::Rejected { method } => AuthNote::Rejected { method },
                        // A second stale nonce in a row counts as refused.
                        AuthOutcome::RetryWithCredentials | AuthOutcome::UseNtlm => {
                            AuthNote::Rejected { method: "digest" }
                        }
                    });
                }
            }
            AuthOutcome::UseNtlm => {
                if let Some(pool) = net.ntlm_for(url, internal) {
                    drop(response);
                    return via_ntlm(&pool, &build, url, read_body, max_bytes, pdf_max_bytes).await;
                }
            }
            AuthOutcome::Rejected { method } => auth = Some(AuthNote::Rejected { method }),
            AuthOutcome::Required { schemes, supported } => {
                auth = Some(AuthNote::Required { schemes, supported })
            }
        }
    }
    read_once(response, url, read_body, max_bytes, pdf_max_bytes, auth).await
}

/// A request on an NTLM connection, kept reserved until the body is read.
async fn via_ntlm(
    pool: &NtlmPool,
    build: &(dyn Fn(&Client) -> reqwest::RequestBuilder + Sync),
    url: &Url,
    read_body: bool,
    max_bytes: u64,
    pdf_max_bytes: Option<u64>,
) -> Result<Once, CheckError> {
    let (response, slot, refused) = pool.send(build).await.map_err(|e| classify(&e))?;
    let auth = refused.then_some(AuthNote::Rejected { method: "ntlm" });
    let once = read_once(response, url, read_body, max_bytes, pdf_max_bytes, auth).await;
    drop(slot);
    once
}

async fn read_once(
    response: Response,
    url: &Url,
    read_body: bool,
    max_bytes: u64,
    pdf_max_bytes: Option<u64>,
    auth: Option<AuthNote>,
) -> Result<Once, CheckError> {
    let status = response.status().as_u16();
    let location = redirect_location(&response).map(str::to_string);
    let (content_type, charset) = parse_content_type(&response);
    let all = |name: header::HeaderName| -> Vec<String> {
        response
            .headers()
            .get_all(name)
            .iter()
            .filter_map(|v| v.to_str().ok().map(str::to_string))
            .collect()
    };
    let x_robots_tag = all(header::HeaderName::from_static("x-robots-tag"));
    let all_headers: Vec<(String, String)> = response
        .headers()
        .iter()
        .map(|(k, v)| {
            (
                k.as_str().to_string(),
                String::from_utf8_lossy(v.as_bytes()).into_owned(),
            )
        })
        .collect();
    let content_length = response
        .headers()
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse().ok());
    let link_header = all(header::LINK);
    let retry_after = response
        .headers()
        .get(header::RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let html = content_type
        .as_deref()
        .is_none_or(|ct| ct == "text/html" || ct == "application/xhtml+xml");
    let pdf_limit = pdf_max_bytes.filter(|_| {
        read_body
            && location.is_none()
            && (200..300).contains(&status)
            && is_pdf(url, content_type.as_deref())
    });
    let mut pdf = None;
    let (body, body_problem) = if let Some(limit) = pdf_limit {
        if content_length.is_some_and(|n| n > limit) {
            pdf = Some(PdfBody {
                bytes: Vec::new(),
                over_limit: true,
            });
            (None, None)
        } else {
            let (bytes, problem) = read_body_partial(response, limit).await;
            let over_limit = problem.is_some() && bytes.len() as u64 >= limit;
            pdf = Some(PdfBody {
                bytes: if over_limit { Vec::new() } else { bytes },
                over_limit,
            });
            // Over the PDF limit is a PDF fact, not a response problem.
            (None, problem.filter(|_| !over_limit))
        }
    } else if read_body && location.is_none() && html {
        let (bytes, problem) = read_body_partial(response, max_bytes).await;
        (Some(bytes), problem)
    } else {
        // Dropping the response closes the stream without reading the body.
        (None, None)
    };
    Ok(Once {
        status,
        location,
        content_type,
        charset,
        x_robots_tag,
        link_header,
        retry_after,
        body,
        body_problem,
        headers: all_headers,
        content_length,
        pdf,
        auth,
    })
}

/// A page resource the renderer asked for (spec 014 plan D4 step 5).
pub(crate) struct ResourceResponse {
    pub status: u16,
    /// Names lowercase, in received order. The body is decoded, so a
    /// `Content-Encoding` header describes the wire, not `body`.
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    /// Raw `Retry-After`, read on 429 and 503.
    pub retry_after: Option<String>,
}

/// Browser request headers not forwarded: the client sets them itself, or
/// they describe the browser's own connection.
const NOT_FORWARDED: &[&str] = &[
    "host",
    "user-agent",
    "cookie",
    "content-length",
    "connection",
    "keep-alive",
    "accept-encoding",
    "upgrade",
    "te",
    "trailer",
    "transfer-encoding",
    "proxy-connection",
    "proxy-authorization",
];

/// Fetches one resource for the renderer with the crawl's client, without
/// following redirects (the browser follows them, so each hop is a request
/// that goes through the same permits). The crawl's custom headers are
/// added as for pages.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn fetch_resource(
    client: &Client,
    url: &Url,
    method: &str,
    browser_headers: &[(String, String)],
    body: Option<Vec<u8>>,
    crawl_headers: &[RequestHeader],
    internal: bool,
    max_bytes: u64,
) -> Result<ResourceResponse, CheckError> {
    let method = reqwest::Method::from_bytes(method.as_bytes()).unwrap_or(reqwest::Method::GET);
    let mut request = client.request(method, url.clone());
    for (n, v) in browser_headers {
        if !NOT_FORWARDED.iter().any(|x| n.eq_ignore_ascii_case(x)) {
            request = request.header(n.as_str(), v.as_str());
        }
    }
    for h in crawl_headers.iter().filter(|h| h.all_hosts || internal) {
        request = request.header(&h.name, &h.value);
    }
    if let Some(b) = body {
        request = request.body(b);
    }
    let response = request.send().await.map_err(|e| classify(&e))?;
    let status = response.status().as_u16();
    let headers = response
        .headers()
        .iter()
        .map(|(k, v)| {
            (
                k.as_str().to_string(),
                String::from_utf8_lossy(v.as_bytes()).into_owned(),
            )
        })
        .collect();
    let retry_after = response
        .headers()
        .get(header::RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let (body, problem) = read_body_partial(response, max_bytes).await;
    // A body cut by a timeout fails the request; otherwise what was received
    // is passed on, as a browser would get it.
    if let Some(p) = problem
        && p.class == ErrorClass::Timeout
    {
        return Err(p);
    }
    Ok(ResourceResponse {
        status,
        headers,
        body,
        retry_after,
    })
}

/// Reads up to `max_bytes`. Unlike [`read_body`], a body over the limit or
/// cut off by the connection keeps what was received, so the page can
/// still be extracted (spec 005 FR-008); the problem is returned with it.
async fn read_body_partial(mut response: Response, max_bytes: u64) -> (Vec<u8>, Option<CheckError>) {
    let too_large = || {
        CheckError::new(
            ErrorClass::ContentProblem,
            format!(
                "The response is larger than the limit of {}; only the first part was read.",
                human_bytes(max_bytes)
            ),
        )
        .with_reason(ErrorReason::OverSize)
    };
    // Sized up front when the length is known, so large bodies (PDFs) are
    // not copied again as the buffer grows.
    let hint = response.content_length().unwrap_or(0).min(max_bytes);
    let mut buf = Vec::with_capacity(hint as usize);
    loop {
        match response.chunk().await {
            Ok(Some(chunk)) => {
                let remaining = (max_bytes - buf.len() as u64) as usize;
                if chunk.len() > remaining {
                    buf.extend_from_slice(&chunk[..remaining]);
                    return (buf, Some(too_large()));
                }
                buf.extend_from_slice(&chunk);
            }
            Ok(None) => return (buf, None),
            Err(e) => return (buf, Some(classify_body(&e))),
        }
    }
}

/// Same host and same explicit port. An http-to-https upgrade on the same
/// host keeps internal headers; another port on the same machine does not.
fn same_host(a: &Url, b: &Url) -> bool {
    a.host_str() == b.host_str() && a.port() == b.port()
}

fn redirect_location(response: &Response) -> Option<&str> {
    let status = response.status();
    if !status.is_redirection() || status == StatusCode::NOT_MODIFIED {
        return None;
    }
    response
        .headers()
        .get(header::LOCATION)?
        .to_str()
        .ok()
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

/// Returns the media type (lowercase, no parameters) and the charset label.
fn parse_content_type(response: &Response) -> (Option<String>, Option<String>) {
    let Some(raw) = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
    else {
        return (None, None);
    };
    let mut parts = raw.split(';');
    let media = parts
        .next()
        .map(|m| m.trim().to_ascii_lowercase())
        .filter(|m| !m.is_empty());
    let charset = parts.find_map(|p| {
        let (key, value) = p.split_once('=')?;
        key.trim()
            .eq_ignore_ascii_case("charset")
            .then(|| value.trim().trim_matches('"').to_string())
    });
    (media, charset)
}

/// Streams the body chunk by chunk so memory never grows past `max_bytes`.
async fn read_body(
    mut response: Response,
    max_bytes: u64,
    on_limit: OnLimit,
    keep: bool,
) -> Result<Option<Vec<u8>>, CheckError> {
    let too_large = || {
        CheckError::new(
            ErrorClass::ContentProblem,
            format!(
                "The response is larger than the limit of {}.",
                human_bytes(max_bytes)
            ),
        )
        .with_reason(ErrorReason::OverSize)
    };
    if matches!(on_limit, OnLimit::Fail) && response.content_length().is_some_and(|n| n > max_bytes) {
        return Err(too_large());
    }

    let mut kept = keep.then(Vec::new);
    let mut total: u64 = 0;
    while let Some(chunk) = response.chunk().await.map_err(|e| classify_body(&e))? {
        let remaining = max_bytes - total;
        if chunk.len() as u64 > remaining {
            match on_limit {
                OnLimit::Fail => return Err(too_large()),
                OnLimit::Truncate => {
                    if let Some(buf) = kept.as_mut() {
                        buf.extend_from_slice(&chunk[..remaining as usize]);
                    }
                    break;
                }
            }
        }
        total += chunk.len() as u64;
        if let Some(buf) = kept.as_mut() {
            buf.extend_from_slice(&chunk);
        }
    }
    Ok(kept)
}

fn human_bytes(n: u64) -> String {
    if n >= 1024 * 1024 && n.is_multiple_of(1024 * 1024) {
        format!("{} MB", n / (1024 * 1024))
    } else {
        format!("{n} bytes")
    }
}

/// Text of every underlying cause, lowercased. The top-level reqwest message
/// is skipped because it contains the URL, which could match our keywords.
fn cause_text(err: &reqwest::Error) -> (String, Option<String>) {
    let mut text = String::new();
    let mut innermost = None;
    let mut source = err.source();
    while let Some(cause) = source {
        let s = cause.to_string();
        text.push_str(&s.to_ascii_lowercase());
        text.push('\n');
        innermost = Some(s);
        source = cause.source();
    }
    (text, innermost)
}

/// A body that stopped early. reqwest reports these as decode errors when
/// its decompression layer is in the read path, so the cause text decides.
fn is_cut_off(text: &str) -> bool {
    [
        "end of file before message length",
        "incomplete",
        "connection closed",
        "connection reset",
        "unexpected eof",
    ]
    .iter()
    .any(|k| text.contains(k))
}

fn is_temporary_dns(err: &reqwest::Error, text: &str) -> bool {
    if text.contains("temporary failure") || text.contains("try again") {
        return true;
    }
    // Windows reports WSATRY_AGAIN (11002) for a temporary resolver failure.
    let mut source = err.source();
    while let Some(cause) = source {
        if let Some(io) = cause.downcast_ref::<std::io::Error>()
            && io.raw_os_error() == Some(11002)
        {
            return true;
        }
        source = cause.source();
    }
    false
}

/// Maps a transport error to exactly one error class and, where it can be
/// told, its sub-reason (spec 001 FR-017, spec 004 FR-012; research R1, R2).
pub(crate) fn classify(err: &reqwest::Error) -> CheckError {
    let (text, innermost) = cause_text(err);
    let detail = innermost.unwrap_or_else(|| err.to_string());
    let host = err
        .url()
        .and_then(|u| u.host_str())
        .unwrap_or("the server")
        .to_string();

    let (class, reason, message) = if err.is_timeout() || text.contains("timed out") {
        if err.is_connect() {
            (
                ErrorClass::Timeout,
                Some(ErrorReason::Connect),
                format!("Connecting to {host} took longer than the time limit."),
            )
        } else {
            (
                ErrorClass::Timeout,
                Some(ErrorReason::Read),
                format!("{host} did not send its answer within the time limit."),
            )
        }
    } else if text.contains("dns error") || text.contains("failed to lookup address") {
        if is_temporary_dns(err, &text) {
            (
                ErrorClass::DnsTemporary,
                None,
                format!("The address of {host} could not be looked up right now."),
            )
        } else {
            (
                ErrorClass::Dns,
                None,
                format!("The host name {host} does not exist."),
            )
        }
    } else if text.contains("certificate") || text.contains("tls") || text.contains("handshake") {
        let reason = if text.contains("expired") {
            ErrorReason::Expired
        } else if text.contains("notvalidforname")
            || text.contains("not valid for")
            || text.contains("hostname")
        {
            ErrorReason::Hostname
        } else if text.contains("unknownissuer")
            || text.contains("unknown issuer")
            || text.contains("self-signed")
            || text.contains("self signed")
        {
            ErrorReason::Untrusted
        } else {
            return CheckError::new(
                ErrorClass::TlsInvalid,
                format!("The secure connection to {host} was rejected. ({detail})"),
            );
        };
        let why = match reason {
            ErrorReason::Expired => "its certificate has expired",
            ErrorReason::Hostname => "its certificate does not match the host name",
            _ => "its certificate is not issued by a trusted authority",
        };
        (
            ErrorClass::TlsInvalid,
            Some(reason),
            format!("The secure connection to {host} was rejected: {why}."),
        )
    } else if err.is_decode() && !is_cut_off(&text) {
        (
            ErrorClass::ContentProblem,
            Some(ErrorReason::Decompression),
            format!("The response from {host} could not be decompressed."),
        )
    } else if err.is_body() || err.is_decode() {
        (
            ErrorClass::ContentProblem,
            Some(ErrorReason::Truncated),
            format!("The response from {host} was cut off before it was complete."),
        )
    } else {
        let reason = if text.contains("refused") {
            Some(ErrorReason::Refused)
        } else if text.contains("reset")
            || text.contains("closed")
            || text.contains("broken pipe")
            || text.contains("aborted")
        {
            Some(ErrorReason::Reset)
        } else {
            None
        };
        let message = match reason {
            Some(ErrorReason::Refused) => format!("{host} refused the connection."),
            Some(_) => format!("{host} closed the connection before answering."),
            None => format!("Could not connect to {host}."),
        };
        (ErrorClass::Connection, reason, message)
    };
    CheckError {
        class,
        reason,
        message: format!("{message} ({detail})"),
        attempts: 1,
    }
}

/// An error while reading a body: a timeout stays a (read) timeout, a
/// decoding error is a decompression problem, anything else means the body
/// was cut off (research R3).
fn classify_body(err: &reqwest::Error) -> CheckError {
    let e = classify(err);
    match e.class {
        ErrorClass::Timeout => CheckError {
            reason: Some(ErrorReason::Read),
            ..e
        },
        ErrorClass::ContentProblem => e,
        _ => CheckError::new(ErrorClass::ContentProblem, e.message).with_reason(ErrorReason::Truncated),
    }
}
