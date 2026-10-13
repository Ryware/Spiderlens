//! Acceptance scenarios of spec 005 at the extraction level (SC-001).
//! Scenarios that need a crawl (redirect hops, internal/external, queueing,
//! storage) are in `tests/extraction.rs`.

use super::*;
use crate::crawl::LinkKind;

fn page(url: &str, html: &str) -> PageData {
    page_bytes(url, None, html.as_bytes(), false)
}

fn page_bytes(url: &str, charset: Option<&str>, body: &[u8], truncated: bool) -> PageData {
    let url = Url::parse(url).unwrap();
    extract_page(&PageInput {
        url: &url,
        charset,
        body,
        truncated,
        robots_token: "SpiderLens",
    })
}

fn p(html: &str) -> PageData {
    page("https://example.com/a/b.html", html)
}

fn refs_of(d: &PageData, kind: LinkKind) -> Vec<&Ref> {
    d.refs.iter().filter(|r| r.kind == kind).collect()
}

// --- User Story 1 ---

#[test]
fn us1_1_title_and_length() {
    let d = p("<title>Red Shoes | Shop</title>");
    assert_eq!(d.title(), Some("Red Shoes | Shop"));
    assert_eq!(d.title().unwrap().chars().count(), 16);
    assert_eq!(d.titles.len(), 1);
}

#[test]
fn us1_2_two_titles_in_order() {
    let d = p("<head><title>One</title><title>Two</title></head>");
    let t: Vec<_> = d.titles.iter().map(|t| t.text.as_str()).collect();
    assert_eq!(t, ["One", "Two"]);
    assert_eq!(d.title(), Some("One"));
}

