//! Spec 024 slice 3 against fake Chrome UX Report and PageSpeed services
//! (plan D12): field data per page and device with ratings and period, the
//! origin fallback cached per origin, lab tests with strategy choice, stored
//! scores, metrics, opportunities and audits, the rate and concurrency limits,
//! slow answers, quota pauses, the daily allowance, removal, and no secrets in
//! the crawl file or the cache. Test names carry the scenario id.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use rusqlite::Connection;
use secrecy::SecretString;
use spiderlens_config::{CrawlConfig, ModeInput, SpeedSettings};
use spiderlens_engine::crawl::{Control as CrawlControl, Crawler, open_for_update};
use spiderlens_integrations::Provider;
use spiderlens_integrations::cache::Cache;
use spiderlens_integrations::credentials::MemoryStore;
use spiderlens_integrations::crux::{self, FieldRequest, FormFactor};
use spiderlens_integrations::hub::Hub;
use spiderlens_integrations::limits::{Daily, GOOGLE_TZ, Ledger, Limits};
use spiderlens_integrations::perf::{Allowance, UrlSet};
use spiderlens_integrations::psi::{self, LabRequest, StrategyChoice};
use spiderlens_integrations::run::{Control, Ctx};
use spiderlens_integrations::settings::{Folder, ProviderSettings};
use spiderlens_integrations::store::{self, RunProgress, RunState};
use spiderlens_test_fixtures::perf_providers::{CruxVitals, FakePerf, PerfData, PsiPage};
use spiderlens_test_fixtures::{Site, SiteBuilder};
use tokio::sync::watch;

const HOST: &str = "127.0.0.1";
const PAGES: usize = 20;
const KEY: &str = "AIza-test-secret-key-123";

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

fn page_url(site: &Site, i: usize) -> String {
    site.url(HOST, &format!("/p/{i}"))
}

fn origin(site: &Site) -> String {
    format!("http://{HOST}:{}", site.port())
}

fn ctx(p: Provider, per_minute: u32, concurrency: u32, cache: Option<Cache>) -> Ctx {
    let mut limits = Limits::defaults(p);
    limits.per_minute = per_minute;
    limits.concurrency = concurrency;
    Ctx::new(p, limits, cache)
}

fn key() -> SecretString {
    SecretString::from(KEY)
}

fn count(path: &Path, sql: &str) -> i64 {
    Connection::open(path)
        .unwrap()
        .query_row(sql, [], |r| r.get(0))
        .unwrap()
}

/// Pages `0..own` have their own record on both devices (even pages good,
/// odd pages poor); every origin has a record for phones only.
fn field_data(site: &Site, own: usize, with_origin: bool) -> PerfData {
    let mut d = PerfData {
        api_key: KEY.into(),
        ..PerfData::default()
    };
    for i in 0..own {
        let v = if i % 2 == 0 {
            CruxVitals::GOOD
        } else {
            CruxVitals::POOR
        };
        for device in ["PHONE", "DESKTOP"] {
            d.crux.insert((page_url(site, i), device.into()), v);
        }
    }
    if with_origin {
        d.crux.insert((origin(site), "PHONE".into()), CruxVitals::POOR);
    }
    d
}

async fn fetch_field(
    fake: &FakePerf,
    path: &Path,
    req: &FieldRequest,
    ctx: &Ctx,
    resume: Option<i64>,
) -> RunProgress {
    let update = open_for_update(path, None).unwrap();
    crux::run_field_data(
        ctx,
        &key(),
        &crux::Api {
            base: fake.crux_url(),
        },
        &update,
        "main",
        req,
        resume,
        &Control::default(),
        &mut |_| {},
    )
    .await
    .unwrap()
}

async fn fetch_lab(
    fake: &FakePerf,
    path: &Path,
    req: &LabRequest,
    ctx: &Ctx,
    allowance: Option<&Allowance>,
    resume: Option<i64>,
) -> RunProgress {
    let update = open_for_update(path, None).unwrap();
    psi::run_lab_tests(
        ctx,
        &key(),
        &psi::Api { base: fake.psi_url() },
        allowance,
        &update,
        "main",
        req,
        resume,
        &Control::default(),
        &mut |_| {},
    )
    .await
    .unwrap()
}

