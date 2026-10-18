//! Spec 026 results: the Accessibility and Mobile tabs, their filters and
//! counts, the rules that read the stored checks, the WCAG target switch
//! and the detail pane (SC-004, SC-006, FR-020, FR-023).
//!
//! The crawl runs with the checks off; the stored results are written
//! through the engine's store, so each page has exactly the data its test
//! needs.

use std::borrow::Cow;
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::Duration;

use spiderlens_config::{A11yLevel, CrawlConfig, ModeInput, SpeedSettings, WcagLevel, WcagTarget, WcagVersion};
use spiderlens_engine::crawl::store::{A11yNodeRow, A11yRow, MobileRow};
use spiderlens_engine::crawl::{Control, CrawlStore, Crawler};
use spiderlens_results::{A11Y_DISCLAIMER, Need, Reader, UrlScope, ViewQuery, tabs};
use spiderlens_test_fixtures::{Site, SiteBuilder};
use tokio::sync::watch;

const E: &str = "a11y-results.test";

fn page(links: &[&str]) -> String {
    let a: String = links.iter().map(|l| format!("<a href=\"{l}\">{l}</a> ")).collect();
    format!("<!doctype html><html lang=\"en\"><head><title>Page</title></head><body>{a}</body></html>")
}

fn site() -> Site {
    SiteBuilder::new()
        .html(E, "/", &page(&["/a", "/b", "/c", "/d"]))
        .html(E, "/a", &page(&[]))
        .html(E, "/b", &page(&[]))
        .html(E, "/c", &page(&[]))
        .html(E, "/d", &page(&[]))
        .start()
}

struct Crawled {
    _dir: tempfile::TempDir,
    path: PathBuf,
    site: Site,
}

fn nodes(n: usize) -> Vec<A11yNodeRow<'static>> {
    (0..n)
        .map(|i| A11yNodeRow {
            selector: Cow::Owned(format!("#n{i}")),
            excerpt: Cow::Borrowed("<input type=\"text\">"),
            data: None,
            bbox: None,
        })
        .collect()
}

fn row(
    check: &str,
    kind: &'static str,
    source: &'static str,
    impact: &'static str,
    wcag: &str,
    total: u32,
    stored: Vec<A11yNodeRow<'static>>,
) -> A11yRow<'static> {
    A11yRow {
        check_id: Cow::Owned(check.to_string()),
        kind,
        source,
        impact: Some(impact),
        wcag: Cow::Owned(wcag.to_string()),
        nodes_total: total,
        nodes: stored,
    }
}

