//! Spec 024 slice 1 against fake Google services (plan D12): sign-in, Search
//! Console page data, paging, matching, orphan candidates, cache, token
//! refresh, quota pause and resume, removal, the writer lock, and no
//! secrets in the crawl file. Test names carry the scenario id.

use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::Connection;
use secrecy::{ExposeSecret, SecretString};
use spiderlens_config::{CrawlConfig, ModeInput, SpeedSettings};
use spiderlens_engine::crawl::{Control as CrawlControl, Crawler, UpdateError, WriterLock, open_for_update};
use spiderlens_integrations::cache::Cache;
use spiderlens_integrations::gsc::{self, Api, PageRequest};
use spiderlens_integrations::http::Http;
use spiderlens_integrations::limits::Limits;
use spiderlens_integrations::oauth::{self, Client, Endpoints, Session, StoredGoogle};
use spiderlens_integrations::run::{Control, Ctx};
use spiderlens_integrations::store::{self, RunState};
use spiderlens_integrations::{Provider, gsc::Property};
use spiderlens_test_fixtures::providers::{FakeGoogle, GoogleData, GscRow};
use spiderlens_test_fixtures::{Site, SiteBuilder};
use tokio::sync::watch;

const HOST: &str = "127.0.0.1";
const PAGES: usize = 20;

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
    let s = crawler.run(rx, None).await.unwrap();
    assert_eq!(s.crawled as usize, PAGES);
    path
}

fn endpoints(g: &FakeGoogle) -> Endpoints {
    Endpoints {
        auth: g.auth_url(),
        token: g.token_url(),
        revoke: g.revoke_url(),
    }
}

fn client() -> Client {
    Client {
        id: "test-client".into(),
        secret: None,
    }
}

fn session(g: &FakeGoogle, refresh: String) -> Session {
    Session::new(
        Http::new(Duration::from_secs(10)),
        endpoints(g),
        StoredGoogle {
            client_id: "test-client".into(),
            client_secret: None,
            refresh_token: refresh,
        },
    )
}

fn ctx(cache: Option<Cache>) -> Ctx {
    let mut limits = Limits::defaults(Provider::SearchConsole);
    limits.per_minute = 1200;
    Ctx::new(Provider::SearchConsole, limits, cache)
}

/// Rows for the crawled pages 0..10 (page 3 under https, as Google may
/// report it), plus `extra` pages the crawl never found.
fn rows(site: &Site, extra: usize) -> Vec<GscRow> {
    let mut v: Vec<GscRow> = (0..10)
        .map(|i| {
            let mut page = site.url(HOST, &format!("/p/{i}"));
            if i == 3 {
                page = page.replacen("http://", "https://", 1);
            }
            GscRow {
                page,
                clicks: 10 + i as u64,
                impressions: 100 * (i as u64 + 1),
                position: 4.5,
            }
        })
        .collect();
    for i in 0..extra {
        v.push(GscRow {
            page: site.url(HOST, &format!("/orphan/{i}")),
            clicks: 1,
            impressions: 5,
            position: 20.0,
        });
    }
    v
}

fn property(site: &Site) -> String {
    site.url(HOST, "/")
}

fn fake(site: &Site, extra: usize) -> FakeGoogle {
    FakeGoogle::start(GoogleData {
        email: "ana@example.com".into(),
        sites: vec![(property(site), "siteOwner".into()), ("sc-domain:other.test".into(), "siteFullUser".into())],
        rows: rows(site, extra),
        ..GoogleData::default()
    })
}

fn req(site: &Site) -> PageRequest {
    PageRequest {
        property: property(site),
        start_date: "2026-09-14".into(),
        end_date: "2026-10-11".into(),
        search_type: "web".into(),
        fresh: false,
    }
}

async fn fetch(g: &FakeGoogle, path: &Path, site: &Site, cache: Option<Cache>) -> store::RunProgress {
    let update = open_for_update(path, None).unwrap();
    let s = session(g, g.issue_refresh_token());
    let api = Api { base: g.webmasters_url() };
    gsc::run_pages(&ctx(cache), &s, &api, &update, "ana@example.com", &req(site), None, &Control::default(), &mut |_| {})
        .await
        .unwrap()
}

fn count(path: &Path, sql: &str) -> i64 {
    Connection::open(path).unwrap().query_row(sql, [], |r| r.get(0)).unwrap()
}

#[tokio::test]
async fn sign_in_uses_pkce_and_loopback_and_labels_the_account() {
    let g = FakeGoogle::start(GoogleData {
        email: "ana@example.com".into(),
        ..GoogleData::default()
    });
    let http = Http::new(Duration::from_secs(10));
    let scope = oauth::scope_for("search_console").unwrap();
    let a = oauth::authorize(&http, &client(), &endpoints(&g), &[scope], Duration::from_secs(20), |url| {
        // Stands in for the system browser: follows the consent redirect
        // back to SpiderLens's loopback listener.
        let url = url.to_string();
        tokio::spawn(async move {
            reqwest::get(url).await.unwrap();
        });
    })
    .await
    .unwrap();
    assert_eq!(a.email, "ana@example.com");
    assert!(a.scopes.iter().any(|s| s == scope));
    assert!(!a.refresh_token.expose_secret().is_empty());

    // Forget revokes at Google (US7.3, FR-017).
    oauth::revoke(&http, &endpoints(&g), &a.refresh_token).await.unwrap();
    assert_eq!(g.revoked(), vec![a.refresh_token.expose_secret().to_string()]);
}

