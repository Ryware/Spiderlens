//! SQL functions and the collation the result queries use (plan D7).
//!
//! SQLite's own `LIKE` and `lower()` fold ASCII only, and it has no natural
//! sort. These are registered on every connection this crate opens, so the
//! same rules apply in the desktop app, the CLI and the tests.

use std::cmp::Ordering;

use rusqlite::Connection;
use rusqlite::functions::FunctionFlags;
use spiderlens_engine::crawl::NotCrawledReason;

const FLAGS: FunctionFlags = FunctionFlags::SQLITE_UTF8.union(FunctionFlags::SQLITE_DETERMINISTIC);

/// Issue rule thresholds by rule id (spec 009 plan D3).
pub type Thresholds = std::collections::HashMap<&'static str, f64>;

/// `thr('<rule id>')`: the threshold in force. Not deterministic, so a
/// settings change applies to the next query without new SQL.
pub fn register_thresholds(
    conn: &Connection,
    values: std::sync::Arc<std::sync::RwLock<Thresholds>>,
) -> rusqlite::Result<()> {
    conn.create_scalar_function("thr", 1, FunctionFlags::SQLITE_UTF8, move |ctx| {
        let key: String = ctx.get(0)?;
        let map = values.read().unwrap_or_else(|e| e.into_inner());
        Ok(map.get(key.as_str()).copied())
    })
}

/// Position of the target WCAG version in the order 2.0, 2.1, 2.2.
fn version_rank(v: spiderlens_config::WcagVersion) -> u8 {
    match v {
        spiderlens_config::WcagVersion::V2_0 => 0,
        spiderlens_config::WcagVersion::V2_1 => 1,
        spiderlens_config::WcagVersion::V2_2 => 2,
    }
}

fn level_rank(l: spiderlens_config::WcagLevel) -> u8 {
    match l {
        spiderlens_config::WcagLevel::A => 0,
        spiderlens_config::WcagLevel::Aa => 1,
        spiderlens_config::WcagLevel::Aaa => 2,
    }
}

/// Whether a space-separated tag string, as axe writes it (`wcag2aa
/// wcag143`) or `best-practice`, falls inside the target: a level tag at or
/// below the target version and level, or best practice when asked for
/// (spec 026 FR-003, FR-020).
pub fn wcag_in_target(tags: &str, target: &spiderlens_config::AccessibilitySettings) -> bool {
    let max_version = version_rank(target.wcag.version);
    let max_level = level_rank(target.wcag.level);
    tags.split_whitespace().any(|tag| {
        if tag == "best-practice" {
            return target.best_practice;
        }
        let Some(rest) = tag.strip_prefix("wcag") else {
            return false;
        };
        // wcag2a, wcag21aa, wcag22aaa; criteria such as wcag143 end in a digit.
        let split = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
        let (digits, letters) = rest.split_at(split);
        let version = match digits {
            "2" => 0,
            "21" => 1,
            "22" => 2,
            _ => return false,
        };
        let level = match letters {
            "a" => 0,
            "aa" => 1,
            "aaa" => 2,
            _ => return false,
        };
        version <= max_version && level <= max_level
    })
}

/// `wcag_in_target(tags)` and `a11y_setting(name)`: the accessibility
/// settings of the crawl (or the target given on the command line). Not
/// deterministic, so a replaced target applies to the next query.
pub fn register_a11y(
    conn: &Connection,
    settings: &spiderlens_config::AccessibilitySettings,
) -> rusqlite::Result<()> {
    let target = *settings;
    conn.create_scalar_function("wcag_in_target", 1, FunctionFlags::SQLITE_UTF8, move |ctx| {
        let tags: Option<String> = ctx.get(0)?;
        Ok(tags.is_some_and(|t| wcag_in_target(&t, &target)))
    })?;
    let values = *settings;
    conn.create_scalar_function("a11y_setting", 1, FunctionFlags::SQLITE_UTF8, move |ctx| {
        let name: String = ctx.get(0)?;
        Ok(match name.as_str() {
            "small_text_px" => Some(i64::from(values.small_text_px)),
            "small_text_share" => Some(i64::from(values.small_text_share)),
            "tap_target_px" => Some(i64::from(values.tap_target_px)),
            "overflow_px" => Some(i64::from(values.overflow_px)),
            _ => None,
        })
    })
}