/// No more calls by each moment than the bucket allows: its starting burst
/// plus the per-minute rate since the first call (SC-003, "no run ever
/// exceeds a provider's configured rate").
fn assert_within_rate(times: &[f64], per_minute: f64) {
    let burst = (per_minute / 10.0 + 1.0).min(per_minute);
    let rate = per_minute / 60.0;
    let first = times[0];
    for (k, t) in times.iter().enumerate() {
        let allowed = burst + rate * (t - first) + 1.5;
        assert!(
            (k + 1) as f64 <= allowed,
            "{} calls by {:.2} s, at most {:.1} allowed",
            k + 1,
            t - first,
            allowed
        );
    }
}

#[tokio::test]
async fn us3_1_field_data_per_page_and_device_with_ratings_and_period() {
    let site = site();
    let dir = tempfile::tempdir().unwrap();
    let path = crawl(&site, dir.path()).await;
    let fake = FakePerf::start(field_data(&site, PAGES, false));

    let p = fetch_field(
        &fake,
        &path,
        &FieldRequest::default(),
        &ctx(Provider::FieldData, 1200, 4, None),
        None,
    )
    .await;
    assert_eq!(p.state, RunState::Finished);
    assert_eq!((p.requested, p.done, p.failed), (PAGES as i64, PAGES as i64, 0));
    // One request per page and device.
    assert_eq!(fake.crux_calls().len(), PAGES * 2);
    assert_eq!(count(&path, "SELECT COUNT(*) FROM cwv_field"), (PAGES * 2) as i64);
    assert_eq!(
        count(&path, "SELECT COUNT(*) FROM cwv_field WHERE level = 'url'"),
        (PAGES * 2) as i64
    );

    let conn = Connection::open(&path).unwrap();
    let row = |i: usize, device: &str| -> (i64, i64, f64, String, String, String, String, String) {
        conn.query_row(
            "SELECT f.lcp_ms, f.inp_ms, f.cls, f.lcp_rating, f.inp_rating, f.cls_rating, f.period_start, f.period_end
             FROM cwv_field f JOIN urls u ON u.id = f.url_id WHERE u.url = ?1 AND f.form_factor = ?2",
            [page_url(&site, i), device.to_string()],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                    r.get(7)?,
                ))
            },
        )
        .unwrap()
    };
    let good = row(0, "phone");
    assert_eq!((good.0, good.1, good.2), (2100, 150, 0.05));
    assert_eq!(
        (good.3.as_str(), good.4.as_str(), good.5.as_str()),
        ("good", "good", "good")
    );
    // The 28-day collection period is stored with the values.
    assert_eq!((good.6.as_str(), good.7.as_str()), ("2026-09-14", "2026-10-11"));
    let poor = row(1, "desktop");
    assert_eq!(
        (poor.3.as_str(), poor.4.as_str(), poor.5.as_str()),
        ("poor", "poor", "poor")
    );
    // The run is recorded and the feature is listed for the results tabs.
    assert_eq!(store::features(&conn).unwrap(), vec!["field_data".to_string()]);
}

#[tokio::test]
async fn us3_2_origin_data_is_labelled_and_fetched_once_per_origin_and_device() {
    let site = site();
    let dir = tempfile::tempdir().unwrap();
    let path = crawl(&site, dir.path()).await;
    // Five pages have their own data; the other fifteen have too little traffic.
    let fake = FakePerf::start(field_data(&site, 5, true));
    let cache_path = dir.path().join("cache.sqlite");

    let p = fetch_field(
        &fake,
        &path,
        &FieldRequest::default(),
        &ctx(
            Provider::FieldData,
            1200,
            4,
            Some(Cache::open(&cache_path).unwrap()),
        ),
        None,
    )
    .await;
    assert_eq!(p.state, RunState::Finished);
    assert_eq!(
        count(&path, "SELECT COUNT(*) FROM cwv_field WHERE level = 'url'"),
        10
    );
    // Phones fall back to the origin; the origin has nothing for desktops.
    assert_eq!(
        count(
            &path,
            "SELECT COUNT(*) FROM cwv_field WHERE level = 'origin' AND form_factor = 'phone'"
        ),
        15
    );
    assert_eq!(
        count(
            &path,
            "SELECT COUNT(*) FROM cwv_field WHERE level = 'none' AND form_factor = 'desktop'"
        ),
        15
    );
    // Origin rows carry the origin's numbers, flagged as origin data.
    assert_eq!(
        count(
            &path,
            "SELECT COUNT(*) FROM cwv_field WHERE level = 'origin' AND lcp_ms = 4800 AND lcp_rating = 'poor'"
        ),
        15
    );
    // Fifteen pages asked about the origin; it was fetched once per device.
    let origin_calls: Vec<_> = fake
        .crux_calls()
        .into_iter()
        .filter(|c| c.detail == "origin")
        .collect();
    assert_eq!(origin_calls.len(), 2, "{origin_calls:?}");
    // Every page has something to show, so none is marked "no data".
    assert_eq!(count(&path, "SELECT COUNT(*) FROM integration_status"), 0);

    // The same day's second run is answered from the cache, 404s included.
    let before = fake.crux_calls().len();
    let again = fetch_field(
        &fake,
        &path,
        &FieldRequest::default(),
        &ctx(
            Provider::FieldData,
            1200,
            4,
            Some(Cache::open(&cache_path).unwrap()),
        ),
        None,
    )
    .await;
    assert_eq!(again.state, RunState::Finished);
    assert_eq!(fake.crux_calls().len(), before);
}

