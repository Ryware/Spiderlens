//! Spec 012 acceptance tests on real crawls of a fixture site before and
//! after known changes (specs/012-crawl-comparison/quickstart.md). The
//! second version is served on the same port, so URLs are identical.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use spiderlens_compare::{Comparison, FilterChange, Group, IssueChange, UrlGroup};
use spiderlens_config::{
    CompareSettings, CrawlConfig, ElementSetting, IssueSettings, IssueSettingsChoice, MapRule, ModeInput,
    RuleOverride, SpeedSettings,
};
use spiderlens_engine::crawl::{Control, Crawler};
use spiderlens_test_fixtures::{Site, SiteBuilder};
use tokio::sync::watch;

const E: &str = "example.test";

fn page(title: Option<&str>, head: &str, body: &str, links: &[&str]) -> String {
    let t = title.map_or(String::new(), |t| format!("<title>{t}</title>"));
    let a: String = links.iter().map(|l| format!("<a href=\"{l}\">x</a>")).collect();
    format!("<!doctype html><html><head>{t}{head}</head><body>{body}{a}</body></html>")
}

fn words(n: usize) -> String {
    format!("<p>{}</p>", "word ".repeat(n))
}

const LD: &str = r#"<script type="application/ld+json">{"@context":"https://schema.org","@type":"Product","name":"X"}</script>"#;

/// Version 1 of the site.
fn v1(host: &str) -> SiteBuilder {
    let home = [
        "/a",
        "/b",
        "/c",
        "/d",
        "/case",
        "/redir-me",
        "/canon",
        "/words",
        "/words2",
        "/text",
        "/sd",
        "/gone",
    ];
    SiteBuilder::new()
        .html(
            host,
            "/",
            &page(Some("Home page of the fixture site"), "", "", &home),
        )
        .html(
            host,
            "/a",
            &page(Some("Alpha page title of a fair length"), "", "", &[]),
        )
        .html(host, "/b", &page(Some("Beta"), "", "", &[]))
        .html(host, "/c", &page(Some("  Gamma  "), "", "", &[]))
        .html(host, "/d", &page(Some("Delta"), "", "", &[]))
        .html(host, "/case", &page(Some("Case title"), "", "", &[]))
        .html(host, "/redir-me", &page(Some("Will redirect"), "", "", &[]))
        .html(
            host,
            "/canon",
            &page(
                Some("Canonical page"),
                &format!("<link rel=\"canonical\" href=\"{{base:{host}}}/canon\">"),
                "",
                &[],
            ),
        )
        .html(host, "/words", &page(Some("Words"), "", &words(1000), &[]))
        .html(host, "/words2", &page(Some("Words two"), "", &words(1000), &[]))
        .html(
            host,
            "/text",
            &page(Some("Text"), "", "<p>The original text of this page.</p>", &[]),
        )
        .html(host, "/sd", &page(Some("Structured"), "", "", &[]))
        .html(host, "/gone", &page(Some("Gone soon"), "", "", &[]))
}

/// Version 2: every change has a known effect.
fn v2(host: &str) -> SiteBuilder {
    let home = [
        "/a",
        "/b",
        "/c",
        "/d",
        "/case",
        "/redir-me",
        "/canon",
        "/words",
        "/words2",
        "/text",
        "/sd",
        "/new1",
        "/new2",
    ];
    SiteBuilder::new()
        .html(
            host,
            "/",
            &page(Some("Home page of the fixture site"), "", "", &home),
        )
        .html(
            host,
            "/a",
            &page(Some("Alpha page title of a fair length"), "", "", &[]),
        )
        .html(host, "/b", &page(Some("Beta updated"), "", "", &[]))
        .html(host, "/c", &page(Some("Gamma"), "", "", &[]))
        .html(host, "/d", &page(None, "", "", &[]))
        .html(host, "/case", &page(Some("case title"), "", "", &[]))
        .redirect(host, "/redir-me", 301, "/a")
        .html(
            host,
            "/canon",
            &page(
                Some("Canonical page"),
                &format!("<link rel=\"canonical\" href=\"{{base:{host}}}/a\">"),
                "",
                &[],
            ),
        )
        .html(host, "/words", &page(Some("Words"), "", &words(1050), &[]))
        .html(host, "/words2", &page(Some("Words two"), "", &words(700), &[]))
        .html(
            host,
            "/text",
            &page(
                Some("Text"),
                "",
                "<p>Completely different words for this page.</p>",
                &[],
            ),
        )
        .html(host, "/sd", &page(Some("Structured"), LD, "", &[]))
        .html(host, "/new1", &page(Some("New one"), "", "", &[]))
        .html(host, "/new2", &page(Some("New two"), "", "", &[]))
}