/// The registry's purpose of the crawler with this robots.txt token, as the
/// snake case word the stored data and the UI use.
fn ai_bot_purpose(token: &str) -> Option<String> {
    use spiderlens_config::ai_bots::BotPurpose;
    let b = spiderlens_config::ai_bots::by_token(token)?;
    Some(
        match b.purpose {
            BotPurpose::Training => "training",
            BotPurpose::Search => "search",
            BotPurpose::UserFetch => "user_fetch",
            BotPurpose::Other => "other",
        }
        .to_string(),
    )
}

pub fn register(conn: &Connection) -> rusqlite::Result<()> {
    conn.create_scalar_function("has_header", 2, FLAGS, |ctx| {
        let headers: Option<String> = ctx.get(0)?;
        let name: String = ctx.get(1)?;
        Ok(headers.map(|h| has_header(&h, &name)))
    })?;
    conn.create_scalar_function("lang_code_ok", 1, FLAGS, |ctx| {
        let s: Option<String> = ctx.get(0)?;
        Ok(s.is_some_and(|s| lang_code_ok(&s)))
    })?;
    conn.create_scalar_function("weak_anchor", 1, FLAGS, |ctx| {
        let s: Option<String> = ctx.get(0)?;
        Ok(s.is_some_and(|s| weak_anchor(&s)))
    })?;
    register_custom(conn)?;
    conn.create_scalar_function("natkey", 1, FLAGS, |ctx| {
        let s: Option<String> = ctx.get(0)?;
        Ok(s.map(|s| natkey(&s)))
    })?;
    conn.create_scalar_function("ci_contains", 2, FLAGS, |ctx| {
        let hay: Option<String> = ctx.get(0)?;
        let needle: String = ctx.get(1)?;
        Ok(hay.is_some_and(|h| ci_contains(&h, &needle)))
    })?;
    conn.create_scalar_function("norm_ws", 1, FLAGS, |ctx| {
        let s: Option<String> = ctx.get(0)?;
        Ok(s.map(|s| norm_ws(&s)))
    })?;
    conn.create_scalar_function("content_kind", 1, FLAGS, |ctx| {
        let ct: Option<String> = ctx.get(0)?;
        Ok(content_kind(ct.as_deref()))
    })?;
    conn.create_scalar_function("status_text", 1, FLAGS, |ctx| {
        let code: Option<i64> = ctx.get(0)?;
        Ok(code.and_then(status_text))
    })?;
    conn.create_scalar_function("final_state", 7, FLAGS, |ctx| {
        Ok(final_state(&FinalStateArgs {
            state: ctx.get::<String>(0)?,
            reason: ctx.get::<Option<String>>(1)?,
            reason_detail: ctx.get::<Option<String>>(2)?,
            check_only: ctx.get::<bool>(3)?,
            fetched: ctx.get::<Option<i64>>(4)?.is_some(),
            error_class: ctx.get::<Option<String>>(5)?,
        }))
    })?;
    conn.create_scalar_function("error_label", 1, FLAGS, |ctx| {
        let s: Option<String> = ctx.get(0)?;
        Ok(s.map(|s| error_class_label(&s).to_string()))
    })?;
    conn.create_scalar_function("url_check", 2, FLAGS, |ctx| {
        let u: Option<String> = ctx.get(0)?;
        let check: String = ctx.get(1)?;
        Ok(u.is_some_and(|u| url_check(&u, &check)))
    })?;
    conn.create_scalar_function("readable_url", 1, FLAGS, |ctx| {
        let s: Option<String> = ctx.get(0)?;
        Ok(s.map(|s| readable_url(&s)))
    })?;
    // Spec 015: the AI crawlers in a stored `aeo_pages.blocked_bots` mask.
    conn.create_scalar_function("ai_bot_names", 1, FLAGS, |ctx| {
        let Some(mask) = ctx.get::<Option<i64>>(0)? else {
            return Ok(None);
        };
        let mask = mask as u64;
        let names: Vec<&str> = spiderlens_config::ai_bots::all()
            .iter()
            .filter(|b| b.index < 64 && mask & (1 << b.index) != 0)
            .map(|b| b.name.as_str())
            .collect();
        Ok(Some(names.join(", ")))
    })?;
    // Spec 016 FR-013: what a stored crawler token is for (`user_fetch` for
    // agents that fetch a page because a person asked), and its name. Both
    // read the shared registry; a token it does not know (a user's own
    // entry) gives NULL.
    conn.create_scalar_function("ai_bot_purpose", 1, FLAGS, |ctx| {
        let token: Option<String> = ctx.get(0)?;
        Ok(token.and_then(|t| ai_bot_purpose(&t)))
    })?;
    conn.create_scalar_function("ai_bot_name", 1, FLAGS, |ctx| {
        let token: Option<String> = ctx.get(0)?;
        Ok(token.and_then(|t| spiderlens_config::ai_bots::by_token(&t).map(|b| b.name.clone())))
    })?;
    conn.create_scalar_function("px_width", 2, FLAGS, |ctx| {
        let text: Option<String> = ctx.get(0)?;
        let style: String = ctx.get(1)?;
        let style = crate::serp::SerpStyle::parse(&style)
            .ok_or_else(|| rusqlite::Error::UserFunctionError(format!("unknown style {style}").into()))?;
        Ok(text.map(|t| crate::serp::px_width(&t, style)))
    })?;
    conn.create_scalar_function("indexability", 7, FLAGS, |ctx| {
        Ok(indexability(&IndexabilityArgs {
            state: ctx.get::<String>(0)?,
            error_class: ctx.get::<Option<String>>(1)?,
            status: ctx.get::<Option<i64>>(2)?,
            content_type: ctx.get::<Option<String>>(3)?,
            noindex: ctx.get::<Option<bool>>(4)?.unwrap_or(false),
            canonical: ctx.get::<Option<String>>(5)?,
            url: ctx.get::<String>(6)?,
        }))
    })?;
    Ok(())
}

