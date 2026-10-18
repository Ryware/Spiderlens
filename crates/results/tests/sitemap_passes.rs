//! Spec 011 part b: the `sitemap_audit` and `orphans` Crawl Analysis passes
//! and the rules that read them (tasks T029, T030; spec.md US2 scenarios 1
//! to 6 and 8, US3 scenarios 2 to 7, US4 scenarios 1 to 5), on a real crawl
//! of a fixture site.
//!
//! The site (`sm.test`):
//! - `/` links to most pages; `/` is the start URL and is in a sitemap;
//! - `/orphan`, `/selfonly` (links only to itself), `/other-robots-url` and
//!   `/outside-user` are in a sitemap and linked from nowhere;
//! - `/nf` is in a sitemap and linked only with `rel="nofollow"`;
//! - `/via-redirect` is in a sitemap and reached only through a redirect
//!   from the linked `/redir-src`;
//! - `/dup` is listed twice, `/fut` has a `lastmod` in the future, `/old` has
//!   a `lastmod` years before its `Last-Modified` header, and `/s1` to `/s3`
//!   are the whole of a sitemap whose entries all share one `lastmod`;
//! - `/sub/robots-map.xml` (listed in robots.txt) lists a URL outside its
//!   folder, which is allowed; `/sub/user-map.xml` (added by the user) does
//!   the same, which is not.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use rusqlite::Connection;
use spiderlens_config::{
    CrawlConfig, IssueSettings, ListInput, Mode, ModeInput, RuleOverride, SpeedSettings,
};
use spiderlens_engine::crawl::{Control, Crawler};
use spiderlens_results::Reader;
use spiderlens_results::analysis::analyse_file;
use spiderlens_test_fixtures::{Site, SiteBuilder};
use tokio::sync::watch;

const S: &str = "sm.test";

fn page(links: &[&str]) -> String {
    let body: String = links
        .iter()
        .map(|l| match l.strip_prefix("nofollow:") {
            Some(href) => format!("<a href=\"{href}\" rel=\"nofollow\">{href}</a>"),
            None => format!("<a href=\"{l}\">{l}</a>"),
        })
        .collect();
    format!("<!doctype html><html lang=\"en\"><head><title>Page</title></head><body>{body}</body></html>")
}

fn urlset(entries: &[(&str, Option<&str>)]) -> String {
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?><urlset>");
    for (path, lastmod) in entries {
        out.push_str(&format!("<url><loc>{{base:{S}}}{path}</loc>"));
        if let Some(m) = lastmod {
            out.push_str(&format!("<lastmod>{m}</lastmod>"));
        }
        out.push_str("</url>");
    }
    out.push_str("</urlset>");
    out
}

fn site() -> Site {
    let main = urlset(&[
        ("/", None),
        ("/linked", None),
        ("/orphan", None),
        ("/selfonly", None),
        ("/nf", None),
        ("/via-redirect", None),
        ("/dup", None),
        ("/dup", None),
        ("/fut", Some("2099-01-01")),
        ("/old", Some("2020-01-01")),
    ]);
    let same = urlset(&[
        ("/s1", Some("2020-05-05")),
        ("/s2", Some("2020-05-05")),
        ("/s3", Some("2020-05-05")),
    ]);
    let robots_map = urlset(&[("/other-robots-url", None)]);
    let user_map = urlset(&[("/outside-user", None)]);
    SiteBuilder::new()
        .robots(
            S,
            "User-agent: *\nAllow: /\n\
             Sitemap: {base:sm.test}/sitemap.xml\n\
             Sitemap: {base:sm.test}/same.xml\n\
             Sitemap: {base:sm.test}/sub/robots-map.xml\n",
        )
        .html(
            S,
            "/",
            &page(&[
                "/linked",
                "/redir-src",
                "nofollow:/nf",
                "/fut",
                "/old",
                "/s1",
                "/s2",
                "/s3",
                "/dup",
            ]),
        )
        .html(S, "/linked", &page(&["/linked", "/"]))
        .html(S, "/orphan", &page(&[]))
        .html(S, "/selfonly", &page(&["/selfonly"]))
        .html(S, "/nf", &page(&[]))
        .redirect(S, "/redir-src", 301, "/via-redirect")
        .html(S, "/via-redirect", &page(&[]))
        .html(S, "/dup", &page(&[]))
        .html(S, "/fut", &page(&[]))
        .html(S, "/old", &page(&[]))
        .header(S, "/old", "Last-Modified", "Wed, 01 Jan 2025 00:00:00 GMT")
        .html(S, "/s1", &page(&[]))
        .html(S, "/s2", &page(&[]))
        .html(S, "/s3", &page(&[]))
        .html(S, "/other-robots-url", &page(&[]))
        .html(S, "/outside-user", &page(&[]))
        .raw(S, "/sitemap.xml", 200, "application/xml", main.as_bytes())
        .raw(S, "/same.xml", 200, "application/xml", same.as_bytes())
        .raw(
            S,
            "/sub/robots-map.xml",
            200,
            "application/xml",
            robots_map.as_bytes(),
        )
        .raw(
            S,
            "/sub/user-map.xml",
            200,
            "application/xml",
            user_map.as_bytes(),
        )
        .start()
}