#[tokio::test]
async fn us3_2_a_page_with_no_data_anywhere_says_so() {
    let site = site();
    let dir = tempfile::tempdir().unwrap();
    let path = crawl(&site, dir.path()).await;
    let fake = FakePerf::start(field_data(&site, 3, false));
    let req = FieldRequest {
        devices: vec![FormFactor::Phone],
        ..FieldRequest::default()
    };
    let p = fetch_field(&fake, &path, &req, &ctx(Provider::FieldData, 1200, 4, None), None).await;
    assert_eq!(p.state, RunState::Finished);
    assert_eq!(count(&path, "SELECT COUNT(*) FROM cwv_field"), PAGES as i64);
    assert_eq!(
        count(&path, "SELECT COUNT(*) FROM cwv_field WHERE level = 'none'"),
        (PAGES - 3) as i64
    );
    assert_eq!(
        count(
            &path,
            "SELECT COUNT(*) FROM integration_status WHERE provider = 'field_data' AND reason = 'no_data'"
        ),
        (PAGES - 3) as i64
    );
}

#[tokio::test]
async fn us3_4_field_data_requests_never_exceed_the_configured_rate() {
    let site = site();
    let dir = tempfile::tempdir().unwrap();
    let path = crawl(&site, dir.path()).await;
    let fake = FakePerf::start(field_data(&site, PAGES, false));
    let per_minute = 300;
    let p = fetch_field(
        &fake,
        &path,
        &FieldRequest::default(),
        &ctx(Provider::FieldData, per_minute, 8, None),
        None,
    )
    .await;
    assert_eq!(p.state, RunState::Finished);
    let times: Vec<f64> = fake.crux_calls().iter().map(|c| c.at).collect();
    assert_eq!(times.len(), PAGES * 2);
    assert_within_rate(&times, f64::from(per_minute));
    // 40 calls with a burst of 31 at 5 a second: the rest were held back.
    assert!(times[times.len() - 1] - times[0] >= 1.0, "{times:?}");
}

#[test]
fn sc_003_one_thousand_pages_on_two_devices_fit_in_fifteen_minutes() {
    let e = crux::estimate(1000, 2, &Limits::defaults(Provider::FieldData));
    assert_eq!(e.requests, 2000);
    assert!(e.duration_secs <= 15 * 60, "{} s", e.duration_secs);
    // Lab tests are paced by Lighthouse, and the estimate says so honestly.
    let lab = psi::estimate(1000, 2, &Limits::defaults(Provider::LabTests));
    assert!(lab.duration_secs > e.duration_secs);
}

