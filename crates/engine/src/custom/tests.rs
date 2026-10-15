//! Spec 013 acceptance scenarios on single pages (SC-004).

use spiderlens_config::{
    CustomRules, ElementKind, ElementLimit, ExtractMode, Extractor, ExtractorKind, Search, SearchCondition,
    SearchTarget,
};
use url::Url;

use super::*;
use crate::extract::{PageInput, extract_page_with};

fn ex(kind: ExtractorKind, expr: &str, mode: ExtractMode) -> Extractor {
    Extractor {
        name: format!("{kind:?} {expr}"),
        kind,
        expr: expr.into(),
        mode,
        ..Default::default()
    }
}

fn search(term: &str) -> Search {
    Search {
        name: term.into(),
        term: term.into(),
        ..Default::default()
    }
}

fn run(html: &str, rules: CustomRules) -> PageRules {
    let compiled = compile(&rules).unwrap().unwrap();
    let url = Url::parse("https://example.com/p").unwrap();
    let d = extract_page_with(
        &PageInput {
            url: &url,
            charset: Some("utf-8"),
            body: html.as_bytes(),
            truncated: false,
            robots_token: "spiderlens",
            content: None,
            a11y: false,
            hook: None,
            keep_text: false,
        },
        Some(&compiled),
    );
    d.custom.expect("rules ran")
}

fn one(html: &str, e: Extractor) -> RuleOutcome {
    run(
        html,
        CustomRules {
            extractors: vec![e],
            searches: vec![],
        },
    )
    .extractors
    .remove(0)
}

fn one_search(html: &str, s: Search) -> RuleOutcome {
    run(
        html,
        CustomRules {
            extractors: vec![],
            searches: vec![s],
        },
    )
    .searches
    .remove(0)
}

const SHOP: &str = r#"<!doctype html><html><head><title>Shoe</title>
<meta itemprop="sku" content="AB-123">
<script src="https://www.googletagmanager.com/gtm.js?id=GTM-K9X2"></script>
<script type="application/ld+json">{"@context":"https://schema.org","@type":"Product","name":"Shoe",
 "offers":{"@type":"Offer","price":49},"aggregateRating":{"reviewCount":12}}</script>
</head><body>
<nav><a class="breadcrumb" href="/">Home</a> <a class="breadcrumb" href="/shoes">Shoes</a> <a class="breadcrumb" href="/shoes/red">Red</a></nav>
<main><h1>Red shoe</h1><span class="product-price"> $49.00 </span>
<div class="specs"><b>Size</b> 42</div>
<div class="review">a</div><div class="review">b</div><div class="review">c</div><div class="review">d</div><div class="review">e</div>
<p>Free shipping on all orders.</p></main>
<footer>Free shipping · Model X100 is no longer sold</footer>
</body></html>"#;

#[test]
fn us1_1_css_text_is_trimmed() {
    let o = one(SHOP, ex(ExtractorKind::Css, ".product-price", ExtractMode::Text));
    assert_eq!(o.values, vec!["$49.00"]);
    assert_eq!(o.count, 1);
    assert_eq!(o.flags, flags::RUN);
}

#[test]
fn us1_2_xpath_attribute() {
    let o = one(
        SHOP,
        ex(
            ExtractorKind::Xpath,
            "//meta[@itemprop=\"sku\"]/@content",
            ExtractMode::Text,
        ),
    );
    assert_eq!(o.values, vec!["AB-123"]);
}

#[test]
fn us1_3_css_attribute_all_matches_in_order() {
    let o = one(
        SHOP,
        Extractor {
            attr: Some("href".into()),
            ..ex(ExtractorKind::Css, "a.breadcrumb", ExtractMode::Attribute)
        },
    );
    assert_eq!(o.values, vec!["/", "/shoes", "/shoes/red"]);
    assert_eq!(o.count, 3);
}

