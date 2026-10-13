//! Spec 003 acceptance scenarios against the multi-host fixture site.
//! Test names carry the scenario id (see specs/003-frontier-scope/quickstart.md).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use spiderlens_config::{
    CrawlConfig, ListInput, Mode, ModeInput, QueryMode, RobotsMode, SpeedSettings, SubdomainMode,
};
use spiderlens_engine::crawl::{
    Control, CrawlEvent, CrawlStore, CrawlSummary, Crawler, DiscoverySource, NotCrawledReason, UrlRow,
    UrlState,
};
use spiderlens_test_fixtures::{Site, SiteBuilder};
use tokio::sync::watch;

const E: &str = "example.test";

fn fast(c: &mut CrawlConfig) {
    c.speed = SpeedSettings::FAST;
}

struct Done {
    _dir: tempfile::TempDir,
    path: PathBuf,
    summary: CrawlSummary,
    rows: HashMap<String, UrlRow>,
}

impl Done {
    fn row(&self, url: &str) -> &UrlRow {
        self.rows.get(url).unwrap_or_else(|| {
            panic!(
                "{url} not recorded; have {:?}",
                self.rows.keys().collect::<Vec<_>>()
            )
        })
    }

    fn reason(&self, url: &str) -> Option<NotCrawledReason> {
        self.row(url).reason
    }

    fn assert_invariant(&self) {
        let s = &self.summary;
        assert_eq!(
            s.crawled + s.not_crawled + s.queued,
            s.discovered,
            "SC-004: {s:?}"
        );
        let by_reason: u64 = s.by_reason.values().sum();
        assert_eq!(by_reason, s.not_crawled, "every not-crawled URL has one reason");
    }
}

fn read_rows(path: &Path) -> HashMap<String, UrlRow> {
    let store = CrawlStore::open(path).unwrap();
    let mut rows = HashMap::new();
    store
        .rows(None, &[], |r| {
            rows.insert(r.url.clone(), r);
            Ok(())
        })
        .unwrap();
    rows
}

async fn run_input(site: &Site, input: ModeInput, mode: Mode, edit: impl FnOnce(&mut CrawlConfig)) -> Done {
    let mut c = CrawlConfig {
        mode,
        ..CrawlConfig::default()
    };
    fast(&mut c);
    edit(&mut c);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.spiderlens");
    let crawler = Crawler::create(&path, c, &input)
        .unwrap()
        .with_resolve(site.resolve())
        .with_retry_base(Duration::from_millis(10));
    let (_tx, rx) = watch::channel(Control::Run);
    let summary = tokio::time::timeout(Duration::from_secs(60), crawler.run(rx, None))
        .await
        .expect("crawl finishes")
        .unwrap();
    let rows = read_rows(&path);
    let done = Done {
        _dir: dir,
        path,
        summary,
        rows,
    };
    done.assert_invariant();
    done
}

async fn crawl(site: &Site, start: &str, edit: impl FnOnce(&mut CrawlConfig)) -> Done {
    let input = ModeInput::Spider {
        start_url: site.url(E, start),
    };
    run_input(site, input, Mode::Spider, edit).await
}

#[tokio::test]
async fn us1_1_spellings_dedupe_and_external_not_followed() {
    let site = SiteBuilder::new()
        .page(
            E,
            "/",
            "",
            &[
                "HTTP://Example.TEST:{port}/a",
                "/x/../a#top",
                "./a",
                "/%61",
                "/b",
                "{base:other.test}/page",
            ],
        )
        .page(E, "/a", "", &["/"])
        .page(E, "/b", "", &[])
        .page("other.test", "/page", "", &["/never"])
        .start();
    let d = crawl(&site, "/", fast).await;
    assert_eq!(site.count(E, "/a"), 1, "fetched exactly once (SC-001)");
    assert_eq!(site.count(E, "/"), 1);
    let a = site.url(E, "/a");
    assert_eq!(d.row(&a).state, UrlState::Crawled);
    // Every spelling is a link to the same normalized URL, raw text kept.
    let store = CrawlStore::open(&d.path).unwrap();
    let raws: Vec<String> = store
        .links_to(&a)
        .unwrap()
        .into_iter()
        .map(|(_, raw, _, _)| raw)
        .collect();
    assert!(
        raws.contains(&"/x/../a#top".to_string()) && raws.contains(&"./a".to_string()),
        "{raws:?}"
    );
    // US1-4: external URL recorded, status-checked once, its links not followed.
    let ext = site.url("other.test", "/page");
    assert_eq!(d.reason(&ext), Some(NotCrawledReason::External));
    assert_eq!(d.row(&ext).status, Some(200));
    assert_eq!(site.count("other.test", "/never"), 0);
    assert_eq!(d.summary.status, "completed");
}

