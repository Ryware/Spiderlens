//! Spec 013 against a fixture site: rules during a crawl, reruns from
//! stored HTML, and the two giving the same values (US1, US2, US3, FR-015).

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use spiderlens_config::{
    CrawlConfig, CustomRules, ExtractMode, Extractor, ExtractorKind, ModeInput, Search, SearchCondition,
    SearchTarget, SpeedSettings,
};
use spiderlens_engine::crawl::store::custom::PageCustom;
use spiderlens_engine::crawl::{Control, CrawlStore, Crawler};
use spiderlens_engine::custom::flags;
use spiderlens_engine::custom::rerun::{RerunError, RerunState, Which, rerun};
use spiderlens_test_fixtures::{Site, SiteBuilder};
use tokio::sync::watch;

const E: &str = "shop.test";

fn rules() -> CustomRules {
    CustomRules {
        extractors: vec![
            Extractor {
                name: "Price".into(),
                kind: ExtractorKind::Css,
                expr: ".product-price".into(),
                ..Default::default()
            },
            Extractor {
                name: "SKU".into(),
                kind: ExtractorKind::Xpath,
                expr: "//meta[@itemprop='sku']/@content".into(),
                ..Default::default()
            },
            Extractor {
                name: "Reviews".into(),
                kind: ExtractorKind::JsonLd,
                expr: "aggregateRating.reviewCount".into(),
                mode: ExtractMode::Value,
                json_type: Some("Product".into()),
                ..Default::default()
            },
        ],
        searches: vec![
            Search {
                name: "Consent script".into(),
                term: "consent-v2.js".into(),
                condition: SearchCondition::NotContains,
                ..Default::default()
            },
            Search {
                name: "Old product".into(),
                term: "model x100".into(),
                target: SearchTarget::Text,
                ignore_case: true,
                ..Default::default()
            },
        ],
    }
}