/// Crawls the site with the checks off, then stores the results by hand:
///
/// - `/`  checked statically, nothing found; viewport ok, not measured
/// - `/a` full: label (critical, 2.0 A, 25 elements, 20 stored),
///   image-alt (critical, 2.0 A), color-contrast (serious, 2.0 AA, with
///   data and a box), heading-order (moderate, best practice), and a
///   result to review; measured: 900 px wide, 55 % small text, 3 targets
/// - `/b` static link_name (serious, 2.0 A) and full target-size (moderate,
///   2.2 AA); no viewport, 2 plugins, desktop window
/// - `/c` checked, nothing found; measured and fine
/// - `/d` not checked
async fn crawl(features: bool) -> Crawled {
    let site = site();
    let mut c = CrawlConfig {
        speed: SpeedSettings::FAST,
        ..CrawlConfig::default()
    };
    c.accessibility.level = A11yLevel::Off;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a11y-results.spiderlens");
    let crawler = Crawler::create(
        &path,
        c,
        &ModeInput::Spider {
            start_url: site.url(E, "/"),
        },
    )
    .unwrap()
    .with_resolve(site.resolve())
    .with_retry_base(Duration::from_millis(10));
    let (_tx, rx) = watch::channel(Control::Run);
    tokio::time::timeout(Duration::from_secs(60), crawler.run(rx, None))
        .await
        .expect("crawl finishes")
        .unwrap();

    if features {
        let store = CrawlStore::open(&path).unwrap();
        let id = |p: &str| store.id_of(&site.url(E, p)).unwrap().unwrap();
        store.add_data_features(&["accessibility_static", "accessibility_full", "mobile_layout"]).unwrap();

        let ok_viewport = MobileRow {
            viewport: Some("width=device-width, initial-scale=1"),
            viewport_state: Some("ok"),
            plugins: Some(0),
            ..MobileRow::default()
        };
        store.record_static_a11y(id("/"), &[], &ok_viewport).unwrap();

        let a = id("/a");
        store.record_static_a11y(a, &[], &ok_viewport).unwrap();
        let mut contrast_nodes = nodes(3);
        contrast_nodes[0].data = Some("{\"fg\":\"#aaaaaa\",\"bg\":\"#ffffff\",\"ratio\":2.32,\"expected\":4.5}".into());
        contrast_nodes[0].bbox = Some([10.0, 20.0, 30.0, 40.0]);
        store
            .record_a11y(
                a,
                "full",
                &[
                    row("label", "violation", "full", "critical", "wcag2a wcag412 wcag131", 25, nodes(25)),
                    row("image-alt", "violation", "full", "critical", "wcag2a wcag111", 1, nodes(1)),
                    row("color-contrast", "violation", "full", "serious", "wcag2aa wcag143", 3, contrast_nodes),
                    row("heading-order", "violation", "full", "moderate", "best-practice", 2, nodes(2)),
                    row(
                        "frame-focusable-content",
                        "needs_review",
                        "full",
                        "serious",
                        "wcag2a wcag211",
                        1,
                        nodes(1),
                    ),
                ],
            )
            .unwrap();
        store.set_a11y_state(a, "full", "checked", None, Some(420)).unwrap();
        store
            .record_mobile(
                a,
                &MobileRow {
                    window: Some("mobile"),
                    measured: Some("yes"),
                    content_px: Some(900.0),
                    window_px: Some(411.0),
                    widest: Some("table#prices"),
                    small_text_share: Some(55.0),
                    small_targets: Some(3),
                    examples: Some(
                        "[{\"selector\":\"nav a:nth-child(3)\",\"w\":16,\"h\":16},{\"selector\":\"footer a\",\"w\":20,\"h\":12}]"
                            .to_string(),
                    ),
                    ..MobileRow::default()
                },
            )
            .unwrap();

        let b = id("/b");
        store
            .record_static_a11y(
                b,
                &[row("static.link_name", "violation", "static", "serious", "wcag2a wcag244 wcag412", 2, nodes(2))],
                &MobileRow {
                    viewport_state: Some("missing"),
                    plugins: Some(2),
                    ..MobileRow::default()
                },
            )
            .unwrap();
        store
            .record_a11y(
                b,
                "full",
                &[row("target-size", "violation", "full", "moderate", "wcag22aa wcag258", 4, nodes(4))],
            )
            .unwrap();
        store.set_a11y_state(b, "full", "checked", None, Some(300)).unwrap();
        store
            .record_mobile(
                b,
                &MobileRow {
                    window: Some("desktop"),
                    measured: Some("desktop_window"),
                    ..MobileRow::default()
                },
            )
            .unwrap();

        let c = id("/c");
        store.record_static_a11y(c, &[], &ok_viewport).unwrap();
        store
            .record_mobile(
                c,
                &MobileRow {
                    window: Some("mobile"),
                    measured: Some("yes"),
                    content_px: Some(411.0),
                    window_px: Some(411.0),
                    small_text_share: Some(10.0),
                    small_targets: Some(0),
                    examples: Some("[]".to_string()),
                    ..MobileRow::default()
                },
            )
            .unwrap();
    }
    Crawled {
        _dir: dir,
        path,
        site,
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
        values: None,
    }
}

fn strip(site: &Site, url: &str) -> String {
    let base = site.url(E, "");
    url.strip_prefix(base.trim_end_matches('/')).unwrap_or(url).to_string()
}

fn paths(r: &mut Reader, c: &Crawled, tab: &str, filter: &str) -> BTreeSet<String> {
    let page = r.open_view(&q(tab, filter), 500).unwrap();
    r.materialize(page.view).unwrap();
    let rows = r.rows(page.view, 0, 1000).unwrap().rows;
    r.close_view(page.view).unwrap();
    rows.iter()
        .map(|row| strip(&c.site, row[1].as_str().unwrap()))
        .collect()
}

fn set(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(|s| s.to_string()).collect()
}

/// The pages a rule flags with their evidence, from its issue tab.
fn flagged(r: &mut Reader, c: &Crawled, rule: &str) -> Vec<(String, Option<String>)> {
    let page = r.open_view(&q(&format!("issue:{rule}"), "all"), 500).unwrap();
    let mut out: Vec<(String, Option<String>)> = page
        .rows
        .iter()
        .map(|row| {
            (
                strip(&c.site, row[1].as_str().unwrap()),
                row[2].as_str().map(str::to_string),
            )
        })
        .collect();
    out.sort();
    out
}