#[tokio::test]
async fn us1_2_depth_is_shortest_click_distance() {
    // / -> /p1 -> /b ; / -> /q1 -> /q2 -> /q3 -> /q4 -> /b
    let site = SiteBuilder::new()
        .page(E, "/", "", &["/q1", "/p1"])
        .page(E, "/q1", "", &["/q2"])
        .page(E, "/q2", "", &["/q3"])
        .page(E, "/q3", "", &["/q4"])
        .page(E, "/q4", "", &["/b"])
        .page(E, "/p1", "", &["/b"])
        .page(E, "/b", "", &[])
        .start();
    let d = crawl(&site, "/", fast).await;
    assert_eq!(d.row(&site.url(E, "/b")).depth, 2);
    assert_eq!(d.row(&site.url(E, "/q4")).depth, 4);
}

#[tokio::test]
async fn us1_3_breadth_first_order() {
    let site = SiteBuilder::new()
        .page(E, "/", "", &["/1a", "/1b"])
        .page(E, "/1a", "", &["/2a", "/2b"])
        .page(E, "/1b", "", &["/2c"])
        .page(E, "/2a", "", &["/3a"])
        .page(E, "/2b", "", &[])
        .page(E, "/2c", "", &[])
        .page(E, "/3a", "", &[])
        .start();
    crawl(&site, "/", |c| {
        c.speed.max_concurrent = 1;
        c.speed.max_per_host = 1;
    })
    .await;
    let order: Vec<String> = site.page_hits().into_iter().map(|h| h.path).collect();
    assert_eq!(order, ["/", "/1a", "/1b", "/2a", "/2b", "/2c", "/3a"]);
}

#[tokio::test]
async fn us1_5_6_www_internal_subdomains_by_setting() {
    let site = SiteBuilder::new()
        .page(
            E,
            "/",
            "",
            &["{base:www.example.test}/about", "{base:blog.example.test}/"],
        )
        .page("www.example.test", "/about", "", &[])
        .page("blog.example.test", "/", "", &["/post"])
        .page("blog.example.test", "/post", "", &[])
        .start();
    let d = crawl(&site, "/", fast).await;
    assert_eq!(
        d.row(&site.url("www.example.test", "/about")).state,
        UrlState::Crawled
    );
    assert_eq!(
        d.reason(&site.url("blog.example.test", "/")),
        Some(NotCrawledReason::External)
    );
    assert_eq!(site.count("blog.example.test", "/post"), 0);

    let d = crawl(&site, "/", |c| {
        fast(c);
        c.scope.subdomains = SubdomainMode::AllSubdomains;
    })
    .await;
    assert_eq!(
        d.row(&site.url("blog.example.test", "/post")).state,
        UrlState::Crawled
    );
}

#[tokio::test]
async fn us2_1_2_robots_group_and_rule_shown() {
    let site = SiteBuilder::new()
        .robots(
            E,
            "User-agent: SpiderLens\nDisallow: /private/\n\nUser-agent: *\nDisallow: /tmp/\n",
        )
        .page(E, "/", "", &["/private/x", "/tmp/y", "/ok"])
        .page(E, "/ok", "", &[])
        .page(E, "/tmp/y", "", &[])
        .start();
    let d = crawl(&site, "/", fast).await;
    let private = site.url(E, "/private/x");
    assert_eq!(
        site.count(E, "/private/x"),
        0,
        "SC-003: no request to disallowed URLs"
    );
    assert_eq!(d.reason(&private), Some(NotCrawledReason::BlockedByRobots));
    assert_eq!(
        d.row(&private).reason_detail.as_deref(),
        Some("Disallow: /private/")
    );
    // Our specific group replaces `*`, so /tmp/ is allowed for SpiderLens.
    assert_eq!(d.row(&site.url(E, "/tmp/y")).state, UrlState::Crawled);
    assert!(
        site.hits()
            .iter()
            .all(|h| h.user_agent.starts_with("SpiderLens/")),
        "FR-029"
    );
}

