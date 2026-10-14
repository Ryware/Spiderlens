//! The analysis on a real crawl of a small AI fixture site (spec 015 US1,
//! US3, US4, US6; SC-006).

use std::path::PathBuf;
use std::time::Duration;

use rusqlite::Connection;
use spiderlens_aeo::analyze::{self, AnalyzeOptions, Progress};
use spiderlens_aeo::score::Outcome;
use spiderlens_config::{CrawlConfig, ModeInput, SpeedSettings};
use spiderlens_engine::crawl::{Control, Crawler};
use spiderlens_test_fixtures::{Site, SiteBuilder};
use tokio::sync::watch;

const E: &str = "example.test";

const ARTICLE: &str = r#"<!doctype html><html lang="en"><head><title>How long does shipping take</title>
<meta name="description" content="Shipping times for every region, updated each month.">
<script type="application/ld+json">{"@context":"https://schema.org","@type":"BlogPosting",
"headline":"How long does shipping take","datePublished":"2026-09-01","dateModified":"2026-10-01",
"image":"https://example.test/ship.png","author":{"@type":"Person","name":"Dana Levi","url":"https://example.test/about",
"sameAs":["https://social.test/dana"]}}</script></head><body><main>
<h1>How long does shipping take</h1><p class="byline">By Dana Levi</p>
<p>Updated <time datetime="2026-10-01">1 October 2026</time></p>
<h2>How long does shipping take?</h2><p>Most orders arrive in 3 to 5 days. Detail follows here.</p>
<p>In 2025, 92% of orders arrived on time, according to <a href="https://stats.other.test/report">the carrier report</a>.
We ship from two warehouses and every parcel is tracked from the moment it leaves the shelf until it reaches you.</p>
<p>Express orders placed before noon leave the same day. Standard orders leave the next working day, and weekends
do not count as working days for any of our carriers in any region we serve today.</p>
<ul><li>Domestic: 3 to 5 days</li><li>Europe: 5 to 8 days</li></ul>
<p>Returns are free within 30 days and the label is in the box. Refunds reach your card within a week of the return
arriving at the warehouse, and we email you at each step so you always know where things stand with your order.</p>
</main><a href="/about">About</a> <a href="/contact">Contact</a></body></html>"#;

const SHELL: &str = r#"<!doctype html><html><head><title>App</title></head><body><div id="root"></div>
<script src="/a.js"></script><script src="/b.js"></script><script src="/c.js"></script>
<script>window.__STATE__ = "........................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................";</script>
</body></html>"#;

fn site() -> Site {
    let home = r#"<!doctype html><html><head><title>Example shop</title>
<script type="application/ld+json">{"@context":"https://schema.org","@type":"Organization","name":"Example",
"url":"https://example.test/","logo":"https://example.test/logo.png","sameAs":["https://social.test/example"]}</script>
</head><body><h1>Example shop</h1><a href="/blog/shipping">Shipping</a> <a href="/app">App</a>
<a href="/private/report">Report</a> <a href="/about">About</a> <a href="/contact">Contact</a></body></html>"#;
    SiteBuilder::new()
        .robots(
            E,
            "Content-Signal: search=yes, ai-train=no\nUser-agent: GPTBot\nDisallow: /\n\nUser-agent: PerplexityBot\nDisallow: /private/\n\nUser-agent: *\nAllow: /\n",
        )
        .raw(
            E,
            "/llms.txt",
            200,
            "text/plain; charset=utf-8",
            b"# Example\n\n> A small shop.\n\n## Guides\n\n- [Shipping](/blog/shipping): how long it takes\n",
        )
        .html(E, "/", home)
        .html(E, "/blog/shipping", ARTICLE)
        .html(E, "/app", SHELL)
        .page(E, "/private/report", "<h1>Report</h1>", &[])
        .page(E, "/about", "<h1>About us</h1>", &[])
        .page(E, "/contact", "<h1>Contact</h1>", &[])
        .start()
}

struct Crawled {
    _dir: tempfile::TempDir,
    path: PathBuf,
    _site: Site,
}

async fn crawl() -> Crawled {
    crawl_with(false).await
}