fn flagged_paths(r: &mut Reader, c: &Crawled, rule: &str) -> BTreeSet<String> {
    flagged(r, c, rule).into_iter().map(|(p, _)| p).collect()
}

fn id_of(r: &mut Reader, c: &Crawled, path: &str) -> i64 {
    let page = r.open_view(&q("internal", "all"), 500).unwrap();
    let want = c.site.url(E, path);
    page.rows
        .iter()
        .find(|row| row[1].as_str() == Some(want.as_str()))
        .map(|row| row[0].as_i64().unwrap())
        .unwrap_or_else(|| panic!("{path} not crawled"))
}

#[tokio::test(flavor = "multi_thread")]
async fn accessibility_tab_filters_follow_the_impact_and_the_wcag_target() {
    let c = crawl(true).await;
    let mut r = Reader::open(&c.path).unwrap();
    assert!(r.features().has(Need::AccessibilityStatic));
    assert!(r.features().has(Need::AccessibilityFull));
    assert!(r.features().has(Need::MobileLayout));

    let p = |r: &mut Reader, f: &str| paths(r, &c, "accessibility", f);
    // The default target is WCAG 2.2 AA with best practices.
    assert_eq!(p(&mut r, "all"), set(&["/", "/a", "/b", "/c", "/d"]));
    assert_eq!(p(&mut r, "critical"), set(&["/a"]));
    assert_eq!(p(&mut r, "serious"), set(&["/a", "/b"]));
    assert_eq!(p(&mut r, "moderate"), set(&["/a", "/b"]));
    assert_eq!(p(&mut r, "minor"), set(&[]));
    assert_eq!(p(&mut r, "needs_review"), set(&["/a"]));
    assert_eq!(p(&mut r, "not_checked"), set(&["/d"]));

    // Every count equals the rows it lists.
    let tab = tabs().iter().find(|t| t.id == "accessibility").unwrap();
    let counts = r.counts("accessibility", UrlScope::All).unwrap();
    for f in &tab.filters {
        assert_eq!(
            counts.counts[&*f.id],
            Some(p(&mut r, &f.id).len() as u64),
            "{}",
            f.id
        );
    }

    // WCAG 2.0 A leaves out color-contrast (2.0 AA) and target-size (2.2 AA)
    // without checking again; best practices still count.
    r.set_a11y_target(WcagTarget {
        version: WcagVersion::V2_0,
        level: WcagLevel::A,
    })
    .unwrap();
    assert_eq!(p(&mut r, "all"), set(&["/", "/a", "/b", "/c", "/d"]));
    assert_eq!(p(&mut r, "critical"), set(&["/a"]));
    assert_eq!(p(&mut r, "serious"), set(&["/b"]), "/a's only serious check is 2.0 AA");
    assert_eq!(p(&mut r, "moderate"), set(&["/a"]), "/b's only moderate check is 2.2 AA");
    let counts = r.counts("accessibility", UrlScope::All).unwrap();
    assert_eq!(counts.counts["serious"], Some(1));
    assert_eq!(counts.counts["moderate"], Some(1));
    assert_eq!(counts.counts["critical"], Some(1));

    // Switching back restores the counts.
    r.set_a11y_target(WcagTarget {
        version: WcagVersion::V2_2,
        level: WcagLevel::Aa,
    })
    .unwrap();
    assert_eq!(p(&mut r, "serious"), set(&["/a", "/b"]));
}