async fn crawl(site: &Site, host: &str, path: &Path) {
    crawl_with(site, host, path, None).await;
}

async fn crawl_with(site: &Site, host: &str, path: &Path, max_urls: Option<u64>) {
    let mut c = CrawlConfig {
        speed: SpeedSettings::FAST,
        ..CrawlConfig::default()
    };
    c.limits.max_urls = max_urls;
    let input = ModeInput::Spider {
        start_url: site.url(host, "/"),
    };
    let crawler = Crawler::create(path, c, &input)
        .unwrap()
        .with_resolve(site.resolve())
        .with_retry_base(Duration::from_millis(10));
    let (_tx, rx) = watch::channel(Control::Run);
    tokio::time::timeout(Duration::from_secs(60), crawler.run(rx, None))
        .await
        .expect("crawl finishes")
        .unwrap();
}

struct Pair {
    _dir: tempfile::TempDir,
    prev: PathBuf,
    cur: PathBuf,
    base: String,
}

async fn same_site_pair() -> Pair {
    let dir = tempfile::tempdir().unwrap();
    let prev = dir.path().join("v1.spiderlens");
    let cur = dir.path().join("v2.spiderlens");
    let mut site = v1(E).start();
    crawl(&site, E, &prev).await;
    site.replace(v2(E));
    crawl(&site, E, &cur).await;
    Pair {
        base: site.url(E, ""),
        _dir: dir,
        prev,
        cur,
    }
}

fn run(p: &Pair, settings: CompareSettings) -> Comparison {
    Comparison::run(
        &p.prev,
        &p.cur,
        settings,
        None,
        &AtomicBool::new(false),
        &mut |_| {},
    )
    .unwrap()
}

/// Paths of a group, from the current address or else the previous one.
fn paths(c: &mut Comparison, base: &str, g: Group) -> BTreeSet<String> {
    let page = c.open_group(&g, "", None, 1000).unwrap();
    assert_eq!(page.rows.len() as u32, page.total, "{g:?}");
    let out = page
        .rows
        .iter()
        .map(|r| {
            let url = r.values[1].as_str().or(r.values[0].as_str()).unwrap().to_string();
            url.strip_prefix(base).map_or(url.clone(), str::to_string)
        })
        .collect();
    c.close_group(page.view).unwrap();
    out
}

fn set(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(|s| s.to_string()).collect()
}

fn filter(tab: &str, filter: &str, change: FilterChange) -> Group {
    Group::Filter {
        tab: tab.into(),
        filter: filter.into(),
        change,
    }
}

fn element(id: &str) -> Group {
    Group::Element { element: id.into() }
}

