//! Comparing the raw and the rendered version of one page (spec 014 plan
//! D5; FR-023, FR-024, FR-029; research R10). Pure functions, no I/O.
//!
//! Each element of the rendered page gets an origin by multiset matching on
//! a key, so duplicates match one for one: a link that appears twice raw and
//! three times rendered gives two `both` and one `rendered`. Elements of the
//! raw page left unmatched are `raw` (gone after rendering).

use std::collections::HashMap;
use std::hash::Hash;

use url::Url;

use super::directives::page_flags;
use super::{PageData, Ref, StructuredBlock, text};
use crate::crawl::LinkKind;

/// Where an element was found (FR-023).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// In both the raw HTML and the rendered page.
    Both,
    /// Only after rendering.
    Rendered,
    /// Only in the raw HTML.
    Raw,
}

impl Origin {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Both => "both",
            Self::Rendered => "rendered",
            Self::Raw => "raw",
        }
    }
}

/// The origins of one element type: one per rendered item, in rendered
/// order (`Both` or `Rendered`), and the positions of raw items that have
/// no rendered counterpart.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Origins {
    pub rendered: Vec<Origin>,
    pub raw_only: Vec<usize>,
}

impl Origins {
    pub fn rendered_only(&self) -> usize {
        self.rendered.iter().filter(|o| **o == Origin::Rendered).count()
    }
}

/// One-for-one multiset matching of `rendered` against `raw`.
pub fn match_items<K: Eq + Hash>(raw: &[K], rendered: &[K]) -> Origins {
    let mut pool: HashMap<&K, Vec<usize>> = HashMap::new();
    for (i, k) in raw.iter().enumerate().rev() {
        pool.entry(k).or_default().push(i);
    }
    let mut used = vec![false; raw.len()];
    let rendered = rendered
        .iter()
        .map(|k| match pool.get_mut(k).and_then(Vec::pop) {
            Some(i) => {
                used[i] = true;
                Origin::Both
            }
            None => Origin::Rendered,
        })
        .collect();
    let raw_only = used
        .iter()
        .enumerate()
        .filter(|(_, u)| !**u)
        .map(|(i, _)| i)
        .collect();
    Origins { rendered, raw_only }
}

/// How one value changed between raw and rendered (FR-024).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Diff {
    Same,
    /// Missing from the raw HTML, present after rendering.
    Added,
    /// Present in the raw HTML, missing after rendering.
    Removed,
    Changed,
}

impl Diff {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Same => "same",
            Self::Added => "added",
            Self::Removed => "removed",
            Self::Changed => "changed",
        }
    }

    fn of<T: PartialEq>(raw: Option<T>, rendered: Option<T>) -> Self {
        match (raw, rendered) {
            (None, None) => Self::Same,
            (None, Some(_)) => Self::Added,
            (Some(_), None) => Self::Removed,
            (Some(a), Some(b)) if a == b => Self::Same,
            _ => Self::Changed,
        }
    }

    fn of_flag(raw: bool, rendered: bool) -> Self {
        Self::of(raw.then_some(()), rendered.then_some(()))
    }
}

/// Element origins of one page, per type.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ElementOrigins {
    pub titles: Origins,
    pub descriptions: Origins,
    pub headings: Origins,
    pub directives: Origins,
    pub structured: Origins,
    /// Every reference (links, images, canonicals, hreflang, ...).
    pub refs: Origins,
}

