//! AMP checks on the parsed page (spec 027 part E, FR-E02).
//!
//! A core subset of the AMP rules, not the official validator: the marker,
//! runtime script, boilerplate style, viewport, canonical, author scripts,
//! the size of `<style amp-custom>` and a short list of disallowed elements.
//! Whether the canonical page links back is checked across the site by the
//! results reader, since it needs the other page.

use scraper::{ElementRef, Html};
use serde::Serialize;

/// The most CSS `<style amp-custom>` may hold, in bytes.
pub const STYLE_LIMIT: usize = 75_000;

const RUNTIME: &str = "https://cdn.ampproject.org/v0.";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AmpFormat {
    /// `<html amp>` or `<html ⚡>`.
    Website,
    /// `<html amp4email>` or `<html ⚡4email>`.
    Email,
}

impl AmpFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Website => "website",
            Self::Email => "email",
        }
    }
}

/// One failed check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AmpFailure {
    /// runtime | boilerplate | viewport | canonical | author_script |
    /// style_size | disallowed
    pub check: &'static str,
    pub detail: String,
    /// The element's position among the page's elements, from 1, when the
    /// failure is one element.
    pub pos: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AmpReport {
    pub format: AmpFormat,
    pub failures: Vec<AmpFailure>,
    /// Bytes in `<style amp-custom>`.
    pub style_bytes: u32,
}

/// The AMP format the root element declares, if any.
pub fn format_of(doc: &Html) -> Option<AmpFormat> {
    let root = doc.root_element().value();
    let has = |a: &str| root.attrs().any(|(k, _)| k.eq_ignore_ascii_case(a));
    if has("amp4email") || has("⚡4email") {
        Some(AmpFormat::Email)
    } else if has("amp") || has("⚡") {
        Some(AmpFormat::Website)
    } else {
        None
    }
}

