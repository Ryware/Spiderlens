//! Spec 010 acceptance tests for the report and the health score, on a
//! real crawl of a fixture site with known problems (T004 to T010).

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use spiderlens_config::{CrawlConfig, IssueSettings, ModeInput, Priority, RuleOverride, SpeedSettings};
use spiderlens_engine::crawl::{Control, Crawler};
use spiderlens_results::report::html::FileOptions;
use spiderlens_results::report::{Block, ReportOptions, SectionState, Target, weight};
use spiderlens_results::{Reader, UrlScope, ViewQuery};
use spiderlens_test_fixtures::{Site, report};
use tokio::sync::watch;

const E: &str = report::HOST;
use report::site;

struct Crawled {
    _dir: tempfile::TempDir,
    path: PathBuf,
    _site: Site,
}

async fn crawl() -> Crawled {
    let site = site();
    let c = CrawlConfig {
        speed: SpeedSettings::FAST,
        ..CrawlConfig::default()
    };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("report.spiderlens");
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
        _dir: dir,
        path,
        _site: site,
    }
}

fn q(tab: &str, filter: &str, scope: UrlScope) -> ViewQuery {
    ViewQuery {
        tab: tab.into(),
        filter: filter.into(),
        scope,
        search: String::new(),
        sort: None,
        segment: None,
        values: None,
    }
}

/// Row ids a view lists.
fn ids(r: &mut Reader, query: &ViewQuery) -> Vec<i64> {
    let page = r.open_view(query, 500).unwrap();
    let total = r.materialize(page.view).unwrap();
    let rows = r.rows(page.view, 0, total.max(1)).unwrap().rows;
    r.close_view(page.view).unwrap();
    rows.iter().map(|row| row[0].as_i64().unwrap()).collect()
}

fn parse(v: &str) -> u64 {
    v.replace(',', "").parse().unwrap()
}

/// The score computed the slow way: each page's issues from the detail
/// pane's list, the worst one's weight.
fn slow_score(r: &mut Reader) -> (u64, f64) {
    let pages = ids(r, &q("page-titles", "all", UrlScope::All));
    let mut lost = 0.0;
    for id in &pages {
        let worst = r.url_issues(*id).unwrap().iter().map(|i| i.priority).min();
        lost += worst.map_or(0.0, weight);
    }
    (pages.len() as u64, lost * 100.0 / pages.len() as f64)
}

#[tokio::test]
async fn us1_every_figure_matches_the_table_and_links_to_it() {
    let c = crawl().await;
    let mut r = Reader::open(&c.path).unwrap();
    let report = r.report(&ReportOptions::default()).unwrap();
    assert!(!report.provisional);
    let ids_of: Vec<_> = report.sections.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(
        ids_of,
        [
            "summary",
            "completeness",
            "top_issues",
            "issues",
            "response_codes",
            "structure",
            "content"
        ]
    );
    for s in &report.sections {
        assert_eq!(s.state, SectionState::Available, "{}", s.id);
    }
    // FR-004, FR-005: every count with a target equals the rows it opens.
    let mut checked = 0;
    let mut check = |r: &mut Reader, value: &str, target: &Target| {
        let Ok(n) = value.replace(',', "").parse::<u64>() else {
            return;
        };
        let rows = match target {
            Target::Tab { tab, filter, scope } => ids(r, &q(tab, filter, *scope)).len() as u64,
            Target::Issue { id } => ids(r, &q(&format!("issue:{id}"), "all", UrlScope::All)).len() as u64,
        };
        assert_eq!(n, rows, "{target:?}");
        checked += 1;
    };
    for s in &report.sections {
        for b in &s.blocks {
            match b {
                Block::Figures { items } => {
                    for f in items {
                        if let Some(t) = &f.target {
                            check(&mut r, &f.value, t);
                        }
                    }
                }
                Block::Table { rows, .. } => {
                    for row in rows {
                        for cell in row {
                            if let Some(t) = &cell.target
                                && s.id != "completeness"
                            {
                                check(&mut r, &cell.text, t);
                            }
                        }
                    }
                }
                Block::Issue { issue } => {
                    let n =
                        ids(&mut r, &q(&format!("issue:{}", issue.id), "all", UrlScope::All)).len() as u64;
                    assert_eq!(issue.count, Some(n), "{}", issue.id);
                    assert!(!issue.why.is_empty() && !issue.fix.is_empty());
                }
                _ => {}
            }
        }
    }
    assert!(checked >= 15, "checked {checked} figures");

    // Response codes: the fixture's 404, 500 and redirects are counted.
    let rc = report.sections.iter().find(|s| s.id == "response_codes").unwrap();
    let Block::Table { rows, .. } = &rc.blocks[0] else {
        panic!()
    };
    let row = |label: &str| rows.iter().find(|r| r[0].text.starts_with(label)).unwrap();
    assert_eq!(parse(&row("Client error")[1].text), 1);
    assert_eq!(parse(&row("Server error")[1].text), 1);
    assert_eq!(parse(&row("Redirection")[1].text), 2);

    // Structure: depth 3 is the deepest level.
    let st = report.sections.iter().find(|s| s.id == "structure").unwrap();
    let deepest = st
        .blocks
        .iter()
        .find_map(|b| match b {
            Block::Figures { items } => items.iter().find(|f| f.label == "Deepest level"),
            _ => None,
        })
        .unwrap();
    assert_eq!(deepest.value, "3");
}