#[tokio::test]
async fn us3_3_lab_tests_store_scores_metrics_opportunities_and_audits() {
    let site = site();
    let dir = tempfile::tempdir().unwrap();
    let path = crawl(&site, dir.path()).await;
    let mut data = PerfData {
        api_key: KEY.into(),
        ..PerfData::default()
    };
    data.psi_pages.insert(
        page_url(&site, 0),
        PsiPage {
            performance: 0.5,
            ..PsiPage::default()
        },
    );
    let fake = FakePerf::start(data);

    // Four pages, both devices.
    let req = LabRequest {
        strategy: StrategyChoice::Both,
        urls: UrlSet::All,
        max: Some(4),
        fresh: false,
    };
    let p = fetch_lab(
        &fake,
        &path,
        &req,
        &ctx(Provider::LabTests, 6000, 4, None),
        None,
        None,
    )
    .await;
    assert_eq!(p.state, RunState::Finished);
    assert_eq!(p.done, 4);
    let calls = fake.psi_calls();
    assert_eq!(calls.len(), 8);
    assert_eq!(calls.iter().filter(|c| c.device == "mobile").count(), 4);
    assert_eq!(calls.iter().filter(|c| c.device == "desktop").count(), 4);
    // All four categories are asked for on every call.
    assert!(
        calls
            .iter()
            .all(|c| c.detail == "PERFORMANCE,ACCESSIBILITY,BEST_PRACTICES,SEO")
    );
    assert_eq!(count(&path, "SELECT COUNT(*) FROM psi_lab"), 8);

    let conn = Connection::open(&path).unwrap();
    let lab_of = |strategy: &str| -> serde_json::Value {
        let json: String = conn
            .query_row(
                "SELECT json_object('performance', l.performance, 'accessibility', l.accessibility,
                   'best_practices', l.best_practices, 'seo', l.seo, 'lcp_ms', l.lcp_ms, 'cls', l.cls,
                   'tbt_ms', l.tbt_ms, 'speed_index_ms', l.speed_index_ms, 'opportunities', l.opportunities)
                 FROM psi_lab l JOIN urls u ON u.id = l.url_id WHERE u.url = ?1 AND l.strategy = ?2",
                [page_url(&site, 0), strategy.to_string()],
                |r| r.get(0),
            )
            .unwrap();
        serde_json::from_str(&json).unwrap()
    };
    let mobile = lab_of("mobile");
    assert_eq!(mobile["performance"], 50);
    assert_eq!(mobile["accessibility"], 92);
    assert_eq!(mobile["best_practices"], 96);
    assert_eq!(mobile["seo"], 80);
    assert_eq!(mobile["lcp_ms"], 2412);
    assert_eq!(mobile["tbt_ms"], 120);
    assert_eq!(mobile["speed_index_ms"], 3100);
    assert_eq!(mobile["cls"], 0.043);
    assert_eq!(lab_of("desktop")["performance"], 50);
    let audits: Vec<u8> = conn
        .query_row(
            "SELECT l.audits FROM psi_lab l JOIN urls u ON u.id = l.url_id
             WHERE u.url = ?1 AND l.strategy = 'mobile'",
            [page_url(&site, 0)],
            |r| r.get(0),
        )
        .unwrap();
    // The five biggest savings, largest first; the sixth is left out.
    let opps: Vec<serde_json::Value> =
        serde_json::from_str(mobile["opportunities"].as_str().unwrap()).unwrap();
    let ids: Vec<&str> = opps.iter().map(|o| o["id"].as_str().unwrap()).collect();
    assert_eq!(
        ids,
        [
            "render-blocking-resources",
            "unused-javascript",
            "uses-optimized-images",
            "uses-text-compression",
            "unused-css-rules"
        ]
    );
    assert_eq!(opps[0]["savings_ms"], 850);
    // The accessibility, best-practices and SEO audits are kept for spec 026,
    // without screenshots.
    let audits = psi::decode_audits(&audits).unwrap();
    assert_eq!(audits.len(), 4);
    let alt = audits.iter().find(|a| a["id"] == "image-alt").unwrap();
    assert_eq!(alt["category"], "accessibility");
    assert_eq!(alt["details"]["items"][0]["node"]["selector"], "img");
    assert!(audits.iter().all(|a| a["id"] != "final-screenshot"));
    assert_eq!(store::features(&conn).unwrap(), vec!["lab_tests".to_string()]);

    // Choosing desktop only sends one call per page.
    let before = fake.psi_calls().len();
    let one = LabRequest {
        strategy: StrategyChoice::Desktop,
        max: Some(2),
        fresh: true,
        ..req
    };
    fetch_lab(
        &fake,
        &path,
        &one,
        &ctx(Provider::LabTests, 6000, 4, None),
        None,
        None,
    )
    .await;
    let new: Vec<_> = fake.psi_calls().into_iter().skip(before).collect();
    assert_eq!(new.len(), 2);
    assert!(new.iter().all(|c| c.device == "desktop"));
}

