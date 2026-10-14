//! The HTTP `Link` header: canonical, hreflang alternates and next/prev
//! (FR-003, FR-018, FR-019, FR-026).
//!
//! `Link: <https://a.test/p>; rel="canonical", <...>; rel="alternate"; hreflang="de"`

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
    } else if rel_has(rel, "amphtml") {
        Some(LinkKind::AmpHtml)
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

pub(super) fn parse(value: &str) -> Vec<Ref> {
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
        let mut hreflang = None;
        for p in params.split(';') {
            let Some((k, v)) = p.split_once('=') else { continue };
            let v = v.trim().trim_matches('"');
            match k.trim().to_ascii_lowercase().as_str() {
                "rel" => rel = Some(v.to_string()),
                "hreflang" => hreflang = Some(v.to_string()),
                _ => {}
            }
        }
        let Some(kind) = link_kind(rel.as_deref(), hreflang.is_some()) else {
            continue;
        };
        let mut r = Ref::new(kind, target.trim(), RefSource::Header);
        r.rel = rel;
        if kind == LinkKind::Hreflang {
            r.lang = hreflang;
        }
        out.push(r);
    }
    out
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
    fn commas_inside_parameters() {
        let got = parse(r#"<https://a.test/x>; rel="canonical"; title="a, b""#);
        assert_eq!(got.len(), 1);
    }
}