#[tokio::test]
async fn us1_us2_changes_between_two_crawls_of_the_same_site() {
    let p = same_site_pair().await;
    // FR-001: the previous crawl of the same site is suggested.
    assert_eq!(
        spiderlens_compare::suggest_previous(
            spiderlens_compare::crawl_files(p.prev.parent().unwrap()),
            &p.cur
        )
        .as_deref(),
        Some(p.prev.as_path())
    );
    assert_eq!(
        spiderlens_compare::suggest_previous(
            spiderlens_compare::crawl_files(p.prev.parent().unwrap()),
            &p.prev
        ),
        None
    );
    let mut c = run(&p, CompareSettings::default());
    let s = c.summary().clone();

    // US1-1: new and missing URLs.
    assert_eq!(s.new, 2);
    assert_eq!(s.missing, 1);
    assert_eq!(s.not_reached, 0);
    assert!(!s.low_overlap);
    assert_eq!(s.missing_pages, 1);
    assert_eq!(
        s.lost_indexable, 2,
        "/canon is canonicalised, /redir-me redirects"
    );
    assert_eq!(
        paths(&mut c, &p.base, Group::Urls { which: UrlGroup::New }),
        set(&["/new1", "/new2"])
    );
    assert_eq!(
        paths(
            &mut c,
            &p.base,
            Group::Urls {
                which: UrlGroup::Missing
            }
        ),
        set(&["/gone"])
    );
    let internal = s
        .filters
        .iter()
        .find(|f| f.tab == "internal" && f.filter == "all")
        .unwrap();
    assert_eq!((internal.new, internal.missing), (2, 1));
    assert_eq!(internal.change(), 1);

    // US1-2: a page that loses its title moves into "Missing", stays in All.
    assert_eq!(
        paths(
            &mut c,
            &p.base,
            filter("page-titles", "missing", FilterChange::Added)
        ),
        set(&["/d"])
    );
    assert!(
        paths(
            &mut c,
            &p.base,
            filter("page-titles", "all", FilterChange::Removed)
        )
        .contains("/redir-me")
    );
    assert!(
        !paths(
            &mut c,
            &p.base,
            filter("page-titles", "all", FilterChange::Removed)
        )
        .contains("/d")
    );

    // US1-3: 200 to 301 moves between response code filters.
    assert_eq!(
        paths(
            &mut c,
            &p.base,
            filter("response-codes", "redirection", FilterChange::Added)
        ),
        set(&["/redir-me"])
    );
    assert_eq!(
        paths(
            &mut c,
            &p.base,
            filter("response-codes", "success", FilterChange::Removed)
        ),
        set(&["/redir-me"])
    );

    // US2-1, US2-2: title changes; whitespace-only is not one, letter case is.
    assert_eq!(
        paths(&mut c, &p.base, element("title")),
        set(&["/b", "/case", "/d"])
    );
    // US2-3: word count +5% is under the 10% threshold, -30% is over it.
    assert_eq!(paths(&mut c, &p.base, element("word_count")), set(&["/words2"]));
    // US2-4: canonical moved away, so indexability changed too.
    assert_eq!(paths(&mut c, &p.base, element("canonical")), set(&["/canon"]));
    let idx = paths(&mut c, &p.base, element("indexability"));
    assert!(idx.contains("/canon") && idx.contains("/redir-me"), "{idx:?}");
    assert_eq!(
        paths(&mut c, &p.base, element("status_code")),
        set(&["/redir-me"])
    );
    assert_eq!(
        paths(&mut c, &p.base, element("redirect_target")),
        set(&["/redir-me"])
    );
    // US2-5: content fingerprint, without comparing text.
    assert!(paths(&mut c, &p.base, element("content")).contains("/text"));
    assert_eq!(paths(&mut c, &p.base, element("structured_data")), set(&["/sd"]));

    // Values side by side with differences.
    let page = c.open_group(&element("word_count"), "", None, 10).unwrap();
    let ids: Vec<_> = page.columns.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "address_previous",
            "address_current",
            "word_count_previous",
            "word_count_current",
            "difference",
            "difference_percent"
        ]
    );
    assert_eq!(page.rows[0].values[2], 1000);
    assert_eq!(page.rows[0].values[3], 700);
    assert_eq!(page.rows[0].values[4], -300);
    assert_eq!(page.rows[0].values[5], -30);

    // US2-6: a threshold change recounts without rescanning.
    let deltas = c
        .set_elements(vec![ElementSetting {
            id: "word_count".into(),
            on: true,
            threshold: Some(4.0),
        }])
        .unwrap();
    assert_eq!(
        deltas.iter().find(|d| d.element == "word_count").unwrap().changed,
        2
    );
    assert_eq!(
        paths(&mut c, &p.base, element("word_count")),
        set(&["/words", "/words2"])
    );

    // US1-5: search and export of a group with both crawls' values.
    let page = c
        .open_group(
            &filter("page-titles", "all", FilterChange::Removed),
            "redir",
            None,
            10,
        )
        .unwrap();
    assert_eq!(page.total, 1);
    let out = p._dir.path().join("removed.csv");
    let n = c
        .export_group(page.view, &out, &AtomicBool::new(false), &mut |_, _| {})
        .unwrap();
    assert_eq!(n, 1);
    let text = std::fs::read_to_string(&out).unwrap();
    assert!(
        text.starts_with("\u{feff}Address (previous),Address (current),Title (previous),Title (current)")
    );
    assert!(text.contains("Will redirect"));
}

fn fingerprint(p: &Path) -> (Vec<u8>, std::time::SystemTime) {
    (
        std::fs::read(p).unwrap(),
        std::fs::metadata(p).unwrap().modified().unwrap(),
    )
}

#[tokio::test]
async fn us6_files_are_never_written_and_cancel_stops() {
    let p = same_site_pair().await;
    let before = (fingerprint(&p.prev), fingerprint(&p.cur));
    let c = run(&p, CompareSettings::default());
    let ws = c.workspace_path().to_path_buf();
    assert!(ws.exists());
    drop(c);
    assert!(!ws.exists(), "closing removes the workspace");
    assert_eq!((fingerprint(&p.prev), fingerprint(&p.cur)), before, "SC-004");

    let cancelled = Comparison::run(
        &p.prev,
        &p.cur,
        CompareSettings::default(),
        None,
        &AtomicBool::new(true),
        &mut |_| {},
    );
    assert!(matches!(cancelled, Err(spiderlens_compare::Error::Cancelled)));
    assert_eq!((fingerprint(&p.prev), fingerprint(&p.cur)), before);

    // Same file twice is refused (edge case).
    assert!(spiderlens_compare::setup(&p.prev, &p.prev).is_err());
}