/// Whether a stored header list (a JSON array of name and value pairs)
/// has `name`, ignoring case.
pub fn has_header(json: &str, name: &str) -> bool {
    serde_json::from_str::<Vec<(String, String)>>(json)
        .is_ok_and(|h| h.iter().any(|(n, _)| n.eq_ignore_ascii_case(name)))
}

/// The shape of an hreflang value (spec 009 research R6): `x-default`, or a
/// 2 or 3 letter language, an optional 4 letter script and an optional
/// region of 2 letters or 3 digits. `en-UK` passes the shape; known wrong
/// regions are listed separately.
pub fn lang_code_ok(s: &str) -> bool {
    let s = s.trim();
    if s.eq_ignore_ascii_case("x-default") {
        return true;
    }
    let parts: Vec<&str> = s.split(['-', '_']).collect();
    let alpha = |p: &str, n: std::ops::RangeInclusive<usize>| {
        n.contains(&p.len()) && p.chars().all(|c| c.is_ascii_alphabetic())
    };
    let region = |p: &str| {
        (p.len() == 2 && alpha(p, 2..=2)) || (p.len() == 3 && p.chars().all(|c| c.is_ascii_digit()))
    };
    let ok = match parts.as_slice() {
        [l] => alpha(l, 2..=3),
        [l, x] => alpha(l, 2..=3) && (alpha(x, 4..=4) || region(x)),
        [l, sc, r] => alpha(l, 2..=3) && alpha(sc, 4..=4) && region(r),
        _ => false,
    };
    // Common mistakes search engines reject: UK is not a region code (GB is).
    ok && !parts.iter().skip(1).any(|p| p.eq_ignore_ascii_case("uk"))
}

/// Link text that says nothing about its target (spec 009 research R7).
pub fn weak_anchor(text: &str) -> bool {
    const WEAK: [&str; 10] = [
        "click here",
        "here",
        "read more",
        "more",
        "learn more",
        "this page",
        "link",
        "go",
        "continue",
        "details",
    ];
    let t = text
        .trim()
        .trim_end_matches(['.', '!', '>', '»', '…'])
        .trim()
        .to_lowercase();
    WEAK.contains(&t.as_str())
}

pub struct IndexabilityArgs {
    pub state: String,
    pub error_class: Option<String>,
    pub status: Option<i64>,
    pub content_type: Option<String>,
    pub noindex: bool,
    pub canonical: Option<String>,
    pub url: String,
}