struct Crawled {
    _dir: tempfile::TempDir,
    path: PathBuf,
    site: Site,
}

impl Crawled {
    /// The path part of a stored URL, `/orphan`.
    fn strip(&self, url: &str) -> String {
        let base = self.site.url(S, "/");
        url.strip_prefix(base.trim_end_matches('/'))
            .unwrap_or(url)
            .to_string()
    }

    fn conn(&self) -> Connection {
        Connection::open(&self.path).unwrap()
    }

    fn url_id(&self, path: &str) -> i64 {
        self.conn()
            .query_row(
                "SELECT id FROM urls WHERE url LIKE ?1 ORDER BY length(url) LIMIT 1",
                [format!("%{path}")],
                |r| r.get(0),
            )
            .unwrap()
    }

    /// Runs the passes and saves their output in the file.
    fn analyse(&self) {
        let never = AtomicBool::new(false);
        let report = analyse_file(&self.path, &mut |_| {}, &never).unwrap();
        assert!(report.saved);
        let ran: BTreeSet<&str> = report.ran.iter().map(|(id, _)| id.as_str()).collect();
        assert!(
            ran.contains("orphans") && ran.contains("sitemap_audit"),
            "{report:?}"
        );
        assert!(report.skipped.is_empty(), "{report:?}");
    }

    /// The URLs a `sitemap_problems` rule recorded, as paths.
    fn problem_paths(&self, rule: &str) -> BTreeSet<String> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare("SELECT u.url FROM sitemap_problems p JOIN urls u ON u.id = p.url_id WHERE p.rule = ?1")
            .unwrap();
        let paths = stmt
            .query_map([rule], |r| r.get::<_, String>(0))
            .unwrap()
            .map(|u| self.strip(&u.unwrap()))
            .collect();
        paths
    }
}

async fn run(path: &Path, site: &Site, config: CrawlConfig, input: ModeInput) {
    let crawler = Crawler::create(path, config, &input)
        .unwrap()
        .with_resolve(site.resolve())
        .with_retry_base(Duration::from_millis(10));
    let (_tx, rx) = watch::channel(Control::Run);
    tokio::time::timeout(Duration::from_secs(60), crawler.run(rx, None))
        .await
        .expect("crawl finishes")
        .unwrap();
}

async fn crawl() -> Crawled {
    let site = site();
    let mut config = CrawlConfig {
        speed: SpeedSettings::FAST,
        ..CrawlConfig::default()
    };
    config.scope.sitemaps.urls = vec![site.url(S, "/sub/user-map.xml")];
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sm.spiderlens");
    let input = ModeInput::Spider {
        start_url: site.url(S, "/"),
    };
    run(&path, &site, config, input).await;
    Crawled {
        _dir: dir,
        path,
        site,
    }
}

