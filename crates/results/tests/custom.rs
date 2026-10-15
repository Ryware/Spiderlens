//! Spec 013 in the results: the Custom extraction and Custom search tabs,
//! their filters and counts, numeric sorting, the detail pane's results and
//! export with every value (FR-022 to FR-025, T022 to T025).

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use spiderlens_config::{
    CrawlConfig, CustomRules, ExtractMode, Extractor, ExtractorKind, ModeInput, Search, SearchCondition,
    SpeedSettings,
};
use spiderlens_engine::crawl::{Control, Crawler};
use spiderlens_results::{Reader, Sort, UrlScope, ViewQuery, tabs, write_csv};
use spiderlens_test_fixtures::{Site, SiteBuilder};
use tokio::sync::watch;

const E: &str = "shop.test";

/// Prices 9.5, 10, 100 and none; two pages lack the consent script; one
/// has three tags.
fn site() -> Site {
    let pages = [
        ("/a", Some("10"), true, 1),
        ("/b", Some("9.5"), false, 0),
        ("/c", Some("100"), true, 3),
        ("/d", None, false, 0),
    ];
    let links: Vec<&str> = pages.iter().map(|p| p.0).collect();
    let mut b = SiteBuilder::new().page(E, "/", "", &links);
    for (path, price, consent, tags) in pages {
        let price = price.map_or(String::new(), |p| format!(r#"<span class="price">{p}</span>"#));
        let consent = if consent {
            r#"<script src="/consent-v2.js"></script>"#
        } else {
            ""
        };
        let tags: String = (0..tags).map(|i| format!("<a class=tag>t{i}</a>")).collect();
        b = b.html(
            E,
            path,
            &format!("<!doctype html><html><head><title>{path}</title>{consent}</head><body>{price}{tags}</body></html>"),
        );
    }
    b.start()
}

fn rules() -> CustomRules {
    CustomRules {
        extractors: vec![
            Extractor {
                name: "Price".into(),
                kind: ExtractorKind::Css,
                expr: ".price".into(),
                ..Default::default()
            },
            Extractor {
                name: "Tags".into(),
                kind: ExtractorKind::Css,
                expr: "a.tag".into(),
                mode: ExtractMode::Text,
                ..Default::default()
            },
        ],
        searches: vec![Search {
            name: "Consent".into(),
            term: "consent-v2.js".into(),
            condition: SearchCondition::NotContains,
            ..Default::default()
        }],
    }
}

struct Crawled {
    site: Site,
    _dir: tempfile::TempDir,
    path: PathBuf,
}

async fn crawl(custom: CustomRules) -> Crawled {
    let site = site();
    let c = CrawlConfig {
        speed: SpeedSettings::FAST,
        custom,
        ..CrawlConfig::default()
    };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("custom.spiderlens");
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
    Crawled {
        site,
        _dir: dir,
        path,
    }
}

fn q(tab: &str, filter: &str) -> ViewQuery {
    ViewQuery {
        tab: tab.into(),
        filter: filter.into(),
        scope: UrlScope::All,
        search: String::new(),
        sort: None,
        segment: None,
    }
}

/// (path, values after the id) of every row, in view order.
fn rows(r: &mut Reader, c: &Crawled, query: &ViewQuery) -> Vec<(String, Vec<serde_json::Value>)> {
    let page = r.open_view(query, 500).unwrap();
    r.materialize(page.view).unwrap();
    let rows = r.rows(page.view, 0, 1000).unwrap().rows;
    r.close_view(page.view).unwrap();
    let base = c.site.url(E, "/");
    let base = base.trim_end_matches('/');
    rows.into_iter()
        .map(|mut row| {
            row.remove(0);
            let url = row[0].as_str().unwrap().to_string();
            (url.strip_prefix(base).unwrap_or(&url).to_string(), row)
        })
        .collect()
}

fn paths(r: &mut Reader, c: &Crawled, query: &ViewQuery) -> Vec<String> {
    rows(r, c, query).into_iter().map(|(p, _)| p).collect()
}

#[tokio::test]
async fn custom_tabs_list_values_and_filter() {
    let c = crawl(rules()).await;
    let mut r = Reader::open(&c.path).unwrap();
    let all = r.tabs().unwrap();
    assert_eq!(
        all.len(),
        tabs().len() + 2,
        "two custom tabs after the fixed ones"
    );
    let ex = all.iter().find(|t| t.id == "custom_extraction").unwrap();
    let labels: Vec<&str> = ex.columns.iter().map(|c| c.label.as_ref()).collect();
    assert_eq!(
        labels,
        ["Address", "Price", "Price: matches", "Tags", "Tags: matches"]
    );
    let price = ex
        .columns
        .iter()
        .find(|c| c.label == "Price")
        .unwrap()
        .id
        .to_string();
    let tags_count = ex
        .columns
        .iter()
        .find(|c| c.label == "Tags: matches")
        .unwrap()
        .id
        .to_string();

    // FR-022: first value and the true count.
    let listed = rows(&mut r, &c, &q("custom_extraction", "all"));
    assert_eq!(listed.len(), 5, "home and four pages");
    let pi = ex.columns.iter().position(|col| col.id == price).unwrap();
    let ti = ex.columns.iter().position(|col| col.id == tags_count).unwrap();
    let c_row = &listed.iter().find(|(p, _)| p == "/c").unwrap().1;
    assert_eq!(c_row[pi], "100");
    assert_eq!(c_row[ti], 3);

    // Filters and their counts agree.
    let has = format!("{price}_has");
    let none = format!("{price}_none");
    let mut with_price = paths(&mut r, &c, &q("custom_extraction", &has));
    with_price.sort();
    assert_eq!(with_price, ["/a", "/b", "/c"]);
    let mut no_price = paths(&mut r, &c, &q("custom_extraction", &none));
    no_price.sort();
    assert_eq!(no_price, ["/", "/d"]);
    for tab in ["custom_extraction", "custom_search"] {
        let counts = r.counts(tab, UrlScope::All).unwrap();
        let t = all.iter().find(|t| t.id == tab).unwrap();
        for f in &t.filters {
            assert_eq!(
                counts.counts[f.id.as_ref()],
                Some(paths(&mut r, &c, &q(tab, &f.id)).len() as u64),
                "{tab}/{}",
                f.id
            );
        }
    }

    // FR-023: "does not contain" lists the pages without the script.
    let search = all.iter().find(|t| t.id == "custom_search").unwrap();
    let meets = &search.filters[1];
    assert_eq!(meets.label, "Consent: does not contain");
    let mut missing = paths(&mut r, &c, &q("custom_search", &meets.id));
    missing.sort();
    assert_eq!(missing, ["/", "/b", "/d"]);

    // Numbers sort numerically: 9.5 < 10 < 100, empty last.
    let mut sorted = q("custom_extraction", "all");
    sorted.sort = Some(Sort {
        column: price.clone(),
        desc: false,
    });
    assert_eq!(paths(&mut r, &c, &sorted), ["/b", "/a", "/c", "/", "/d"]);
}

#[tokio::test]
async fn detail_lists_every_value_and_export_joins_them() {
    let c = crawl(rules()).await;
    let mut r = Reader::open(&c.path).unwrap();
    let page = r.open_view(&q("custom_extraction", "all"), 500).unwrap();
    let want = c.site.url(E, "/c");
    let id = page
        .rows
        .iter()
        .find(|row| row[1].as_str() == Some(want.as_str()))
        .map(|row| row[0].as_i64().unwrap())
        .unwrap();
    // FR-024.
    let results = r.custom_results(id).unwrap();
    let tags = results.iter().find(|x| x.name == "Tags").unwrap();
    assert_eq!(tags.values, ["t0", "t1", "t2"]);
    assert_eq!(tags.count, Some(3));
    let consent = results.iter().find(|x| x.name == "Consent").unwrap();
    assert_eq!(consent.kind, "search");
    assert_eq!(consent.count, Some(1));
    assert!(consent.flags.is_empty(), "contains it, so not met");

    // FR-025: every value joined, and the first only without the option.
    let tags_id = r
        .find_tab("custom_extraction")
        .unwrap()
        .columns
        .iter()
        .find(|c| c.label == "Tags")
        .unwrap()
        .id
        .to_string();
    let export = |all: Option<&str>| {
        let mut out = Vec::new();
        let never = AtomicBool::new(false);
        write_csv(
            &c.path,
            &q("custom_extraction", "all"),
            &["address".into(), tags_id.clone()],
            None,
            all,
            &mut out,
            &never,
            &mut |_, _| {},
        )
        .unwrap();
        String::from_utf8(out).unwrap()
    };
    let joined = export(Some(" | "));
    assert!(joined.lines().any(|l| l.ends_with("/c,t0 | t1 | t2")), "{joined}");
    let first = export(None);
    assert!(first.lines().any(|l| l.ends_with("/c,t0")), "{first}");
}

#[tokio::test]
async fn crawls_without_rules_have_no_custom_tabs() {
    let c = crawl(CustomRules::default()).await;
    let mut r = Reader::open(&c.path).unwrap();
    assert_eq!(r.tabs().unwrap().len(), tabs().len());
    assert!(r.find_tab("custom_extraction").is_err());
    assert!(r.custom_results(1).unwrap().is_empty());
}
