//! URL discovery from one page (FR-030, FR-033).
//!
//! Only what the frontier needs: anchors with their rel values, canonical and
//! hreflang (from HTML and the `Link` header), images, the `<base>` element
//! and page-level nofollow from meta robots and `X-Robots-Tag`. Spec 005 owns
//! full extraction (anchor text, headings, structured data, ...).

use scraper::{Html, Selector};
use url::Url;

use super::reason::LinkKind;
use crate::Title;
use crate::title::{detect_encoding, title_of};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoundLink {
    /// The reference exactly as written (FR-004).
    pub raw: String,
    pub kind: LinkKind,
    /// `rel` contains `nofollow` (link level).
    pub nofollow: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageLinks {
    /// What relative links resolve against: `<base href>` or the page URL.
    pub base: Url,
    pub links: Vec<FoundLink>,
    /// Meta robots or X-Robots-Tag says nofollow (or none) for all crawlers
    /// or for our token.
    pub page_nofollow: bool,
    pub title: Title,
}

fn sel(s: &str) -> Selector {
    Selector::parse(s).expect("static selector is valid")
}

fn rel_has(rel: Option<&str>, token: &str) -> bool {
    rel.is_some_and(|r| r.split_ascii_whitespace().any(|t| t.eq_ignore_ascii_case(token)))
}

/// `nofollow` or `none` in a comma-separated directive list.
fn directives_nofollow(list: &str) -> bool {
    list.split(',')
        .map(|d| d.trim().to_ascii_lowercase())
        .any(|d| d == "nofollow" || d == "none")
}

/// One `X-Robots-Tag` value: `nofollow, noindex` applies to everyone,
/// `googlebot: nofollow` only to that crawler.
fn header_nofollow(value: &str, token: &str) -> bool {
    if let Some((agent, rest)) = value.split_once(':') {
        let agent = agent.trim();
        // `unavailable_after: <date>` is a directive, not a crawler name.
        let is_agent = !agent.contains(',') && !agent.eq_ignore_ascii_case("unavailable_after");
        if is_agent {
            return agent.eq_ignore_ascii_case(token) && directives_nofollow(rest);
        }
    }
    directives_nofollow(value)
}

/// `Link: <https://a.test/p>; rel="canonical", <...>; rel="alternate"; hreflang="de"`
fn link_header_entries(value: &str) -> Vec<FoundLink> {
    // Entries are separated by commas; a comma not followed by `<` belongs
    // to a parameter value.
    let mut parts: Vec<String> = Vec::new();
    for piece in value.split(',') {
        match parts.last_mut() {
            Some(last) if !piece.trim_start().starts_with('<') => {
                last.push(',');
                last.push_str(piece);
            }
            _ => parts.push(piece.trim_start().to_string()),
        }
    }
    let mut out = Vec::new();
    for part in parts.iter().map(|p| p.trim_start_matches('<')) {
        let Some((target, params)) = part.split_once('>') else {
            continue;
        };
        let mut rel = None;
        let mut hreflang = false;
        for p in params.split(';') {
            let Some((k, v)) = p.split_once('=') else { continue };
            let v = v.trim().trim_matches('"');
            match k.trim().to_ascii_lowercase().as_str() {
                "rel" => rel = Some(v.to_string()),
                "hreflang" => hreflang = true,
                _ => {}
            }
        }
        let kind = if rel_has(rel.as_deref(), "canonical") {
            LinkKind::Canonical
        } else if rel_has(rel.as_deref(), "alternate") && hreflang {
            LinkKind::Hreflang
        } else {
            continue;
        };
        out.push(FoundLink {
            raw: target.trim().to_string(),
            kind,
            nofollow: false,
        });
    }
    out
}

pub fn extract(
    page: &Url,
    charset: Option<&str>,
    body: &[u8],
    token: &str,
    x_robots_tag: &[String],
    link_header: &[String],
) -> PageLinks {
    let encoding = detect_encoding(charset, body);
    let (text, _, _) = encoding.decode(body);
    let doc = Html::parse_document(&text);

    let base = doc
        .select(&sel("base[href]"))
        .next()
        .and_then(|b| b.value().attr("href"))
        .and_then(|h| page.join(h.trim()).ok())
        .filter(|u| matches!(u.scheme(), "http" | "https"))
        .unwrap_or_else(|| page.clone());

    let mut page_nofollow = x_robots_tag.iter().any(|v| header_nofollow(v, token));
    for meta in doc.select(&sel("meta[name][content]")) {
        let name = meta.value().attr("name").unwrap_or("");
        if (name.eq_ignore_ascii_case("robots") || name.eq_ignore_ascii_case(token))
            && directives_nofollow(meta.value().attr("content").unwrap_or(""))
        {
            page_nofollow = true;
        }
    }

    let mut links = Vec::new();
    for a in doc.select(&sel("a[href], area[href]")) {
        let v = a.value();
        links.push(FoundLink {
            raw: v.attr("href").unwrap_or("").to_string(),
            kind: LinkKind::Anchor,
            nofollow: rel_has(v.attr("rel"), "nofollow"),
        });
    }
    for l in doc.select(&sel("link[rel][href]")) {
        let v = l.value();
        let rel = v.attr("rel");
        let kind = if rel_has(rel, "canonical") {
            LinkKind::Canonical
        } else if rel_has(rel, "alternate") && v.attr("hreflang").is_some() {
            LinkKind::Hreflang
        } else {
            continue;
        };
        links.push(FoundLink {
            raw: v.attr("href").unwrap_or("").to_string(),
            kind,
            nofollow: false,
        });
    }
    for img in doc.select(&sel("img[src]")) {
        links.push(FoundLink {
            raw: img.value().attr("src").unwrap_or("").to_string(),
            kind: LinkKind::Image,
            nofollow: false,
        });
    }
    for value in link_header {
        links.extend(link_header_entries(value));
    }

    PageLinks {
        base,
        links,
        page_nofollow,
        title: title_of(&doc),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(html: &str, xrt: &[&str]) -> PageLinks {
        let xrt: Vec<String> = xrt.iter().map(|s| s.to_string()).collect();
        extract(
            &Url::parse("https://a.test/dir/page").unwrap(),
            None,
            html.as_bytes(),
            "SpiderLens",
            &xrt,
            &[],
        )
    }

    #[test]
    fn anchors_rel_and_raw_text() {
        let p = run(
            r#"<a href="../x">X</a><a href=" /y " rel="Sponsored NoFollow">Y</a><a>no href</a>
               <map><area href="/z"></map>"#,
            &[],
        );
        let got: Vec<_> = p
            .links
            .iter()
            .map(|l| (l.raw.as_str(), l.kind, l.nofollow))
            .collect();
        assert_eq!(
            got,
            [
                ("../x", LinkKind::Anchor, false),
                (" /y ", LinkKind::Anchor, true),
                ("/z", LinkKind::Anchor, false),
            ]
        );
        assert!(!p.page_nofollow);
    }

    #[test]
    fn base_canonical_hreflang_images() {
        let p = run(
            r#"<head><base href="https://cdn.a.test/x/"><link rel="canonical" href="/c">
               <link rel="alternate" hreflang="de" href="/de/"><link rel="stylesheet" href="/s.css"></head>
               <body><img src="i.png"></body>"#,
            &[],
        );
        assert_eq!(p.base.as_str(), "https://cdn.a.test/x/");
        let kinds: Vec<_> = p.links.iter().map(|l| l.kind).collect();
        assert_eq!(kinds, [LinkKind::Canonical, LinkKind::Hreflang, LinkKind::Image]);
    }

    #[test]
    fn page_level_nofollow_sources() {
        assert!(run(r#"<meta name="robots" content="noindex, nofollow">"#, &[]).page_nofollow);
        assert!(run(r#"<meta name="ROBOTS" content="none">"#, &[]).page_nofollow);
        assert!(run(r#"<meta name="spiderlens" content="nofollow">"#, &[]).page_nofollow);
        assert!(!run(r#"<meta name="googlebot" content="nofollow">"#, &[]).page_nofollow);
        assert!(run("", &["noindex, nofollow"]).page_nofollow);
        assert!(run("", &["SpiderLens: nofollow"]).page_nofollow);
        assert!(!run("", &["googlebot: nofollow"]).page_nofollow);
        assert!(!run("", &["unavailable_after: 2030-01-01"]).page_nofollow);
    }

    #[test]
    fn link_header_canonical() {
        let got = link_header_entries(
            r#"<https://a.test/c>; rel="canonical", <https://a.test/de>; rel="alternate"; hreflang="de""#,
        );
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].raw, "https://a.test/c");
        assert_eq!(got[1].kind, LinkKind::Hreflang);
    }
}
