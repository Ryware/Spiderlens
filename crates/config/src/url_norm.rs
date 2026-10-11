//! URL normalization shared by start URLs, List mode (spec 002 research R5)
//! and discovered links (spec 003 FR-001..FR-006, research R1).

use url::Url;

use crate::{QueryMode, QueryParams};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct NormError(pub String);

/// Parses user input into a normalized http(s) URL.
///
/// - A missing scheme becomes `https://`.
/// - The WHATWG parser lowercases scheme and host, converts IDN hosts to
///   punycode, removes default ports and resolves `.` and `..` segments.
/// - The fragment is removed.
pub fn normalize_url(input: &str) -> Result<Url, NormError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(NormError("the address is empty".into()));
    }
    let candidate = if trimmed.contains("://") {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    };
    let mut url = Url::parse(&candidate).map_err(|e| NormError(format!("not a valid web address ({e})")))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(NormError("only http and https addresses are supported".into()));
    }
    match url.host() {
        None => return Err(NormError("the address has no host name".into())),
        Some(url::Host::Domain(d)) if !is_plausible_domain(d) => {
            return Err(NormError(format!("\"{d}\" is not a valid host name")));
        }
        Some(_) => {}
    }
    url.set_fragment(None);
    tidy_percent_encoding(&mut url);
    Ok(url)
}

/// Why a link does not give a crawlable URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkProblem {
    /// Empty or fragment-only: the link points at the page itself.
    SelfReference,
    /// The href cannot be resolved to a URL. Keeps the raw text.
    Malformed { raw: String, message: String },
    /// A `mailto:`, `tel:`, `javascript:` or other non-web link.
    UnsupportedScheme { url: String, scheme: String },
    /// Longer than the configured maximum after normalization.
    TooLong { url: String, length: usize },
}

/// Resolves a link against its page's base URL and normalizes it (FR-001):
/// lower-case scheme and host, punycode hosts, no default port, no fragment,
/// dot segments resolved, unreserved characters decoded and percent hex in
/// upper case. Path and query case are kept (FR-002). Then the query rules
/// apply (FR-005) and the length limit (FR-006).
pub fn normalize_link(
    base: &Url,
    href: &str,
    query: &QueryParams,
    max_length: usize,
) -> Result<Url, LinkProblem> {
    let trimmed = href.trim_matches(|c: char| c.is_ascii_whitespace());
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return Err(LinkProblem::SelfReference);
    }
    let mut url = base.join(trimmed).map_err(|e| LinkProblem::Malformed {
        raw: href.to_string(),
        message: e.to_string(),
    })?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(LinkProblem::UnsupportedScheme {
            url: url.to_string(),
            scheme: url.scheme().to_string(),
        });
    }
    match url.host() {
        None => {
            return Err(LinkProblem::Malformed {
                raw: href.to_string(),
                message: "no host name".into(),
            });
        }
        Some(url::Host::Domain(d)) if !is_plausible_domain(d) => {
            return Err(LinkProblem::Malformed {
                raw: href.to_string(),
                message: format!("\"{d}\" is not a valid host name"),
            });
        }
        Some(_) => {}
    }
    url.set_fragment(None);
    tidy_percent_encoding(&mut url);
    apply_query_rules(&mut url, query);
    let length = url.as_str().len();
    if length > max_length {
        return Err(LinkProblem::TooLong {
            url: url.to_string(),
            length,
        });
    }
    Ok(url)
}

/// Removes or sorts query parameters as configured. Parameter names are
/// compared as written (servers treat them case-sensitively).
pub fn apply_query_rules(url: &mut Url, rules: &QueryParams) {
    let Some(query) = url.query() else { return };
    if rules.mode == QueryMode::Keep && !rules.sort {
        return;
    }
    if rules.mode == QueryMode::RemoveAll {
        url.set_query(None);
        return;
    }
    let mut pairs: Vec<&str> = query.split('&').filter(|p| !p.is_empty()).collect();
    let name = |pair: &str| pair.split('=').next().unwrap_or("").to_string();
    if rules.mode == QueryMode::Remove {
        pairs.retain(|p| !rules.names.iter().any(|n| n.trim() == name(p)));
    }
    if rules.sort {
        // Stable, so repeated names keep their order.
        pairs.sort_by_key(|p| name(p));
    }
    let joined = pairs.join("&");
    url.set_query((!joined.is_empty()).then_some(joined.as_str()));
}

