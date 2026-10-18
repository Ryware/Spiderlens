//! Spec 024 slice 2 (US2) against the fake URL Inspection service: the
//! default URL set and its order, the stored fields, the canonical mismatch
//! flag, the daily allowance with pause and next-day resume, the cache, and
//! the Index status tab. Test names carry the scenario id.

use std::path::{Path, PathBuf};
use std::time::Duration;

use jiff::{SignedDuration, Timestamp};
use rusqlite::Connection;
use spiderlens_config::{CrawlConfig, ModeInput, SpeedSettings};
use spiderlens_engine::crawl::{Control as CrawlControl, Crawler, open_for_update};
use spiderlens_integrations::Provider;
use spiderlens_integrations::cache::Cache;
use spiderlens_integrations::http::Http;
use spiderlens_integrations::inspect::{self, InspectApi, InspectRequest};
use spiderlens_integrations::limits::{Daily, GOOGLE_TZ, Ledger, Limits};
use spiderlens_integrations::oauth::{Endpoints, Session, StoredGoogle};
use spiderlens_integrations::run::{Control, Ctx};
use spiderlens_integrations::store::{self, RunProgress, RunState};
use spiderlens_test_fixtures::google_apis::{FakeInspection, InspectionAnswer};
use spiderlens_test_fixtures::providers::{FakeGoogle, GoogleData};
use spiderlens_test_fixtures::{Site, SiteBuilder};
use tokio::sync::watch;

const HOST: &str = "127.0.0.1";
const PAGES: usize = 10;
const ACCOUNT: &str = "ana@example.com";

/// A ring of ten pages (page `i` is at depth `i`), plus a noindex page and a
/// missing page that the default set must leave out.
fn site() -> Site {
    let mut b = SiteBuilder::new();
    for i in 0..PAGES {
        let next = format!("/p/{}", (i + 1) % PAGES);
        let links: Vec<&str> = if i == 0 {
            vec![next.as_str(), "/noindex", "/gone"]
        } else {
            vec![next.as_str()]
        };
        b = b.page(HOST, &format!("/p/{i}"), "", &links);
    }
    b = b.html(
        HOST,
        "/noindex",
        "<!doctype html><html><head><meta name=\"robots\" content=\"noindex\"><title>n</title></head><body></body></html>",
    );
    b = b.status(HOST, "/gone", 404);
    b.start()
}

async fn crawl(site: &Site, dir: &Path) -> PathBuf {
    let path = dir.join("c.spiderlens");
    let config = CrawlConfig {
        speed: SpeedSettings::FAST,
        ..CrawlConfig::default()
    };
    let crawler = Crawler::create(
        &path,
        config,
        &ModeInput::Spider {
            start_url: site.url(HOST, "/p/0"),
        },
    )
    .unwrap();
    let (_tx, rx) = watch::channel(CrawlControl::Run);
    crawler.run(rx, None).await.unwrap();
    path
}

fn property(site: &Site) -> String {
    site.url(HOST, "/")
}

fn page(site: &Site, i: usize) -> String {
    site.url(HOST, &format!("/p/{i}"))
}

fn endpoints(g: &FakeGoogle) -> Endpoints {
    Endpoints {
        auth: g.auth_url(),
        token: g.token_url(),
        revoke: g.revoke_url(),
    }
}

fn session(g: &FakeGoogle) -> Session {
    Session::new(
        Http::new(Duration::from_secs(10)),
        endpoints(g),
        StoredGoogle {
            client_id: "test-client".into(),
            client_secret: None,
            refresh_token: g.issue_refresh_token(),
        },
    )
}

fn google() -> FakeGoogle {
    FakeGoogle::start(GoogleData {
        email: ACCOUNT.into(),
        ..GoogleData::default()
    })
}