/// Checks a page the root element marks as AMP; None for other pages.
pub fn check(doc: &Html) -> Option<AmpReport> {
    let format = format_of(doc)?;
    let web = format == AmpFormat::Website;
    let mut failures = Vec::new();
    let mut fail = |check, detail: String, pos| failures.push(AmpFailure { check, detail, pos });
    let mut runtime = false;
    let mut boilerplate = false;
    let mut viewport = false;
    let mut canonical = false;
    let mut style_bytes = 0usize;
    let mut pos = 0u32;

    for node in doc.root_element().descendants() {
        let Some(el) = ElementRef::wrap(node) else {
            continue;
        };
        pos += 1;
        let e = el.value();
        let attr = |a: &str| e.attr(a);
        match e.name() {
            "script" => {
                let src = attr("src").unwrap_or("").trim();
                let ty = attr("type").unwrap_or("").trim().to_ascii_lowercase();
                runtime |= src.starts_with(RUNTIME);
                let allowed = src.starts_with(RUNTIME)
                    || attr("custom-element").is_some()
                    || attr("custom-template").is_some()
                    || ty == "application/ld+json"
                    || ty == "application/json";
                if !allowed {
                    let what = if src.is_empty() {
                        "An inline script".to_string()
                    } else {
                        format!("The script {src}")
                    };
                    fail(
                        "author_script",
                        format!("{what} is not an AMP component."),
                        Some(pos),
                    );
                }
            }
            "style" => {
                if attr("amp-boilerplate").is_some() || attr("amp4email-boilerplate").is_some() {
                    boilerplate = true;
                } else if attr("amp-custom").is_some() {
                    style_bytes += el.text().map(str::len).sum::<usize>();
                }
            }
            "meta" if attr("name").is_some_and(|n| n.trim().eq_ignore_ascii_case("viewport")) => {
                viewport = true
            }
            "link"
                if attr("rel").is_some_and(|r| {
                    r.split_ascii_whitespace()
                        .any(|t| t.eq_ignore_ascii_case("canonical"))
                }) =>
            {
                canonical = true;
            }
            name @ ("frame" | "frameset" | "object" | "embed" | "applet") => {
                fail(
                    "disallowed",
                    format!("<{name}> is not allowed in AMP."),
                    Some(pos),
                );
            }
            "base" => {
                let target = attr("target").map(|t| t.trim().to_ascii_lowercase());
                if target.as_deref().is_some_and(|t| t != "_blank" && t != "_top") {
                    fail(
                        "disallowed",
                        "<base> may only have target _blank or _top.".into(),
                        Some(pos),
                    );
                }
            }
            _ => {}
        }
    }
    if !runtime {
        fail(
            "runtime",
            "The AMP runtime script (cdn.ampproject.org/v0.js) is missing.".into(),
            None,
        );
    }
    if !boilerplate {
        let tag = if web {
            "amp-boilerplate"
        } else {
            "amp4email-boilerplate"
        };
        fail(
            "boilerplate",
            format!("The required <style {tag}> is missing."),
            None,
        );
    }
    if web && !viewport {
        fail("viewport", "The viewport meta tag is missing.".into(), None);
    }
    if web && !canonical {
        fail("canonical", "The canonical link is missing.".into(), None);
    }
    if style_bytes > STYLE_LIMIT {
        fail(
            "style_size",
            format!("<style amp-custom> holds {style_bytes} bytes; the limit is {STYLE_LIMIT}."),
            None,
        );
    }
    Some(AmpReport {
        format,
        failures,
        style_bytes: style_bytes as u32,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID: &str = r#"<!doctype html><html ⚡ lang="en"><head><meta charset="utf-8">
        <script async src="https://cdn.ampproject.org/v0.js"></script>
        <script async custom-element="amp-carousel" src="https://cdn.ampproject.org/v0/amp-carousel-0.1.js"></script>
        <title>AMP</title><link rel="canonical" href="/page">
        <meta name="viewport" content="width=device-width">
        <script type="application/ld+json">{}</script>
        <style amp-boilerplate>body{visibility:hidden}</style><style amp-custom>p{color:red}</style>
        </head><body><p>Hi</p></body></html>"#;

    fn checks(html: &str) -> Vec<&'static str> {
        check(&Html::parse_document(html))
            .unwrap()
            .failures
            .iter()
            .map(|f| f.check)
            .collect()
    }

    #[test]
    fn a_valid_page_passes() {
        let r = check(&Html::parse_document(VALID)).unwrap();
        assert_eq!(r.format, AmpFormat::Website);
        assert!(r.failures.is_empty(), "{:?}", r.failures);
        assert_eq!(r.style_bytes, 12);
    }

    #[test]
    fn other_pages_are_not_checked() {
        assert!(check(&Html::parse_document("<html><body>x</body></html>")).is_none());
    }

    #[test]
    fn each_missing_part_fails_its_check() {
        let bare = r#"<html amp><head><title>x</title></head><body></body></html>"#;
        assert_eq!(checks(bare), ["runtime", "boilerplate", "viewport", "canonical"]);
    }

    #[test]
    fn author_scripts_and_disallowed_elements_fail_with_their_position() {
        let html = VALID.replace(
            "<p>Hi</p>",
            r#"<script>alert(1)</script><script src="/app.js"></script><object></object><embed><base target="_self">"#,
        );
        let r = check(&Html::parse_document(&html)).unwrap();
        let got: Vec<_> = r.failures.iter().map(|f| f.check).collect();
        assert_eq!(
            got,
            [
                "author_script",
                "author_script",
                "disallowed",
                "disallowed",
                "disallowed"
            ]
        );
        assert!(r.failures[1].detail.contains("/app.js"));
        assert!(r.failures.iter().all(|f| f.pos.is_some()));
        assert!(r.failures[0].pos < r.failures[1].pos);
    }

    #[test]
    fn custom_css_over_the_limit_fails() {
        let big = "a".repeat(STYLE_LIMIT + 1);
        let html = VALID.replace("p{color:red}", &big);
        assert_eq!(checks(&html), ["style_size"]);
    }

    #[test]
    fn email_needs_no_viewport_or_canonical() {
        let html = r#"<html ⚡4email><head><script async src="https://cdn.ampproject.org/v0.js"></script>
            <style amp4email-boilerplate>body{visibility:hidden}</style></head><body></body></html>"#;
        let r = check(&Html::parse_document(html)).unwrap();
        assert_eq!(r.format, AmpFormat::Email);
        assert!(r.failures.is_empty());
    }
}