/// Writes percent-encoding in one form: unreserved characters decoded and
/// hex digits upper-cased, in the path and the query. `%7e` and `~` are the
/// same URL, but the WHATWG parser keeps both spellings.
fn tidy_percent_encoding(url: &mut Url) {
    let path = tidy(url.path());
    if path != url.path() {
        url.set_path(&path);
    }
    if let Some(q) = url.query() {
        let query = tidy(q);
        if query != q {
            url.set_query(Some(&query));
        }
    }
}

/// Percent-encoding in one form: unreserved characters decoded, other
/// escapes with upper-case hex. Also used for robots.txt patterns, so that a
/// rule written as `/%7Euser` matches the URL `/~user`.
pub fn tidy_percent(s: &str) -> String {
    tidy(s)
}

fn tidy(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(h), Some(l)) = (hex(bytes[i + 1]), hex(bytes[i + 2]))
        {
            let v = h * 16 + l;
            if v.is_ascii_alphanumeric() || matches!(v, b'-' | b'.' | b'_' | b'~') {
                out.push(v as char);
            } else {
                out.push('%');
                out.push(char::from(b"0123456789ABCDEF"[usize::from(h)]));
                out.push(char::from(b"0123456789ABCDEF"[usize::from(l)]));
            }
            i += 3;
            continue;
        }
        // The input is valid UTF-8 and every non-ASCII byte is already
        // percent-encoded by the parser, so bytes here are ASCII.
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn hex(b: u8) -> Option<u8> {
    (b as char).to_digit(16).map(|d| d as u8)
}