/// A context whose daily allowance is `allowance`, as the property's is.
fn ctx(allowance: u64, cache: Option<Cache>) -> Ctx {
    let mut limits = Limits::defaults(Provider::UrlInspection);
    limits.per_minute = 600;
    limits.daily = Some(Daily {
        allowance,
        tz: GOOGLE_TZ,
    });
    Ctx::new(Provider::UrlInspection, limits, cache)
}

/// The provider's clock: noon Pacific on day 0, a day later for each step.
fn clock(day: i64) -> Timestamp {
    "2026-10-14T19:00:00Z".parse::<Timestamp>().unwrap() + SignedDuration::from_hours(24 * day)
}

struct Rig {
    site: Site,
    dir: tempfile::TempDir,
    path: PathBuf,
    google: FakeGoogle,
    fake: FakeInspection,
    ledger: Ledger,
}

impl Rig {
    async fn new(allowance: u64) -> Self {
        let site = site();
        let dir = tempfile::tempdir().unwrap();
        let path = crawl(&site, dir.path()).await;
        let fake = FakeInspection::start(vec![property(&site)], allowance);
        let ledger = Ledger::new(dir.path().join("usage.json"));
        Self {
            site,
            dir,
            path,
            google: google(),
            fake,
            ledger,
        }
    }

    fn request(&self) -> InspectRequest {
        InspectRequest::indexable(property(&self.site))
    }

    /// One run, or one resume, of the request at `day`.
    async fn run(
        &self,
        allowance: u64,
        req: &InspectRequest,
        resume: Option<i64>,
        day: i64,
        cache: Option<Cache>,
    ) -> RunProgress {
        let update = open_for_update(&self.path, None).unwrap();
        inspect::run_inspection(
            &ctx(allowance, cache),
            &session(&self.google),
            &InspectApi {
                base: self.fake.base(),
            },
            &update,
            &self.ledger,
            ACCOUNT,
            req,
            resume,
            &Control::default(),
            &move || clock(day),
            &mut |_| {},
        )
        .await
        .unwrap()
    }

    fn count(&self, sql: &str) -> i64 {
        Connection::open(&self.path)
            .unwrap()
            .query_row(sql, [], |r| r.get(0))
            .unwrap()
    }
}

#[tokio::test]
async fn us2_1_default_set_is_indexable_html_ordered_by_clicks_then_depth() {
    let rig = Rig::new(2000).await;
    // Search Console clicks put pages 7 and 3 first; the rest follow by depth.
    {
        let conn = Connection::open(&rig.path).unwrap();
        for (i, clicks) in [(3, 20), (7, 50)] {
            conn.execute(
                "INSERT INTO gsc_pages (url_id, run_id, clicks, impressions, ctr, position)
                 SELECT id, 0, ?2, 100, 0.1, 3 FROM urls WHERE url = ?1",
                rusqlite::params![page(&rig.site, i), clicks],
            )
            .unwrap();
        }
        // The noindex page and the missing page are not in the default set.
        let selected = inspect::count_selected(&conn, &rig.request()).unwrap();
        assert_eq!(selected, PAGES as i64);
        let all = InspectRequest {
            urls: "all".into(),
            ..rig.request()
        };
        assert!(inspect::count_selected(&conn, &all).unwrap() > PAGES as i64);
        let capped = InspectRequest {
            max: Some(4),
            ..rig.request()
        };
        assert_eq!(inspect::count_selected(&conn, &capped).unwrap(), 4);
    }
    let req = InspectRequest {
        max: Some(4),
        ..rig.request()
    };
    let p = rig.run(2000, &req, None, 0, None).await;
    assert_eq!(p.state, RunState::Finished);
    assert_eq!((p.requested, p.done, p.failed), (4, 4, 0));
    assert_eq!(
        rig.fake.inspected(),
        vec![page(&rig.site, 7), page(&rig.site, 3), page(&rig.site, 0), page(&rig.site, 1)]
    );
    // The request carries the property and the page, nothing else about the crawl.
    let log = rig.fake.log();
    assert!(log[0].body.contains(&format!("\"siteUrl\":\"{}\"", property(&rig.site))));
    assert!(log[0].body.contains(&page(&rig.site, 7)));
}