#[test]
fn us1_3_missing_versus_empty_description() {
    assert_eq!(p("<title>x</title>").meta_description(), None);
    let d = p(r#"<meta name="description" content="">"#);
    assert_eq!(d.meta_description(), Some(""));
    let d = p(r#"<meta name="Description" content="  Shoes &amp; boots  ">"#);
    assert_eq!(d.meta_description(), Some("Shoes & boots"));
}

#[test]
fn us1_4_headings_in_order_with_levels() {
    let d = p("<h1>Main</h1><h2>A</h2><h4>Deep</h4><h2>B</h2><h2>C <em>c</em></h2>");
    let h: Vec<_> = d.headings.iter().map(|h| (h.level, h.text.as_str())).collect();
    assert_eq!(h, [(1, "Main"), (2, "A"), (4, "Deep"), (2, "B"), (2, "C c")]);
}

#[test]
fn us1_6_word_count_excludes_scripts_styles_templates() {
    let words = vec!["word"; 250].join(" ");
    let html = format!(
        "<head><title>T T T</title><style>p {{ x: y }}</style></head><body><p>{words}</p>\
         <script>var a = 'not words here';</script><template><p>hidden words</p></template>\
         <!-- a comment with words --><noscript>no script words</noscript></body>"
    );
    assert_eq!(p(&html).word_count, 250);
}

#[test]
fn inline_elements_do_not_split_words_but_blocks_do() {
    assert_eq!(
        p("<p>Hel<b>lo</b></p><p>world</p><div>a<br>b</div>").word_count,
        4
    );
}

// --- User Story 2 ---

#[test]
fn us2_1_anchor_text_and_raw_href() {
    let d = p(r#"<a href="../c.html">Next  page</a>"#);
    let a = refs_of(&d, LinkKind::Anchor);
    assert_eq!(a[0].raw, "../c.html");
    assert_eq!(a[0].text.as_deref(), Some("Next page"));
    assert_eq!(
        d.base.join(&a[0].raw).unwrap().as_str(),
        "https://example.com/c.html"
    );
}

#[test]
fn us2_2_rel_values() {
    let d = p(r#"<a href="/x" rel="NoFollow sponsored">x</a><a href="/y" rel="ugc">y</a>"#);
    let a = refs_of(&d, LinkKind::Anchor);
    assert!(a[0].nofollow && a[0].sponsored && !a[0].ugc);
    assert_eq!(a[0].rel.as_deref(), Some("NoFollow sponsored"));
    assert!(!a[1].nofollow && a[1].ugc);
}

#[test]
fn us2_3_base_href() {
    let d = p(r#"<base href="https://cdn.example.com/x/"><base href="/ignored/"><a href="p.html">p</a>"#);
    assert_eq!(d.base.as_str(), "https://cdn.example.com/x/");
}

#[test]
fn us2_4_image_link() {
    let d = p(r#"<a href="/"><img src="/logo.png" alt="Home"></a>"#);
    let a = refs_of(&d, LinkKind::Anchor);
    assert_eq!(a[0].text.as_deref(), Some(""));
    assert_eq!(a[0].alt.as_deref(), Some("Home"));
    assert!(a[0].in_link);
    let i = refs_of(&d, LinkKind::Image);
    assert!(i[0].in_link);
}

#[test]
fn us2_7_page_nofollow_links_are_still_extracted() {
    let d = p(r#"<meta name="robots" content="nofollow"><a href="/a">a</a>"#);
    assert_eq!(refs_of(&d, LinkKind::Anchor).len(), 1);
    assert_eq!(directives::page_flags(&d.directives, "SpiderLens"), (false, true));
}

#[test]
fn duplicate_links_are_all_kept_and_areas_count() {
    let d = p(r#"<a href="/a">A</a><a href="/a">A</a><map><area href="/m" alt="Map"></map><a>no href</a>"#);
    let a = refs_of(&d, LinkKind::Anchor);
    assert_eq!(a.len(), 3);
    assert_eq!(a[2].text.as_deref(), Some("Map"));
}

#[test]
fn noscript_links_are_flagged() {
    let d =
        p(r#"<body><noscript><a href="/ns">x</a><img src="/px.gif"></noscript><a href="/y">y</a></body>"#);
    let flags: Vec<_> = d.refs.iter().map(|r| (r.raw.as_str(), r.noscript)).collect();
    assert_eq!(flags, [("/ns", true), ("/px.gif", true), ("/y", false)]);
}

// --- User Story 3 ---

#[test]
fn us3_1_meta_and_header_directives() {
    let d = p(r#"<meta name="robots" content="noindex, follow">"#);
    let h = header_facts(&["nosnippet".into()], &[]);
    let all: Vec<_> = d
        .directives
        .iter()
        .chain(&h.directives)
        .map(|x| (x.directive.as_str(), x.source, x.crawler.as_str()))
        .collect();
    assert_eq!(
        all,
        [
            ("noindex", DirectiveSource::Meta, "all"),
            ("follow", DirectiveSource::Meta, "all"),
            ("nosnippet", DirectiveSource::Header, "all"),
        ]
    );
}

#[test]
fn crawler_specific_meta() {
    let d =
        p(r#"<meta name="googlebot" content="noindex"><meta name="viewport" content="width=device-width">"#);
    assert_eq!(d.directives.len(), 1);
    assert_eq!(d.directives[0].crawler, "googlebot");
}

#[test]
fn us3_3_canonical_from_html_and_header() {
    let d = p(r#"<head><link rel="canonical" href="/p"></head>"#);
    let h = header_facts(&[], &[r#"<https://example.com/q>; rel="canonical""#.into()]);
    let c = refs_of(&d, LinkKind::Canonical);
    assert_eq!(
        (c[0].raw.as_str(), c[0].source, c[0].in_head),
        ("/p", RefSource::Html, Some(true))
    );
    assert_eq!(h.refs[0].source, RefSource::Header);
}

#[test]
fn us3_4_canonical_in_body() {
    let d = p(r#"<head></head><body><p>x</p><link rel="canonical" href="/p"></body>"#);
    assert_eq!(refs_of(&d, LinkKind::Canonical)[0].in_head, Some(false));
}

#[test]
fn us3_5_hreflang_and_next_prev() {
    let d = p(
        r#"<link rel="alternate" hreflang="en" href="/en"><link rel="alternate" hreflang="de" href="/de">
        <link rel="alternate" hreflang="x-default" href="/"><link rel="alternate" href="/feed" type="application/rss+xml">
        <link rel="next" href="/2"><link rel="prev" href="/0">"#,
    );
    let h: Vec<_> = refs_of(&d, LinkKind::Hreflang)
        .iter()
        .map(|r| (r.lang.as_deref().unwrap(), r.raw.as_str()))
        .collect();
    assert_eq!(h, [("en", "/en"), ("de", "/de"), ("x-default", "/")]);
    assert_eq!(refs_of(&d, LinkKind::Next)[0].raw, "/2");
    assert_eq!(refs_of(&d, LinkKind::Prev)[0].raw, "/0");
}

#[test]
fn meta_refresh_is_a_reference() {
    let d = p(r#"<meta http-equiv="refresh" content="0;url=/new">"#);
    let r = d.meta_refresh.as_ref().unwrap();
    assert_eq!((r.delay, r.url.as_deref()), (0.0, Some("/new")));
    assert_eq!(refs_of(&d, LinkKind::MetaRefresh)[0].raw, "/new");
}

// --- User Story 4 ---

#[test]
fn us4_1_no_head_no_body_unclosed_title() {
    let d = p("<title>Broken page<p>after</p>");
    // Browsers read everything up to the end of the document as the title.
    assert_eq!(d.title(), Some("Broken page<p>after</p>"));
    assert!(d.titles[0].in_head);
}

#[test]
fn us4_2_description_after_head_was_closed() {
    let d = p(r#"<head><title>t</title>stray text<meta name="description" content="Late"></head>"#);
    assert_eq!(d.meta_description(), Some("Late"));
    assert!(!d.meta_descriptions[0].in_head);
    assert!(d.titles[0].in_head);
}

#[test]
fn us4_3_windows_1252_from_meta() {
    let (bytes, _, _) =
        encoding_rs::WINDOWS_1252.encode(r#"<meta charset="windows-1252"><title>Café crème</title>"#);
    let d = page_bytes("https://e.test/", None, &bytes, false);
    assert_eq!(d.title(), Some("Café crème"));
    assert_eq!(d.encoding, "windows-1252");
    assert!(!d.encoding_errors);
}

#[test]
fn us4_4_invalid_bytes_are_replaced_and_flagged() {
    let d = page_bytes(
        "https://e.test/",
        Some("utf-8"),
        b"<title>a\xFF\xFEb</title>",
        false,
    );
    assert_eq!(d.title(), Some("a\u{FFFD}\u{FFFD}b"));
    assert!(d.encoding_errors);
}

#[test]
fn us4_5_truncated_body_is_extracted() {
    let d = page_bytes("https://e.test/", None, b"<title>Cut</title><h1>Head", true);
    assert!(d.partial);
    assert_eq!(d.title(), Some("Cut"));
    assert_eq!(d.headings[0].text, "Head");
}

#[test]
fn binary_or_empty_html_is_no_content() {
    assert!(page_bytes("https://e.test/", None, b"", false).no_content);
    assert!(page_bytes("https://e.test/", None, b"\x89PNG\r\n\x1a\n\0\0\0", false).no_content);
    assert!(!p("<p>x</p>").no_content);
}

#[test]
fn deep_nesting_does_not_overflow() {
    let html = "<div>".repeat(5_000) + "deep";
    assert_eq!(p(&html).word_count, 1);
}

#[test]
fn parse_time_budget_cuts_pathological_pages() {
    let html = "<title>t</title>".to_string() + &"<div>".repeat(100_000) + "end";
    let url = Url::parse("https://e.test/").unwrap();
    let started = std::time::Instant::now();
    let d = walk::walk(&html, &url, "x", std::time::Duration::from_millis(200));
    assert!(started.elapsed() < std::time::Duration::from_secs(20));
    assert!(d.partial);
    assert_eq!(d.title(), Some("t"));
}

// --- User Story 5 ---

#[test]
fn us5_1_alt_states() {
    let d = p(
        r#"<img src="/a.jpg" alt=""><img src="/b.jpg"><img src="/c.jpg" alt=" Cat " width="300" height="20px">"#,
    );
    let i = refs_of(&d, LinkKind::Image);
    assert_eq!(i[0].alt.as_deref(), Some(""));
    assert_eq!(i[1].alt, None);
    assert_eq!(i[2].alt.as_deref(), Some("Cat"));
    assert_eq!((i[2].width, i[2].height), (Some(300), Some(20)));
}

#[test]
fn us5_2_srcset_candidates() {
    let d = p(r#"<img src="/s.jpg" srcset="/s.jpg 1x, /m.jpg 2x, /l.jpg 3x" alt="x">"#);
    let i: Vec<_> = refs_of(&d, LinkKind::Image)
        .iter()
        .map(|r| (r.raw.as_str(), r.srcset))
        .collect();
    assert_eq!(
        i,
        [
            ("/s.jpg", false),
            ("/s.jpg", true),
            ("/m.jpg", true),
            ("/l.jpg", true)
        ]
    );
}

#[test]
fn us5_3_json_ld_blocks() {
    let d = p(r#"<script type="application/ld+json">{"@type":"Product"}</script>
                 <script type="Application/LD+JSON">{"broken": </script>"#);
    let s: Vec<_> = d.structured.iter().map(|b| (b.kind, b.parseable)).collect();
    assert_eq!(
        s,
        [(StructuredKind::JsonLd, true), (StructuredKind::JsonLd, false)]
    );
    assert_eq!(d.structured[0].raw, r#"{"@type":"Product"}"#);
}

#[test]
fn us5_4_microdata_product() {
    let d = p(
        r##"<div itemscope itemtype="https://schema.org/Product" itemid="#p1">
        <span itemprop="name">Red Shoes</span>
        <div itemprop="offers" itemscope itemtype="https://schema.org/Offer">
          <meta itemprop="price" content="49.90"><a itemprop="url" href="/buy">buy</a>
        </div></div>"##,
    );
    assert_eq!(d.structured.len(), 1);
    let v: serde_json::Value = serde_json::from_str(&d.structured[0].raw).unwrap();
    assert_eq!(v["type"][0], "https://schema.org/Product");
    assert_eq!(v["id"], "#p1");
    assert_eq!(v["properties"]["name"][0], "Red Shoes");
    let offer = &v["properties"]["offers"][0];
    assert_eq!(offer["properties"]["price"][0], "49.90");
    assert_eq!(offer["properties"]["url"][0], "/buy");
}

// --- Other edge cases ---

#[test]
fn language_and_text_fingerprint() {
    let a = p(r#"<html lang="he"><body><p>Same   text</p></body></html>"#);
    let b = p(r#"<html><body><div>Same text</div></body></html>"#);
    assert_eq!(a.lang.as_deref(), Some("he"));
    assert_eq!(a.text_hash, b.text_hash);
    assert_ne!(a.text_hash, p("<p>Other</p>").text_hash);
}

#[test]
fn character_references_are_decoded() {
    let d = p("<h1>Tom &amp; Jerry &#233;</h1>");
    assert_eq!(d.headings[0].text, "Tom & Jerry é");
}

#[test]
fn svg_title_and_links_are_not_html() {
    let d = p(r#"<body><svg><title>Icon</title><a href="/svg">x</a></svg></body>"#);
    assert!(d.titles.is_empty());
    assert!(d.refs.is_empty());
}

#[test]
fn extraction_is_deterministic() {
    let html = r#"<title>x</title><a href="/a">a</a><img src="/i"><h2>h</h2>"#;
    assert_eq!(p(html), p(html));
}