/// "Indexable", or "Non-indexable: <reason>" (spec 007 Assumptions): not a
/// successful HTML response, a noindex directive for this crawl's robots
/// token, or a canonical pointing to another URL. The first reason wins.
pub fn indexability(a: &IndexabilityArgs) -> String {
    let reason = if a.state != "crawled" {
        Some("not crawled")
    } else if a.error_class.is_some() || a.status.is_none() {
        Some("no response")
    } else {
        match a.status.unwrap_or(0) {
            300..=399 => Some("redirected"),
            400..=499 => Some("client error"),
            500..=599 => Some("server error"),
            200..=299 if content_kind(a.content_type.as_deref()) != "html" => Some("not HTML"),
            200..=299 if a.noindex => Some("noindex"),
            200..=299 if a.canonical.as_deref().is_some_and(|c| c != a.url) => Some("canonicalised"),
            200..=299 => None,
            _ => Some("no response"),
        }
    };
    reason.map_or_else(|| "Indexable".to_string(), |r| format!("Non-indexable: {r}"))
}

/// Case-insensitive "contains" with Unicode lowercasing on both sides (FR-016).
pub fn ci_contains(hay: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    hay.to_lowercase().contains(&needle.to_lowercase())
}

/// Trim and collapse whitespace runs, the duplicate rule of FR-011.
pub fn norm_ws(s: &str) -> String {
    let normal = !s.starts_with(char::is_whitespace)
        && !s.ends_with(char::is_whitespace)
        && !s
            .chars()
            .zip(s.chars().skip(1))
            .any(|(a, b)| a.is_whitespace() && (b.is_whitespace() || a != ' '));
    if normal {
        return s.to_string();
    }
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A sort key whose plain (binary) order is [`natural_cmp`]'s order:
/// lowercase text, and each digit run prefixed by its length so numbers
/// compare by value. Computed once per row, so sorting a million rows is a
/// memcmp sort instead of a million callback comparisons.
pub fn natkey(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c.is_ascii_digit() {
            let mut run = String::from(c);
            while let Some(d) = chars.peek().copied().filter(char::is_ascii_digit) {
                run.push(d);
                chars.next();
            }
            let digits = run.trim_start_matches('0');
            let digits = if digits.is_empty() { "0" } else { digits };
            // Length prefix in the digit range keeps "digits before letters".
            if digits.len() <= 8 {
                out.push(char::from(b'0' + digits.len() as u8));
            } else {
                out.push('9');
                out.push_str(&format!("{:03}", digits.len().min(999)));
            }
            out.push_str(digits);
        } else {
            out.extend(c.to_lowercase());
        }
    }
    out
}

/// Natural, case-insensitive order (FR-015): "page 2" before "page 10".
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (mut x, mut y) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (x.peek().copied(), y.peek().copied()) {
            (None, None) => return a.cmp(b),
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(c), Some(d)) if c.is_ascii_digit() && d.is_ascii_digit() => {
                let n = take_digits(&mut x);
                let m = take_digits(&mut y);
                let (n, m) = (n.trim_start_matches('0'), m.trim_start_matches('0'));
                let o = n.len().cmp(&m.len()).then_with(|| n.cmp(m));
                if o != Ordering::Equal {
                    return o;
                }
            }
            (Some(c), Some(d)) => {
                let o = c.to_lowercase().cmp(d.to_lowercase());
                if o != Ordering::Equal {
                    return o;
                }
                x.next();
                y.next();
            }
        }
    }
}

fn take_digits(it: &mut std::iter::Peekable<std::str::Chars<'_>>) -> String {
    let mut s = String::new();
    while let Some(c) = it.peek().copied().filter(char::is_ascii_digit) {
        s.push(c);
        it.next();
    }
    s
}

/// The URL tab's checks on the part after the host (FR-009). Stored URLs
/// are normalised: the host is lowercase and non-ASCII is percent-encoded.
pub fn url_check(u: &str, check: &str) -> bool {
    let after_scheme = u.find("://").map_or(0, |i| i + 3);
    let rest = u[after_scheme..].find('/').map_or("", |i| &u[after_scheme + i..]);
    let path = rest.split(['?', '#']).next().unwrap_or("");
    match check {
        // %80-%FF: an encoded byte of a multi-byte UTF-8 character.
        "non_ascii" => !rest.is_ascii() || has_high_escape(rest),
        "underscore" => rest.contains('_'),
        "uppercase" => without_escapes(path).chars().any(|c| c.is_uppercase()),
        "multiple_slashes" => path.contains("//"),
        "spaces" => rest.contains("%20") || rest.contains(' ') || rest.contains('+'),
        _ => false,
    }
}

