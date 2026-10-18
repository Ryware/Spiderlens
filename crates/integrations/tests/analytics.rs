//! Spec 024 slice 4 (US4) against the fake Analytics Admin and Data
//! services: the property and stream pick, landing pages with offset
//! paging, combining or keeping query strings, matching and orphan
//! candidates, the token quota pause and the Analytics tab. Test names
//! carry the scenario id.

use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::Connection;
use spiderlens_config::{CrawlConfig, ModeInput, SpeedSettings};
use spiderlens_engine::crawl::{Control as CrawlControl, Crawler, open_for_update};
use spiderlens_integrations::Provider;
use spiderlens_integrations::cache::Cache;
use spiderlens_integrations::ga4::{self, Ga4Api, Ga4Request};
use spiderlens_integrations::http::Http;
use spiderlens_integrations::limits::Limits;
use spiderlens_integrations::oauth::{Endpoints, Session, StoredGoogle};
use spiderlens_integrations::run::{Control, Ctx};
use spiderlens_integrations::store::{self, RunProgress, RunState};
use spiderlens_test_fixtures::google_apis::{AnalyticsData, FakeAnalytics, GaProperty, GaQuota, GaRow, GaStream};
use spiderlens_test_fixtures::providers::{FakeGoogle, GoogleData};
use spiderlens_test_fixtures::{Site, SiteBuilder};
use tokio::sync::watch;

const HOST: &str = "127.0.0.1";
const PAGES: usize = 6;
const ACCOUNT: &str = "ana@example.com";