#[tokio::test]
async fn us2_2_fields_canonical_mismatch_and_robots_state_are_stored() {
    let rig = Rig::new(2000).await;
    rig.fake.set_answer(
        &page(&rig.site, 1),
        InspectionAnswer::canonical_elsewhere(&page(&rig.site, 1), &page(&rig.site, 2)),
    );
    rig.fake
        .set_answer(&page(&rig.site, 2), InspectionAnswer::blocked_by_robots(&page(&rig.site, 2)));
    rig.fake
        .set_answer(&page(&rig.site, 3), InspectionAnswer::not_indexed(&page(&rig.site, 3)));
    let p = rig.run(2000, &rig.request(), None, 0, None).await;
    assert_eq!(p.state, RunState::Finished);
    assert_eq!(p.done, PAGES as i64);
    assert_eq!(rig.count("SELECT COUNT(*) FROM gsc_inspection"), PAGES as i64);

    let conn = Connection::open(&rig.path).unwrap();
    let row = |i: usize| {
        conn.query_row(
            "SELECT g.verdict, g.coverage_state, g.google_canonical, g.user_canonical, g.canonical_mismatch,
                    g.last_crawl, g.crawled_as, g.robots_state, g.page_fetch_state, g.indexing_state, length(g.raw)
             FROM gsc_inspection g JOIN urls u ON u.id = g.url_id WHERE u.url = ?1",
            [page(&rig.site, i)],
            |r| {
                Ok((
                    r.get::<_, Option<String>>(0)?,
                    r.get::<_, Option<String>>(1)?,
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, Option<String>>(3)?,
                    r.get::<_, Option<i64>>(4)?,
                    r.get::<_, Option<i64>>(5)?,
                    r.get::<_, Option<String>>(6)?,
                    r.get::<_, Option<String>>(7)?,
                    r.get::<_, Option<String>>(8)?,
                    r.get::<_, Option<String>>(9)?,
                    r.get::<_, i64>(10)?,
                ))
            },
        )
        .unwrap()
    };
    // An ordinary page: indexed under its own address.
    let r0 = row(0);
    assert_eq!(r0.0.as_deref(), Some("PASS"));
    assert_eq!(r0.1.as_deref(), Some("Submitted and indexed"));
    assert_eq!(r0.2.as_deref(), Some(page(&rig.site, 0).as_str()));
    assert_eq!(r0.3.as_deref(), Some(page(&rig.site, 0).as_str()));
    assert_eq!(r0.4, Some(0));
    assert_eq!(r0.5, Some(1_791_793_800));
    assert_eq!(r0.6.as_deref(), Some("MOBILE"));
    assert_eq!(r0.7.as_deref(), Some("ALLOWED"));
    assert_eq!(r0.8.as_deref(), Some("SUCCESSFUL"));
    assert_eq!(r0.9.as_deref(), Some("INDEXING_ALLOWED"));
    assert!(r0.10 > 0);
    // Google chose another page as the canonical: flagged.
    let r1 = row(1);
    assert_eq!(r1.2.as_deref(), Some(page(&rig.site, 2).as_str()));
    assert_eq!(r1.4, Some(1));
    // Blocked by robots.txt, per Google: no canonical, so no mismatch flag.
    let r2 = row(2);
    assert_eq!(r2.7.as_deref(), Some("DISALLOWED"));
    assert_eq!(r2.8.as_deref(), Some("BLOCKED_ROBOTS_TXT"));
    assert_eq!(r2.4, None);
    let r3 = row(3);
    assert_eq!(r3.1.as_deref(), Some("Crawled - currently not indexed"));
    assert_eq!(
        store::features(&conn).unwrap(),
        vec!["url_inspection".to_string()]
    );
}

