//! Spec 024 slice 5 against fake Ahrefs, Majestic and Moz (plan D12): batching
//! per provider limit, the estimate in the provider's units, the re-estimate
//! after the first batch, the cap, cache reuse, quota pause and resume, the
//! Link metrics tab, and no keys in the crawl file. Test names carry the
//! scenario id.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use rusqlite::Connection;
use secrecy::SecretString;
use spiderlens_config::{CrawlConfig, ModeInput, SpeedSettings};
use spiderlens_engine::crawl::{Control as CrawlControl, Crawler, open_for_update};
use spiderlens_integrations::cache::Cache;
use spiderlens_integrations::credentials::MemoryStore;
use spiderlens_integrations::hub::Hub;
use spiderlens_integrations::limits::Limits;
use spiderlens_integrations::links::{self, Apis, LinkRequest, UrlSet};
use spiderlens_integrations::run::{Control, Ctx};
use spiderlens_integrations::settings::{Folder, ProviderSettings};
use spiderlens_integrations::store::{self, RunProgress, RunState};
use spiderlens_integrations::{Error, Provider};
use spiderlens_test_fixtures::links::{FakeLinks, LinkKeys, expected};
use spiderlens_test_fixtures::{Site, SiteBuilder};
use tokio::sync::watch;

const HOST: &str = "127.0.0.1";
const PAGES: usize = 130;

fn site() -> Site {
    let mut b = SiteBuilder::new();
    for i in 0..PAGES {
        let next = format!("/p/{}", (i + 1) % PAGES);
        b = b.page(HOST, &format!("/p/{i}"), "", &[&next, "/p/0"]);
    }
    b.start()
}

