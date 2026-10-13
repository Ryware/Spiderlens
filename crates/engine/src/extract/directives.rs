//! Robots directives from `<meta name="robots">`, crawler-specific meta tags
//! and `X-Robots-Tag` headers (FR-016, FR-017).

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DirectiveSource {
    Meta,
    Header,
}

impl DirectiveSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Meta => "meta",
            Self::Header => "header",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Directive {
    /// Lowercase name when known, otherwise the token as written (trimmed).
    pub directive: String,
    /// The argument of `max-snippet:50`, `unavailable_after: <date>`, ...
    pub value: Option<String>,
    pub known: bool,
    pub source: DirectiveSource,
    /// `all`, or the crawler it is addressed to, lowercase.
    pub crawler: String,
    /// Meta tags only: found inside `<head>` (FR-010).
    pub in_head: Option<bool>,
}

const KNOWN: &[&str] = &[
    "all",
    "index",
    "noindex",
    "follow",
    "nofollow",
    "none",
    "noarchive",
    "nocache",
    "nosnippet",
    "noimageindex",
    "notranslate",
    "indexifembedded",
    "max-snippet",
    "max-image-preview",
    "max-video-preview",
    "unavailable_after",
    "noodp",
    "noydir",
];

/// Directives that take an argument after a colon, so `name: x` is not a
/// crawler prefix.
const WITH_VALUE: &[&str] = &[
    "max-snippet",
    "max-image-preview",
    "max-video-preview",
    "unavailable_after",
];

/// Meta tag names that address one crawler, besides `robots` and the
/// crawl's own robots token.
const CRAWLER_METAS: &[&str] = &[
    "googlebot",
    "googlebot-news",
    "googlebot-image",
    "googlebot-video",
    "bingbot",
    "msnbot",
    "adidxbot",
    "slurp",
    "yandex",
    "baiduspider",
    "duckduckbot",
    "applebot",
];

/// The crawler a `<meta name>` addresses, or None when the meta tag is not
/// a robots tag at all.
pub fn meta_crawler(name: &str, token: &str) -> Option<String> {
    let name = name.trim().to_ascii_lowercase();
    if name == "robots" {
        Some("all".into())
    } else if CRAWLER_METAS.contains(&name.as_str()) || name.eq_ignore_ascii_case(token) {
        Some(name)
    } else {
        None
    }
}

fn directive(token: &str, value: Option<&str>, source: DirectiveSource, crawler: &str) -> Directive {
    let lower = token.to_ascii_lowercase();
    let known = KNOWN.contains(&lower.as_str());
    Directive {
        directive: if known { lower } else { token.to_string() },
        value: value.map(|v| v.trim().to_string()).filter(|v| !v.is_empty()),
        known,
        source,
        crawler: crawler.to_string(),
        in_head: None,
    }
}

/// `content` of a robots meta tag: a comma-separated list.
pub fn parse_meta(content: &str, crawler: &str, in_head: bool) -> Vec<Directive> {
    parse_list(content, DirectiveSource::Meta, Some(crawler))
        .into_iter()
        .map(|mut d| {
            d.in_head = Some(in_head);
            d
        })
        .collect()
}

/// One `X-Robots-Tag` header value: `noindex, nofollow` for everyone,
/// `googlebot: noindex` for one crawler. A later `otherbot: x` switches the
/// crawler for the tokens after it.
pub fn parse_header(value: &str) -> Vec<Directive> {
    parse_list(value, DirectiveSource::Header, None)
}

fn parse_list(list: &str, source: DirectiveSource, fixed_crawler: Option<&str>) -> Vec<Directive> {
    let mut out: Vec<Directive> = Vec::new();
    let mut crawler = fixed_crawler.unwrap_or("all").to_string();
    // `unavailable_after` dates may contain commas; tokens after it that are
    // not directives belong to its value.
    let mut open_date = false;
    for raw in list.split(',') {
        let token = raw.trim();
        if token.is_empty() {
            continue;
        }
        let (mut name, mut value) = match token.split_once(':') {
            Some((n, v)) => (n.trim(), Some(v.trim())),
            None => (token, None),
        };
        let lower = name.to_ascii_lowercase();
        let agent_prefix = fixed_crawler.is_none()
            && value.is_some()
            && !WITH_VALUE.contains(&lower.as_str())
            && !KNOWN.contains(&lower.as_str())
            && looks_like_agent(name);
        if open_date && !KNOWN.contains(&lower.as_str()) && !agent_prefix {
            if let Some(last) = out.last_mut() {
                let v = last.value.get_or_insert_with(String::new);
                v.push_str(", ");
                v.push_str(token);
            }
            continue;
        }
        open_date = false;
        if agent_prefix && let Some(v) = value {
            // `googlebot: noindex` — a crawler prefix, then the first token.
            crawler = lower;
            let (n, rest) = match v.split_once(':') {
                Some((n, r)) => (n.trim(), Some(r.trim())),
                None => (v, None),
            };
            if n.is_empty() {
                continue;
            }
            name = n;
            value = rest;
        }
        let d = directive(name, value, source, &crawler);
        open_date = d.directive == "unavailable_after";
        out.push(d);
    }
    out
}