#[tokio::test]
async fn us2_3_the_allowance_pauses_the_run_and_the_next_day_resumes_without_repeats() {
    let rig = Rig::new(4).await;
    let req = rig.request();
    let first = rig.run(4, &req, None, 0, None).await;
    assert_eq!(first.state, RunState::PausedQuota);
    assert_eq!((first.requested, first.done), (PAGES as i64, 4));
    assert!(first.resume_at.is_some());
    let message = first.message.clone().unwrap();
    assert!(message.contains("allowance"), "{message}");
    assert!(message.contains("6 URLs are left"), "{message}");
    // Finished results are kept; the rest are marked as waiting on the allowance.
    assert_eq!(rig.count("SELECT COUNT(*) FROM gsc_inspection"), 4);
    assert_eq!(
        rig.count("SELECT COUNT(*) FROM integration_status WHERE provider = 'url_inspection' AND reason = 'quota'"),
        6
    );
    assert_eq!(rig.fake.used_today(), 4);

    // Resuming the same day asks Google for nothing.
    let calls = rig.fake.log().len();
    let again = rig.run(4, &req, Some(first.id), 0, None).await;
    assert_eq!(again.state, RunState::PausedQuota);
    assert_eq!(rig.fake.log().len(), calls);

    // The next day continues with the pending URLs only.
    rig.fake.next_day();
    let second = rig.run(4, &req, Some(first.id), 1, None).await;
    assert_eq!(second.state, RunState::PausedQuota);
    assert_eq!(second.done, 8);
    rig.fake.next_day();
    let third = rig.run(4, &req, Some(first.id), 2, None).await;
    assert_eq!(third.state, RunState::Finished);
    assert_eq!((third.done, third.failed), (PAGES as i64, 0));
    let mut inspected = rig.fake.inspected();
    assert_eq!(inspected.len(), PAGES);
    inspected.sort();
    inspected.dedup();
    assert_eq!(inspected.len(), PAGES, "no URL was inspected twice");
    assert_eq!(rig.count("SELECT COUNT(*) FROM gsc_inspection"), PAGES as i64);
    assert_eq!(
        rig.count("SELECT COUNT(*) FROM integration_status WHERE provider = 'url_inspection'"),
        0
    );
}

#[tokio::test]
async fn us2_3_googles_own_daily_refusal_also_pauses_and_fills_the_local_count() {
    // The local allowance (2,000) is not reached, but Google says no after three.
    let rig = Rig::new(3).await;
    let p = rig.run(2000, &rig.request(), None, 0, None).await;
    assert_eq!(p.state, RunState::PausedQuota);
    assert_eq!(p.done, 3);
    assert!(p.message.unwrap().contains("Google asked SpiderLens to stop"));
    let key = Ledger::key(Provider::UrlInspection, ACCOUNT, &property(&rig.site));
    assert_eq!(rig.ledger.used(&key, "2026-10-14"), 2000);
}

#[test]
fn us2_4_more_urls_than_the_allowance_need_more_than_one_day() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE urls (id INTEGER PRIMARY KEY, url TEXT NOT NULL, internal INTEGER NOT NULL,
           state TEXT NOT NULL, error_class TEXT, status INTEGER, content_type TEXT, noindex INTEGER,
           canonical TEXT, depth INTEGER NOT NULL DEFAULT 0);",
    )
    .unwrap();
    for i in 0..10 {
        conn.execute(
            "INSERT INTO urls (url, internal, state, status, content_type) VALUES (?1, 1, 'crawled', 200, 'text/html')",
            [format!("https://a.test/{i}")],
        )
        .unwrap();
    }
    let req = InspectRequest::indexable("sc-domain:a.test");
    let daily = Daily {
        allowance: 4,
        tz: GOOGLE_TZ,
    };
    let now = clock(0);
    let plan = inspect::plan(&conn, &req, daily, 1, now).unwrap();
    assert_eq!((plan.selected, plan.remaining_today, plan.days_needed), (10, 3, 3));
    assert_eq!(plan.resets_at, "2026-10-15T07:00:00Z");
    let one_day = inspect::plan(&conn, &req, daily, 0, now).unwrap();
    assert_eq!(one_day.days_needed, 3);
    let few = InspectRequest {
        max: Some(3),
        ..req
    };
    assert_eq!(inspect::plan(&conn, &few, daily, 0, now).unwrap().days_needed, 1);
}