async fn crawl(site: &Site, dir: &Path) -> PathBuf {
    std::fs::create_dir_all(dir).unwrap();
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

fn apis(f: &FakeLinks) -> Apis {
    Apis {
        ahrefs: f.ahrefs_url(),
        majestic: f.majestic_url(),
        moz: f.moz_url(),
    }
}

fn ctx(p: Provider, cache: Option<Cache>) -> Ctx {
    let mut limits = Limits::defaults(p);
    limits.per_minute = 6000;
    Ctx::new(p, limits, cache)
}

fn key_for(p: Provider) -> SecretString {
    let k = LinkKeys::default();
    SecretString::from(match p {
        Provider::Ahrefs => k.ahrefs,
        Provider::Majestic => k.majestic,
        _ => k.moz,
    })
}

fn request(p: Provider) -> LinkRequest {
    LinkRequest {
        provider: p,
        url_set: UrlSet::Indexable,
        metrics: Vec::new(),
        cap: None,
        refresh: false,
    }
}

async fn run(
    f: &FakeLinks,
    path: &Path,
    cache: Option<Cache>,
    req: &LinkRequest,
    resume: Option<i64>,
) -> RunProgress {
    let update = open_for_update(path, None).unwrap();
    let mut c = ctx(req.provider, cache);
    c.refresh = req.refresh;
    links::run_links(
        &c,
        &key_for(req.provider),
        &apis(f),
        &update,
        "Agency",
        req,
        resume,
        &Control::default(),
        &mut |_| {},
    )
    .await
    .unwrap()
}

fn estimate(path: &Path, cache: Option<Cache>, req: &LinkRequest) -> links::LinkEstimate {
    let db = Connection::open(path).unwrap();
    links::estimate(&db, &ctx(req.provider, cache), "Agency", req).unwrap()
}

fn count(path: &Path, sql: &str) -> i64 {
    Connection::open(path)
        .unwrap()
        .query_row(sql, [], |r| r.get(0))
        .unwrap()
}

fn cache_in(dir: &Path) -> Cache {
    Cache::open(&dir.join("cache.sqlite")).unwrap()
}

#[tokio::test]
async fn us5_1_estimate_is_in_provider_units_and_a_run_uses_what_was_estimated() {
    let site = site();
    let dir = tempfile::tempdir().unwrap();
    let path = crawl(&site, dir.path()).await;
    let f = FakeLinks::start(LinkKeys::default());
    for (p, name) in [
        (Provider::Ahrefs, "ahrefs"),
        (Provider::Majestic, "majestic"),
        (Provider::Moz, "moz"),
    ] {
        let req = request(p);
        let est = estimate(&path, None, &req);
        // Nothing is sent by estimating.
        assert_eq!(f.requests(name), 0);
        assert_eq!(est.urls, PAGES as u64);
        assert_eq!(est.cached, 0);
        assert_eq!(est.unit, links::unit_name(p));
        assert_eq!(est.default_cap, est.units + est.units.div_ceil(10));
        assert_eq!(est.cap, est.default_cap);

        let r = run(&f, &path, None, &req, None).await;
        assert_eq!(r.state, RunState::Finished, "{p:?}: {:?}", r.message);
        assert_eq!(r.done as usize, PAGES);
        // SC-006: the estimate is within 10% of what the provider charged.
        let used = f.used(name);
        assert!(
            used.abs_diff(est.units) * 10 <= est.units,
            "{name}: {used} vs {}",
            est.units
        );
        assert_eq!(r.usage, Some(used as i64));
        assert_eq!(f.requests(name) as u64, est.requests, "{name}");
        let rows = count(
            &path,
            &format!("SELECT COUNT(*) FROM link_metrics WHERE provider = '{name}'"),
        );
        assert_eq!(rows, PAGES as i64);
    }
}

#[tokio::test]
async fn us5_2_batches_follow_each_providers_maximum_and_values_land_on_the_right_page() {
    let site = site();
    let dir = tempfile::tempdir().unwrap();
    let path = crawl(&site, dir.path()).await;
    let f = FakeLinks::start(LinkKeys::default());
    for (p, name, most) in [
        (Provider::Ahrefs, "ahrefs", 100),
        (Provider::Majestic, "majestic", 100),
        (Provider::Moz, "moz", 50),
    ] {
        let r = run(&f, &path, None, &request(p), None).await;
        assert_eq!(r.state, RunState::Finished);
        let sizes: Vec<usize> = f
            .log()
            .iter()
            .filter(|l| l.provider == name)
            .map(|l| l.targets)
            .collect();
        assert!(sizes.len() >= 3, "{name}: {sizes:?}");
        assert!(sizes.iter().all(|n| *n <= most), "{name}: {sizes:?}");
        assert_eq!(sizes.iter().sum::<usize>(), PAGES, "{name}");
    }
    // The numbers stored for a page are the ones the provider gave for its address.
    let url = site.url(HOST, "/p/3");
    let db = Connection::open(&path).unwrap();
    let metrics = |provider: &str| -> serde_json::Value {
        let m: String = db
            .query_row(
                "SELECT l.metrics FROM link_metrics l JOIN urls u ON u.id = l.url_id
                 WHERE u.url = ?1 AND l.provider = ?2",
                [&url, provider],
                |r| r.get(0),
            )
            .unwrap();
        serde_json::from_str(&m).unwrap()
    };
    let v = expected(&url);
    let a = metrics("ahrefs");
    assert_eq!(a["domain_rating"], v.domain_rating);
    assert_eq!(a["refdomains"], v.refdomains);
    let m = metrics("majestic");
    assert_eq!(m["trust_flow"], v.trust_flow);
    assert_eq!(m["backlinks"], v.backlinks);
    let z = metrics("moz");
    assert_eq!(z["page_authority"], v.page_authority);
    assert_eq!(z["spam_score"], v.spam_score);
    // Each provider's data is its own row; the run recorded the feature.
    assert_eq!(
        count(&path, "SELECT COUNT(*) FROM link_metrics"),
        3 * PAGES as i64
    );
    let features = store::features(&db).unwrap();
    assert!(features.contains(&"link_metrics".to_string()), "{features:?}");
}

#[tokio::test]
async fn us5_1_chosen_metrics_are_all_that_is_asked_and_stored() {
    let site = site();
    let dir = tempfile::tempdir().unwrap();
    let path = crawl(&site, dir.path()).await;
    let f = FakeLinks::start(LinkKeys::default());
    let mut req = request(Provider::Ahrefs);
    req.metrics = vec!["domain_rating".into(), "backlinks".into()];
    let est = estimate(&path, None, &req);
    let r = run(&f, &path, None, &req, None).await;
    assert_eq!(r.state, RunState::Finished);
    // Two fields instead of four: the units follow the field count.
    assert!(f.used("ahrefs") < 2 * PAGES as u64 + 100);
    assert!(f.used("ahrefs").abs_diff(est.units) * 10 <= est.units);
    let json: String = Connection::open(&path)
        .unwrap()
        .query_row("SELECT metrics FROM link_metrics LIMIT 1", [], |r| r.get(0))
        .unwrap();
    let m: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(m.as_object().unwrap().len(), 2, "{json}");
    assert!(
        links::resolve_metrics(Provider::Ahrefs, &["nope".into()]).is_err(),
        "unknown metric"
    );
}

#[tokio::test]
async fn fr_029_the_first_batch_re_estimates_and_a_run_that_costs_more_stops_before_overspending() {
    let site = site();
    let dir = tempfile::tempdir().unwrap();
    let path = crawl(&site, dir.path()).await;
    let f = FakeLinks::start(LinkKeys::default());
    // Ahrefs charges three times what the estimate assumes.
    f.set_ahrefs_rate(3);
    let req = request(Provider::Ahrefs);
    let est = estimate(&path, None, &req);
    let r = run(&f, &path, None, &req, None).await;
    assert_eq!(r.state, RunState::Finished);
    let message = r.message.clone().unwrap_or_default();
    assert!(message.contains("would use about"), "{message}");
    // Only the first batch was paid for, and it stayed within the confirmed cap.
    assert_eq!(f.requests("ahrefs"), 1);
    assert!(f.used("ahrefs") <= est.cap, "{} > {}", f.used("ahrefs"), est.cap);
    assert!(r.done < r.requested, "{} of {}", r.done, r.requested);
    // The pages that were fetched are kept.
    assert_eq!(count(&path, "SELECT COUNT(*) FROM link_metrics"), r.done);
}

#[tokio::test]
async fn fr_029_a_run_never_uses_more_than_the_users_cap() {
    let site = site();
    let dir = tempfile::tempdir().unwrap();
    let path = crawl(&site, dir.path()).await;
    let f = FakeLinks::start(LinkKeys::default());
    let mut req = request(Provider::Moz);
    req.cap = Some(60);
    let est = estimate(&path, None, &req);
    assert_eq!(est.cap, 60);
    assert_eq!(est.units, PAGES as u64);
    let r = run(&f, &path, None, &req, None).await;
    assert_eq!(r.state, RunState::Finished);
    assert!(r.message.unwrap_or_default().contains("cap of 60"));
    // The last batch was cut down to what the cap left.
    assert_eq!(f.used("moz"), 60);
    assert_eq!(r.done, 60);
    assert_eq!(r.usage, Some(60));
    assert_eq!(count(&path, "SELECT COUNT(*) FROM link_metrics"), 60);
    // A cap too small for even one page fetches nothing.
    let path2 = crawl(&site, &dir.path().join("two")).await;
    let mut tiny = request(Provider::Majestic);
    tiny.cap = Some(0);
    let r = run(&f, &path2, None, &tiny, None).await;
    assert_eq!(r.done, 0);
    assert_eq!(f.used("majestic"), 0);
}

#[tokio::test]
async fn us5_3_a_second_run_reuses_cached_answers_and_a_refresh_does_not() {
    let site = site();
    let dir = tempfile::tempdir().unwrap();
    let path = crawl(&site, dir.path()).await;
    let f = FakeLinks::start(LinkKeys::default());
    let req = request(Provider::Moz);
    let first = run(&f, &path, Some(cache_in(dir.path())), &req, None).await;
    assert_eq!(first.state, RunState::Finished);
    let used = f.used("moz");
    let requests = f.requests("moz");
    let fetched_at: i64 = count(&path, "SELECT MIN(fetched_at) FROM link_metrics");

    // The estimate already knows: every page is cached, nothing to pay.
    let est = estimate(&path, Some(cache_in(dir.path())), &req);
    assert_eq!((est.cached, est.units, est.requests), (PAGES as u64, 0, 0));

    let second = run(&f, &path, Some(cache_in(dir.path())), &req, None).await;
    assert_eq!(second.state, RunState::Finished);
    assert_eq!(f.used("moz"), used);
    assert_eq!(f.requests("moz"), requests);
    assert_eq!(second.usage, Some(0));
    assert_eq!(second.done as usize, PAGES);
    // The date shown is the day the data was fetched, not today's.
    assert_eq!(
        count(&path, "SELECT MIN(fetched_at) FROM link_metrics"),
        fetched_at
    );

    let mut fresh = req.clone();
    fresh.refresh = true;
    let third = run(&f, &path, Some(cache_in(dir.path())), &fresh, None).await;
    assert_eq!(third.state, RunState::Finished);
    assert_eq!(f.used("moz"), 2 * used);
}

#[tokio::test]
async fn fr_006_running_out_of_units_pauses_and_resume_does_not_pay_twice() {
    let site = site();
    let dir = tempfile::tempdir().unwrap();
    let path = crawl(&site, dir.path()).await;
    let f = FakeLinks::start(LinkKeys::default());
    f.set_allowance("majestic", 60);
    let req = request(Provider::Majestic);
    let r = run(&f, &path, None, &req, None).await;
    assert_eq!(r.state, RunState::PausedQuota, "{:?}", r.message);
    assert_eq!(r.done, 25);
    assert_eq!(f.used("majestic"), 25);

    f.set_allowance("majestic", 1_000);
    let r = run(&f, &path, None, &req, Some(r.id)).await;
    assert_eq!(r.state, RunState::Finished, "{:?}", r.message);
    assert_eq!(r.done as usize, PAGES);
    // Every page was paid for once.
    assert_eq!(f.used("majestic"), PAGES as u64);
    assert_eq!(count(&path, "SELECT COUNT(*) FROM link_metrics"), PAGES as i64);
}

#[tokio::test]
async fn a_server_error_is_retried_and_a_refused_key_fails_the_run() {
    let site = site();
    let dir = tempfile::tempdir().unwrap();
    let path = crawl(&site, dir.path()).await;
    let f = FakeLinks::start(LinkKeys::default());
    f.fail_next(503, "busy");
    let r = run(&f, &path, None, &request(Provider::Moz), None).await;
    assert_eq!(r.state, RunState::Finished, "{:?}", r.message);

    let f = FakeLinks::start(LinkKeys {
        ahrefs: "another-key".into(),
        ..LinkKeys::default()
    });
    let path2 = crawl(&site, &dir.path().join("two")).await;
    let r = run(&f, &path2, None, &request(Provider::Ahrefs), None).await;
    assert_eq!(r.state, RunState::Failed);
    assert!(r.message.unwrap_or_default().contains("refused"));
}

#[tokio::test]
async fn sc_004_no_key_reaches_the_crawl_file() {
    let site = site();
    let dir = tempfile::tempdir().unwrap();
    let path = crawl(&site, dir.path()).await;
    let f = FakeLinks::start(LinkKeys::default());
    for p in [Provider::Ahrefs, Provider::Majestic, Provider::Moz] {
        let r = run(&f, &path, Some(cache_in(dir.path())), &request(p), None).await;
        assert_eq!(r.state, RunState::Finished);
    }
    let keys = LinkKeys::default();
    let secrets = [
        keys.ahrefs.as_str(),
        keys.majestic.as_str(),
        keys.moz.as_str(),
        "moz-secret",
    ];
    for file in [path.clone(), dir.path().join("cache.sqlite")] {
        // Checkpoint, so the main file holds everything.
        if let Ok(db) = Connection::open(&file) {
            let _ = db.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)");
        }
        let bytes = std::fs::read(&file).unwrap();
        for s in secrets {
            assert!(
                !bytes.windows(s.len()).any(|w| w == s.as_bytes()),
                "{s} found in {}",
                file.display()
            );
        }
    }
    for l in f.log() {
        assert!(!l.body.contains("majestic-key"), "key in the log");
    }
}