#[tokio::test]
async fn us2_3_crawl_delay_spaces_requests_to_one_host() {
    let site = SiteBuilder::new()
        .robots(E, "User-agent: *\nCrawl-delay: 1\n")
        .page(E, "/", "", &["/a", "/b", "{base:other.test}/x"])
        .page(E, "/a", "", &[])
        .page(E, "/b", "", &[])
        .page("other.test", "/x", "", &[])
        .start();
    let d = crawl(&site, "/", fast).await;
    let hits: Vec<_> = site.page_hits().into_iter().filter(|h| h.host == E).collect();
    assert_eq!(hits.len(), 3);
    for pair in hits.windows(2) {
        let gap = pair[1].at - pair[0].at;
        assert!(
            gap >= Duration::from_millis(990),
            "gap {gap:?} shorter than crawl-delay"
        );
    }
    assert_eq!(
        d.summary
            .hosts
            .iter()
            .find(|h| h.host.contains(E))
            .unwrap()
            .crawl_delay_secs,
        Some(1.0)
    );
}

#[tokio::test]
async fn us2_4_5_robots_missing_allows_and_server_error_blocks() {
    let site = SiteBuilder::new()
        .robots_status(E, 404)
        .page(E, "/", "", &["/a"])
        .page(E, "/a", "", &[])
        .start();
    let d = crawl(&site, "/", fast).await;
    assert_eq!(d.row(&site.url(E, "/a")).state, UrlState::Crawled);

    let site = SiteBuilder::new()
        .robots_status(E, 503)
        .page(E, "/", "", &[])
        .start();
    let d = crawl(&site, "/", fast).await;
    let root = site.url(E, "/");
    assert_eq!(d.reason(&root), Some(NotCrawledReason::RobotsUnavailable));
    assert_eq!(
        site.count(E, "/robots.txt"),
        3,
        "retried with the standard policy"
    );
    assert_eq!(site.count(E, "/"), 0);
    let host = d.summary.hosts.iter().find(|h| h.host.contains(E)).unwrap();
    assert_eq!(host.robots, "unavailable");
    assert!(host.error.as_deref().unwrap().contains("503"));
    assert!(
        d.summary.warnings.iter().any(|w| w.contains("start URL")),
        "the user is told"
    );
}

#[tokio::test]
async fn us2_6_ignore_but_report_crawls_and_records_rule() {
    let site = SiteBuilder::new()
        .robots(E, "User-agent: *\nDisallow: /private/\n")
        .page(E, "/", "", &["/private/x"])
        .page(E, "/private/x", "", &[])
        .start();
    let d = crawl(&site, "/", |c| {
        fast(c);
        c.robots.mode = RobotsMode::IgnoreReport;
    })
    .await;
    let row = d.row(&site.url(E, "/private/x"));
    assert_eq!(row.state, UrlState::Crawled);
    assert_eq!(row.robots_rule.as_deref(), Some("Disallow: /private/"));
}

#[tokio::test]
async fn us3_rules_limits_and_params() {
    let site = SiteBuilder::new()
        .page(
            E,
            "/docs/",
            "",
            &[
                "/docs/a?sort=x",
                "/blog/1",
                "/docs/b?utm_source=x",
                "/docs/b?utm_medium=y",
                "/docs/r",
            ],
        )
        .page(E, "/docs/b", "", &["/docs/c"])
        .page(E, "/docs/c", "", &["/docs/d"])
        .page(E, "/docs/d", "", &[])
        .redirect(E, "/docs/r", 301, "/docs/x?sort=1")
        .page(E, "/blog/1", "", &[])
        .start();
    let d = crawl(&site, "/docs/", |c| {
        fast(c);
        c.scope.exclude = vec![r"\?sort=".into()];
        c.scope.stay_in_folder = true;
        c.limits.max_depth = Some(2);
        c.scope.query_params.mode = QueryMode::Remove;
        c.scope.query_params.names = vec!["utm_source".into(), "utm_medium".into()];
    })
    .await;
    // US3-1: excluded, pattern named, never requested.
    let sorted = site.url(E, "/docs/a?sort=x");
    assert_eq!(d.reason(&sorted), Some(NotCrawledReason::Excluded));
    assert_eq!(d.row(&sorted).reason_detail.as_deref(), Some(r"\?sort="));
    // US3-7: outside the start folder.
    assert_eq!(
        d.reason(&site.url(E, "/blog/1")),
        Some(NotCrawledReason::OutsideStartFolder)
    );
    assert_eq!(site.count(E, "/blog/1"), 0);
    // US3-6: both tracking variants become /docs/b, crawled once.
    assert_eq!(d.row(&site.url(E, "/docs/b")).state, UrlState::Crawled);
    assert_eq!(site.count(E, "/docs/b"), 1);
    // US3-4: depth 2 crawled, links on it over the limit and not requested.
    assert_eq!(d.row(&site.url(E, "/docs/c")).state, UrlState::Crawled);
    assert_eq!(
        d.reason(&site.url(E, "/docs/d")),
        Some(NotCrawledReason::OverDepthLimit)
    );
    assert_eq!(site.count(E, "/docs/d"), 0);
    // US3-8: the redirect is recorded; its excluded target is not requested.
    let r = d.row(&site.url(E, "/docs/r"));
    assert_eq!(r.status, Some(301));
    assert_eq!(
        d.reason(&site.url(E, "/docs/x?sort=1")),
        Some(NotCrawledReason::Excluded)
    );
    assert_eq!(site.count(E, "/docs/x?sort=1"), 0);
}