fn has_high_escape(s: &str) -> bool {
    let b = s.as_bytes();
    b.windows(3).any(|w| {
        w[0] == b'%' && matches!(w[1], b'8'..=b'9' | b'A'..=b'F' | b'a'..=b'f') && w[2].is_ascii_hexdigit()
    })
}

/// The text with %XX escapes removed, so their hex digits are not mistaken
/// for uppercase letters.
fn without_escapes(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = String::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() && b[i + 1].is_ascii_hexdigit() && b[i + 2].is_ascii_hexdigit() {
            i += 3;
            continue;
        }
        let c = s[i..].chars().next().expect("in bounds");
        out.push(c);
        i += c.len_utf8();
    }
    out
}

/// The content type groups of the Internal and External filters.
pub fn content_kind(ct: Option<&str>) -> &'static str {
    let Some(ct) = ct else { return "other" };
    let mime = ct.split(';').next().unwrap_or("").trim().to_ascii_lowercase();
    match mime.as_str() {
        "text/html" | "application/xhtml+xml" => "html",
        "text/css" => "css",
        "application/pdf" => "pdf",
        "application/javascript"
        | "text/javascript"
        | "application/x-javascript"
        | "application/ecmascript"
        | "text/ecmascript"
        | "module" => "javascript",
        m if m.starts_with("image/") => "image",
        _ => "other",
    }
}

/// The reason phrase for a status code, for the Status column.
pub fn status_text(code: i64) -> Option<&'static str> {
    Some(match code {
        100 => "Continue",
        101 => "Switching Protocols",
        200 => "OK",
        201 => "Created",
        202 => "Accepted",
        203 => "Non-Authoritative Information",
        204 => "No Content",
        206 => "Partial Content",
        300 => "Multiple Choices",
        301 => "Moved Permanently",
        302 => "Found",
        303 => "See Other",
        304 => "Not Modified",
        307 => "Temporary Redirect",
        308 => "Permanent Redirect",
        400 => "Bad Request",
        401 => "Unauthorized",
        402 => "Payment Required",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        406 => "Not Acceptable",
        407 => "Proxy Authentication Required",
        408 => "Request Timeout",
        409 => "Conflict",
        410 => "Gone",
        411 => "Length Required",
        412 => "Precondition Failed",
        413 => "Content Too Large",
        414 => "URI Too Long",
        415 => "Unsupported Media Type",
        416 => "Range Not Satisfiable",
        418 => "I'm a teapot",
        421 => "Misdirected Request",
        422 => "Unprocessable Content",
        425 => "Too Early",
        426 => "Upgrade Required",
        428 => "Precondition Required",
        429 => "Too Many Requests",
        431 => "Request Header Fields Too Large",
        451 => "Unavailable For Legal Reasons",
        500 => "Internal Server Error",
        501 => "Not Implemented",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        504 => "Gateway Timeout",
        505 => "HTTP Version Not Supported",
        507 => "Insufficient Storage",
        508 => "Loop Detected",
        511 => "Network Authentication Required",
        _ => return None,
    })
}

/// The stored columns that decide how a URL ended.
pub struct FinalStateArgs {
    pub state: String,
    pub reason: Option<String>,
    pub reason_detail: Option<String>,
    pub check_only: bool,
    pub fetched: bool,
    pub error_class: Option<String>,
}

/// One readable final state per URL: crawled, checked, failed (with its
/// class), not checked, blocked, or the not-crawled reason.
pub fn final_state(a: &FinalStateArgs) -> String {
    if let Some(class) = &a.error_class {
        return format!("Failed: {}", error_class_label(class));
    }
    match a.state.as_str() {
        "crawled" => "Crawled".into(),
        "queued" | "in_flight" => "Queued".into(),
        "waiting" => "Waiting to retry".into(),
        _ if a.check_only && a.fetched => "Checked".into(),
        _ if a.check_only => a.reason_detail.clone().unwrap_or_else(|| "Not checked".into()),
        _ if a.reason.as_deref() == Some("external") => "Not checked".into(),
        _ => a
            .reason
            .as_deref()
            .and_then(NotCrawledReason::parse)
            .map_or_else(|| "Not crawled".into(), |r| r.label().to_string()),
    }
}