#[tokio::test(flavor = "multi_thread")]
async fn accessibility_rules_fire_on_the_pages_and_follow_the_target() {
    let c = crawl(true).await;
    let mut r = Reader::open(&c.path).unwrap();

    assert_eq!(flagged_paths(&mut r, &c, "accessibility.link_name"), set(&["/b"]));
    assert_eq!(flagged_paths(&mut r, &c, "accessibility.form_label"), set(&[]));
    assert_eq!(flagged_paths(&mut r, &c, "accessibility.needs_review"), set(&["/a"]));
    assert_eq!(
        flagged(&mut r, &c, "accessibility.needs_review")[0].1.as_deref(),
        Some("1 checks to review")
    );

    // Target 2.2 AA: /a fails four checks on 31 elements; /b only target-size.
    let full = flagged(&mut r, &c, "accessibility.full_violation");
    assert_eq!(
        full,
        [
            ("/a".to_string(), Some("4 checks failed, on 31 elements".to_string())),
            ("/b".to_string(), Some("1 checks failed, on 4 elements".to_string())),
        ]
    );

    // Target 2.0 A: color-contrast and target-size drop out.
    r.set_a11y_target(WcagTarget {
        version: WcagVersion::V2_0,
        level: WcagLevel::A,
    })
    .unwrap();
    let full = flagged(&mut r, &c, "accessibility.full_violation");
    assert_eq!(
        full,
        [("/a".to_string(), Some("3 checks failed, on 28 elements".to_string()))]
    );
    // The static link_name check is 2.0 A: still inside.
    assert_eq!(flagged_paths(&mut r, &c, "accessibility.link_name"), set(&["/b"]));

    // The summary lists the rules with the same counts.
    let summary = r.issues_summary(false).unwrap();
    let count = |id: &str| summary.issues.iter().find(|i| i.id == id).unwrap().count;
    assert_eq!(count("accessibility.full_violation"), Some(1));
    assert_eq!(count("accessibility.link_name"), Some(1));
}