#[tokio::test]
async fn us4_mapping_matches_a_migrated_site() {
    let dir = tempfile::tempdir().unwrap();
    let prev = dir.path().join("old.spiderlens");
    let cur = dir.path().join("new.spiderlens");
    let old = v1("old.test").start();
    crawl(&old, "old.test", &prev).await;
    let new = v2("new.test").start();
    crawl(&new, "new.test", &cur).await;

    // US4-1: different origins get a suggested host rule.
    let setup = spiderlens_compare::setup(&prev, &cur).unwrap();
    let rule = setup.suggested_mapping.clone().expect("host rule");
    assert_eq!(rule.find, old.url("old.test", "/"));
    assert_eq!(rule.replace, new.url("new.test", "/"));
    assert!(setup.config_diffs.iter().any(|d| d.field == "start_url"));

    // Without mapping almost nothing matches.
    let c = Comparison::run(
        &prev,
        &cur,
        CompareSettings::default(),
        None,
        &AtomicBool::new(false),
        &mut |_| {},
    )
    .unwrap();
    assert!(c.summary().low_overlap);
    drop(c);

    // US4-3: the tester shows the mapped URL and whether it exists.
    let tests = spiderlens_compare::test_mapping(
        &prev,
        &cur,
        std::slice::from_ref(&rule),
        &[old.url("old.test", "/gone")],
        0,
    )
    .unwrap();
    assert_eq!(tests[0].mapped, new.url("new.test", "/gone"));
    assert!(!tests[0].exists_in_current);

    // US4-2, US4-4: a host rule plus a collision rule.
    let collide = MapRule {
        find: format!("^{}$", old.url("old.test", "/c").replace('.', r"\.")),
        replace: new.url("new.test", "/a"),
        regex: true,
    };
    let settings = CompareSettings {
        mapping: vec![collide, rule],
        ..CompareSettings::default()
    };
    let mut c = Comparison::run(&prev, &cur, settings, None, &AtomicBool::new(false), &mut |_| {}).unwrap();
    let s = c.summary().clone();
    assert!(!s.low_overlap);
    let base = new.url("new.test", "");
    assert_eq!(
        paths(&mut c, &base, Group::Urls { which: UrlGroup::New }),
        set(&["/c", "/new1", "/new2"]),
        "/c is new: its old page now maps onto /a"
    );
    assert_eq!(s.missing, 1);
    assert_eq!(s.collisions, 1);
    assert_eq!(
        paths(
            &mut c,
            &base,
            Group::Urls {
                which: UrlGroup::Collisions
            }
        ),
        set(&["/a"])
    );
    assert!(paths(&mut c, &base, element("title")).contains("/b"));
}

#[tokio::test]
async fn fr015_urls_an_incomplete_crawl_did_not_reach_are_not_missing() {
    let dir = tempfile::tempdir().unwrap();
    let prev = dir.path().join("full.spiderlens");
    let cur = dir.path().join("limited.spiderlens");
    let site = v1(E).start();
    crawl(&site, E, &prev).await;
    crawl_with(&site, E, &cur, Some(4)).await;

    let c = Comparison::run(
        &prev,
        &cur,
        CompareSettings::default(),
        None,
        &AtomicBool::new(false),
        &mut |_| {},
    )
    .unwrap();
    let s = c.summary();
    assert!(!s.current.complete && s.current.url_limit_reached);
    assert_eq!(s.missing, 0, "nothing was removed from the site");
    assert!(s.not_reached >= 8, "{}", s.not_reached);
    assert!(s.config_diffs.iter().any(|d| d.field == "limits.max_urls"));
    let titles = s
        .filters
        .iter()
        .find(|f| f.tab == "page-titles" && f.filter == "all")
        .unwrap();
    assert_eq!(titles.missing, 0);
    assert_eq!(titles.removed, 0);
    assert!(titles.not_reached >= 8);
}