/// `hook`: read content facts during the crawl, as the CLI and app do.
async fn crawl_with(hook: bool) -> Crawled {
    let site = site();
    let mut c = CrawlConfig {
        speed: SpeedSettings::FAST,
        ..CrawlConfig::default()
    };
    c.extraction.store_html = true;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ai.spiderlens");
    let input = ModeInput::Spider {
        start_url: site.url(E, "/"),
    };
    let mut crawler = Crawler::create(&path, c, &input)
        .unwrap()
        .with_resolve(site.resolve())
        .with_retry_base(Duration::from_millis(10));
    if hook {
        crawler.set_page_hook(spiderlens_aeo::content::page_hook());
    }
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

fn page_row(conn: &Connection, path: &str) -> (String, i64, i64, i64) {
    conn.query_row(
        "SELECT a.page_type, a.score, a.flags, a.blocked_bots FROM aeo_pages a JOIN urls u ON u.id = a.url_id
         WHERE u.url LIKE ?1",
        [format!("%{E}%{path}")],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    )
    .unwrap()
}

fn findings(conn: &Connection, path: &str) -> Vec<String> {
    let mut st = conn
        .prepare(
            "SELECT f.rule_id FROM aeo_findings f JOIN urls u ON u.id = f.url_id
             WHERE u.url LIKE ?1 AND f.outcome = 'fail' ORDER BY f.rule_id",
        )
        .unwrap();
    st.query_map([format!("%{E}%{path}")], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

fn bit(token: &str) -> i64 {
    1 << spiderlens_config::ai_bots::all()
        .iter()
        .find(|b| b.token == token)
        .unwrap()
        .index
}

#[tokio::test(flavor = "multi_thread")]
async fn analysis_of_a_crawled_site() {
    let c = crawl().await;
    // Before the run, the AI search tab says why it is empty.
    {
        let mut r = spiderlens_results::Reader::open(&c.path).unwrap();
        let counts = r.counts("ai-search", spiderlens_results::UrlScope::All).unwrap();
        assert_eq!(counts.counts["search_blocked"], None);
        assert!(counts.unavailable["search_blocked"].contains("AI checks have not run"));
    }
    let seen = std::sync::Mutex::new(Vec::new());
    let opts = AnalyzeOptions {
        workers: Some(2),
        ..Default::default()
    };
    let s = analyze::analyze(&c.path, &opts, &|p| seen.lock().unwrap().push(p)).unwrap();
    let seen = seen.into_inner().unwrap();
    assert!(matches!(seen.first(), Some(Progress::Hosts { .. })));
    assert_eq!(seen.last(), Some(&Progress::Site));

    assert_eq!(s.pages_analyzed, 6);
    let host = &s.hosts[0];
    assert_eq!(host.robots, "ok");
    assert_eq!(host.signals.len(), 1);
    let gpt = host.access.iter().find(|a| a.bot == "gptbot").unwrap();
    assert_eq!(gpt.urls_blocked, gpt.urls_total);
    let ppx = host.access.iter().find(|a| a.bot == "perplexitybot").unwrap();
    assert_eq!(ppx.urls_blocked, 1);
    assert_eq!(ppx.rules[0].line_no, 6);

    let conn = Connection::open(&c.path).unwrap();
    // US1: training-only block everywhere; PerplexityBot blocked on /private/.
    let (_, _, flags, blocked) = page_row(&conn, "/private/report");
    assert_eq!(blocked, bit("gptbot") | bit("perplexitybot"));
    assert_ne!(flags & 1, 0, "search crawler blocked");
    let (_, _, flags, blocked) = page_row(&conn, "/blog/shipping");
    assert_eq!(blocked, bit("gptbot"));
    assert_ne!(flags & 2, 0, "training only");
    assert_eq!(flags & 1, 0);

    // A good article: classified, scored high, no failures beyond training-only.
    let (ty, score, _, _) = page_row(&conn, "/blog/shipping");
    assert_eq!(ty, "article");
    assert_eq!(
        findings(&conn, "/blog/shipping"),
        ["ai.access.training_only_blocked"]
    );
    assert_eq!(score, 100);

    // US3: the application shell is suspected of needing JavaScript.
    let (_, _, flags, _) = page_row(&conn, "/app");
    assert_ne!(flags & (1 << 5), 0);
    assert!(findings(&conn, "/app").contains(&"ai.readable.js_suspected".to_string()));

    // Site: entity from the home page, about and contact found, a valid llms.txt.
    assert_eq!(s.entity.as_ref().and_then(|e| e.name.as_deref()), Some("Example"));
    assert!(s.about_url.as_deref().unwrap().ends_with("/about"));
    let outcome = |id: &str| s.site_outcomes.iter().find(|o| o.rule_id == id).unwrap().outcome;
    assert_eq!(outcome("ai.site.entity"), Outcome::Pass);
    assert_eq!(outcome("ai.site.about_contact"), Outcome::Pass);
    assert_eq!(outcome("ai.site.search_access"), Outcome::Pass);
    assert_eq!(outcome("ai.site.llms_txt"), Outcome::Pass);
    let llms = host.llms_txt.as_ref().unwrap();
    assert_eq!(llms.outcome, "ok");
    assert_eq!(llms.report.as_ref().unwrap().links.len(), 1);
    // The other two site files were asked for once and are missing.
    let conn2 = Connection::open(&c.path).unwrap();
    let outcomes: Vec<(String, String)> = conn2
        .prepare("SELECT path, outcome FROM site_files ORDER BY path")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        outcomes,
        [
            ("/.well-known/tdmrep.json".to_string(), "not_found".to_string()),
            ("/llms-full.txt".into(), "not_found".into()),
            ("/llms.txt".into(), "ok".into()),
        ]
    );
    drop(conn2);
    assert!(s.score > 0 && s.score <= 100);

    // The stored summary reads back equal.
    assert_eq!(analyze::summary(&conn).unwrap().as_ref(), Some(&s));
    drop(conn);

    // The AI search tab's filters count the stored flags (FR-041).
    {
        let mut r = spiderlens_results::Reader::open(&c.path).unwrap();
        let counts = r.counts("ai-search", spiderlens_results::UrlScope::All).unwrap();
        assert_eq!(counts.counts["all"], Some(6));
        assert_eq!(counts.counts["search_blocked"], Some(1));
        assert_eq!(counts.counts["training_only_blocked"], Some(5));
        assert_eq!(counts.counts["js_suspected"], Some(1));
        let q = spiderlens_results::ViewQuery {
            tab: "ai-search".into(),
            filter: "search_blocked".into(),
            scope: spiderlens_results::UrlScope::All,
            search: String::new(),
            sort: None,
            segment: None,
        };
        let page = r.open_view(&q, 10).unwrap();
        r.materialize(page.view).unwrap();
        let rows = r.rows(page.view, 0, 10).unwrap().rows;
        assert_eq!(rows.len(), 1);
        let row = serde_json::to_value(&rows[0]).unwrap().to_string();
        assert!(row.contains("PerplexityBot"), "{row}");
    }

    // SC-006: a second run gives the same rows.
    let dump = |p: &PathBuf| -> Vec<String> {
        let conn = Connection::open(p).unwrap();
        let mut st = conn.prepare("SELECT * FROM aeo_pages ORDER BY url_id").unwrap();
        let n = st.column_count();
        st.query_map([], |r| {
            Ok((0..n)
                .map(|i| format!("{:?}", r.get_ref(i).unwrap()))
                .collect::<Vec<_>>()
                .join("|"))
        })
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
    };
    let first = dump(&c.path);
    let again = analyze::analyze(&c.path, &AnalyzeOptions::default(), &|_| {}).unwrap();
    assert_eq!(again.score, s.score);
    assert_eq!(dump(&c.path), first);
}

/// Every row the analysis wrote for pages, in a fixed order.
fn results(conn: &Connection) -> Vec<Vec<rusqlite::types::Value>> {
    let mut out = Vec::new();
    for sql in [
        "SELECT * FROM aeo_pages ORDER BY url_id",
        "SELECT * FROM aeo_findings ORDER BY url_id, rule_id",
    ] {
        let mut st = conn.prepare(sql).unwrap();
        let n = st.column_count();
        let rows = st
            .query_map([], |r| {
                (0..n).map(|i| r.get::<_, rusqlite::types::Value>(i)).collect()
            })
            .unwrap()
            .collect::<Result<Vec<Vec<_>>, _>>()
            .unwrap();
        out.extend(rows);
    }
    out
}

#[tokio::test(flavor = "multi_thread")]
async fn facts_read_during_the_crawl_give_the_same_results() {
    let c = crawl_with(true).await;
    let conn = Connection::open(&c.path).unwrap();
    let stored: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM page_facts WHERE kind = 'aeo_content'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(stored >= 6, "{stored} pages with facts");

    analyze::analyze(&c.path, &AnalyzeOptions::default(), &|_: Progress| {}).unwrap();
    let from_facts = results(&conn);
    assert!(!from_facts.is_empty());

    // Without the facts the pass parses the stored HTML, as for a file
    // crawled by an older build.
    conn.execute("DELETE FROM page_facts", []).unwrap();
    analyze::analyze(&c.path, &AnalyzeOptions::default(), &|_: Progress| {}).unwrap();
    assert_eq!(results(&conn), from_facts);
}