/// Labels for the engine's error classes (spec 001 FR-017, spec 004
/// FR-012).
pub fn error_class_label(class: &str) -> &str {
    match class {
        "dns" => "DNS lookup failed",
        "dns_temporary" => "DNS temporarily unavailable",
        "connection" => "Connection failed",
        "timeout" => "Timed out",
        "tls_invalid" => "Invalid TLS certificate",
        "redirect_loop" => "Redirect loop",
        "redirect_problem" => "Redirect problem",
        "content_problem" => "Content problem",
        "malformed_url" => "Malformed URL",
        "host_unavailable" => "Host unavailable",
        // Derived from the status code (spec 004 data-model.md).
        "http_429" => "Too many requests (429)",
        "http_4xx" => "Client error (4xx)",
        "http_5xx" => "Server error (5xx)",
        "http_other" => "Unusual status code",
        other => other,
    }
}

/// The readable form of an internationalised URL: punycode host and
/// percent-encoded UTF-8 path decoded for display (Edge case: non-Latin text).
pub fn readable_url(s: &str) -> String {
    let Ok(u) = url::Url::parse(s) else {
        return s.to_string();
    };
    let host = u.host_str().map(|h| {
        if h.split('.').any(|l| l.starts_with("xn--")) {
            url::quirks::domain_to_unicode(h)
        } else {
            h.to_string()
        }
    });
    let mut out = String::new();
    out.push_str(u.scheme());
    out.push_str("://");
    if let Some(h) = host {
        out.push_str(&h);
    }
    if let Some(p) = u.port() {
        out.push_str(&format!(":{p}"));
    }
    let rest = &s[u[..url::Position::BeforePath].len()..];
    out.push_str(&percent_decode_utf8(rest));
    out
}

/// Decodes %XX sequences only where they form valid UTF-8 and do not
/// produce characters that change the URL's meaning or are invisible.
fn percent_decode_utf8(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            // Collect a run of %XX escapes and try to decode it as UTF-8.
            let mut j = i;
            let mut buf = Vec::new();
            while j + 2 < bytes.len() && bytes[j] == b'%' {
                let Some(b) = std::str::from_utf8(&bytes[j + 1..j + 3])
                    .ok()
                    .and_then(|h| u8::from_str_radix(h, 16).ok())
                else {
                    break;
                };
                buf.push(b);
                j += 3;
            }
            if buf.is_empty() {
                out.push('%');
                i += 1;
                continue;
            }
            match std::str::from_utf8(&buf) {
                Ok(t)
                    if t.chars()
                        .all(|c| !c.is_ascii() && !c.is_control() && !c.is_whitespace()) =>
                {
                    out.push_str(t);
                }
                _ => out.push_str(&s[i..j]),
            }
            i = j;
        } else {
            let c = s[i..].chars().next().expect("in bounds");
            out.push(c);
            i += c.len_utf8();
        }
    }
    out
}