/// 20 product pages; pages 13-19 lack the consent script; every third
/// page mentions the old product; one external page.
fn site() -> Site {
    let links: Vec<String> = (0..20).map(|i| format!("/p/{i}")).collect();
    let mut links: Vec<&str> = links.iter().map(String::as_str).collect();
    links.push("{base:other.test}/ext");
    let mut b = SiteBuilder::new().page(E, "/", "", &links);
    for i in 0..20 {
        let consent = if i < 13 {
            r#"<script src="/consent-v2.js"></script>"#
        } else {
            ""
        };
        let old = if i % 3 == 0 {
            "<p>The Model X100 is no longer sold.</p>"
        } else {
            ""
        };
        let html = format!(
            r#"<!doctype html><html><head><title>Product {i}</title>
            <meta itemprop="sku" content="SKU-{i}">{consent}
            <script type="application/ld+json">{{"@type":"Product","aggregateRating":{{"reviewCount":{i}}}}}</script>
            </head><body><h1>Product {i}</h1><span class="product-price">${i}.00</span>{old}</body></html>"#
        );
        b = b.html(E, &format!("/p/{i}"), &html);
    }
    b.html("other.test", "/ext", r#"<span class="product-price">$1</span>"#)
        .start()
}

async fn crawl(site: &Site, edit: impl FnOnce(&mut CrawlConfig)) -> (tempfile::TempDir, PathBuf) {
    let mut c = CrawlConfig {
        speed: SpeedSettings::FAST,
        ..CrawlConfig::default()
    };
    edit(&mut c);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.spiderlens");
    let input = ModeInput::Spider {
        start_url: site.url(E, "/"),
    };
    let crawler = Crawler::create(&path, c, &input)
        .unwrap()
        .with_resolve(site.resolve())
        .with_retry_base(Duration::from_millis(10));
    let (_tx, rx) = watch::channel(Control::Run);
    tokio::time::timeout(Duration::from_secs(60), crawler.run(rx, None))
        .await
        .expect("crawl finishes")
        .unwrap();
    (dir, path)
}

fn custom_of(path: &Path, url: &str) -> Option<PageCustom> {
    let store = CrawlStore::open(path).unwrap();
    let id = store.id_of(url).unwrap()?;
    store.page_custom(id).unwrap()
}

fn slot_of(path: &Path, name: &str) -> u32 {
    let store = CrawlStore::open(path).unwrap();
    store
        .custom_rules()
        .unwrap()
        .into_iter()
        .find(|r| r.name == name)
        .unwrap()
        .slot
}

#[tokio::test]
async fn us1_us2_values_during_the_crawl() {
    let site = site();
    let (_d, path) = crawl(&site, |c| c.custom = rules()).await;
    let price = slot_of(&path, "Price");
    let sku = slot_of(&path, "SKU");
    let reviews = slot_of(&path, "Reviews");
    let consent = slot_of(&path, "Consent script");
    let old = slot_of(&path, "Old product");

    let p5 = custom_of(&path, &site.url(E, "/p/5")).expect("rules ran on p/5");
    assert_eq!(p5.slots[price as usize].2.as_deref(), Some("$5.00"));
    assert_eq!(p5.slots[sku as usize].2.as_deref(), Some("SKU-5"));
    assert_eq!(p5.slots[reviews as usize].2.as_deref(), Some("5"));

    let mut missing_consent = 0;
    let mut old_mentions = 0;
    for i in 0..20 {
        let p = custom_of(&path, &site.url(E, &format!("/p/{i}"))).unwrap();
        if p.slots[consent as usize].1 & flags::MEETS != 0 {
            missing_consent += 1;
        }
        if p.slots[old as usize].1 & flags::MEETS != 0 {
            old_mentions += 1;
            assert_eq!(p.all_values(old).len(), 1, "context kept");
        }
    }
    assert_eq!(missing_consent, 7, "US2.1");
    assert_eq!(old_mentions, 7);

    // The home page has no price: no match, distinct from a value.
    let home = custom_of(&path, &site.url(E, "/")).unwrap();
    assert_ne!(home.slots[price as usize].1 & flags::NO_MATCH, 0);
    // External pages get no rule results.
    assert!(custom_of(&path, &site.url("other.test", "/ext")).is_none());
}

#[tokio::test]
async fn us3_1_rerun_equals_crawl_and_sends_no_requests() {
    let site = site();
    let (_a, with_rules) = crawl(&site, |c| c.custom = rules()).await;
    let (_b, plain) = crawl(&site, |_| {}).await;
    assert!(
        !CrawlStore::open(&plain).unwrap().has_custom_tables().unwrap(),
        "FR-027"
    );

    let hits_before = site.hits().len();
    let cancel = AtomicBool::new(false);
    let mut seen = Vec::new();
    let report = rerun(&plain, Which::These(&rules()), &[], &cancel, |p| seen.push(p)).unwrap();
    assert_eq!(report.state, RerunState::Complete);
    assert_eq!(report.pages, 21);
    assert_eq!(site.hits().len(), hits_before, "a rerun sends no requests");
    assert_eq!(seen.last().unwrap().done, 21);

    for path in ["/", "/p/0", "/p/7", "/p/14", "/p/19"] {
        let a = custom_of(&with_rules, &site.url(E, path)).unwrap();
        let b = custom_of(&plain, &site.url(E, path)).unwrap();
        assert_eq!(a, b, "{path}: crawl and rerun agree (FR-015)");
    }
}

#[tokio::test]
async fn us3_2_changed_rule_replaces_only_its_values() {
    let site = site();
    let (_d, path) = crawl(&site, |c| c.custom = rules()).await;
    let mut changed = rules();
    changed.extractors[0].expr = "h1".into();
    let cancel = AtomicBool::new(false);
    rerun(&path, Which::These(&changed), &["Price".into()], &cancel, |_| {}).unwrap();
    let p3 = custom_of(&path, &site.url(E, "/p/3")).unwrap();
    assert_eq!(
        p3.slots[slot_of(&path, "Price") as usize].2.as_deref(),
        Some("Product 3")
    );
    assert_eq!(
        p3.slots[slot_of(&path, "SKU") as usize].2.as_deref(),
        Some("SKU-3")
    );
    let store = CrawlStore::open(&path).unwrap();
    let price = store
        .custom_rules()
        .unwrap()
        .into_iter()
        .find(|r| r.name == "Price")
        .unwrap();
    assert_eq!(price.run_state.as_deref(), Some("complete"));
    assert_eq!(price.run_pages, Some(21));
}

#[tokio::test]
async fn us3_3_cancel_records_a_partial_run() {
    let site = site();
    let (_d, path) = crawl(&site, |_| {}).await;
    let cancel = AtomicBool::new(true);
    let report = rerun(&path, Which::These(&rules()), &[], &cancel, |_| {}).unwrap();
    assert_eq!(report.state, RerunState::Partial);
    let store = CrawlStore::open(&path).unwrap();
    assert!(
        store
            .custom_rules()
            .unwrap()
            .iter()
            .all(|r| r.run_state.as_deref() == Some("partial"))
    );
}

#[tokio::test]
async fn us3_4_no_stored_html_is_not_possible() {
    let site = site();
    let (_d, path) = crawl(&site, |c| c.extraction.store_html = false).await;
    let cancel = AtomicBool::new(false);
    let err = rerun(&path, Which::These(&rules()), &[], &cancel, |_| {}).unwrap_err();
    assert!(matches!(err, RerunError::NotPossible(m) if m.contains("no stored page HTML")));
}

#[tokio::test]
async fn invalid_rules_refuse_to_start() {
    let site = site();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.spiderlens");
    let mut c = CrawlConfig::default();
    c.custom.extractors.push(Extractor {
        name: "Bad".into(),
        kind: ExtractorKind::Regex,
        expr: "(".into(),
        ..Default::default()
    });
    let input = ModeInput::Spider {
        start_url: site.url(E, "/"),
    };
    let crawler = Crawler::create(&path, c, &input)
        .unwrap()
        .with_resolve(site.resolve());
    let (_tx, rx) = watch::channel(Control::Run);
    let err = crawler.run(rx, None).await.unwrap_err();
    assert!(err.to_string().contains("custom rules"), "{err}");
}