/// The `render_comparisons` row of one page (data-model.md), plus the
/// element origins the rendered rows are written with.
#[derive(Debug, Clone, PartialEq)]
pub struct Comparison {
    pub words_raw: u32,
    pub words_rendered: u32,
    pub words_change: i64,
    /// Anchor links.
    pub links_raw: u32,
    pub links_rendered: u32,
    pub links_rendered_only: u32,
    pub links_raw_only: u32,
    pub title_diff: Diff,
    pub description_diff: Diff,
    pub h1_diff: Diff,
    pub canonical_diff: Diff,
    pub noindex_diff: Diff,
    pub nofollow_diff: Diff,
    pub hreflang_diff: Diff,
    pub structured_rendered_only: u32,
    /// Share (0 to 1) found only after rendering; `None` when the rendered
    /// page has none of that type.
    pub dep_text: Option<f64>,
    pub dep_headings: Option<f64>,
    pub dep_internal_links: Option<f64>,
    pub dep_structured: Option<f64>,
    pub dep_images: Option<f64>,
    pub raw_has_title: bool,
    pub raw_has_description: bool,
    pub raw_has_canonical: bool,
    pub raw_has_directives: bool,
    /// Overall JavaScript dependence, 0 to 100: `dep_text` as a percentage.
    pub dependence: f64,
    pub origins: ElementOrigins,
}

impl Comparison {
    /// Whether rendering changed anything extracted (US3 scenario 7).
    pub fn changed_anything(&self) -> bool {
        let o = &self.origins;
        let all = [
            &o.titles,
            &o.descriptions,
            &o.headings,
            &o.directives,
            &o.structured,
            &o.refs,
        ];
        self.words_change != 0
            || self.dep_text.is_some_and(|d| d > 0.0)
            || all
                .iter()
                .any(|x| x.rendered_only() > 0 || !x.raw_only.is_empty())
    }
}

/// The resolved target of a reference, as matched across versions.
fn target(base: &Url, r: &Ref) -> String {
    base.join(r.raw.trim())
        .map(String::from)
        .unwrap_or_else(|_| r.raw.trim().to_owned())
}

#[derive(PartialEq, Eq, Hash)]
struct RefKey {
    kind: LinkKind,
    target: String,
    /// Anchor text for links; not part of the key for images (plan D5).
    text: Option<String>,
}

fn ref_keys(page: &PageData) -> Vec<RefKey> {
    page.refs
        .iter()
        .map(|r| RefKey {
            kind: r.kind,
            target: target(&page.base, r),
            text: if r.kind == LinkKind::Image {
                None
            } else {
                r.text.clone()
            },
        })
        .collect()
}

/// JSON with object keys sorted, so key order does not count as a change.
fn canonical_json(v: &serde_json::Value, out: &mut String) {
    match v {
        serde_json::Value::Object(m) => {
            let mut keys: Vec<_> = m.keys().collect();
            keys.sort();
            out.push('{');
            for (i, k) in keys.into_iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&serde_json::Value::String(k.clone()).to_string());
                out.push(':');
                canonical_json(&m[k], out);
            }
            out.push('}');
        }
        serde_json::Value::Array(a) => {
            out.push('[');
            for (i, x) in a.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                canonical_json(x, out);
            }
            out.push(']');
        }
        other => out.push_str(&other.to_string()),
    }
}

/// xxh3 of the normalised block (plan D5).
pub fn structured_key(b: &StructuredBlock) -> (super::StructuredKind, i64) {
    let normalised = match serde_json::from_str::<serde_json::Value>(&b.raw) {
        Ok(v) => {
            let mut s = String::new();
            canonical_json(&v, &mut s);
            s
        }
        Err(_) => text::collapse(&b.raw),
    };
    (b.kind, text::hash(&normalised))
}

fn share(rendered_only: usize, total: usize) -> Option<f64> {
    (total > 0).then(|| rendered_only as f64 / total as f64)
}

fn first_h1(p: &PageData) -> Option<&str> {
    p.headings.iter().find(|h| h.level == 1).map(|h| h.text.as_str())
}

fn canonical(p: &PageData) -> Option<String> {
    p.refs
        .iter()
        .find(|r| r.kind == LinkKind::Canonical)
        .map(|r| target(&p.base, r))
}

fn hreflang_set(p: &PageData) -> Option<Vec<(String, String)>> {
    let mut set: Vec<_> = p
        .refs
        .iter()
        .filter(|r| r.kind == LinkKind::Hreflang)
        .map(|r| {
            (
                r.lang.clone().unwrap_or_default().to_ascii_lowercase(),
                target(&p.base, r),
            )
        })
        .collect();
    set.sort();
    set.dedup();
    (!set.is_empty()).then_some(set)
}