#[tokio::test]
async fn us3_2_include_spares_start_url() {
    let site = SiteBuilder::new()
        .page(E, "/", "", &["/blog/a", "/shop/b"])
        .page(E, "/blog/a", "", &[])
        .start();
    let d = crawl(&site, "/", |c| {
        fast(c);
        c.scope.include = vec!["/blog/".into()];
    })
    .await;
    assert_eq!(d.row(&site.url(E, "/")).state, UrlState::Crawled);
    assert_eq!(d.row(&site.url(E, "/blog/a")).state, UrlState::Crawled);
    assert_eq!(
        d.reason(&site.url(E, "/shop/b")),
        Some(NotCrawledReason::NotIncluded)
    );
}

#[tokio::test]
async fn us3_5_url_limit() {
    let links: Vec<String> = (0..20).map(|i| format!("/p{i}")).collect();
    let refs: Vec<&str> = links.iter().map(String::as_str).collect();
    let mut b = SiteBuilder::new().page(E, "/", "", &refs);
    for l in &links {
        b = b.page(E, l, "", &[]);
    }
    let site = b.start();
    let d = crawl(&site, "/", |c| {
        fast(c);
        c.limits.max_urls = Some(5);
    })
    .await;
    assert_eq!(d.summary.crawled, 5);
    assert!(d.summary.url_limit_reached);
    // The start page counts toward the limit: 21 URLs, 5 crawled, 16 over.
    assert_eq!(
        d.summary.by_reason.get(&NotCrawledReason::OverUrlLimit),
        Some(&16)
    );
    assert!(site.page_hits().len() <= 5, "SC-010");
}

#[tokio::test]
async fn us4_every_reason_once_and_stop_resume() {
    let long = format!("/{}", "l".repeat(2100));
    let site = SiteBuilder::new()
        .robots(E, "User-agent: *\nDisallow: /s/blocked\n")
        .page(
            E,
            "/s/",
            r#"<a href="http://exa mple.test/">bad</a><a href="mailto:x@example.test">m</a>
               <a href="/s/nf" rel="nofollow">nf</a>"#,
            &[
                "{base:other.test}/",
                "/elsewhere",
                "/s/skip",
                "/s/zzz",
                "/s/deep",
                "/s/blocked",
                &long,
            ],
        )
        .page(E, "/s/deep", "", &["/s/deeper"])
        .page(E, "/s/deeper", "", &[])
        .start();
    let d = crawl(&site, "/s/", |c| {
        fast(c);
        c.scope.stay_in_folder = true;
        c.scope.exclude = vec!["skip".into()];
        c.scope.include = vec!["/s/d".into(), "/s/n".into(), "/s/b".into()];
        c.limits.max_depth = Some(1);
    })
    .await;
    let reasons: Vec<_> = d.summary.by_reason.keys().copied().collect();
    for r in [
        NotCrawledReason::MalformedUrl,
        NotCrawledReason::UnsupportedScheme,
        NotCrawledReason::External,
        NotCrawledReason::OutsideStartFolder,
        NotCrawledReason::Excluded,
        NotCrawledReason::NotIncluded,
        NotCrawledReason::OverDepthLimit,
        NotCrawledReason::Nofollow,
        NotCrawledReason::BlockedByRobots,
    ] {
        assert!(reasons.contains(&r), "missing {r}: {reasons:?}");
    }
    // Every not-crawled URL knows where it was found (US4-1).
    for row in d.rows.values().filter(|r| r.state == UrlState::NotCrawled) {
        assert!(row.found_on.is_some(), "{} has no source page", row.url);
    }
}