/// Spec 013: reading the packed rule results (plan D9). A slot the rule
/// never ran on reads as NULL.
fn register_custom(conn: &Connection) -> rusqlite::Result<()> {
    use spiderlens_engine::crawl::store::custom::{unpack_slot, unpack_values};
    use spiderlens_engine::custom::flags;

    fn slot(ctx: &rusqlite::functions::Context<'_>) -> rusqlite::Result<Option<(u32, u16)>> {
        let packed: Option<Vec<u8>> = ctx.get(0)?;
        let i: i64 = ctx.get(1)?;
        Ok(packed
            .and_then(|p| unpack_slot(&p, usize::try_from(i).ok()?))
            .filter(|(_, f)| f & flags::RUN != 0))
    }
    conn.create_scalar_function("custom_count", 2, FLAGS, |ctx| {
        Ok(slot(ctx)?.map(|(c, _)| i64::from(c)))
    })?;
    conn.create_scalar_function("custom_flags", 2, FLAGS, |ctx| {
        Ok(slot(ctx)?.map(|(_, f)| i64::from(f)))
    })?;
    conn.create_scalar_function("custom_num", 1, FLAGS, |ctx| {
        let s: Option<String> = ctx.get(0)?;
        Ok(s.and_then(|s| s.trim().parse::<f64>().ok())
            .filter(|n| n.is_finite()))
    })?;
    // custom_all(firsts, values_blob, slot, sep): every stored value.
    conn.create_scalar_function("custom_all", 4, FLAGS, |ctx| {
        let firsts: Option<String> = ctx.get(0)?;
        let data: Option<Vec<u8>> = ctx.get(1)?;
        let slot: i64 = ctx.get(2)?;
        let sep: String = ctx.get(3)?;
        let first = firsts
            .and_then(|f| serde_json::from_str::<Vec<serde_json::Value>>(&f).ok())
            .and_then(|f| f.get(usize::try_from(slot).ok()?)?.as_str().map(str::to_string));
        let Some(first) = first else { return Ok(None) };
        let mut all = vec![first];
        if let Some(data) = data
            && let Ok(mut values) = unpack_values(&data)
            && let Some((rest, _)) = u32::try_from(slot).ok().and_then(|s| values.remove(&s))
        {
            all.extend(rest);
        }
        Ok(Some(all.join(&sep)))
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {

    #[test]
    fn issue_helpers() {
        assert!(has_header(
            r#"[["Strict-Transport-Security","max-age=1"]]"#,
            "strict-transport-security"
        ));
        assert!(!has_header(r#"[["Server","x"]]"#, "strict-transport-security"));
        for ok in ["en", "en-GB", "he-IL", "zh-Hant-TW", "es-419", "x-default"] {
            assert!(lang_code_ok(ok), "{ok}");
        }
        for bad in ["english", "en-UK", "e", "en-GB-x", ""] {
            assert!(!lang_code_ok(bad), "{bad}");
        }
        assert!(weak_anchor(" Click here "));
        assert!(weak_anchor("Read more »"));
        assert!(!weak_anchor("Shipping rates"));
    }

    use super::*;

    #[test]
    fn ai_bot_purpose_comes_from_the_registry() {
        let conn = Connection::open_in_memory().unwrap();
        register(&conn).unwrap();
        let user_fetch = spiderlens_config::ai_bots::active()
            .find(|b| b.purpose == spiderlens_config::ai_bots::BotPurpose::UserFetch)
            .expect("the registry has a user-fetch agent");
        let get = |sql: &str, token: &str| -> Option<String> {
            conn.query_row(sql, [token], |r| r.get(0)).unwrap()
        };
        assert_eq!(
            get("SELECT ai_bot_purpose(?1)", &user_fetch.token).as_deref(),
            Some("user_fetch")
        );
        assert_eq!(
            get("SELECT ai_bot_purpose(?1)", &user_fetch.token.to_uppercase()).as_deref(),
            Some("user_fetch")
        );
        assert_eq!(
            get("SELECT ai_bot_name(?1)", &user_fetch.token),
            Some(user_fetch.name.clone())
        );
        assert_eq!(get("SELECT ai_bot_purpose(?1)", "not-a-registered-bot"), None);
        assert_eq!(get("SELECT ai_bot_name(?1)", "not-a-registered-bot"), None);
        let null: Option<String> = conn
            .query_row("SELECT ai_bot_purpose(NULL)", [], |r| r.get(0))
            .unwrap();
        assert_eq!(null, None);
    }

    #[test]
    fn natural_order() {
        let mut v = vec!["page 10", "Page 2", "page 1", "apple", "Banana", "page 02x"];
        v.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(v, ["apple", "Banana", "page 1", "Page 2", "page 02x", "page 10"]);
        assert_eq!(natural_cmp("a", "A"), "a".cmp("A"));
        assert_eq!(natural_cmp("x9", "x10"), Ordering::Less);
        assert_eq!(natural_cmp("x", "x1"), Ordering::Less);
    }

    proptest::proptest! {
        #[test]
        fn natkey_orders_like_natural_cmp(a in "[a-cA-C0-9 ]{0,8}", b in "[a-cA-C0-9 ]{0,8}") {
            let by_key = natkey(&a).cmp(&natkey(&b));
            if by_key != Ordering::Equal {
                proptest::prop_assert_eq!(by_key, natural_cmp(&a, &b), "{:?} vs {:?}", a, b);
            }
        }
    }

    #[test]
    fn natkey_examples() {
        let mut v = vec![
            "page 10",
            "Page 2",
            "page 1",
            "apple",
            "Banana",
            "x1000000000",
            "x999",
        ];
        v.sort_by_key(|s| natkey(s));
        assert_eq!(
            v,
            [
                "apple",
                "Banana",
                "page 1",
                "Page 2",
                "page 10",
                "x999",
                "x1000000000"
            ]
        );
    }

    #[test]
    fn norm_ws_fast_path() {
        assert_eq!(norm_ws("a b"), "a b");
        assert_eq!(norm_ws("a\tb"), "a b");
        assert_eq!(norm_ws("a  b"), "a b");
    }

    #[test]
    fn contains_folds_unicode() {
        assert!(ci_contains("Ελληνικά ΣΕΛΙΔΑ", "σελιδα"));
        assert!(ci_contains("Привет МИР", "мир"));
        assert!(ci_contains("CAFÉ", "café"));
        assert!(ci_contains("דף הבית", "הבית"));
        assert!(!ci_contains("Home", "about"));
        assert!(ci_contains("anything", ""));
    }

    #[test]
    fn whitespace_normalised() {
        assert_eq!(norm_ws("  Home \t page\n"), "Home page");
        assert_eq!(norm_ws("   "), "");
    }

    #[test]
    fn content_kinds() {
        assert_eq!(content_kind(Some("text/html; charset=utf-8")), "html");
        assert_eq!(content_kind(Some("Image/PNG")), "image");
        assert_eq!(content_kind(Some("application/javascript")), "javascript");
        assert_eq!(content_kind(Some("text/css")), "css");
        assert_eq!(content_kind(Some("application/pdf")), "pdf");
        assert_eq!(content_kind(Some("application/json")), "other");
        assert_eq!(content_kind(None), "other");
    }

    #[test]
    fn url_checks() {
        assert!(url_check("https://a.test/%D7%93%D7%A3", "non_ascii"));
        assert!(!url_check("https://a.test/%2Fx", "non_ascii"));
        assert!(url_check("https://a.test/a_b", "underscore"));
        assert!(url_check("https://a.test/About", "uppercase"));
        assert!(!url_check("https://a.test/%D7%93", "uppercase"));
        assert!(!url_check("https://a.test/a?Q=1", "uppercase"));
        assert!(url_check("https://a.test/a//b", "multiple_slashes"));
        assert!(!url_check("https://a.test/a/b", "multiple_slashes"));
        assert!(url_check("https://a.test/a%20b", "spaces"));
    }

    #[test]
    fn readable_urls() {
        assert_eq!(
            readable_url("https://xn--4dbrk0ce.example/%D7%93%D7%A3?q=%D7%90"),
            "https://ישראל.example/דף?q=א"
        );
        // Encoded ASCII stays encoded: decoding %2F would change the path.
        assert_eq!(
            readable_url("https://a.test/a%2Fb%20c"),
            "https://a.test/a%2Fb%20c"
        );
        assert_eq!(readable_url("not a url"), "not a url");
    }

    #[test]
    fn final_states() {
        let base = FinalStateArgs {
            state: "crawled".into(),
            reason: None,
            reason_detail: None,
            check_only: false,
            fetched: true,
            error_class: None,
        };
        assert_eq!(final_state(&base), "Crawled");
        let failed = FinalStateArgs {
            error_class: Some("timeout".into()),
            ..base
        };
        assert_eq!(final_state(&failed), "Failed: Timed out");
        let not_checked = FinalStateArgs {
            state: "not_crawled".into(),
            reason: Some("external".into()),
            reason_detail: None,
            check_only: true,
            fetched: false,
            error_class: None,
        };
        assert_eq!(final_state(&not_checked), "Not checked");
        let blocked = FinalStateArgs {
            reason_detail: Some("Blocked by robots.txt".into()),
            ..not_checked
        };
        assert_eq!(final_state(&blocked), "Blocked by robots.txt");
        let excluded = FinalStateArgs {
            state: "not_crawled".into(),
            reason: Some("excluded".into()),
            reason_detail: None,
            check_only: false,
            fetched: false,
            error_class: None,
        };
        assert_eq!(final_state(&excluded), "Excluded by rule");
    }
}
