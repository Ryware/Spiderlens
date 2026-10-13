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

pub fn register(conn: &Connection) -> rusqlite::Result<()> {
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

/// Labels for the engine's error classes (spec 001 FR-017).
pub fn error_class_label(class: &str) -> &str {
    match class {
        "dns" => "DNS lookup failed",
        "dns_temporary" => "DNS temporarily unavailable",
        "connection" => "Connection failed",
        "timeout" => "Timed out",
        "tls_invalid" => "Invalid TLS certificate",
        "redirect_loop" => "Redirect loop",
        "content_problem" => "Content problem",
        "malformed_url" => "Malformed URL",
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

#[cfg(test)]
mod tests {
    use super::*;

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