#[tokio::test]
async fn fr_008_cached_answers_cost_no_allowance_and_no_requests() {
    let rig = Rig::new(2000).await;
    let cache_path = rig.dir.path().join("cache.sqlite");
    let req = InspectRequest {
        max: Some(5),
        ..rig.request()
    };
    rig.run(2000, &req, None, 0, Some(Cache::open(&cache_path).unwrap()))
        .await;
    let calls = rig.fake.log().len();
    let key = Ledger::key(Provider::UrlInspection, ACCOUNT, &property(&rig.site));
    assert_eq!(rig.ledger.used(&key, "2026-10-14"), 5);
    let p = rig
        .run(2000, &req, None, 0, Some(Cache::open(&cache_path).unwrap()))
        .await;
    assert_eq!(p.state, RunState::Finished);
    assert_eq!(p.done, 5);
    assert_eq!(rig.fake.log().len(), calls);
    assert_eq!(rig.ledger.used(&key, "2026-10-14"), 5);
}

#[tokio::test]
async fn no_access_to_the_property_fails_the_run_at_once() {
    let rig = Rig::new(2000).await;
    let req = InspectRequest::indexable("sc-domain:not-mine.test");
    let p = rig.run(2000, &req, None, 0, None).await;
    assert_eq!(p.state, RunState::Failed);
    assert_eq!(p.done, 0);
    assert!(p.message.unwrap().contains("denied access"));
    assert_eq!(rig.fake.log().len(), 1);
}

#[tokio::test]
async fn t030_index_status_tab_lists_and_filters_the_results() {
    use spiderlens_results::{Need, Reader, UrlScope, ViewQuery};

    let rig = Rig::new(2000).await;
    let before = Reader::open(&rig.path).unwrap();
    assert!(!before.features().has(Need::UrlInspection));
    drop(before);

    rig.fake.set_answer(
        &page(&rig.site, 1),
        InspectionAnswer::canonical_elsewhere(&page(&rig.site, 1), &page(&rig.site, 2)),
    );
    rig.fake
        .set_answer(&page(&rig.site, 2), InspectionAnswer::blocked_by_robots(&page(&rig.site, 2)));
    rig.fake
        .set_answer(&page(&rig.site, 3), InspectionAnswer::not_indexed(&page(&rig.site, 3)));
    let req = InspectRequest {
        max: Some(6),
        ..rig.request()
    };
    rig.run(2000, &req, None, 0, None).await;

    let mut r = Reader::open(&rig.path).unwrap();
    assert!(r.features().has(Need::UrlInspection));
    let list = |r: &mut Reader, filter: &str| {
        let q = ViewQuery {
            tab: "index-status".into(),
            filter: filter.into(),
            scope: UrlScope::All,
            search: String::new(),
            sort: None,
            segment: None,
            values: None,
        };
        let page = r.open_view(&q, 500).unwrap();
        let total = r.materialize(page.view).unwrap();
        let rows = r.rows(page.view, 0, 1000).unwrap().rows;
        r.close_view(page.view).unwrap();
        assert_eq!(rows.len() as u32, total);
        rows
    };
    // The tab lists every crawled HTML page, the noindex one too.
    assert_eq!(list(&mut r, "all").len(), PAGES + 1);
    assert_eq!(list(&mut r, "inspected").len(), 6);
    assert_eq!(list(&mut r, "not_inspected").len(), PAGES + 1 - 6);
    // Pages 2 (blocked) and 3 (crawled, not indexed) are not indexed per Google.
    assert_eq!(list(&mut r, "not_indexed").len(), 2);
    assert_eq!(list(&mut r, "canonical_mismatch").len(), 1);
    assert_eq!(list(&mut r, "blocked_by_robots").len(), 1);
}