/// Rendered-only words over rendered words, per word: the rendered count
/// minus the raw count when positive (research R10). Needs `visible_text`
/// on both sides (extraction with `keep_text`).
fn text_dependence(raw: &PageData, rendered: &PageData) -> Option<f64> {
    let rendered_text = rendered.visible_text.as_deref().unwrap_or("");
    let mut counts: HashMap<&str, i64> = HashMap::new();
    let mut total = 0usize;
    for w in text::words(rendered_text) {
        *counts.entry(w).or_default() += 1;
        total += 1;
    }
    for w in text::words(raw.visible_text.as_deref().unwrap_or("")) {
        if let Some(c) = counts.get_mut(w) {
            *c -= 1;
        }
    }
    let only: i64 = counts.values().filter(|c| **c > 0).sum();
    share(only as usize, total)
}

/// Compares the raw and the rendered extraction of one page. `token` is the
/// crawl's robots.txt token (for crawler-specific meta tags); `internal`
/// tells whether a link target belongs to the crawled site.
pub fn compare(
    raw: &PageData,
    rendered: &PageData,
    token: &str,
    internal: &dyn Fn(&Url) -> bool,
) -> Comparison {
    let titles = match_items(&texts(&raw.titles), &texts(&rendered.titles));
    let descriptions = match_items(
        &texts(&raw.meta_descriptions),
        &texts(&rendered.meta_descriptions),
    );
    let heading_key = |p: &PageData| -> Vec<(u8, String)> {
        p.headings.iter().map(|h| (h.level, h.text.clone())).collect()
    };
    let headings = match_items(&heading_key(raw), &heading_key(rendered));
    let directive_key = |p: &PageData| -> Vec<(&'static str, String, String, Option<String>)> {
        p.directives
            .iter()
            .map(|d| {
                (
                    d.source.as_str(),
                    d.crawler.clone(),
                    d.directive.clone(),
                    d.value.clone(),
                )
            })
            .collect()
    };
    let directives = match_items(&directive_key(raw), &directive_key(rendered));
    let structured_keys = |p: &PageData| -> Vec<_> { p.structured.iter().map(structured_key).collect() };
    let structured = match_items(&structured_keys(raw), &structured_keys(rendered));
    let (raw_refs, rendered_refs) = (ref_keys(raw), ref_keys(rendered));
    let refs = match_items(&raw_refs, &rendered_refs);

    // Counts by kind over the matched references.
    let of_kind = |kind: LinkKind| {
        rendered
            .refs
            .iter()
            .zip(&refs.rendered)
            .filter(move |(r, _)| r.kind == kind)
    };
    let anchors_rendered = of_kind(LinkKind::Anchor).count();
    let anchors_rendered_only = of_kind(LinkKind::Anchor)
        .filter(|(_, o)| **o == Origin::Rendered)
        .count();
    let anchors_raw = raw.refs.iter().filter(|r| r.kind == LinkKind::Anchor).count();
    let anchors_raw_only = refs
        .raw_only
        .iter()
        .filter(|&&i| raw.refs[i].kind == LinkKind::Anchor)
        .count();
    let internal_anchors: Vec<_> = of_kind(LinkKind::Anchor)
        .filter(|(r, _)| {
            Url::parse(&target(&rendered.base, r))
                .map(|u| internal(&u))
                .unwrap_or(false)
        })
        .collect();
    let internal_only = internal_anchors
        .iter()
        .filter(|(_, o)| **o == Origin::Rendered)
        .count();
    let images = of_kind(LinkKind::Image).count();
    let images_only = of_kind(LinkKind::Image)
        .filter(|(_, o)| **o == Origin::Rendered)
        .count();

    let (raw_noindex, raw_nofollow) = page_flags(&raw.directives, token);
    let (r_noindex, r_nofollow) = page_flags(&rendered.directives, token);
    let dep_text = text_dependence(raw, rendered);

    Comparison {
        words_raw: raw.word_count,
        words_rendered: rendered.word_count,
        words_change: i64::from(rendered.word_count) - i64::from(raw.word_count),
        links_raw: anchors_raw as u32,
        links_rendered: anchors_rendered as u32,
        links_rendered_only: anchors_rendered_only as u32,
        links_raw_only: anchors_raw_only as u32,
        title_diff: Diff::of(raw.title(), rendered.title()),
        description_diff: Diff::of(raw.meta_description(), rendered.meta_description()),
        h1_diff: Diff::of(first_h1(raw), first_h1(rendered)),
        canonical_diff: Diff::of(canonical(raw), canonical(rendered)),
        noindex_diff: Diff::of_flag(raw_noindex, r_noindex),
        nofollow_diff: Diff::of_flag(raw_nofollow, r_nofollow),
        hreflang_diff: Diff::of(hreflang_set(raw), hreflang_set(rendered)),
        structured_rendered_only: structured.rendered_only() as u32,
        dep_text,
        dep_headings: share(headings.rendered_only(), rendered.headings.len()),
        dep_internal_links: share(internal_only, internal_anchors.len()),
        dep_structured: share(structured.rendered_only(), rendered.structured.len()),
        dep_images: share(images_only, images),
        raw_has_title: raw.title().is_some(),
        raw_has_description: raw.meta_description().is_some(),
        raw_has_canonical: canonical(raw).is_some(),
        raw_has_directives: !raw.directives.is_empty(),
        dependence: dep_text.unwrap_or(0.0) * 100.0,
        origins: ElementOrigins {
            titles,
            descriptions,
            headings,
            directives,
            structured,
            refs,
        },
    }
}

