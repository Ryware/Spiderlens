//! The HTTP `Link` header: canonical, hreflang alternates and next/prev
//! (FR-003, FR-018, FR-019, FR-026).
//!
//! `Link: <https://a.test/p>; rel="canonical", <...>; rel="alternate"; hreflang="de"`

use super::agents::{AgentLink, AgentLinkKind};
use super::{Ref, RefSource};
use crate::crawl::LinkKind;

pub(super) fn rel_has(rel: Option<&str>, token: &str) -> bool {
    rel.is_some_and(|r| r.split_ascii_whitespace().any(|t| t.eq_ignore_ascii_case(token)))
}

/// The kind of a `<link>` element or `Link` header entry, if it is one we
/// record.
pub(super) fn link_kind(rel: Option<&str>, has_hreflang: bool) -> Option<LinkKind> {
    if rel_has(rel, "canonical") {
        Some(LinkKind::Canonical)
    } else if rel_has(rel, "alternate") && has_hreflang {
        Some(LinkKind::Hreflang)
    } else if rel_has(rel, "next") {
        Some(LinkKind::Next)
    } else if rel_has(rel, "prev") || rel_has(rel, "previous") {
        Some(LinkKind::Prev)
    } else {
        None
    }
}

/// One `Link` entry: target, rel, hreflang and type.
struct Entry<'a> {
    target: &'a str,
    rel: Option<String>,
    hreflang: Option<String>,
    mime: Option<String>,
}

fn entries(value: &str) -> Vec<Entry<'_>> {
    // Entries are separated by commas; a comma not followed by `<` belongs
    // to a parameter value.
    let mut parts: Vec<&str> = Vec::new();
    let mut start = 0;
    for (i, _) in value.match_indices(',') {
        if value[i + 1..].trim_start().starts_with('<') {
            parts.push(&value[start..i]);
            start = i + 1;
        }
    }
    parts.push(&value[start..]);
    let mut out = Vec::new();
    for part in parts.iter().map(|p| p.trim_start().trim_start_matches('<')) {
        let Some((target, params)) = part.split_once('>') else {
            continue;
        };
        let mut e = Entry {
            target: target.trim(),
            rel: None,
            hreflang: None,
            mime: None,
        };
        for p in params.split(';') {
            let Some((k, v)) = p.split_once('=') else { continue };
            let v = v.trim().trim_matches('"');
            match k.trim().to_ascii_lowercase().as_str() {
                "rel" => e.rel = Some(v.to_string()),
                "hreflang" => e.hreflang = Some(v.to_string()),
                "type" => e.mime = Some(v.to_string()),
                _ => {}
            }
        }
        out.push(e);
    }
    out
}

pub(super) fn parse(value: &str) -> Vec<Ref> {
    let mut out = Vec::new();
    for e in entries(value) {
        let Some(kind) = link_kind(e.rel.as_deref(), e.hreflang.is_some()) else {
            continue;
        };
        let mut r = Ref::new(kind, e.target, RefSource::Header);
        r.rel = e.rel;
        if kind == LinkKind::Hreflang {
            r.lang = e.hreflang;
        }
        out.push(r);
    }
    out
}

/// Markdown alternates and service links (spec 016), which are not crawl
/// links.
pub(super) fn parse_agent(value: &str) -> Vec<AgentLink> {
    entries(value)
        .into_iter()
        .filter_map(|e| {
            Some(AgentLink {
                kind: AgentLinkKind::from_rel(e.rel.as_deref(), e.mime.as_deref())?,
                href: e.target.to_string(),
                source: RefSource::Header,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_hreflang_and_next() {
        let got = parse(
            r#"<https://a.test/c>; rel="canonical", <https://a.test/de>; rel="alternate"; hreflang="de", <https://a.test/2>; rel=next, </s.css>; rel=preload"#,
        );
        let kinds: Vec<_> = got
            .iter()
            .map(|r| (r.kind, r.raw.as_str(), r.lang.as_deref()))
            .collect();
        assert_eq!(
            kinds,
            [
                (LinkKind::Canonical, "https://a.test/c", None),
                (LinkKind::Hreflang, "https://a.test/de", Some("de")),
                (LinkKind::Next, "https://a.test/2", None),
            ]
        );
        assert!(got.iter().all(|r| r.source == RefSource::Header));
    }

    #[test]
    fn agent_links() {
        let got = parse_agent(
            r#"</p.md>; rel="alternate"; type="text/markdown", </openapi.json>; rel="service-desc", </c>; rel=canonical"#,
        );
        let kinds: Vec<_> = got.iter().map(|l| (l.kind, l.href.as_str())).collect();
        assert_eq!(
            kinds,
            [
                (AgentLinkKind::MarkdownAlternate, "/p.md"),
                (AgentLinkKind::ServiceDesc, "/openapi.json"),
            ]
        );
    }

    #[test]
    fn commas_inside_parameters() {
        let got = parse(r#"<https://a.test/x>; rel="canonical"; title="a, b""#);
        assert_eq!(got.len(), 1);
    }
}