#[tokio::test]
async fn us3_4_slow_lab_tests_run_in_parallel_up_to_the_concurrency_limit() {
    let site = site();
    let dir = tempfile::tempdir().unwrap();
    let path = crawl(&site, dir.path()).await;
    let fake = FakePerf::start(PerfData {
        api_key: KEY.into(),
        psi_delay: Duration::from_millis(250),
        ..PerfData::default()
    });
    let req = LabRequest {
        strategy: StrategyChoice::Mobile,
        urls: UrlSet::All,
        max: Some(9),
        fresh: false,
    };
    let start = std::time::Instant::now();
    let p = fetch_lab(
        &fake,
        &path,
        &req,
        &ctx(Provider::LabTests, 6000, 3, None),
        None,
        None,
    )
    .await;
    let took = start.elapsed();
    assert_eq!(p.state, RunState::Finished);
    assert_eq!(p.done, 9);
    // Three at a time: never more, and clearly faster than one at a time.
    assert_eq!(fake.max_psi_in_flight(), 3);
    assert!(took < Duration::from_millis(250 * 9), "{took:?}");
    assert_within_rate(&fake.psi_calls().iter().map(|c| c.at).collect::<Vec<_>>(), 6000.0);
}

#[tokio::test]
async fn fr_006_a_quota_stop_keeps_the_waiting_pages_and_resume_finishes_them() {
    let site = site();
    let dir = tempfile::tempdir().unwrap();
    let path = crawl(&site, dir.path()).await;
    let fake = FakePerf::start(PerfData {
        api_key: KEY.into(),
        ..PerfData::default()
    });
    let req = LabRequest {
        strategy: StrategyChoice::Mobile,
        urls: UrlSet::All,
        max: Some(4),
        fresh: false,
    };
    let quota = r#"{"error":{"code":429,"message":"Quota exceeded for quota metric 'Queries' and limit 'Queries per day'"}}"#;
    fake.fail_next("psi", 429, quota);
    let first = fetch_lab(
        &fake,
        &path,
        &req,
        &ctx(Provider::LabTests, 6000, 1, None),
        None,
        None,
    )
    .await;
    assert_eq!(first.state, RunState::PausedQuota);
    assert!(first.resume_at.is_some());
    assert_eq!(first.done, 0);
    assert_eq!(
        count(
            &path,
            "SELECT COUNT(*) FROM integration_queue WHERE state = 'pending'"
        ),
        4
    );

    let second = fetch_lab(
        &fake,
        &path,
        &req,
        &ctx(Provider::LabTests, 6000, 1, None),
        None,
        Some(first.id),
    )
    .await;
    assert_eq!(second.state, RunState::Finished);
    assert_eq!(second.done, 4);
    assert_eq!(
        count(
            &path,
            "SELECT COUNT(*) FROM integration_queue WHERE state = 'pending'"
        ),
        0
    );
    assert_eq!(count(&path, "SELECT COUNT(*) FROM psi_lab"), 4);
}

#[tokio::test]
async fn fr_006_the_daily_allowance_stops_a_run_and_the_ledger_is_shared() {
    let site = site();
    let dir = tempfile::tempdir().unwrap();
    let path = crawl(&site, dir.path()).await;
    let fake = FakePerf::start(PerfData {
        api_key: KEY.into(),
        ..PerfData::default()
    });
    let allowance = Allowance::new(
        Ledger::new(dir.path().join("usage.json")),
        "lab_tests|main|-".into(),
        Daily {
            allowance: 2,
            tz: GOOGLE_TZ,
        },
    );
    let req = LabRequest {
        strategy: StrategyChoice::Mobile,
        urls: UrlSet::All,
        max: Some(5),
        fresh: false,
    };
    let p = fetch_lab(
        &fake,
        &path,
        &req,
        &ctx(Provider::LabTests, 6000, 1, None),
        Some(&allowance),
        None,
    )
    .await;
    assert_eq!(p.state, RunState::PausedQuota);
    assert!(p.resume_at.is_some());
    assert_eq!(p.done, 2);
    assert_eq!(fake.psi_calls().len(), 2);
    assert_eq!(allowance.used_today(), 2);
}

#[tokio::test]
async fn a_key_the_provider_refuses_fails_the_run_at_once() {
    let site = site();
    let dir = tempfile::tempdir().unwrap();
    let path = crawl(&site, dir.path()).await;
    let fake = FakePerf::start(PerfData {
        api_key: "another-key".into(),
        ..PerfData::default()
    });
    let p = fetch_field(
        &fake,
        &path,
        &FieldRequest::default(),
        &ctx(Provider::FieldData, 1200, 4, None),
        None,
    )
    .await;
    assert_eq!(p.state, RunState::Failed);
    assert!(p.message.unwrap().contains("API key not valid"));
    assert_eq!(count(&path, "SELECT COUNT(*) FROM cwv_field"), 0);
}