#[tokio::test(flavor = "multi_thread")]
async fn mobile_tab_and_rules_use_the_stored_measures() {
    let c = crawl(true).await;
    let mut r = Reader::open(&c.path).unwrap();

    let p = |r: &mut Reader, f: &str| paths(r, &c, "mobile", f);
    assert_eq!(p(&mut r, "all"), set(&["/", "/a", "/b", "/c", "/d"]));
    assert_eq!(p(&mut r, "viewport_issues"), set(&["/b"]));
    assert_eq!(p(&mut r, "content_wider"), set(&["/a"]));
    assert_eq!(p(&mut r, "small_text"), set(&["/a"]));
    assert_eq!(p(&mut r, "small_targets"), set(&["/a"]));
    assert_eq!(p(&mut r, "not_measured"), set(&["/", "/b", "/d"]));
    let tab = tabs().iter().find(|t| t.id == "mobile").unwrap();
    let counts = r.counts("mobile", UrlScope::All).unwrap();
    for f in &tab.filters {
        assert_eq!(
            counts.counts[&*f.id],
            Some(p(&mut r, &f.id).len() as u64),
            "{}",
            f.id
        );
    }

    assert_eq!(flagged_paths(&mut r, &c, "mobile.viewport_missing"), set(&["/b"]));
    assert_eq!(flagged_paths(&mut r, &c, "mobile.plugins"), set(&["/b"]));
    assert_eq!(flagged_paths(&mut r, &c, "mobile.zoom_disabled"), set(&[]));
    assert_eq!(flagged_paths(&mut r, &c, "mobile.viewport_width"), set(&[]));
    assert_eq!(
        flagged(&mut r, &c, "mobile.content_wider"),
        [("/a".to_string(), Some("900 px wide on a 411 px screen".to_string()))]
    );
    assert_eq!(
        flagged(&mut r, &c, "mobile.small_text"),
        [("/a".to_string(), Some("55% of the text is below 12 px".to_string()))]
    );
    assert_eq!(
        flagged(&mut r, &c, "mobile.small_targets"),
        [("/a".to_string(), Some("3 targets under 24 px".to_string()))]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_detail_lists_checks_by_impact_with_the_cap_help_links_and_the_disclaimer() {
    let c = crawl(true).await;
    let mut r = Reader::open(&c.path).unwrap();

    let a = id_of(&mut r, &c, "/a");
    let d = r.detail(a).unwrap();
    let acc = d.accessibility.expect("accessibility section");
    assert_eq!(acc.static_state.as_deref(), Some("checked"));
    assert_eq!(acc.full_state.as_deref(), Some("checked"));
    assert_eq!(acc.full_reason, None);
    assert_eq!(acc.disclaimer, A11Y_DISCLAIMER);
    assert!(acc.capped, "label has 25 failing elements and 20 are stored");
    // Critical, serious, moderate; then by check id.
    let ids: Vec<&str> = acc.checks.iter().map(|c| c.check_id.as_str()).collect();
    assert_eq!(
        ids,
        ["image-alt", "label", "color-contrast", "frame-focusable-content", "heading-order"]
    );
    let label = &acc.checks[1];
    assert_eq!((label.kind.as_str(), label.source.as_str()), ("violation", "full"));
    assert_eq!(label.impact.as_deref(), Some("critical"));
    assert_eq!(label.wcag.as_deref(), Some("wcag2a wcag412 wcag131"));
    assert_eq!(label.nodes_total, 25);
    assert_eq!(label.nodes.len(), 20);
    assert_eq!(label.title, "Label");
    assert_eq!(
        label.help_url.as_deref(),
        Some("https://dequeuniversity.com/rules/axe/4.14/label")
    );
    let contrast = &acc.checks[2];
    assert_eq!(contrast.nodes.len(), 3);
    assert!(contrast.nodes[0].data.as_deref().unwrap().contains("2.32"));
    assert_eq!(contrast.nodes[0].bbox, Some(vec![10.0, 20.0, 30.0, 40.0]));
    assert_eq!(contrast.nodes[1].bbox, None);
    let review = &acc.checks[3];
    assert_eq!(review.kind, "needs_review");

    let m = d.mobile.expect("mobile section");
    assert_eq!(m.viewport_state.as_deref(), Some("ok"));
    assert_eq!(m.measured.as_deref(), Some("yes"));
    assert_eq!(m.window.as_deref(), Some("mobile"));
    assert_eq!((m.content_px, m.window_px), (Some(900.0), Some(411.0)));
    assert_eq!(m.widest.as_deref(), Some("table#prices"));
    assert_eq!((m.small_text_share, m.small_targets), (Some(55.0), Some(3)));
    assert_eq!(m.examples.len(), 2);
    assert_eq!(m.examples[0].selector, "nav a:nth-child(3)");
    assert_eq!((m.examples[1].w, m.examples[1].h), (20.0, 12.0));

    // A static result has no public documentation link and a readable title.
    let b = id_of(&mut r, &c, "/b");
    let d = r.detail(b).unwrap();
    let acc = d.accessibility.unwrap();
    assert!(!acc.capped);
    let ids: Vec<&str> = acc.checks.iter().map(|c| c.check_id.as_str()).collect();
    assert_eq!(ids, ["static.link_name", "target-size"]);
    assert_eq!(acc.checks[0].source, "static");
    assert_eq!(acc.checks[0].help_url, None);
    assert_eq!(acc.checks[0].title, "Link without a name");
    assert!(acc.checks[1].help_url.is_some());
    let m = d.mobile.unwrap();
    assert_eq!(m.viewport_state.as_deref(), Some("missing"));
    assert_eq!(m.plugins, Some(2));
    assert_eq!(m.measured.as_deref(), Some("desktop_window"));
    assert!(m.examples.is_empty());

    // A checked page without findings still has its section; a page the
    // checks did not reach has none.
    let clean = id_of(&mut r, &c, "/c");
    let acc = r.detail(clean).unwrap().accessibility.unwrap();
    assert!(acc.checks.is_empty());
    assert_eq!(acc.static_state.as_deref(), Some("checked"));
    assert_eq!(acc.disclaimer, A11Y_DISCLAIMER);
    let unchecked = id_of(&mut r, &c, "/d");
    let d = r.detail(unchecked).unwrap();
    assert!(d.accessibility.is_none());
    assert!(d.mobile.is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_crawl_without_the_checks_has_no_tabs_data_or_sections() {
    let c = crawl(false).await;
    let mut r = Reader::open(&c.path).unwrap();
    assert!(!r.features().has(Need::AccessibilityStatic));
    assert!(!r.features().has(Need::AccessibilityFull));
    assert!(!r.features().has(Need::MobileLayout));
    for tab in ["accessibility", "mobile"] {
        let counts = r.counts(tab, UrlScope::All).unwrap();
        assert_eq!(counts.counts["all"], None, "{tab}");
        assert!(r.open_view(&q(tab, "all"), 10).is_err(), "{tab}");
    }
    let id = id_of(&mut r, &c, "/a");
    let d = r.detail(id).unwrap();
    assert!(d.accessibility.is_none());
    assert!(d.mobile.is_none());
    let summary = r.issues_summary(false).unwrap();
    let rule = summary
        .issues
        .iter()
        .find(|i| i.id == "accessibility.full_violation")
        .unwrap();
    assert_eq!(rule.count, None);
    assert!(rule.unavailable.is_some());
}