#[test]
fn us1_4_and_5_inner_and_outer_html() {
    let inner = one(SHOP, ex(ExtractorKind::Css, ".specs", ExtractMode::InnerHtml));
    assert_eq!(inner.values, vec!["<b>Size</b> 42"]);
    let outer = one(SHOP, ex(ExtractorKind::Css, ".specs", ExtractMode::OuterHtml));
    assert_eq!(outer.values, vec![r#"<div class="specs"><b>Size</b> 42</div>"#]);
    let x = one(
        SHOP,
        ex(
            ExtractorKind::Xpath,
            "//div[@class='specs']",
            ExtractMode::InnerHtml,
        ),
    );
    assert_eq!(x.values, inner.values);
}

#[test]
fn us1_6_regex_first_group() {
    let o = one(
        SHOP,
        ex(
            ExtractorKind::Regex,
            r"gtm\.js\?id=(GTM-[A-Z0-9]+)",
            ExtractMode::Value,
        ),
    );
    assert_eq!(o.values, vec!["GTM-K9X2"]);
    let whole = one(
        SHOP,
        ex(ExtractorKind::Regex, r"GTM-[A-Z0-9]+", ExtractMode::Value),
    );
    assert_eq!(whole.values, vec!["GTM-K9X2"]);
}

#[test]
fn us1_7_json_ld_item_path() {
    let o = one(
        SHOP,
        Extractor {
            json_type: Some("Product".into()),
            ..ex(ExtractorKind::JsonLd, "offers.price", ExtractMode::Value)
        },
    );
    assert_eq!(o.values, vec!["49"]);
}

#[test]
fn us1_8_xpath_function_value() {
    let o = one(
        SHOP,
        ex(
            ExtractorKind::Xpath,
            "count(//div[@class=\"review\"])",
            ExtractMode::Value,
        ),
    );
    assert_eq!(o.values, vec!["5"]);
}

#[test]
fn us1_9_no_match_differs_from_empty_value() {
    let none = one(SHOP, ex(ExtractorKind::Css, ".nothing", ExtractMode::Text));
    assert_eq!(none.flags, flags::RUN | flags::NO_MATCH);
    assert!(none.values.is_empty());
    let empty = one("<p class=e></p>", ex(ExtractorKind::Css, ".e", ExtractMode::Text));
    assert_eq!(empty.flags, flags::RUN);
    assert_eq!(empty.values, vec![""]);
}

#[test]
fn us2_1_does_not_contain() {
    let mut s = search("consent-v2.js");
    s.condition = SearchCondition::NotContains;
    let o = one_search(SHOP, s.clone());
    assert_eq!(o.count, 0);
    assert_ne!(o.flags & flags::MEETS, 0);
    let o = one_search("<script src=/consent-v2.js></script>", s);
    assert_eq!(o.count, 1);
    assert_eq!(o.flags & flags::MEETS, 0);
}

#[test]
fn us2_2_ignore_case_in_text() {
    let mut s = search("Model X100");
    s.target = SearchTarget::Text;
    s.ignore_case = true;
    let o = one_search("<body><p>model x100 is no longer sold</p></body>", s);
    assert_eq!(o.count, 1);
    assert_ne!(o.flags & flags::MEETS, 0);
}

#[test]
fn us2_3_text_ignores_scripts_styles_comments_attributes() {
    let html = r#"<body><script>var x="needle"</script><style>.needle{}</style><!-- needle -->
        <a title="needle">link</a></body>"#;
    let mut text = search("needle");
    text.target = SearchTarget::Text;
    assert_eq!(one_search(html, text).count, 0);
    assert_eq!(one_search(html, search("needle")).count, 4);
}

#[test]
fn us2_4_regex_counts_every_match() {
    let s = Search {
        regex: true,
        target: SearchTarget::Text,
        ..search(r"\b(?:lorem|ipsum)\b")
    };
    assert_eq!(one_search("<body><p>lorem ipsum, lorem.</p></body>", s).count, 3);
}

#[test]
fn us2_5_contexts_are_kept() {
    let mut s = search("shipping");
    s.target = SearchTarget::Text;
    let o = one_search(SHOP, s);
    assert_eq!(o.count, 2);
    assert_eq!(o.values.len(), 2);
    assert!(
        o.values[0].contains("Free shipping on all orders"),
        "{:?}",
        o.values
    );
}

#[test]
fn us6_element_limit() {
    let mut s = search("free shipping");
    s.target = SearchTarget::Text;
    s.ignore_case = true;
    s.within = Some(ElementLimit {
        kind: ElementKind::Css,
        expr: "main".into(),
    });
    assert_eq!(one_search(SHOP, s.clone()).count, 1);
    s.within = Some(ElementLimit {
        kind: ElementKind::Xpath,
        expr: "//main".into(),
    });
    assert_eq!(one_search(SHOP, s.clone()).count, 1);
    s.within = Some(ElementLimit {
        kind: ElementKind::Css,
        expr: "article".into(),
    });
    let o = one_search(SHOP, s);
    assert_eq!(o.count, 0);
    assert_ne!(o.flags & flags::ELEMENT_NOT_FOUND, 0);
}

#[test]
fn nested_element_limits_count_once() {
    let mut s = search("x");
    s.target = SearchTarget::Text;
    s.within = Some(ElementLimit {
        kind: ElementKind::Css,
        expr: "div".into(),
    });
    assert_eq!(one_search("<div><div>x</div></div>", s).count, 1);
}

#[test]
fn values_are_capped_and_counted() {
    let html: String = (0..150).map(|i| format!("<i>{i}</i>")).collect();
    let o = one(&html, ex(ExtractorKind::Css, "i", ExtractMode::Text));
    assert_eq!(o.count, 150);
    assert_eq!(o.values.len(), MAX_VALUES);
    assert_ne!(o.flags & flags::MORE_THAN_STORED, 0);
    let long = format!("<p>{}</p>", "a".repeat(MAX_VALUE_CHARS + 10));
    let o = one(&long, ex(ExtractorKind::Css, "p", ExtractMode::Text));
    assert_eq!(o.values[0].chars().count(), MAX_VALUE_CHARS);
    assert_ne!(o.flags & flags::TRUNCATED, 0);
}

#[test]
fn bad_json_ld_is_flagged() {
    let html = r#"<script type="application/ld+json">{oops</script>"#;
    let o = one(html, ex(ExtractorKind::JsonLd, "name", ExtractMode::Value));
    assert_ne!(o.flags & flags::BAD_JSON, 0);
    assert_ne!(o.flags & flags::NO_MATCH, 0);
}

#[test]
fn budget_marks_the_rest_timed_out() {
    let rules = compile(&CustomRules {
        extractors: vec![ex(ExtractorKind::Css, "p", ExtractMode::Text)],
        searches: vec![search("a")],
    })
    .unwrap()
    .unwrap();
    let doc = Html::parse_document("<p>a</p>");
    let view = DocView {
        doc: &doc,
        source: "<p>a</p>",
        visible: "a",
        structured: &[],
    };
    let out = apply_within(&rules, &view, Duration::ZERO);
    assert_ne!(out.searches[0].flags & flags::TIMED_OUT, 0);
}

#[test]
fn no_rules_compile_to_nothing() {
    assert!(compile(&CustomRules::default()).unwrap().is_none());
}

#[test]
fn empty_html_page_meets_not_contains() {
    let rules = CustomRules {
        extractors: vec![ex(ExtractorKind::Css, "p", ExtractMode::Text)],
        searches: vec![Search {
            condition: SearchCondition::NotContains,
            ..search("x")
        }],
    };
    let out = run("   ", rules);
    assert_ne!(out.extractors[0].flags & flags::NO_MATCH, 0);
    assert_ne!(out.searches[0].flags & flags::MEETS, 0);
}

#[test]
fn deterministic() {
    let rules = CustomRules {
        extractors: vec![ex(ExtractorKind::Xpath, "//a/@href", ExtractMode::Text)],
        searches: vec![search("shoe")],
    };
    assert_eq!(run(SHOP, rules.clone()), run(SHOP, rules));
}

/// Cost of each rule kind on a bench-site page (plan T032). Run with
/// `BENCH_PAGE=page.html cargo test --release -p spiderlens-engine
/// custom::tests::cost_by_kind -- --ignored --nocapture`.
#[test]
#[ignore]
fn cost_by_kind() {
    let Ok(page) = std::env::var("BENCH_PAGE") else {
        return;
    };
    let html = std::fs::read(page).unwrap();
    let all: CustomRules = spiderlens_config::import_rules(include_str!(
        "../../../../specs/013-custom-extraction-search/bench-rules.json"
    ))
    .unwrap();
    let url = Url::parse("http://127.0.0.1:8700/p/5").unwrap();
    let input = PageInput {
        url: &url,
        charset: Some("utf-8"),
        body: &html,
        truncated: false,
        robots_token: "spiderlens",
        content: None,
        a11y: false,
        hook: None,
        keep_text: false,
    };
    let time = |rules: Option<&CompiledRules>| {
        let n = 3_000;
        let t = Instant::now();
        for _ in 0..n {
            std::hint::black_box(extract_page_with(&input, rules));
        }
        t.elapsed().as_secs_f64() * 1e6 / n as f64
    };
    let base = time(None);
    println!("no rules: {base:.1} us/page");
    type Keep = fn(&Extractor) -> bool;
    let kinds: [(&str, Keep, bool); 6] = [
        ("css", |e| e.kind == ExtractorKind::Css, false),
        ("xpath", |e| e.kind == ExtractorKind::Xpath, false),
        ("regex", |e| e.kind == ExtractorKind::Regex, false),
        ("json-ld", |e| e.kind == ExtractorKind::JsonLd, false),
        ("searches", |_| false, true),
        ("all", |_| true, true),
    ];
    for (name, keep, searches) in kinds {
        let set = CustomRules {
            extractors: all.extractors.iter().filter(|e| keep(e)).cloned().collect(),
            searches: if searches { all.searches.clone() } else { vec![] },
        };
        let c = compile(&set).unwrap().unwrap();
        println!("{name}: +{:.1} us/page", time(Some(&c)) - base);
    }
}