#[tokio::test]
async fn us4_4_stop_then_resume_crawls_each_url_once() {
    let links: Vec<String> = (0..30).map(|i| format!("/p{i}")).collect();
    let refs: Vec<&str> = links.iter().map(String::as_str).collect();
    let mut b = SiteBuilder::new()
        .robots(E, "User-agent: *\nCrawl-delay: 0.05\n")
        .page(E, "/", "", &refs);
    for l in &links {
        b = b.page(E, l, "", &[]);
    }
    let site = b.start();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.spiderlens");
    let mut c = CrawlConfig::default();
    fast(&mut c);
    let input = ModeInput::Spider {
        start_url: site.url(E, "/"),
    };
    let crawler = Crawler::create(&path, c, &input)
        .unwrap()
        .with_resolve(site.resolve());
    let (tx, rx) = watch::channel(Control::Run);
    let handle = tokio::spawn(crawler.run(rx, None));
    tokio::time::sleep(Duration::from_millis(400)).await;
    tx.send(Control::Stop).unwrap();
    let s = handle.await.unwrap().unwrap();
    assert_eq!(s.status, "stopped");
    let stopped = *s
        .by_reason
        .get(&NotCrawledReason::CrawlStopped)
        .expect("some URLs stopped");
    assert!(stopped > 0);
    assert_eq!(
        s.crawled + s.not_crawled,
        s.discovered,
        "SC-004 holds for a stopped crawl"
    );

    let (_tx, rx) = watch::channel(Control::Run);
    let s = Crawler::open(&path)
        .unwrap()
        .with_resolve(site.resolve())
        .run(rx, None)
        .await
        .unwrap();
    assert_eq!(s.status, "completed");
    assert_eq!(s.crawled, 31);
    assert!(!s.by_reason.contains_key(&NotCrawledReason::CrawlStopped));
    for l in &links {
        assert_eq!(
            site.count(E, l),
            1,
            "SC-008: {l} crawled once across stop and resume"
        );
    }
    assert_eq!(
        site.count(E, "/robots.txt"),
        1,
        "robots.txt is reused after resume (FR-023)"
    );
}

#[tokio::test]
async fn us5_nofollow_handling() {
    let site = SiteBuilder::new()
        .page(
            E,
            "/",
            r#"<a href="/x" rel="nofollow">x</a><a href="/both" rel="nofollow">b</a>"#,
            &["/both", "/np"],
        )
        .html(
            E,
            "/np",
            r#"<meta name="robots" content="nofollow"><a href="/only-from-np">o</a>"#,
        )
        .page(E, "/x", "", &[])
        .page(E, "/both", "", &[])
        .page(E, "/only-from-np", "", &[])
        .start();
    let d = crawl(&site, "/", fast).await;
    assert_eq!(d.reason(&site.url(E, "/x")), Some(NotCrawledReason::Nofollow));
    assert_eq!(site.count(E, "/x"), 0);
    assert_eq!(d.row(&site.url(E, "/both")).state, UrlState::Crawled, "US5-4");
    let only = d.row(&site.url(E, "/only-from-np"));
    assert_eq!(only.reason, Some(NotCrawledReason::Nofollow));
    assert_eq!(only.reason_detail.as_deref(), Some("page"));

    let d = crawl(&site, "/", |c| {
        fast(c);
        c.scope.follow_internal_nofollow = true;
    })
    .await;
    let x = d.row(&site.url(E, "/x"));
    assert_eq!(x.state, UrlState::Crawled);
    assert!(x.via_nofollow, "US5-3");
}