/// A user-agent token: no spaces, only the characters agent names use.
fn looks_like_agent(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/'))
}

/// Whether the directives tell `token` (or all crawlers) not to index and
/// not to follow. `none` means both.
pub fn page_flags(directives: &[Directive], token: &str) -> (bool, bool) {
    let mut noindex = false;
    let mut nofollow = false;
    for d in directives {
        if d.crawler != "all" && !d.crawler.eq_ignore_ascii_case(token) {
            continue;
        }
        match d.directive.as_str() {
            "noindex" => noindex = true,
            "nofollow" => nofollow = true,
            "none" => {
                noindex = true;
                nofollow = true;
            }
            _ => {}
        }
    }
    (noindex, nofollow)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(ds: &[Directive]) -> Vec<(&str, &str, Option<&str>)> {
        ds.iter()
            .map(|d| (d.directive.as_str(), d.crawler.as_str(), d.value.as_deref()))
            .collect()
    }

    #[test]
    fn meta_list_us3_1() {
        let ds = parse_meta("noindex, follow", "all", true);
        assert_eq!(names(&ds), [("noindex", "all", None), ("follow", "all", None)]);
        assert!(ds.iter().all(|d| d.source == DirectiveSource::Meta && d.in_head == Some(true)));
    }

    #[test]
    fn header_for_one_crawler_us3_2() {
        let ds = parse_header("googlebot: noindex");
        assert_eq!(names(&ds), [("noindex", "googlebot", None)]);
        assert_eq!(ds[0].source, DirectiveSource::Header);
        assert_eq!(ds[0].in_head, None);
    }

    #[test]
    fn mixed_case_values_and_unknown() {
        let ds = parse_header(" NoIndex ,MAX-SNIPPET:50, max-image-preview: large, Banana ");
        assert_eq!(
            names(&ds),
            [
                ("noindex", "all", None),
                ("max-snippet", "all", Some("50")),
                ("max-image-preview", "all", Some("large")),
                ("Banana", "all", None),
            ]
        );
        assert!(!ds[3].known);
    }

    #[test]
    fn unavailable_after_with_commas_in_the_date() {
        let ds = parse_header("unavailable_after: Wednesday, 03-Nov-2027 15:00:00 GMT, noarchive");
        assert_eq!(
            names(&ds),
            [
                ("unavailable_after", "all", Some("Wednesday, 03-Nov-2027 15:00:00 GMT")),
                ("noarchive", "all", None),
            ]
        );
    }

    #[test]
    fn two_crawlers_in_one_header() {
        let ds = parse_header("googlebot: nofollow, bingbot: noindex, noarchive");
        assert_eq!(
            names(&ds),
            [
                ("nofollow", "googlebot", None),
                ("noindex", "bingbot", None),
                ("noarchive", "bingbot", None),
            ]
        );
    }

    #[test]
    fn page_level_flags() {
        let mut ds = parse_meta("none", "all", true);
        assert_eq!(page_flags(&ds, "spiderlens"), (true, true));
        ds = parse_header("googlebot: noindex");
        assert_eq!(page_flags(&ds, "spiderlens"), (false, false));
        assert_eq!(page_flags(&ds, "Googlebot"), (true, false));
        ds = parse_header("SpiderLens: nofollow");
        assert_eq!(page_flags(&ds, "SpiderLens"), (false, true));
    }

    #[test]
    fn meta_names() {
        assert_eq!(meta_crawler("ROBOTS", "x").as_deref(), Some("all"));
        assert_eq!(meta_crawler("Googlebot", "x").as_deref(), Some("googlebot"));
        assert_eq!(meta_crawler("spiderlens", "SpiderLens").as_deref(), Some("spiderlens"));
        assert_eq!(meta_crawler("description", "x"), None);
    }
}