#[tokio::test]
async fn us2_score_follows_the_formula_and_explains_itself() {
    let c = crawl().await;
    let mut r = Reader::open(&c.path).unwrap();
    let report = r.report(&ReportOptions::default()).unwrap();
    let s = &report.score;
    let (pages, lost) = slow_score(&mut r);
    assert_eq!(s.pages, pages);
    assert_eq!(s.value, Some((100.0 - lost).round() as u8), "SC-008");
    // The breakdown adds up (US2-2).
    let by_prio: f64 = s.by_priority.iter().map(|p| p.points).sum();
    let shares: f64 = s.issues.iter().map(|i| i.share).sum();
    assert!((by_prio - lost).abs() < 1e-6);
    assert!((shares - lost).abs() < 1e-6);
    assert!(s.value.unwrap() < 100, "the fixture has problems");
    // Top issues are ordered by gain.
    assert!(s.issues.windows(2).all(|w| w[0].gain >= w[1].gain));

    // US2-3: turning the costliest issue off raises the score by its gain.
    let top = s.issues[0].clone();
    assert!(top.gain > 0.0);
    let mut settings = IssueSettings::default();
    settings.rules.insert(
        top.id.clone(),
        RuleOverride {
            enabled: Some(false),
            ..RuleOverride::default()
        },
    );
    assert!(r.set_issue_settings(&settings).is_empty());
    let after = r.report(&ReportOptions::default()).unwrap();
    let lost_after: f64 = after.score.by_priority.iter().map(|p| p.points).sum();
    assert!((lost - lost_after - top.gain).abs() < 1e-6);
    // US2-4, US1-5: custom settings are stated.
    assert!(after.meta.custom_issue_settings);
    assert!(!report.meta.custom_issue_settings);

    // Lowering a priority changes the score without recrawling.
    let mut lower = IssueSettings::default();
    for i in &s.issues {
        lower.rules.insert(
            i.id.clone(),
            RuleOverride {
                priority: Some(Priority::Opportunity),
                ..RuleOverride::default()
            },
        );
    }
    r.set_issue_settings(&lower);
    assert_eq!(
        r.report(&ReportOptions::default()).unwrap().score.value,
        Some(100)
    );
}

#[tokio::test]
async fn us1_running_reports_are_provisional() {
    let c = crawl().await;
    let mut r = Reader::open(&c.path).unwrap();
    let report = r
        .report(&ReportOptions {
            running: true,
            profile: None,
            ..Default::default()
        })
        .unwrap();
    assert!(report.provisional);
    assert!(
        report
            .sections
            .iter()
            .all(|s| s.state == SectionState::Provisional)
    );
}

#[tokio::test]
async fn us3_report_file_is_self_contained_and_escaped() {
    let c = crawl().await;
    let mut r = Reader::open(&c.path).unwrap();
    let opts = FileOptions {
        title: "Audit for <Client>".into(),
        note: "Hello & welcome. דוח ללקוח".into(),
        urls_per_issue: 1,
    };
    let html = r.report_html(&ReportOptions::default(), &opts).unwrap();
    // FR-014: nothing loads from anywhere, nothing runs.
    let lower = html.to_lowercase();
    for bad in [
        "<script", " src=\"", " href=\"", "url(", "@import", "<link", "<iframe", "<img",
    ] {
        assert!(!lower.contains(bad), "report contains {bad}");
    }
    // FR-016: title and note, escaped.
    assert!(html.contains("<h1 dir=\"auto\">Audit for &lt;Client&gt;</h1>"));
    assert!(html.contains("Hello &amp; welcome"));
    // FR-017: Hebrew text is there, marked for direction.
    assert!(html.contains("<div class=\"note\" dir=\"auto\">Hello &amp; welcome. דוח ללקוח</div>"));
    // Every section is present.
    let report = r.report(&ReportOptions::default()).unwrap();
    for s in &report.sections {
        assert!(html.contains(&format!("<h2>{}</h2>", s.title)), "{}", s.title);
    }
    assert!(
        html.contains("The full list is in the issues export")
            || report.score.issues.iter().all(|i| i.pages <= 1)
    );

    // US3-3: the limit caps each issue's list; 0 shows none.
    let none = r
        .report_html(
            &ReportOptions::default(),
            &FileOptions {
                urls_per_issue: 0,
                ..FileOptions::default()
            },
        )
        .unwrap();
    assert!(!none.contains("<ol class=\"urls\">"));
    assert!(
        r.report_html(
            &ReportOptions::default(),
            &FileOptions {
                urls_per_issue: 10_001,
                ..FileOptions::default()
            }
        )
        .is_err()
    );

    // US3-5: provisional reports say so.
    let running = r
        .report_html(
            &ReportOptions {
                running: true,
                profile: None,
                ..Default::default()
            },
            &FileOptions::default(),
        )
        .unwrap();
    assert!(running.contains("Provisional."));
}

#[tokio::test]
async fn report_issue_lists_hold_evidence() {
    let c = crawl().await;
    let mut r = Reader::open(&c.path).unwrap();
    let list = r.issue_urls("title.missing", 10).unwrap();
    assert_eq!(list.total, 1);
    assert!(list.urls[0].0.ends_with("/notitle"));
    let none = r.issue_urls("no.such.rule", 10).unwrap();
    assert_eq!(none.total, 0);
    let _ = HashMap::<(), ()>::new();
}