fn texts(items: &[super::TextItem]) -> Vec<&str> {
    items.iter().map(|t| t.text.as_str()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extract::{PageInput, extract_page};

    const URL: &str = "https://js.test/page";

    fn data(html: &str) -> PageData {
        let url = Url::parse(URL).unwrap();
        extract_page(&PageInput {
            url: &url,
            charset: None,
            body: html.as_bytes(),
            truncated: false,
            robots_token: "SpiderLens",
            keep_text: true,
        })
    }

    fn cmp(raw: &str, rendered: &str) -> Comparison {
        let internal = |u: &Url| u.host_str() == Some("js.test");
        compare(&data(raw), &data(rendered), "SpiderLens", &internal)
    }

    fn page(head: &str, body: &str) -> String {
        format!("<!DOCTYPE html><html><head>{head}</head><body>{body}</body></html>")
    }

    fn links(range: std::ops::Range<usize>) -> String {
        range
            .map(|i| format!("<a href=\"/p/{i}\">Page {i}</a>"))
            .collect()
    }

    #[test]
    fn multiset_matching_is_one_for_one() {
        let o = match_items(&["a", "a", "b"], &["a", "a", "a", "c"]);
        assert_eq!(
            o.rendered,
            [Origin::Both, Origin::Both, Origin::Rendered, Origin::Rendered]
        );
        assert_eq!(o.raw_only, [2]);
        let o = match_items::<&str>(&["x", "y"], &[]);
        assert_eq!(o.raw_only, [0, 1]);
    }

    #[test]
    fn us3_1_links_added_by_javascript() {
        let raw = page("<title>T</title>", &links(0..28));
        let rendered = page("<title>T</title>", &links(0..40));
        let c = cmp(&raw, &rendered);
        assert_eq!((c.links_raw, c.links_rendered), (28, 40));
        assert_eq!(c.links_rendered_only, 12);
        assert_eq!(c.links_raw_only, 0);
        assert_eq!(c.dep_internal_links, Some(12.0 / 40.0));
        // The same target with other anchor text is a different link.
        let c = cmp(
            &page("", "<a href='/a'>One</a>"),
            &page("", "<a href='https://js.test/a'>Two</a>"),
        );
        assert_eq!((c.links_rendered_only, c.links_raw_only), (1, 1));
        // Same target and text written differently match.
        let c = cmp(
            &page("", "<a href='/a'>One</a>"),
            &page("", "<a href='https://js.test/a'>One</a>"),
        );
        assert_eq!((c.links_rendered_only, c.links_raw_only), (0, 0));
    }

    #[test]
    fn us3_2_text_added_by_javascript() {
        let raw = page("", &format!("<p>{}</p>", "alpha ".repeat(50)));
        let rendered = page(
            "",
            &format!("<p>{}</p><p>{}</p>", "alpha ".repeat(50), "beta ".repeat(850)),
        );
        let c = cmp(&raw, &rendered);
        assert_eq!((c.words_raw, c.words_rendered, c.words_change), (50, 900, 850));
        assert_eq!(c.dep_text, Some(850.0 / 900.0));
        assert!((c.dependence - 850.0 / 9.0).abs() < 1e-9);
    }

    #[test]
    fn us3_4_canonical_mismatch() {
        let raw = page("<link rel=canonical href='/a'>", "");
        let rendered = page("<link rel=canonical href='/b'>", "");
        let c = cmp(&raw, &rendered);
        assert_eq!(c.canonical_diff, Diff::Changed);
        assert!(c.raw_has_canonical);
        assert_eq!(c.origins.refs.rendered, [Origin::Rendered]);
        assert_eq!(c.origins.refs.raw_only, [0]);
        // Only added after rendering.
        let c = cmp(&page("", ""), &rendered);
        assert_eq!(c.canonical_diff, Diff::Added);
        assert!(!c.raw_has_canonical);
        // Written differently, same target.
        let c = cmp(&raw, &page("<link rel=canonical href='https://js.test/a'>", ""));
        assert_eq!(c.canonical_diff, Diff::Same);
    }

    #[test]
    fn us3_5_title_description_and_h1_updated_removed_or_added() {
        let raw = page(
            "<title>Old</title><meta name=description content='Old d'>",
            "<h1>Old h</h1>",
        );
        let c = cmp(
            &raw,
            &page(
                "<title>New</title><meta name=description content='New d'>",
                "<h1>New h</h1>",
            ),
        );
        assert_eq!(
            (c.title_diff, c.description_diff, c.h1_diff),
            (Diff::Changed, Diff::Changed, Diff::Changed)
        );
        let c = cmp(&raw, &page("", ""));
        assert_eq!(
            (c.title_diff, c.description_diff, c.h1_diff),
            (Diff::Removed, Diff::Removed, Diff::Removed)
        );
        let c = cmp(&page("", ""), &raw);
        assert_eq!(
            (c.title_diff, c.description_diff, c.h1_diff),
            (Diff::Added, Diff::Added, Diff::Added)
        );
        assert!(!c.raw_has_title && !c.raw_has_description);
        assert_eq!(c.origins.titles.rendered, [Origin::Rendered]);
        assert_eq!(c.dep_headings, Some(1.0));
    }

    #[test]
    fn noindex_and_nofollow_added_or_removed() {
        let ni = page("<meta name=robots content='noindex, nofollow'>", "");
        let c = cmp(&ni, &page("", ""));
        assert_eq!((c.noindex_diff, c.nofollow_diff), (Diff::Removed, Diff::Removed));
        assert!(c.raw_has_directives);
        assert_eq!(c.origins.directives.raw_only.len(), 2);
        let c = cmp(&page("", ""), &ni);
        assert_eq!((c.noindex_diff, c.nofollow_diff), (Diff::Added, Diff::Added));
        // Another crawler's noindex does not count for ours.
        let other = page("<meta name=otherbot content=noindex>", "");
        assert_eq!(cmp(&page("", ""), &other).noindex_diff, Diff::Same);
    }

    #[test]
    fn hreflang_set_and_structured_data() {
        let h = |langs: &[&str]| -> String {
            langs
                .iter()
                .map(|l| format!("<link rel=alternate hreflang={l} href='/{l}/'>"))
                .collect()
        };
        assert_eq!(
            cmp(&page(&h(&["en", "de"]), ""), &page(&h(&["de", "en"]), "")).hreflang_diff,
            Diff::Same
        );
        assert_eq!(
            cmp(&page(&h(&["en"]), ""), &page(&h(&["en", "fr"]), "")).hreflang_diff,
            Diff::Changed
        );
        assert_eq!(
            cmp(&page("", ""), &page(&h(&["en"]), "")).hreflang_diff,
            Diff::Added
        );

        let ld = |json: &str| format!("<script type='application/ld+json'>{json}</script>");
        let raw = page(&ld(r#"{"@type": "Organization", "name": "A"}"#), "");
        // Key order and spacing are not differences.
        let same = page(&ld(r#"{"name":"A","@type":"Organization"}"#), "");
        let c = cmp(&raw, &same);
        assert_eq!(c.structured_rendered_only, 0);
        assert_eq!(c.dep_structured, Some(0.0));
        let added = format!(
            "{}{}",
            ld(r#"{"@type": "Organization", "name": "A"}"#),
            ld(r#"{"@type":"Product"}"#)
        );
        let c = cmp(&raw, &page(&added, ""));
        assert_eq!(c.structured_rendered_only, 1);
        assert_eq!(c.dep_structured, Some(0.5));
    }

    #[test]
    fn images_match_by_source_only() {
        let raw = page("", "<img src='/a.png' alt='one'><img src='/b.png'>");
        let rendered = page(
            "",
            "<img src='https://js.test/a.png' alt='two'><img src='/c.png'>",
        );
        let c = cmp(&raw, &rendered);
        assert_eq!(c.dep_images, Some(0.5));
        assert_eq!(c.origins.refs.raw_only.len(), 1);
    }

    #[test]
    fn us3_7_and_us7_2_nothing_changed() {
        let html = page(
            "<title>T</title><meta name=description content=D><link rel=canonical href='/'>",
            &format!(
                "<h1>H</h1><p>{}</p>{}<img src='/i.png'><script type='application/ld+json'>{{\"a\":1}}</script>",
                "text ".repeat(100),
                links(0..5)
            ),
        );
        let c = cmp(&html, &html);
        assert!(!c.changed_anything());
        for d in [
            c.title_diff,
            c.description_diff,
            c.h1_diff,
            c.canonical_diff,
            c.noindex_diff,
            c.nofollow_diff,
            c.hreflang_diff,
        ] {
            assert_eq!(d, Diff::Same);
        }
        assert_eq!(c.words_change, 0);
        assert_eq!(
            (
                c.links_rendered_only,
                c.links_raw_only,
                c.structured_rendered_only
            ),
            (0, 0, 0)
        );
        // US7.2: 0% for every element.
        for d in [
            c.dep_text,
            c.dep_headings,
            c.dep_internal_links,
            c.dep_structured,
            c.dep_images,
        ] {
            assert_eq!(d, Some(0.0));
        }
        assert_eq!(c.dependence, 0.0);
    }

    #[test]
    fn us7_1_empty_shell_is_fully_dependent() {
        let shell = page(
            "<title>App</title>",
            "<div id=root></div><script src='/app.js'></script>",
        );
        let rendered = page(
            "<title>App</title>",
            &format!(
                "<h1>Welcome</h1><h2>More</h2><p>{}</p>{}",
                "word ".repeat(200),
                links(0..10)
            ),
        );
        let c = cmp(&shell, &rendered);
        assert_eq!(c.dep_text, Some(1.0));
        assert_eq!(c.dependence, 100.0);
        assert_eq!(c.dep_headings, Some(1.0));
        assert_eq!(c.dep_internal_links, Some(1.0));
        assert_eq!(c.title_diff, Diff::Same);
        assert!(c.changed_anything());
        // No images or structured data rendered: no share.
        assert_eq!((c.dep_images, c.dep_structured), (None, None));
    }

    #[test]
    fn text_dependence_counts_words_not_order() {
        let c = cmp(&page("", "<p>a b c</p>"), &page("", "<p>c b a a</p>"));
        assert_eq!(c.dep_text, Some(0.25));
        // An empty rendered page has no share and no dependence.
        let c = cmp(&page("", "<p>a</p>"), &page("", ""));
        assert_eq!((c.dep_text, c.dependence), (None, 0.0));
    }
}