/// Host names are letters, digits, hyphens and underscores in dot-separated
/// labels (IDN hosts are already punycode here). The WHATWG parser accepts
/// more, such as `ht!tp`, which no site can use.
fn is_plausible_domain(domain: &str) -> bool {
    !domain.is_empty()
        && domain.trim_end_matches('.').split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && label
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn duplicates_collapse() {
        let a = normalize_url("HTTPS://Example.COM:443/a#top").unwrap();
        let b = normalize_url("https://example.com/a").unwrap();
        assert_eq!(a, b);
        assert_eq!(
            normalize_url("example.com").unwrap().as_str(),
            "https://example.com/"
        );
    }

    #[test]
    fn rejects_bad_input() {
        for bad in [
            "",
            "ht!tp//bad",
            "ftp://x.test",
            "http://",
            "https://exa mple.com",
        ] {
            assert!(normalize_url(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn idn_host_becomes_punycode() {
        assert_eq!(
            normalize_url("https://bücher.de/").unwrap().host_str(),
            Some("xn--bcher-kva.de")
        );
    }

    fn link(base: &str, href: &str) -> Result<Url, LinkProblem> {
        normalize_link(&Url::parse(base).unwrap(), href, &QueryParams::default(), 2000)
    }

    #[test]
    fn spellings_of_one_page_collapse() {
        let base = "http://example.com/";
        for href in [
            "HTTP://Example.com/a",
            "http://example.com:80/a#top",
            "http://example.com/x/../a",
            "./a",
            "/%61",
        ] {
            assert_eq!(
                link(base, href).unwrap().as_str(),
                "http://example.com/a",
                "{href}"
            );
        }
    }

    #[test]
    fn path_case_and_trailing_slash_are_kept() {
        let base = "https://a.test/";
        assert_eq!(link(base, "/About").unwrap().as_str(), "https://a.test/About");
        assert_ne!(link(base, "/a/").unwrap(), link(base, "/a").unwrap());
    }

    #[test]
    fn percent_encoding_has_one_form() {
        let base = "https://a.test/";
        assert_eq!(link(base, "/%7euser").unwrap().as_str(), "https://a.test/~user");
        assert_eq!(link(base, "/a%2fb").unwrap().as_str(), "https://a.test/a%2Fb");
        assert_eq!(link(base, "/caf%c3%a9").unwrap(), link(base, "/café").unwrap());
        assert_eq!(link(base, "/?q=%7e").unwrap().as_str(), "https://a.test/?q=~");
    }

    #[test]
    fn idn_spellings_collapse() {
        assert_eq!(
            link("https://a.test/", "https://BÜCHER.de/x").unwrap(),
            link("https://a.test/", "https://xn--bcher-kva.de/x").unwrap()
        );
    }

    #[test]
    fn link_problems() {
        let base = "https://a.test/p";
        assert_eq!(link(base, ""), Err(LinkProblem::SelfReference));
        assert_eq!(link(base, "#top"), Err(LinkProblem::SelfReference));
        assert!(matches!(
            link(base, "mailto:x@a.test"),
            Err(LinkProblem::UnsupportedScheme { scheme, .. }) if scheme == "mailto"
        ));
        assert!(matches!(
            link(base, "javascript:void(0)"),
            Err(LinkProblem::UnsupportedScheme { .. })
        ));
        assert!(matches!(
            link(base, "http://exa mple.com/"),
            Err(LinkProblem::Malformed { .. })
        ));
        assert!(matches!(
            link(base, "http://[::1/"),
            Err(LinkProblem::Malformed { .. })
        ));
        let long = format!("/{}", "a".repeat(3000));
        assert!(matches!(link(base, &long), Err(LinkProblem::TooLong { length, .. }) if length > 2000));
    }

    #[test]
    fn query_rules() {
        let base = Url::parse("https://a.test/").unwrap();
        let remove = QueryParams {
            mode: QueryMode::Remove,
            names: vec!["utm_source".into(), "utm_medium".into()],
            sort: false,
        };
        for href in ["/a?utm_source=x", "/a?utm_medium=y"] {
            assert_eq!(
                normalize_link(&base, href, &remove, 2000).unwrap().as_str(),
                "https://a.test/a"
            );
        }
        assert_eq!(
            normalize_link(&base, "/a?b=1&utm_source=x&a=2", &remove, 2000)
                .unwrap()
                .as_str(),
            "https://a.test/a?b=1&a=2"
        );
        let sort = QueryParams {
            sort: true,
            ..QueryParams::default()
        };
        assert_eq!(
            normalize_link(&base, "/a?b=1&a=2&b=0", &sort, 2000)
                .unwrap()
                .as_str(),
            "https://a.test/a?a=2&b=1&b=0"
        );
        let all = QueryParams {
            mode: QueryMode::RemoveAll,
            ..QueryParams::default()
        };
        assert_eq!(
            normalize_link(&base, "/a?x=1", &all, 2000).unwrap().as_str(),
            "https://a.test/a"
        );
        // Keep is the default and keeps the query exactly.
        assert_eq!(
            link("https://a.test/", "/a?B=1&a=2").unwrap().as_str(),
            "https://a.test/a?B=1&a=2"
        );
    }

    #[test]
    fn start_urls_and_links_agree() {
        assert_eq!(
            normalize_url("https://a.test/%7Ex").unwrap(),
            link("https://a.test/", "/~x").unwrap()
        );
    }

    proptest! {
        #[test]
        fn link_normalization_is_idempotent(s in "(/[a-zA-Z0-9%~._-]{0,8}){1,4}(\\?[a-z]=[a-zA-Z0-9%]{0,4})?") {
            let base = Url::parse("https://a.test/").unwrap();
            if let Ok(once) = normalize_link(&base, &s, &QueryParams::default(), 2000) {
                let twice = normalize_link(&base, once.as_str(), &QueryParams::default(), 2000).unwrap();
                prop_assert_eq!(once, twice);
            }
        }


        #[test]
        fn normalization_is_idempotent(s in "[a-z]{1,10}\\.[a-z]{2,3}(/[a-zA-Z0-9]{0,8}){0,3}") {
            if let Ok(once) = normalize_url(&s) {
                let twice = normalize_url(once.as_str()).unwrap();
                prop_assert_eq!(once, twice);
            }
        }
    }
}