fn set(paths: &[&str]) -> BTreeSet<String> {
    paths.iter().map(|s| s.to_string()).collect()
}

fn rows(c: &Crawled, r: &Reader, nofollow_only: bool) -> BTreeSet<String> {
    r.orphan_rows(nofollow_only)
        .unwrap()
        .iter()
        .map(|o| c.strip(&o.url))
        .collect()
}

fn count(r: &mut Reader, id: &str) -> Option<u64> {
    r.issues_summary(false)
        .unwrap()
        .issues
        .into_iter()
        .find(|i| i.id == id)
        .unwrap_or_else(|| panic!("no rule {id}"))
        .count
}

/// The pass-backed rules that flag `path`.
fn flagged(c: &Crawled, r: &mut Reader, path: &str) -> BTreeSet<String> {
    let id = c.url_id(path);
    r.url_issues(id)
        .unwrap()
        .into_iter()
        .map(|i| i.id)
        .filter(|i| PASS_RULES.contains(&i.as_str()))
        .collect()
}

const PASS_RULES: &[&str] = &[
    "sitemap.orphan",
    "sitemap.nofollow_only",
    "sitemap.duplicate",
    "sitemap.outside_location",
    "sitemap.lastmod_future",
    "sitemap.lastmod_old",
    "sitemap.lastmod_identical",
];