/// A small site for issue deltas. Untitled pages in the first version: /a,
/// /b, /c and /gone. In the second, /a and /c get a title (fixed), /gone is
/// removed (gone), /b stays untitled, and /ok and the new /new1 are
/// untitled (newly affected).
fn titles(host: &str, second: bool) -> SiteBuilder {
    let long = Some("A title long enough to be fine");
    let other = Some("Another title long enough to be fine");
    if second {
        SiteBuilder::new()
            .html(
                host,
                "/",
                &page(long, "", "", &["/a", "/b", "/c", "/ok", "/new1", "/new2"]),
            )
            .html(host, "/a", &page(long, "", "", &[]))
            .html(host, "/b", &page(None, "", "", &[]))
            .html(host, "/c", &page(other, "", "", &[]))
            .html(host, "/ok", &page(None, "", "", &[]))
            .html(host, "/new1", &page(None, "", "", &[]))
            .html(
                host,
                "/new2",
                &page(Some("A third title that is fine"), "", "", &[]),
            )
    } else {
        SiteBuilder::new()
            .html(
                host,
                "/",
                &page(long, "", "", &["/a", "/b", "/c", "/gone", "/ok"]),
            )
            .html(host, "/a", &page(None, "", "", &[]))
            .html(host, "/b", &page(None, "", "", &[]))
            .html(host, "/c", &page(None, "", "", &[]))
            .html(host, "/gone", &page(None, "", "", &[]))
            .html(host, "/ok", &page(other, "", "", &[]))
    }
}

#[tokio::test]
async fn us3_issue_deltas_add_up() {
    let dir = tempfile::tempdir().unwrap();
    let (prev, cur) = (dir.path().join("i1.spiderlens"), dir.path().join("i2.spiderlens"));
    let mut site = titles(E, false).start();
    crawl(&site, E, &prev).await;
    site.replace(titles(E, true));
    crawl(&site, E, &cur).await;
    let p = Pair {
        base: site.url(E, ""),
        _dir: dir,
        prev,
        cur,
    };
    let before = (fingerprint(&p.prev), fingerprint(&p.cur));
    let mut c = run(&p, CompareSettings::default());
    let s = c.summary().clone();

    // The basis is stated: this rule set, the current crawl's settings.
    assert_eq!(
        s.issue_basis.rules_version,
        spiderlens_results::issues::RULES_VERSION
    );
    assert_eq!(s.issue_basis.settings, IssueSettingsChoice::Current);
    assert!(!s.issue_basis.custom);

    let d = s.issues.iter().find(|i| i.rule == "title.missing").unwrap();
    assert!(d.comparable);
    assert_eq!((d.previous, d.current), (Some(4), Some(3)));
    assert_eq!((d.newly_affected, d.fixed, d.gone), (2, 2, 1));
    let issue = |change| Group::Issue {
        rule: "title.missing".into(),
        change,
    };
    assert_eq!(
        paths(&mut c, &p.base, issue(IssueChange::NewlyAffected)),
        set(&["/new1", "/ok"])
    );
    assert_eq!(
        paths(&mut c, &p.base, issue(IssueChange::Fixed)),
        set(&["/a", "/c"])
    );
    assert_eq!(paths(&mut c, &p.base, issue(IssueChange::Gone)), set(&["/gone"]));

    // For every rule: previous - fixed - gone + newly affected = current.
    for i in s.issues.iter().filter(|i| i.comparable) {
        assert_eq!(
            i.previous.unwrap() + i.newly_affected,
            i.current.unwrap() + i.fixed + i.gone,
            "{}",
            i.rule
        );
    }
    // Most important first.
    assert!(s.issues.windows(2).all(|w| w[0].priority <= w[1].priority));

    // The report carries the changed issues and renders as a section.
    let report = c.report().unwrap();
    assert!(report.issues_available);
    assert!(report.issues.iter().any(|i| i.rule == "title.missing"));
    let section = report.section();
    assert_eq!(section.id, "comparison");
    assert!(section.blocks.iter().any(|b| matches!(
        b,
        spiderlens_results::report::Block::Table { columns, .. } if columns[0] == "Priority"
    )));
    drop(c);

    // The app's settings with the rule switched off: it is not compared.
    let app = IssueSettings {
        rules: [(
            "title.missing".to_string(),
            RuleOverride {
                enabled: Some(false),
                ..RuleOverride::default()
            },
        )]
        .into(),
    };
    let off = CompareSettings {
        issue_settings: IssueSettingsChoice::App,
        ..CompareSettings::default()
    };
    let c = Comparison::run(
        &p.prev,
        &p.cur,
        off,
        Some(&app),
        &AtomicBool::new(false),
        &mut |_| {},
    )
    .unwrap();
    assert!(c.summary().issue_basis.custom);
    assert!(c.summary().issues.iter().all(|i| i.rule != "title.missing"));
    drop(c);
    assert_eq!(
        (fingerprint(&p.prev), fingerprint(&p.cur)),
        before,
        "neither file is written"
    );
}