fn site() -> Site {
    let mut b = SiteBuilder::new();
    for i in 0..PAGES {
        let next = format!("/p/{}", (i + 1) % PAGES);
        b = b.page(HOST, &format!("/p/{i}"), "", &[&next, "/p/0"]);
    }
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

fn session(g: &FakeGoogle) -> Session {
    Session::new(
        Http::new(Duration::from_secs(10)),
        Endpoints {
            auth: g.auth_url(),
            token: g.token_url(),
            revoke: g.revoke_url(),
        },
        StoredGoogle {
            client_id: "test-client".into(),
            client_secret: None,
            refresh_token: g.issue_refresh_token(),
        },
    )
}

fn ctx(cache: Option<Cache>) -> Ctx {
    let mut limits = Limits::defaults(Provider::Analytics);
    limits.per_minute = 600;
    Ctx::new(Provider::Analytics, limits, cache)
}

struct Rig {
    site: Site,
    dir: tempfile::TempDir,
    path: PathBuf,
    google: FakeGoogle,
    fake: FakeAnalytics,
}

impl Rig {
    /// Landing pages of stream 456: page 1 twice (the second with a query
    /// string), pages 2 and 3, and `/orphan`, which the crawl never found.
    /// Stream 999 belongs to another site and must not be read.
    async fn new(quota: GaQuota) -> Self {
        let site = site();
        let dir = tempfile::tempdir().unwrap();
        let path = crawl(&site, dir.path()).await;
        let host = format!("{HOST}:{}", site.port());
        let mut p2 = GaRow::new("456", &host, "/p/2", 20, 10);
        p2.key_events = 3.0;
        p2.revenue = 42.5;
        let fake = FakeAnalytics::start(AnalyticsData {
            properties: vec![
                GaProperty {
                    id: "123".into(),
                    name: "Shop".into(),
                    streams: vec![
                        GaStream::web("456", "Website", &format!("http://{HOST}")),
                        GaStream {
                            kind: "ANDROID_APP_DATA_STREAM".into(),
                            ..GaStream::web("457", "App", "")
                        },
                    ],
                },
                GaProperty {
                    id: "124".into(),
                    name: "Blog".into(),
                    streams: vec![GaStream::web("999", "Other site", "https://other.test")],
                },
            ],
            rows: vec![
                GaRow::new("456", &host, "/p/1", 10, 8),
                GaRow::new("456", &host, "/p/1?utm_source=x", 5, 2),
                p2,
                GaRow::new("456", &host, "/p/3", 4, 4),
                GaRow::new("456", &host, "/orphan", 7, 3),
                GaRow::new("999", "other.test", "/p/4", 100, 100),
            ],
            quota,
        });
        let google = FakeGoogle::start(GoogleData {
            email: ACCOUNT.into(),
            ..GoogleData::default()
        });
        Self {
            site,
            dir,
            path,
            google,
            fake,
        }
    }

    fn api(&self) -> Ga4Api {
        Ga4Api {
            admin_base: self.fake.admin_base(),
            data_base: self.fake.data_base(),
        }
    }

    fn request(&self) -> Ga4Request {
        Ga4Request {
            start_date: "2026-09-16".into(),
            end_date: "2026-10-13".into(),
            ..Ga4Request::last_28_days("123", "456")
        }
    }

    async fn run(&self, req: &Ga4Request, resume: Option<i64>, cache: Option<Cache>) -> RunProgress {
        let update = open_for_update(&self.path, None).unwrap();
        ga4::run_pages(
            &ctx(cache),
            &session(&self.google),
            &self.api(),
            &update,
            ACCOUNT,
            req,
            resume,
            &Control::default(),
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

    fn sessions_of(&self, i: usize) -> (i64, i64, f64, f64, f64) {
        Connection::open(&self.path)
            .unwrap()
            .query_row(
                "SELECT g.sessions, g.engaged_sessions, g.engagement_rate, g.key_events, g.revenue
                 FROM ga4_pages g JOIN urls u ON u.id = g.url_id WHERE u.url = ?1",
                [self.site.url(HOST, &format!("/p/{i}"))],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .unwrap()
    }
}

#[tokio::test]
async fn us4_1_streams_are_listed_with_the_crawled_site_s_stream_first() {
    let rig = Rig::new(GaQuota::default()).await;
    let list = ga4::streams(
        &ctx(None),
        &session(&rig.google),
        &rig.api(),
        Some(&rig.site.url(HOST, "/p/0")),
    )
    .await
    .unwrap();
    // The app stream is not a web data stream and is left out.
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].property, "properties/123");
    assert_eq!(list[0].stream, "456");
    assert_eq!(list[0].property_name, "Shop");
    assert!(list[0].suggested);
    assert!(!list[1].suggested);
    assert_eq!(list[1].stream, "999");
    // Without a crawl nothing is pre-selected.
    let none = ga4::streams(&ctx(None), &session(&rig.google), &rig.api(), None)
        .await
        .unwrap();
    assert!(none.iter().all(|s| !s.suggested));
}

#[tokio::test]
async fn us4_2_landing_pages_match_crawled_urls_combine_query_strings_and_list_orphans() {
    let rig = Rig::new(GaQuota::default()).await;
    let p = rig.run(&rig.request(), None, None).await;
    assert_eq!(p.state, RunState::Finished);
    // Four landing pages after combining: 1, 2, 3 match; /orphan does not.
    assert_eq!((p.done, p.matched, p.unmatched), (4, Some(3), Some(1)));

    // Page 1 combines both variants: 15 sessions, 10 engaged.
    let (sessions, engaged, rate, _, _) = rig.sessions_of(1);
    assert_eq!((sessions, engaged), (15, 10));
    assert!((rate - 10.0 / 15.0).abs() < 1e-9);
    let (sessions, _, _, key_events, revenue) = rig.sessions_of(2);
    assert_eq!((sessions, key_events, revenue), (20, 3.0, 42.5));
    assert_eq!(rig.sessions_of(3).0, 4);
    // Every crawled page has a row; the others show zero and are marked "no data".
    assert_eq!(rig.count("SELECT COUNT(*) FROM ga4_pages"), PAGES as i64);
    assert_eq!(rig.sessions_of(0).0, 0);
    assert_eq!(
        rig.count("SELECT COUNT(*) FROM integration_status WHERE provider = 'analytics' AND reason = 'no_data'"),
        3
    );
    // The landing page the crawl never found is an orphan candidate from Analytics.
    let conn = Connection::open(&rig.path).unwrap();
    let (orphans, total) = store::orphans(&conn, Some("analytics"), 0, 10).unwrap();
    assert_eq!(total, 1);
    assert!(orphans[0].url.ends_with("/orphan"));
    assert_eq!(orphans[0].sessions, Some(7));
    assert_eq!(orphans[0].source, "analytics");
    assert_eq!(store::features(&conn).unwrap(), vec!["analytics".to_string()]);

    // The report asks for the chosen stream and range, the combined dimension and the quota.
    let reports = rig.fake.reports();
    assert_eq!(reports.len(), 1);
    let body = &reports[0];
    assert_eq!(body["dimensions"][1]["name"], "landingPage");
    assert_eq!(body["dimensionFilter"]["filter"]["stringFilter"]["value"], "456");
    assert_eq!(body["dateRanges"][0]["startDate"], "2026-09-16");
    assert_eq!(body["returnPropertyQuota"], true);
}

#[tokio::test]
async fn us4_2_keeping_query_strings_reports_them_apart() {
    let rig = Rig::new(GaQuota::default()).await;
    let req = Ga4Request {
        keep_query: true,
        ..rig.request()
    };
    let p = rig.run(&req, None, None).await;
    assert_eq!(p.state, RunState::Finished);
    assert_eq!((p.done, p.matched, p.unmatched), (5, Some(3), Some(2)));
    assert_eq!(rig.sessions_of(1).0, 10);
    assert_eq!(
        rig.fake.reports()[0]["dimensions"][1]["name"],
        "landingPagePlusQueryString"
    );
    let conn = Connection::open(&rig.path).unwrap();
    let (orphans, _) = store::orphans(&conn, Some("analytics"), 0, 10).unwrap();
    assert!(orphans.iter().any(|o| o.url.ends_with("/p/1?utm_source=x")));
}

#[tokio::test]
async fn us4_2_rows_are_read_page_by_page_with_offsets() {
    let rig = Rig::new(GaQuota::default()).await;
    let req = Ga4Request {
        page_size: Some(2),
        ..rig.request()
    };
    let p = rig.run(&req, None, None).await;
    assert_eq!(p.state, RunState::Finished);
    assert_eq!(p.done, 4);
    let offsets: Vec<u64> = rig
        .fake
        .reports()
        .iter()
        .map(|b| b["offset"].as_u64().unwrap())
        .collect();
    assert_eq!(offsets, vec![0, 2]);
    assert_eq!(rig.sessions_of(1).0, 15);
}

#[tokio::test]
async fn us4_3_the_token_allowance_pauses_the_run_and_resume_continues_at_the_offset() {
    // 25 tokens an hour at 10 per report: the second page leaves 5, too few for a third.
    let rig = Rig::new(GaQuota {
        per_hour: 25,
        per_day: 200_000,
        cost: 10,
    })
    .await;
    let req = Ga4Request {
        page_size: Some(1),
        ..rig.request()
    };
    let first = rig.run(&req, None, None).await;
    assert_eq!(first.state, RunState::PausedQuota);
    assert_eq!(first.done, 2);
    assert!(first.resume_at.is_some());
    assert!(first.message.as_deref().unwrap().contains("hourly"));
    assert_eq!(rig.fake.reports().len(), 2);
    // What was fetched is kept.
    assert!(rig.count("SELECT COUNT(*) FROM ga4_pages WHERE sessions > 0") >= 1);

    rig.fake.next_hour();
    let second = rig.run(&req, Some(first.id), None).await;
    assert_eq!(second.state, RunState::Finished);
    assert_eq!(second.done, 4);
    let offsets: Vec<u64> = rig
        .fake
        .reports()
        .iter()
        .map(|b| b["offset"].as_u64().unwrap())
        .collect();
    assert_eq!(offsets, vec![0, 1, 2, 3]);
    // Tokens used: four reports at ten each.
    assert_eq!(second.usage, Some(40));
    assert_eq!(rig.sessions_of(1).0, 15);
}

#[tokio::test]
async fn us4_3_a_refusal_from_the_provider_pauses_without_losing_anything() {
    let rig = Rig::new(GaQuota::default()).await;
    rig.fake.set_hour_left(0);
    let first = rig.run(&rig.request(), None, None).await;
    assert_eq!(first.state, RunState::PausedQuota);
    assert_eq!(first.done, 0);
    assert!(first.resume_at.is_some());
    rig.fake.next_hour();
    let second = rig.run(&rig.request(), Some(first.id), None).await;
    assert_eq!(second.state, RunState::Finished);
    assert_eq!(second.done, 4);
}

#[tokio::test]
async fn fr_008_a_second_identical_fetch_sends_no_requests() {
    let rig = Rig::new(GaQuota::default()).await;
    let cache_path = rig.dir.path().join("cache.sqlite");
    rig.run(&rig.request(), None, Some(Cache::open(&cache_path).unwrap()))
        .await;
    let before = rig.fake.reports().len();
    let p = rig
        .run(&rig.request(), None, Some(Cache::open(&cache_path).unwrap()))
        .await;
    assert_eq!(p.state, RunState::Finished);
    assert_eq!(rig.fake.reports().len(), before);
    assert_eq!(rig.sessions_of(1).0, 15);
}

#[tokio::test]
async fn no_access_to_the_property_fails_the_run() {
    let rig = Rig::new(GaQuota::default()).await;
    let req = Ga4Request::last_28_days("777", "1");
    let p = rig.run(&req, None, None).await;
    assert_eq!(p.state, RunState::Failed);
    assert!(p.message.unwrap().contains("denied access"));
}

#[tokio::test]
async fn t039_analytics_tab_lists_and_filters_the_pages() {
    use spiderlens_results::{Need, Reader, UrlScope, ViewQuery, tabs};

    let rig = Rig::new(GaQuota::default()).await;
    let before = Reader::open(&rig.path).unwrap();
    assert!(!before.features().has(Need::Analytics));
    drop(before);
    rig.run(&rig.request(), None, None).await;

    let mut r = Reader::open(&rig.path).unwrap();
    assert!(r.features().has(Need::Analytics));
    let list = |r: &mut Reader, filter: &str| {
        let q = ViewQuery {
            tab: "analytics".into(),
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
    assert_eq!(list(&mut r, "all").len(), PAGES);
    assert_eq!(list(&mut r, "with_sessions").len(), 3);
    assert_eq!(list(&mut r, "no_sessions").len(), 3);
    assert_eq!(list(&mut r, "no_data").len(), 3);
    assert_eq!(list(&mut r, "sessions_not_indexable").len(), 0);

    let tab = tabs().iter().find(|t| t.id == "analytics").unwrap();
    let col = tab.columns.iter().position(|c| c.id == "ga_sessions").unwrap();
    let url2 = rig.site.url(HOST, "/p/2");
    let row = list(&mut r, "all")
        .into_iter()
        .find(|row| row[1].as_str() == Some(url2.as_str()))
        .unwrap();
    assert_eq!(row[1 + col], serde_json::json!(20));
}