#[tokio::test(flavor = "multi_thread")]
async fn us2_orphans_follow_links_redirects_and_nofollow() {
    let c = crawl().await;
    // Spec 024 candidates add a source to a URL the sitemap also lists.
    let orphan_url: String = c
        .conn()
        .query_row("SELECT url FROM urls WHERE url LIKE '%/orphan'", [], |r| r.get(0))
        .unwrap();
    c.conn()
        .execute(
            "INSERT INTO orphan_candidates (url, source, run_id) VALUES (?1, 'search_console', 1)",
            [&orphan_url],
        )
        .unwrap();

    // Before the pass has run the view says so and lists nothing.
    let before = Reader::open(&c.path).unwrap();
    let s = before.orphan_summary().unwrap();
    assert!(!s.available && !s.list_mode && s.reason.is_some());
    assert!(before.orphan_rows(false).unwrap().is_empty());
    drop(before);

    c.analyse();
    let r = Reader::open(&c.path).unwrap();
    // Scenario 1 and 5: no inlinks, or only a self link, is an orphan.
    // Scenario 2: a followed anchor, a redirect from a linked URL and the
    // start URL are not.
    assert_eq!(
        rows(&c, &r, false),
        set(&["/orphan", "/selfonly", "/other-robots-url", "/outside-user"])
    );
    // Scenario 3: only nofollow links.
    assert_eq!(rows(&c, &r, true), set(&["/nf"]));

    let s = r.orphan_summary().unwrap();
    assert!(s.available && !s.list_mode && s.reason.is_none());
    assert_eq!((s.orphans, s.nofollow_only), (4, 1));
    assert!(!s.provisional && !s.incomplete, "{s:?}");

    let all = r.orphan_rows(false).unwrap();
    let o = all.iter().find(|o| o.url == orphan_url).unwrap();
    assert_eq!(o.class, "orphan");
    let mut sources = o.sources.clone();
    sources.sort();
    assert_eq!(sources, ["search_console", "sitemap"]);
    assert_eq!(o.status, Some(200));
    assert!(o.indexability.is_some());
    assert!(o.sitemap.as_deref().is_some_and(|f| f.ends_with("/sitemap.xml")));
    let other = all.iter().find(|o| o.url.ends_with("/selfonly")).unwrap();
    assert_eq!(other.sources, ["sitemap"]);
    drop(r);

    // Scenario 8: URLs still queued mean the crawl did not see every link.
    c.conn()
        .execute("UPDATE urls SET state = 'queued' WHERE url LIKE '%/s3'", [])
        .unwrap();
    let r = Reader::open(&c.path).unwrap();
    let s = r.orphan_summary().unwrap();
    assert!(s.incomplete, "{s:?}");
    assert_eq!(s.not_crawled, 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn us2_list_mode_has_no_orphans_and_says_why() {
    let site = site();
    let config = CrawlConfig {
        mode: Mode::List,
        speed: SpeedSettings::FAST,
        ..CrawlConfig::default()
    };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("list.spiderlens");
    let input = ModeInput::List(ListInput {
        urls: vec![site.url(S, "/orphan"), site.url(S, "/linked")],
        ..ListInput::default()
    });
    run(&path, &site, config, input).await;
    let never = AtomicBool::new(false);
    analyse_file(&path, &mut |_| {}, &never).unwrap();

    let r = Reader::open(&path).unwrap();
    let s = r.orphan_summary().unwrap();
    assert!(s.list_mode && !s.available);
    assert!(
        s.reason.as_deref().is_some_and(|m| m.contains("Spider mode")),
        "{s:?}"
    );
    assert_eq!((s.orphans, s.nofollow_only), (0, 0));
    assert!(r.orphan_rows(false).unwrap().is_empty());
    assert!(r.orphan_rows(true).unwrap().is_empty());
    let conn = Connection::open(&path).unwrap();
    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM orphans", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn us3_us4_audit_records_what_needs_the_whole_crawl() {
    let c = crawl().await;
    c.analyse();
    // US3 scenario 6: the URL listed twice.
    assert_eq!(c.problem_paths("sitemap.duplicate"), set(&["/dup"]));
    let n: i64 = c
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM sitemap_problems WHERE rule = 'sitemap.duplicate'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 2, "one problem for each entry");
    // US3 scenario 5: outside the file's folder, unless robots.txt of the
    // start host listed the file.
    assert_eq!(
        c.problem_paths("sitemap.outside_location"),
        set(&["/outside-user"])
    );
    // US4 scenario 2: a date after the crawl started.
    assert_eq!(c.problem_paths("sitemap.lastmod_future"), set(&["/fut"]));
    // US4 scenario 4: older than the page's Last-Modified.
    assert_eq!(c.problem_paths("sitemap.lastmod_old"), set(&["/old"]));
    let days: String = c
        .conn()
        .query_row(
            "SELECT detail FROM sitemap_problems WHERE rule = 'sitemap.lastmod_old'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(days.parse::<i64>().unwrap() > 1000, "{days}");
    // US4 scenario 3: one file whose entries share one lastmod.
    let identical: Vec<(String, String)> = {
        let conn = c.conn();
        let mut stmt = conn
            .prepare(
                "SELECT f.location, p.detail FROM sitemap_problems p JOIN sitemap_files f ON f.id = p.file_id
                 WHERE p.rule = 'sitemap.lastmod_identical'",
            )
            .unwrap();
        let found = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        found
    };
    assert_eq!(identical.len(), 1, "{identical:?}");
    assert!(identical[0].0.ends_with("/same.xml"));
    assert_eq!(identical[0].1, "3");

    // Running again changes nothing: the pass replaces its rows.
    c.conn()
        .execute("DELETE FROM meta WHERE key = 'analysis'", [])
        .unwrap();
    c.analyse();
    let n: i64 = c
        .conn()
        .query_row("SELECT COUNT(*) FROM sitemap_problems", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 2 + 1 + 1 + 1 + 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn each_pass_rule_flags_its_url_and_not_the_others() {
    let c = crawl().await;
    // Until the passes ran the rules cannot say anything.
    let mut r = Reader::open(&c.path).unwrap();
    for id in PASS_RULES {
        assert_eq!(count(&mut r, id), None, "{id}");
    }
    drop(r);

    c.analyse();
    let mut r = Reader::open(&c.path).unwrap();
    assert_eq!(count(&mut r, "sitemap.orphan"), Some(4));
    assert_eq!(count(&mut r, "sitemap.nofollow_only"), Some(1));
    assert_eq!(count(&mut r, "sitemap.duplicate"), Some(1));
    assert_eq!(count(&mut r, "sitemap.outside_location"), Some(1));
    assert_eq!(count(&mut r, "sitemap.lastmod_future"), Some(1));
    assert_eq!(count(&mut r, "sitemap.lastmod_old"), Some(1));
    // The default threshold is 100 entries: three is far below it.
    assert_eq!(count(&mut r, "sitemap.lastmod_identical"), Some(0));

    // Positive and negative case for each rule.
    assert_eq!(flagged(&c, &mut r, "/orphan"), set(&["sitemap.orphan"]));
    assert_eq!(flagged(&c, &mut r, "/nf"), set(&["sitemap.nofollow_only"]));
    assert_eq!(flagged(&c, &mut r, "/dup"), set(&["sitemap.duplicate"]));
    assert_eq!(
        flagged(&c, &mut r, "/outside-user"),
        set(&["sitemap.orphan", "sitemap.outside_location"])
    );
    assert_eq!(flagged(&c, &mut r, "/fut"), set(&["sitemap.lastmod_future"]));
    assert_eq!(flagged(&c, &mut r, "/old"), set(&["sitemap.lastmod_old"]));
    // Linked, redirected to, listed once, inside its folder, dated sanely.
    assert_eq!(flagged(&c, &mut r, "/linked"), set(&[]));
    assert_eq!(flagged(&c, &mut r, "/via-redirect"), set(&[]));
    assert_eq!(flagged(&c, &mut r, "/other-robots-url"), set(&["sitemap.orphan"]));
    assert_eq!(flagged(&c, &mut r, "/s1"), set(&[]));
    drop(r);

    // US4 scenario 3: with the entry threshold lowered, the file's three
    // URLs are flagged by the rule, and nothing else is.
    let mut s = IssueSettings::default();
    s.rules.insert(
        "sitemap.lastmod_identical".into(),
        RuleOverride {
            threshold: Some(3.0),
            ..Default::default()
        },
    );
    let mut r = Reader::open(&c.path).unwrap();
    assert!(r.set_issue_settings(&s).is_empty());
    assert_eq!(count(&mut r, "sitemap.lastmod_identical"), Some(3));
    assert_eq!(flagged(&c, &mut r, "/s1"), set(&["sitemap.lastmod_identical"]));
    assert_eq!(flagged(&c, &mut r, "/linked"), set(&[]));
}

#[tokio::test(flavor = "multi_thread")]
async fn passes_on_a_read_only_file_run_into_temp() {
    let c = crawl().await;
    let mut r = Reader::open(&c.path).unwrap();
    assert!(!r.orphan_summary().unwrap().available);
    assert_eq!(count(&mut r, "sitemap.orphan"), None);

    let never = AtomicBool::new(false);
    let report = r.analyse_in_session(&mut |_| {}, &never).unwrap();
    assert!(!report.saved);
    let ran: BTreeSet<&str> = report.ran.iter().map(|(id, _)| id.as_str()).collect();
    assert!(
        ran.contains("orphans") && ran.contains("sitemap_audit"),
        "{report:?}"
    );

    // The results show in this session ...
    let s = r.orphan_summary().unwrap();
    assert!(s.available);
    assert_eq!((s.orphans, s.nofollow_only), (4, 1));
    assert_eq!(rows(&c, &r, true), set(&["/nf"]));
    assert_eq!(count(&mut r, "sitemap.orphan"), Some(4));
    assert_eq!(count(&mut r, "sitemap.lastmod_future"), Some(1));
    drop(r);

    // ... and the file itself was not written.
    let conn = c.conn();
    for table in ["orphans", "sitemap_problems"] {
        let n: i64 = conn
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0, "{table}");
    }
    let analysis: Option<String> = conn
        .query_row("SELECT value FROM meta WHERE key = 'analysis'", [], |r| r.get(0))
        .ok();
    assert!(
        analysis.is_none_or(|a| !a.contains("orphans") && !a.contains("sitemap_audit")),
        "analysis was recorded"
    );
    let fresh = Reader::open(&c.path).unwrap();
    assert!(!fresh.orphan_summary().unwrap().available);
}