#[tokio::test]
async fn us6_sitemaps_from_robots() {
    let index = "<sitemapindex><sitemap><loc>{base:example.test}/s1.xml</loc></sitemap>\
                 <sitemap><loc>{base:example.test}/s2.xml.gz</loc></sitemap>\
                 <sitemap><loc>{base:example.test}/broken.xml</loc></sitemap></sitemapindex>";
    let s1 = "<urlset><url><loc>{base:example.test}/</loc></url><url><loc>{base:example.test}/orphan</loc></url></urlset>";
    let s2 = "<urlset><url><loc>{base:other.test}/x</loc></url><url><loc>{base:example.test}/skip-me</loc></url></urlset>";
    let site = SiteBuilder::new()
        .robots(
            E,
            "User-agent: *\nAllow: /\nSitemap: {base:example.test}/index.xml\n",
        )
        .page(E, "/", "", &[])
        .page(E, "/orphan", "", &[])
        .raw(E, "/index.xml", 200, "application/xml", index.as_bytes())
        .raw(E, "/s1.xml", 200, "application/xml", s1.as_bytes())
        .gzip(E, "/s2.xml.gz", "application/gzip", s2)
        .raw(E, "/broken.xml", 200, "application/xml", b"<urlset><url>")
        .host("other.test")
        .start();
    let d = crawl(&site, "/", |c| {
        fast(c);
        c.scope.use_sitemaps = true;
        c.scope.exclude = vec!["skip-me".into()];
    })
    .await;
    let orphan = d.row(&site.url(E, "/orphan"));
    assert_eq!(orphan.state, UrlState::Crawled, "US6-1");
    assert_eq!(orphan.source, DiscoverySource::Sitemap);
    assert!(orphan.in_sitemap);
    assert!(d.row(&site.url(E, "/")).in_sitemap, "also found in a sitemap");
    assert_eq!(
        d.reason(&site.url(E, "/skip-me")),
        Some(NotCrawledReason::Excluded),
        "US6-2"
    );
    assert!(
        d.summary
            .sitemaps
            .iter()
            .any(|s| s.url.ends_with("/broken.xml") && s.error.is_some()),
        "US6-3"
    );
    assert_eq!(d.summary.status, "completed");

    let d = crawl(&site, "/", fast).await;
    assert!(!d.rows.contains_key(&site.url(E, "/orphan")));
    assert_eq!(site.count(E, "/index.xml"), 1, "US6-4: not requested when off");
}

#[tokio::test]
async fn list_mode_depth_zero_and_hosts_internal() {
    let site = SiteBuilder::new()
        .page(E, "/a", "", &["/a2"])
        .page("other.test", "/b", "", &["/b2"])
        .start();
    let input = ModeInput::List(ListInput {
        urls: vec![site.url(E, "/a"), site.url("other.test", "/b"), site.url(E, "/a")],
        duplicates: 1,
        invalid_count: 0,
        invalid_sample: vec![],
        notices: vec![],
    });
    let d = run_input(&site, input, Mode::List, fast).await;
    assert_eq!(d.row(&site.url("other.test", "/b")).state, UrlState::Crawled);
    assert_eq!(site.count(E, "/a"), 1);
    assert_eq!(
        d.reason(&site.url(E, "/a2")),
        Some(NotCrawledReason::OverDepthLimit)
    );
    assert_eq!(site.count(E, "/a2"), 0);
}

#[tokio::test]
async fn start_redirect_to_www_is_adopted() {
    let site = SiteBuilder::new()
        .redirect(E, "/", 301, "{base:www.example.test}/")
        .page("www.example.test", "/", "", &["/about"])
        .page("www.example.test", "/about", "", &[])
        .start();
    let d = crawl(&site, "/", |c| {
        fast(c);
        c.scope.include = vec!["/nothing-matches/".into()];
    })
    .await;
    assert_eq!(d.row(&site.url("www.example.test", "/")).state, UrlState::Crawled);
    assert_eq!(d.row(&site.url(E, "/")).status, Some(301));
}