#[tokio::test]
async fn us1_2_suggests_the_property_covering_the_crawled_site() {
    let site = site();
    let g = fake(&site, 0);
    let s = session(&g, g.issue_refresh_token());
    let props: Vec<Property> = gsc::properties(&ctx(None), &s, &Api { base: g.webmasters_url() }, Some(&site.url(HOST, "/p/0")))
        .await
        .unwrap();
    assert_eq!(props.len(), 2);
    assert!(props[0].suggested);
    assert_eq!(props[0].id, property(&site));
}

#[tokio::test]
async fn us1_3_to_us1_5_pages_match_zero_rows_and_orphans_with_paging() {
    let site = site();
    let dir = tempfile::tempdir().unwrap();
    let path = crawl(&site, dir.path()).await;
    // 60,000 rows the crawl never found: three pages of 25,000.
    let g = fake(&site, 60_000);
    let p = fetch(&g, &path, &site, None).await;
    assert_eq!(p.state, RunState::Finished);
    assert_eq!(p.matched, Some(10));
    assert_eq!(p.unmatched, Some(60_000));
    assert_eq!(p.done, 60_010);
    assert_eq!(g.log().iter().filter(|l| l.path.ends_with("/query")).count(), 3);

    // Every crawled page has a row; the ten with data carry Google's numbers,
    // including the https variant of page 3.
    assert_eq!(count(&path, "SELECT COUNT(*) FROM gsc_pages"), PAGES as i64);
    let url3 = site.url(HOST, "/p/3");
    let c: i64 = Connection::open(&path)
        .unwrap()
        .query_row(
            "SELECT g.clicks FROM gsc_pages g JOIN urls u ON u.id = g.url_id WHERE u.url = ?1",
            [url3],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(c, 13);
    // The others show zero and are marked "no data", not "not fetched".
    assert_eq!(count(&path, "SELECT COUNT(*) FROM gsc_pages WHERE impressions = 0"), (PAGES - 10) as i64);
    assert_eq!(
        count(&path, "SELECT COUNT(*) FROM integration_status WHERE provider = 'search_console' AND reason = 'no_data'"),
        (PAGES - 10) as i64
    );
    assert_eq!(count(&path, "SELECT COUNT(*) FROM orphan_candidates WHERE source = 'search_console'"), 60_000);
    let conn = Connection::open(&path).unwrap();
    assert_eq!(store::features(&conn).unwrap(), vec!["search_console".to_string()]);
    assert_eq!(count(&path, "SELECT COUNT(*) FROM events WHERE kind = 'integration_run'"), 1);
}

#[tokio::test]
async fn sc_005_a_second_identical_fetch_sends_no_requests() {
    let site = site();
    let dir = tempfile::tempdir().unwrap();
    let path = crawl(&site, dir.path()).await;
    let g = fake(&site, 5);
    let cache_path = dir.path().join("cache.sqlite");
    fetch(&g, &path, &site, Some(Cache::open(&cache_path).unwrap())).await;
    let before = g.api_calls();
    let p = fetch(&g, &path, &site, Some(Cache::open(&cache_path).unwrap())).await;
    assert_eq!(p.state, RunState::Finished);
    assert_eq!(g.api_calls(), before);
    assert_eq!(count(&path, "SELECT COUNT(*) FROM orphan_candidates"), 5);
}

#[tokio::test]
async fn expired_access_tokens_are_refreshed_once() {
    let site = site();
    let dir = tempfile::tempdir().unwrap();
    let path = crawl(&site, dir.path()).await;
    let g = fake(&site, 0);
    let s = session(&g, g.issue_refresh_token());
    let api = Api { base: g.webmasters_url() };
    gsc::properties(&ctx(None), &s, &api, None).await.unwrap();
    g.expire_access_tokens();
    let update = open_for_update(&path, None).unwrap();
    let p = gsc::run_pages(&ctx(None), &s, &api, &update, "a", &req(&site), None, &Control::default(), &mut |_| {})
        .await
        .unwrap();
    assert_eq!(p.state, RunState::Finished);
}

#[tokio::test]
async fn fr_006_quota_pauses_with_reset_time_and_resume_continues_from_the_cursor() {
    let site = site();
    let dir = tempfile::tempdir().unwrap();
    let path = crawl(&site, dir.path()).await;
    let g = fake(&site, 30_000);
    let s = session(&g, g.issue_refresh_token());
    let api = Api { base: g.webmasters_url() };
    // The first page succeeds; the second hits the daily quota.
    let quota = r#"{"error":{"code":429,"message":"Quota exceeded for quota metric 'Queries' and limit 'Queries per day'"}}"#;
    {
        let update = open_for_update(&path, None).unwrap();
        let first = gsc::run_pages(
            &ctx(None),
            &s,
            &api,
            &update,
            "a",
            &req(&site),
            None,
            &Control::default(),
            &mut |p| {
                if p.done > 0 && p.state == RunState::Running {
                    g.fail_next(429, quota, None);
                }
            },
        )
        .await
        .unwrap();
        assert_eq!(first.state, RunState::PausedQuota);
        assert!(first.resume_at.is_some());
        assert_eq!(first.done, 25_000);
        assert!(first.message.unwrap().contains("slow down"));
    }
    let queries_before = g.log().iter().filter(|l| l.path.ends_with("/query")).count();
    let update = open_for_update(&path, None).unwrap();
    let id = store::runs(update.conn()).unwrap()[0].id;
    let p = gsc::run_pages(&ctx(None), &s, &api, &update, "a", &req(&site), Some(id), &Control::default(), &mut |_| {})
        .await
        .unwrap();
    assert_eq!(p.state, RunState::Finished);
    assert_eq!(p.done, 30_010);
    // Only the remaining page was requested, starting after the first.
    let after: Vec<_> = g.log().into_iter().filter(|l| l.path.ends_with("/query")).skip(queries_before).collect();
    assert_eq!(after.len(), 1);
    assert!(after[0].body.contains("\"startRow\":25000"));
    drop(update);
    assert_eq!(count(&path, "SELECT COUNT(*) FROM orphan_candidates"), 30_000);
}

#[tokio::test]
async fn fr_002_a_running_crawl_holds_the_file() {
    let site = site();
    let dir = tempfile::tempdir().unwrap();
    let path = crawl(&site, dir.path()).await;
    let _crawl = WriterLock::acquire(&path).unwrap();
    assert!(matches!(open_for_update(&path, None), Err(UpdateError::InUse(_))));
}

#[tokio::test]
async fn fr_011_remove_drops_one_provider_s_data() {
    let site = site();
    let dir = tempfile::tempdir().unwrap();
    let path = crawl(&site, dir.path()).await;
    let g = fake(&site, 3);
    fetch(&g, &path, &site, None).await;
    let update = open_for_update(&path, None).unwrap();
    store::remove(update.conn(), Some(Provider::SearchConsole)).unwrap();
    drop(update);
    for t in ["gsc_pages", "orphan_candidates", "integration_runs", "integration_status"] {
        assert_eq!(count(&path, &format!("SELECT COUNT(*) FROM {t}")), 0, "{t}");
    }
    assert!(store::features(&Connection::open(&path).unwrap()).unwrap().is_empty());
    // The crawl itself is untouched.
    assert_eq!(count(&path, "SELECT COUNT(*) FROM urls WHERE internal = 1"), PAGES as i64);
}

#[tokio::test]
async fn us1_6_top_queries_on_demand() {
    let site = site();
    let dir = tempfile::tempdir().unwrap();
    let path = crawl(&site, dir.path()).await;
    let page = site.url(HOST, "/p/1");
    let g = FakeGoogle::start(GoogleData {
        email: "a".into(),
        sites: vec![(property(&site), "siteOwner".into())],
        rows: rows(&site, 0),
        queries: [(page.clone(), vec![("spider lens".to_string(), 7, 70), ("seo crawler".to_string(), 2, 40)])]
            .into_iter()
            .collect(),
    });
    fetch(&g, &path, &site, None).await;
    let update = open_for_update(&path, None).unwrap();
    let id: i64 = update.conn().query_row("SELECT id FROM urls WHERE url = ?1", [&page], |r| r.get(0)).unwrap();
    let s = session(&g, g.issue_refresh_token());
    let q = gsc::top_queries(&ctx(None), &s, &Api { base: g.webmasters_url() }, update.conn(), "a", id)
        .await
        .unwrap();
    assert_eq!(q.len(), 2);
    assert_eq!(q[0].query, "spider lens");
    assert_eq!(q[0].clicks, 7);
}

#[tokio::test]
async fn sc_004_no_token_reaches_the_crawl_file() {
    let site = site();
    let dir = tempfile::tempdir().unwrap();
    let path = crawl(&site, dir.path()).await;
    let g = fake(&site, 2);
    let refresh = g.issue_refresh_token();
    let update = open_for_update(&path, None).unwrap();
    let s = session(&g, refresh.clone());
    let cache_path = dir.path().join("cache.sqlite");
    gsc::run_pages(
        &ctx(Some(Cache::open(&cache_path).unwrap())),
        &s,
        &Api { base: g.webmasters_url() },
        &update,
        "ana@example.com",
        &req(&site),
        None,
        &Control::default(),
        &mut |_| {},
    )
    .await
    .unwrap();
    let access = s.bearer().await.unwrap();
    drop(update);
    let secret = SecretString::from(refresh);
    for file in [&path, &cache_path] {
        let bytes = std::fs::read(file).unwrap();
        for needle in [secret.expose_secret(), access.as_str()] {
            assert!(
                !bytes.windows(needle.len()).any(|w| w == needle.as_bytes()),
                "{} contains a token",
                file.display()
            );
        }
    }
}