#[tokio::test]
async fn sc_004_the_key_never_reaches_the_crawl_file_or_the_cache() {
    let site = site();
    let dir = tempfile::tempdir().unwrap();
    let path = crawl(&site, dir.path()).await;
    let fake = FakePerf::start(field_data(&site, 5, true));
    let cache_path = dir.path().join("cache.sqlite");
    fetch_field(
        &fake,
        &path,
        &FieldRequest::default(),
        &ctx(
            Provider::FieldData,
            1200,
            4,
            Some(Cache::open(&cache_path).unwrap()),
        ),
        None,
    )
    .await;
    let lab = LabRequest {
        max: Some(3),
        ..LabRequest::default()
    };
    fetch_lab(
        &fake,
        &path,
        &lab,
        &ctx(
            Provider::LabTests,
            6000,
            2,
            Some(Cache::open(&cache_path).unwrap()),
        ),
        None,
        None,
    )
    .await;
    for file in [&path, &cache_path] {
        let bytes = std::fs::read(file).unwrap();
        assert!(
            !bytes.windows(KEY.len()).any(|w| w == KEY.as_bytes()),
            "{} contains the key",
            file.display()
        );
    }
}

#[tokio::test]
async fn fr_011_remove_drops_field_data_and_lab_tests() {
    let site = site();
    let dir = tempfile::tempdir().unwrap();
    let path = crawl(&site, dir.path()).await;
    let fake = FakePerf::start(field_data(&site, 5, true));
    fetch_field(
        &fake,
        &path,
        &FieldRequest::default(),
        &ctx(Provider::FieldData, 1200, 4, None),
        None,
    )
    .await;
    let lab = LabRequest {
        max: Some(3),
        ..LabRequest::default()
    };
    fetch_lab(
        &fake,
        &path,
        &lab,
        &ctx(Provider::LabTests, 6000, 2, None),
        None,
        None,
    )
    .await;

    let update = open_for_update(&path, None).unwrap();
    store::remove(update.conn(), Some(Provider::FieldData)).unwrap();
    store::remove(update.conn(), Some(Provider::LabTests)).unwrap();
    drop(update);
    for t in [
        "cwv_field",
        "psi_lab",
        "integration_runs",
        "integration_queue",
        "integration_status",
    ] {
        assert_eq!(count(&path, &format!("SELECT COUNT(*) FROM {t}")), 0, "{t}");
    }
    assert!(
        store::features(&Connection::open(&path).unwrap())
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        count(&path, "SELECT COUNT(*) FROM urls WHERE internal = 1"),
        PAGES as i64
    );
}

#[tokio::test]
async fn us3_page_experience_tab_lists_pages_and_filters_by_poor_vitals_and_origin_data() {
    use spiderlens_results::{Need, Reader, UrlScope, ViewQuery, tabs};

    let site = site();
    let dir = tempfile::tempdir().unwrap();
    let path = crawl(&site, dir.path()).await;

    let r = Reader::open(&path).unwrap();
    assert!(!r.features().has(Need::FieldData));
    drop(r);

    let fake = FakePerf::start(field_data(&site, 5, true));
    fetch_field(
        &fake,
        &path,
        &FieldRequest::default(),
        &ctx(Provider::FieldData, 1200, 4, None),
        None,
    )
    .await;

    let mut r = Reader::open(&path).unwrap();
    assert!(r.features().has(Need::FieldData));
    let q = |filter: &str| ViewQuery {
        tab: "page-experience".into(),
        filter: filter.into(),
        scope: UrlScope::All,
        search: String::new(),
        sort: None,
        segment: None,
        values: None,
    };
    let list = |r: &mut Reader, filter: &str| {
        let page = r.open_view(&q(filter), 500).unwrap();
        let total = r.materialize(page.view).unwrap();
        let rows = r.rows(page.view, 0, 1000).unwrap().rows;
        r.close_view(page.view).unwrap();
        assert_eq!(rows.len() as u32, total);
        rows
    };
    assert_eq!(list(&mut r, "all").len(), PAGES);
    // Pages 1 and 3 have poor numbers of their own; the origin's poor
    // numbers are not credited to the fifteen pages that fall back to it.
    assert_eq!(list(&mut r, "poor_lcp").len(), 2);
    assert_eq!(list(&mut r, "poor_inp").len(), 2);
    assert_eq!(list(&mut r, "poor_cls").len(), 2);
    assert_eq!(list(&mut r, "origin_only").len(), PAGES - 5);

    let tab = tabs().iter().find(|t| t.id == "page-experience").unwrap();
    let lcp = tab.columns.iter().position(|c| c.id == "phone_lcp").unwrap();
    let level = tab.columns.iter().position(|c| c.id == "phone_level").unwrap();
    let url0 = page_url(&site, 0);
    let url9 = page_url(&site, 9);
    let rows = list(&mut r, "all");
    let own = rows
        .iter()
        .find(|row| row[1].as_str() == Some(url0.as_str()))
        .unwrap();
    assert_eq!(own[1 + lcp], serde_json::json!(2100));
    assert_eq!(own[1 + level], serde_json::json!("Page data"));
    // A fallback page shows no page-level number and says why.
    let fallback = rows
        .iter()
        .find(|row| row[1].as_str() == Some(url9.as_str()))
        .unwrap();
    assert_eq!(fallback[1 + lcp], serde_json::Value::Null);
    assert_eq!(
        fallback[1 + level],
        serde_json::json!("No page-level data (origin data)")
    );
}