/// T018 guard: 2,000 pages, each linked several times with different
/// spellings, are fetched exactly once at their shortest click distance.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn guard_2000_pages_once_each_at_shortest_depth() {
    const N: usize = 2000;
    // Page i links to its children 4i+1..4i+4, back home, and to page i+1
    // with a fragment, so most pages are reachable along several paths.
    let mut b = SiteBuilder::new();
    for i in 0..N {
        let path = if i == 0 {
            "/".to_string()
        } else {
            format!("/p/{i}")
        };
        let mut links: Vec<String> = (1..=4)
            .map(|k| 4 * i + k)
            .filter(|&c| c < N)
            .map(|c| format!("/p/{c}"))
            .collect();
        links.push("/".into());
        if i + 1 < N {
            links.push(format!("/p/{}#again", i + 1));
        }
        let refs: Vec<&str> = links.iter().map(String::as_str).collect();
        b = b.page(E, &path, "", &refs);
    }
    let site = b.robots(E, "User-agent: *\nAllow: /\n").start();
    let done = crawl(&site, "/", |_| {}).await;
    assert_eq!(done.summary.crawled, N as u64);
    let mut fetched: HashMap<String, usize> = HashMap::new();
    for h in site.page_hits() {
        *fetched.entry(h.path).or_default() += 1;
    }
    assert_eq!(fetched.len(), N);
    assert!(
        fetched.values().all(|&n| n == 1),
        "SC-001: each page fetched once"
    );
    // Shortest distance: by tree level, except that page i+1 is one click
    // further than page i, which can only shorten it.
    let mut depth = vec![u32::MAX; N];
    depth[0] = 0;
    for i in 0..N {
        for c in [4 * i + 1, 4 * i + 2, 4 * i + 3, 4 * i + 4, i + 1] {
            if c < N {
                depth[c] = depth[c].min(depth[i] + 1);
            }
        }
    }
    for (i, &d) in depth.iter().enumerate().skip(1) {
        let url = site.url(E, &format!("/p/{i}"));
        assert_eq!(done.row(&url).depth, d, "SC-002 for {url}");
    }
}

/// The desktop status bar reads these figures (crawled, queued, not crawled,
/// errors) straight from the progress events.
#[tokio::test]
async fn progress_events_carry_status_bar_figures() {
    let site = SiteBuilder::new()
        .page(E, "/", "", &["/a", "/missing", "/private/x"])
        .page(E, "/a", "", &[])
        .robots(E, "User-agent: *\nDisallow: /private/\n")
        .start();
    let mut c = CrawlConfig::default();
    fast(&mut c);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.spiderlens");
    let input = ModeInput::Spider {
        start_url: site.url(E, "/"),
    };
    let crawler = Crawler::create(&path, c, &input)
        .unwrap()
        .with_resolve(site.resolve());
    let (_tx, rx) = watch::channel(Control::Run);
    let (events_tx, mut events_rx) = tokio::sync::mpsc::unbounded_channel();
    let summary = tokio::time::timeout(Duration::from_secs(60), crawler.run(rx, Some(events_tx)))
        .await
        .expect("crawl finishes")
        .unwrap();
    let mut last = None;
    while let Ok(e) = events_rx.try_recv() {
        if let CrawlEvent::Progress(p) = e {
            last = Some(p);
        }
    }
    let p = last.expect("at least the final progress event");
    assert_eq!(p.crawled, summary.crawled);
    assert_eq!(p.crawled, 3, "/, /a and /missing");
    assert_eq!(p.errors, 1, "/missing answers 404");
    assert_eq!(p.not_crawled, 1, "/private/x is blocked by robots.txt");
    assert_eq!(p.queued, 0);
    assert_eq!(p.in_flight, 0);
    assert_eq!(p.discovered, summary.discovered);
}

/// With robots.txt ignored, the first pass over a new host used to find
/// nothing to start and wait forever (spec 008 T006).
#[tokio::test]
async fn robots_ignored_crawl_runs_to_the_end() {
    let site = SiteBuilder::new()
        .robots(E, "User-agent: *\nDisallow: /\n")
        .page(E, "/", "", &["/a"])
        .page(E, "/a", "", &[])
        .start();
    let d = crawl(&site, "/", |c| {
        fast(c);
        c.robots.mode = RobotsMode::Ignore;
    })
    .await;
    assert_eq!(d.summary.crawled, 2);
    assert_eq!(site.count(E, "/robots.txt"), 0, "not even fetched");
}

/// A 429 with Retry-After holds the whole site until the wait is over
/// (spec 008 FR-014, FR-015, SC-005).
#[tokio::test]
async fn retry_after_holds_the_site() {
    let site = SiteBuilder::new()
        .page(E, "/", "", &["/busy", "/b"])
        .raw(E, "/busy", 429, "text/plain", b"slow down")
        .header(E, "/busy", "Retry-After", "2")
        .page(E, "/b", "", &[])
        .start();
    // One request at a time, so /b cannot already be in flight when the 429
    // arrives and the gap is the hold alone.
    let d = crawl(&site, "/", |c| {
        fast(c);
        c.speed.max_per_host = 1;
        c.speed.adaptive = false;
    })
    .await;
    assert_eq!(d.row(&site.url(E, "/busy")).status, Some(429));
    assert_eq!(d.row(&site.url(E, "/b")).state, UrlState::Crawled);
    let hits = site.page_hits();
    let at = |path: &str| hits.iter().find(|h| h.path == path).unwrap().at;
    let waited = at("/b").duration_since(at("/busy"));
    assert!(
        waited >= Duration::from_millis(1900),
        "next request after {waited:?}, Retry-After asked for 2 s"
    );
}