#[tokio::test]
async fn us5_link_metrics_tab_has_per_provider_columns_and_filters() {
    use spiderlens_results::{Need, Reader, UrlScope, ViewQuery, tabs};

    let site = site();
    let dir = tempfile::tempdir().unwrap();
    let path = crawl(&site, dir.path()).await;

    let r = Reader::open(&path).unwrap();
    assert!(!r.features().has(Need::LinkMetrics));
    drop(r);

    let f = FakeLinks::start(LinkKeys::default());
    // Ahrefs for the first 25 pages only (a small cap), Moz for all.
    let mut part = request(Provider::Ahrefs);
    part.cap = Some(100);
    run(&f, &path, None, &part, None).await;
    run(&f, &path, None, &request(Provider::Moz), None).await;

    let mut r = Reader::open(&path).unwrap();
    assert!(r.features().has(Need::LinkMetrics));
    let q = |filter: &str| ViewQuery {
        tab: "link-metrics".into(),
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
    assert_eq!(list(&mut r, "moz").len(), PAGES);
    assert_eq!(list(&mut r, "ahrefs").len(), 25);
    assert_eq!(list(&mut r, "majestic").len(), 0);
    assert_eq!(list(&mut r, "not_fetched").len(), 0);

    let tab = tabs().iter().find(|t| t.id == "link-metrics").unwrap();
    let at = |id: &str| 1 + tab.columns.iter().position(|c| c.id == id).unwrap();
    let url = site.url(HOST, "/p/3");
    let row = list(&mut r, "all")
        .into_iter()
        .find(|row| row[1].as_str() == Some(url.as_str()))
        .unwrap();
    let v = expected(&url);
    assert_eq!(
        row[at("ahrefs_domain_rating")],
        serde_json::json!(v.domain_rating)
    );
    assert_eq!(row[at("moz_page_authority")], serde_json::json!(v.page_authority));
    assert!(row[at("majestic_trust_flow")].is_null());
    assert!(row[at("link_fetched")].is_string());
    // A page past the first 25 has Moz but no Ahrefs numbers.
    let last = site.url(HOST, "/p/129");
    let row = list(&mut r, "all")
        .into_iter()
        .find(|row| row[1].as_str() == Some(last.as_str()))
        .unwrap();
    assert!(row[at("ahrefs_domain_rating")].is_null());
    assert!(!row[at("moz_domain_authority")].is_null());
}

fn hub(dir: &Path, f: &FakeLinks) -> Hub {
    Hub::new(
        Folder::new(dir),
        Arc::new(MemoryStore::default()),
        Some(dir.join("cache.sqlite")),
    )
    .with_links(apis(f))
}

fn crawl_sync(site: &Site, dir: &Path) -> PathBuf {
    tokio::runtime::Runtime::new().unwrap().block_on(crawl(site, dir))
}

#[test]
fn the_hub_needs_the_provider_on_and_a_saved_key_then_runs_estimates_and_resumes() {
    let site = site();
    let dir = tempfile::tempdir().unwrap();
    let path = crawl_sync(&site, dir.path());
    let f = FakeLinks::start(LinkKeys::default());
    let h = hub(dir.path(), &f);
    let req = request(Provider::Majestic);

    // No key yet.
    let e = h.estimate_links(&path, None, &req).unwrap_err();
    assert!(matches!(e, Error::NoConnection(_)), "{e}");
    h.save_key(
        Provider::Majestic,
        "Agency",
        SecretString::from("majestic-key".to_string()),
    )
    .unwrap();
    let est = h.estimate_links(&path, None, &req).unwrap();
    assert_eq!(est.unit, "units");
    assert_eq!(est.units, PAGES as u64);

    // Off until turned on (FR-001).
    let e = h
        .fetch_links(&path, None, &req, &Control::default(), &mut |_| {})
        .unwrap_err();
    assert!(e.to_string().contains("is off"), "{e}");
    assert_eq!(f.requests("majestic"), 0);
    h.save_settings(
        Provider::Majestic,
        ProviderSettings {
            enabled: true,
            ..ProviderSettings::default()
        },
    )
    .unwrap();

    // Pause at once with the control, then resume through the hub.
    let control = Control::default();
    control.pause();
    let r = h.fetch_links(&path, None, &req, &control, &mut |_| {}).unwrap();
    assert_eq!(r.state, RunState::PausedUser);
    assert_eq!(f.requests("majestic"), 0);
    let r = h
        .resume(&path, Some(r.id), &Control::default(), &mut |_| {})
        .unwrap();
    assert_eq!(r.state, RunState::Finished, "{:?}", r.message);
    assert_eq!(r.done as usize, PAGES);
    assert_eq!(f.used("majestic"), PAGES as u64);

    // A second key of the same provider needs a choice.
    h.save_key(
        Provider::Majestic,
        "Other",
        SecretString::from("other".to_string()),
    )
    .unwrap();
    assert!(h.estimate_links(&path, None, &req).is_err());
    assert!(h.estimate_links(&path, Some("Agency"), &req).is_ok());
    let status = h.status();
    let m = status
        .providers
        .iter()
        .find(|p| p.provider == Provider::Majestic)
        .unwrap();
    assert!(m.available);
    assert_eq!(m.connections.len(), 2);
}