#[test]
fn the_hub_saves_one_google_key_for_both_providers_and_runs_them() {
    let site = site();
    let dir = tempfile::tempdir().unwrap();
    let path = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(crawl(&site, dir.path()));
    let fake = FakePerf::start(field_data(&site, 5, true));
    let store = Arc::new(MemoryStore::default());
    let cache = Some(dir.path().join("hub-cache.sqlite"));
    let hub = Hub::new(Folder::new(dir.path()), store, cache).with_perf(
        crux::Api {
            base: fake.crux_url(),
        },
        psi::Api { base: fake.psi_url() },
    );

    // Off until turned on, and refused without a key.
    let req = FieldRequest {
        max: Some(5),
        ..FieldRequest::default()
    };
    assert!(
        hub.fetch_field_data(&path, None, &req, &Control::default(), &mut |_| {})
            .is_err()
    );
    for p in [Provider::FieldData, Provider::LabTests] {
        hub.save_settings(
            p,
            ProviderSettings {
                enabled: true,
                per_minute: None,
                concurrency: None,
            },
        )
        .unwrap();
    }
    assert!(
        hub.fetch_field_data(&path, None, &req, &Control::default(), &mut |_| {})
            .is_err()
    );

    // One key, both providers.
    let saved = hub.save_google_api_key("main", SecretString::from(KEY)).unwrap();
    assert_eq!(saved.len(), 2);
    let status = hub.status();
    for p in [Provider::FieldData, Provider::LabTests] {
        let s = status.providers.iter().find(|s| s.provider == p).unwrap();
        assert!(s.available);
        assert_eq!(s.connections, vec!["main".to_string()]);
    }

    let estimate = hub.estimate_field_data(&path, &req).unwrap();
    assert_eq!((estimate.urls, estimate.requests), (5, 10));
    let field = hub
        .fetch_field_data(&path, None, &req, &Control::default(), &mut |_| {})
        .unwrap();
    assert_eq!(field.state, RunState::Finished);
    assert_eq!(field.provider, Provider::FieldData);

    let lab = LabRequest {
        max: Some(3),
        ..LabRequest::default()
    };
    let est = hub.estimate_lab_tests(&path, &lab).unwrap();
    assert_eq!((est.urls, est.requests), (3, 3));
    let done = hub
        .fetch_lab_tests(&path, Some("main"), &lab, &Control::default(), &mut |_| {})
        .unwrap();
    assert_eq!(done.state, RunState::Finished);
    assert_eq!(hub.runs(&path).unwrap().len(), 2);
    // Nothing is paused, so there is nothing to resume.
    assert!(hub.resume(&path, None, &Control::default(), &mut |_| {}).is_err());

    // Forgetting a key removes it for that provider only.
    hub.forget("field_data", "main").unwrap();
    assert!(
        hub.fetch_field_data(&path, None, &req, &Control::default(), &mut |_| {})
            .is_err()
    );
    assert!(
        hub.fetch_lab_tests(&path, None, &lab, &Control::default(), &mut |_| {})
            .is_ok()
    );
}