#[tokio::test]
async fn external_checks_record_robots_blocks_redirects_and_forbidden_robots() {
    let site = SiteBuilder::new()
        .page(
            E,
            "/",
            "",
            &[
                "{base:blocked.test}/profile",
                "{base:moved.test}/",
                "{base:closed.test}/",
            ],
        )
        .robots("blocked.test", "User-agent: *\nDisallow: /\n")
        .page("blocked.test", "/profile", "", &[])
        .redirect("moved.test", "/", 308, "/us/en")
        .page("moved.test", "/us/en", "", &[])
        .robots_status("closed.test", 403)
        .page("closed.test", "/", "", &[])
        .start();
    let d = crawl(&site, "/", fast).await;

    // A check robots.txt disallows is not fetched, but says why.
    let blocked = d.row(&site.url("blocked.test", "/profile"));
    assert_eq!(blocked.reason, Some(NotCrawledReason::External));
    assert_eq!(blocked.status, None);
    assert_eq!(blocked.reason_detail.as_deref(), Some("Blocked by robots.txt"));
    assert_eq!(blocked.robots_rule.as_deref(), Some("Disallow: /"));
    assert_eq!(site.count("blocked.test", "/profile"), 0);
    assert_eq!(d.summary.external_blocked, 1);

    // An external redirect records its target, which is checked in turn.
    let moved = d.row(&site.url("moved.test", "/"));
    assert_eq!(moved.status, Some(308));
    let target = d.row(&site.url("moved.test", "/us/en"));
    assert_eq!(target.source, DiscoverySource::Redirect);
    assert_eq!(
        target.found_on.as_deref(),
        Some(site.url("moved.test", "/").as_str())
    );
    assert_eq!(target.status, Some(200));
    assert_eq!(site.count("moved.test", "/us/en"), 1);

    // robots.txt answering 403 is labelled as such and allows everything.
    let host = d
        .summary
        .hosts
        .iter()
        .find(|h| h.host.contains("//closed.test"))
        .unwrap();
    assert_eq!(host.robots, "forbidden");
    assert_eq!(d.row(&site.url("closed.test", "/")).status, Some(200));
    assert_eq!(d.summary.external_checked, 3);
}

/// Spec 007 FR-004 and FR-005: active time accumulates across runs of the
/// same file, and the summary breaks failures down by error class.
#[tokio::test]
async fn spec007_active_time_and_failures_by_class() {
    let site = SiteBuilder::new()
        .page(E, "/", "", &["/a", "http://dead.test/x"])
        .page(E, "/a", "", &[])
        .start();
    // dead.test resolves to a port nothing listens on: a connection error.
    let closed: url::Url = spiderlens_test_fixtures::closed_port_url().parse().unwrap();
    let dead: std::net::SocketAddr = format!("127.0.0.1:{}", closed.port().unwrap()).parse().unwrap();
    let mut resolve = site.resolve();
    resolve.push(("dead.test".into(), dead));
    let mut c = CrawlConfig::default();
    fast(&mut c);
    // Otherwise the unreachable robots.txt blocks the check instead.
    c.robots.mode = RobotsMode::Ignore;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.spiderlens");
    let input = ModeInput::Spider {
        start_url: site.url(E, "/"),
    };
    let run = |crawler: Crawler| async move {
        let (_tx, rx) = watch::channel(Control::Run);
        tokio::time::timeout(Duration::from_secs(60), crawler.run(rx, None))
            .await
            .expect("crawl finishes")
            .unwrap()
    };
    let crawler = Crawler::create(&path, c, &input)
        .unwrap()
        .with_resolve(resolve.clone())
        .with_retry_base(Duration::from_millis(10));
    let summary = run(crawler).await;
    assert!(summary.active_ms > 0);
    assert_eq!(
        summary.failed_by_class.get("connection"),
        Some(&1),
        "{:?}",
        summary.failed_by_class
    );
    let first = summary.active_ms;
    let again = run(Crawler::open(&path).unwrap().with_resolve(resolve)).await;
    assert!(again.active_ms >= first, "{} < {first}", again.active_ms);
}
