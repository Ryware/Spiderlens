//! Spec 006 (crawl storage) acceptance scenarios: writer lock, recovery,
//! resume, Save As, disk and write failures, versions and damaged files.
//! Test names carry the scenario or success-criterion id.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use spiderlens_config::{Change, CrawlConfig, ModeInput, SpeedSettings};
use spiderlens_engine::crawl::file::{self, LocalErrorKind};
use spiderlens_engine::crawl::{
    Access, Control, CrawlError, CrawlEvent, CrawlStore, CrawlSummary, Crawler, ResumeOptions, RunCommand,
    Upgrade, store::FORMAT_VERSION,
};
use spiderlens_test_fixtures::{Site, SiteBuilder};
use tokio::sync::{mpsc, oneshot, watch};

const HOST: &str = "127.0.0.1";
const PAGES: usize = 300;

/// A tree of pages: each links to its ten children and the home page.
fn tree_site() -> Site {
    let mut b = SiteBuilder::new();
    for i in 0..PAGES {
        let links: Vec<String> = (1..=10)
            .map(|k| i * 10 + k)
            .filter(|&c| c < PAGES)
            .map(|c| format!("/p/{c}"))
            .chain(std::iter::once("/p/0".to_string()))
            .collect();
        let refs: Vec<&str> = links.iter().map(String::as_str).collect();
        b = b.page(HOST, &format!("/p/{i}"), "", &refs);
    }
    b.start()
}

fn config(rate: Option<f64>) -> CrawlConfig {
    CrawlConfig {
        speed: SpeedSettings {
            max_urls_per_sec: rate,
            ..SpeedSettings::FAST
        },
        ..CrawlConfig::default()
    }
}

fn input(site: &Site) -> ModeInput {
    ModeInput::Spider {
        start_url: site.url(HOST, "/p/0"),
    }
}

/// Every URL with its final state and status, for comparing crawls.
fn outcome(path: &Path) -> BTreeMap<String, (String, Option<u16>)> {
    let store = CrawlStore::open_read_only(path).unwrap();
    let mut out = BTreeMap::new();
    store
        .rows(None, &[], |r| {
            out.insert(r.url.clone(), (r.state.to_string(), r.status));
            Ok(())
        })
        .unwrap();
    out
}

async fn straight(site: &Site, dir: &Path) -> PathBuf {
    let path = dir.join("straight.spiderlens");
    let crawler = Crawler::create(&path, config(None), &input(site)).unwrap();
    let (_tx, rx) = watch::channel(Control::Run);
    let s = crawler.run(rx, None).await.unwrap();
    assert_eq!(s.status, "completed");
    assert_eq!(s.crawled as usize, PAGES);
    path
}

/// Runs a crawl and pauses it once `at` URLs are crawled.
async fn run_until(crawler: Crawler, at: u64) -> CrawlSummary {
    let (tx, rx) = watch::channel(Control::Run);
    let (etx, mut erx) = mpsc::unbounded_channel();
    let watcher = tokio::spawn(async move {
        while let Some(e) = erx.recv().await {
            if let CrawlEvent::Progress(p) = e
                && p.crawled >= at
            {
                let _ = tx.send(Control::Pause);
            }
        }
        drop(tx);
    });
    let s = tokio::time::timeout(Duration::from_secs(60), crawler.run(rx, Some(etx)))
        .await
        .expect("crawl pauses")
        .unwrap();
    watcher.await.unwrap();
    s
}

#[tokio::test]
async fn fr_020_a_second_writer_is_refused() {
    let site = tree_site();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("c.spiderlens");
    let first = Crawler::create(&path, config(None), &input(&site)).unwrap();
    match Crawler::open(&path) {
        Err(CrawlError::Lock(e)) => assert!(e.to_string().contains("in use"), "{e}"),
        Err(e) => panic!("expected in use, got {e}"),
        Ok(_) => panic!("a second writer opened the file"),
    }
    let info = file::info(&path).unwrap();
    assert!(matches!(info.access, Access::InUse { .. }), "{:?}", info.access);
    drop(first);
    assert!(
        !file::lock_path(&path).exists(),
        "the lock file goes with the lock"
    );
    Crawler::open(&path).expect("free again once the first writer is gone");
}

#[tokio::test]
async fn us1_3_us1_5_a_crash_is_recovered_without_duplicates() {
    let site = tree_site();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("c.spiderlens");
    let crawler = Crawler::create(&path, config(Some(200.0)), &input(&site)).unwrap();
    let s = run_until(crawler, 60).await;
    assert_eq!(s.status, "paused");
    // What a crash leaves: status running and rows in flight, no lock.
    {
        let store = CrawlStore::open(&path).unwrap();
        store.set_meta("status", "running").unwrap();
        store
            .conn_for_tests()
            .execute(
                "UPDATE urls SET state = 'in_flight' WHERE id IN (SELECT id FROM urls WHERE state = 'queued' LIMIT 5)",
                [],
            )
            .unwrap();
    }
    let n = file::recover_path(&path).unwrap();
    assert_eq!(n, Some(5));
    let info = file::info(&path).unwrap();
    assert_eq!(info.status, "paused");
    assert!(info.recovered);
    assert!(info.events.iter().any(|e| e.kind == "recovered"));
    assert_eq!(
        info.counts.crawled + info.counts.not_crawled + info.counts.queued,
        info.counts.discovered,
        "every URL is final or queued"
    );
    assert_eq!(
        file::recover_path(&path).unwrap(),
        None,
        "a second recovery has nothing to do"
    );

    // Resumed, it finishes like a crawl that never crashed (SC-003).
    let straight = straight(&site, dir.path()).await;
    let crawler = Crawler::open(&path).unwrap();
    let (_tx, rx) = watch::channel(Control::Run);
    crawler.run(rx, None).await.unwrap();
    assert_eq!(outcome(&path), outcome(&straight));
    let info = file::info(&path).unwrap();
    assert!(!info.recovered, "completing clears the recovered mark");
}

#[tokio::test]
async fn sc_003_pause_reopen_resume_equals_an_uninterrupted_crawl() {
    let site = tree_site();
    let dir = tempfile::tempdir().unwrap();
    let straight = straight(&site, dir.path()).await;
    let path = dir.path().join("paused.spiderlens");
    let crawler = Crawler::create(&path, config(Some(300.0)), &input(&site)).unwrap();
    let s = run_until(crawler, (PAGES as u64) * 3 / 10).await;
    assert_eq!(s.status, "paused");
    assert!(s.crawled < PAGES as u64);
    assert_eq!(file::info(&path).unwrap().pause_reason.as_deref(), Some("user"));

    // Reopened as after a relaunch, faster this time (FR-011).
    let options = ResumeOptions {
        changes: vec![Change::Speed(SpeedSettings::FAST)],
        ..ResumeOptions::default()
    };
    let crawler = Crawler::open_with(&path, &options).unwrap();
    assert_eq!(crawler.config().speed, SpeedSettings::FAST);
    let (_tx, rx) = watch::channel(Control::Run);
    let s = crawler.run(rx, None).await.unwrap();
    assert_eq!(s.status, "completed");
    assert_eq!(outcome(&path), outcome(&straight));
    let info = file::info(&path).unwrap();
    let kinds: Vec<&str> = info.events.iter().map(|e| e.kind.as_str()).collect();
    assert_eq!(
        kinds,
        ["created", "paused", "config_changed", "resumed", "completed"],
        "history"
    );
    let changes = &info.config.unwrap().changes;
    assert_eq!(changes.len(), 1, "the change is in the configuration history");
}

#[tokio::test]
async fn us2_3_us2_4_stopped_crawls_resume() {
    let site = tree_site();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("stopped.spiderlens");
    let crawler = Crawler::create(&path, config(Some(300.0)), &input(&site)).unwrap();
    let (tx, rx) = watch::channel(Control::Run);
    let (etx, mut erx) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        while let Some(CrawlEvent::Progress(p)) = erx.recv().await {
            if p.crawled >= 40 {
                let _ = tx.send(Control::Stop);
            }
        }
    });
    let s = crawler.run(rx, Some(etx)).await.unwrap();
    assert_eq!(s.status, "stopped");
    let straight = straight(&site, dir.path()).await;
    let crawler = Crawler::open(&path).unwrap();
    let (_tx, rx) = watch::channel(Control::Run);
    crawler.run(rx, None).await.unwrap();
    assert_eq!(outcome(&path), outcome(&straight));
}

#[tokio::test]
async fn us3_1_save_as_moves_a_running_crawl() {
    let site = tree_site();
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("unsaved.spiderlens");
    let dest_dir = dir.path().join("saved");
    std::fs::create_dir(&dest_dir).unwrap();
    let dest = dest_dir.join("my crawl.spiderlens");
    let crawler = Crawler::create(&src, config(Some(300.0)), &input(&site)).unwrap();
    let (_tx, rx) = watch::channel(Control::Run);
    let (etx, mut erx) = mpsc::unbounded_channel();
    let (ctx, crx) = mpsc::unbounded_channel();
    let target = dest.clone();
    let saver = tokio::spawn(async move {
        let mut asked = None;
        while let Some(e) = erx.recv().await {
            if let CrawlEvent::Progress(p) = e
                && p.crawled >= 50
                && asked.is_none()
            {
                let (reply, wait) = oneshot::channel();
                ctx.send(RunCommand::SaveAs {
                    dest: target.clone(),
                    reply,
                })
                .unwrap();
                asked = Some((wait, p.crawled));
            }
            if asked.is_some() {
                break;
            }
        }
        let (wait, at) = asked.unwrap();
        (wait.await.unwrap(), at)
    });
    let s = crawler.run_with(rx, Some(etx), Some(crx)).await.unwrap();
    let (moved, at) = saver.await.unwrap();
    assert_eq!(moved.unwrap(), dest);
    assert_eq!(s.status, "completed");
    assert!(at < PAGES as u64, "saved while running");
    // Everything after the move went into the new file.
    let info = file::info(&dest).unwrap();
    assert_eq!(info.counts.crawled as usize, PAGES);
    assert!(info.events.iter().any(|e| e.kind == "saved"));
    assert!(!file::partial_path(&dest).exists());
    assert!(!file::lock_path(&src).exists() && !file::lock_path(&dest).exists());
    let old = file::info(&src).unwrap();
    assert!(
        (old.counts.crawled as usize) < PAGES,
        "the old copy stopped at the move"
    );
}

#[tokio::test]
async fn fr_016_save_as_replaces_a_file_only_when_complete() {
    let site = tree_site();
    let dir = tempfile::tempdir().unwrap();
    let src = straight(&site, dir.path()).await;
    let dest = dir.path().join("copy.spiderlens");
    std::fs::write(&dest, b"an older file").unwrap();
    file::save_as(&src, &dest).unwrap();
    assert!(!file::partial_path(&dest).exists());
    // A single, self-contained file: no log beside it.
    let mut wal = dest.file_name().unwrap().to_os_string();
    wal.push("-wal");
    assert!(!dest.with_file_name(wal).exists());
    assert_eq!(outcome(&dest), outcome(&src));
}

#[tokio::test]
async fn sc_006_disk_full_pauses_and_resumes_without_loss() {
    let site = tree_site();
    let dir = tempfile::tempdir().unwrap();
    let straight = straight(&site, dir.path()).await;
    let path = dir.path().join("full.spiderlens");
    let crawler = Crawler::create(&path, config(None), &input(&site))
        .unwrap()
        .with_max_pages(80)
        .unwrap();
    let (_tx, rx) = watch::channel(Control::Run);
    let err = crawler.run(rx, None).await.unwrap_err();
    let CrawlError::Local(local) = err else {
        panic!("expected a local error, got {err}")
    };
    assert_eq!(local.kind, LocalErrorKind::DiskFull);
    assert!(local.message.contains("disk is full"), "{}", local.message);
    assert_eq!(local.path, path);

    // The file opens with what was written; no URL is crawled without its data.
    let store = CrawlStore::open_read_only(&path).unwrap();
    let missing: i64 = store
        .conn_for_tests()
        .query_row(
            "SELECT COUNT(*) FROM urls WHERE state = 'crawled' AND status IS NULL AND error_class IS NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(missing, 0);
    drop(store);

    // Space freed (a new connection has no page limit): resume and finish.
    let crawler = Crawler::open(&path).unwrap();
    let (_tx, rx) = watch::channel(Control::Run);
    crawler.run(rx, None).await.unwrap();
    assert_eq!(outcome(&path), outcome(&straight));
}

#[tokio::test]
async fn fr_027_low_free_space_pauses_before_the_disk_fills() {
    let site = tree_site();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("low.spiderlens");
    let crawler = Crawler::create(&path, config(None), &input(&site))
        .unwrap()
        .with_disk_reserve(Some(u64::MAX));
    let (_tx, rx) = watch::channel(Control::Run);
    let CrawlError::Local(local) = crawler.run(rx, None).await.unwrap_err() else {
        panic!("expected a local error")
    };
    assert_eq!(local.kind, LocalErrorKind::DiskLow);
    assert!(local.free_bytes.is_some());
    let info = file::info(&path).unwrap();
    assert_eq!(info.status, "paused");
    assert_eq!(info.pause_reason.as_deref(), Some("disk_low"));
    let ev = info
        .events
        .iter()
        .find(|e| e.kind == "local_error")
        .expect("recorded as a local error");
    assert!(ev.detail.as_deref().unwrap().contains("disk_low"));
}

fn reference(name: &str, dir: &Path) -> PathBuf {
    let src = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/reference")
        .join(name);
    let to = dir.join(name);
    std::fs::copy(src, &to).unwrap();
    to
}

#[test]
fn sc_007_reference_files_from_earlier_versions_open_unchanged() {
    let expected: serde_json::Value = serde_json::from_str(include_str!("reference/expected.json")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    for (name, want) in expected.as_object().unwrap() {
        let path = reference(name, dir.path());
        let before = std::fs::read(&path).unwrap();
        let s = CrawlSummary::read(&path).unwrap();
        assert_eq!(s.status, want["status"], "{name}");
        assert_eq!(s.discovered, want["discovered"].as_u64().unwrap(), "{name}");
        assert_eq!(s.crawled, want["crawled"].as_u64().unwrap(), "{name}");
        let store = CrawlStore::open_read_only(&path).unwrap();
        let title: Option<String> = store
            .conn_for_tests()
            .query_row(
                "SELECT title FROM urls WHERE url = ?1",
                [want["sample_url"].as_str().unwrap()],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(title.as_deref(), want["sample_title"].as_str(), "{name}");
        drop(store);
        let info = file::info(&path).unwrap();
        assert_eq!(info.format_version, 2);
        assert!(
            matches!(info.access, Access::NeedsUpgrade { from: 2 }),
            "{:?}",
            info.access
        );
        assert_eq!(
            std::fs::read(&path).unwrap(),
            before,
            "viewing never changes a file (FR-036)"
        );
    }
}

#[tokio::test]
async fn fr_036_resuming_an_older_file_upgrades_it_and_keeps_a_copy() {
    let dir = tempfile::tempdir().unwrap();
    let path = reference("v2-paused.spiderlens", dir.path());
    let original = std::fs::read(&path).unwrap();
    match Crawler::open(&path) {
        Err(CrawlError::NeedsUpgrade { from: 2 }) => {}
        Err(e) => panic!("{e}"),
        Ok(_) => panic!("upgraded without asking"),
    }
    let crawler = Crawler::open_with(
        &path,
        &ResumeOptions {
            upgrade: Some(Upgrade::KeepCopy),
            ..ResumeOptions::default()
        },
    )
    .unwrap();
    let (from, backup) = crawler.opened().upgraded.clone().unwrap();
    assert_eq!(from, 2);
    let backup = backup.unwrap();
    assert_eq!(
        backup.file_name().unwrap().to_string_lossy(),
        "v2-paused (format 2 backup).spiderlens"
    );
    assert_eq!(std::fs::read(&backup).unwrap(), original, "the copy is untouched");
    drop(crawler);
    let info = file::info(&path).unwrap();
    assert_eq!(info.format_version, FORMAT_VERSION);
    assert!(info.versions.iter().any(|v| v.kind == "upgraded"));

    // Declining the copy upgrades without one.
    let other = dir.path().join("other.spiderlens");
    std::fs::write(&other, &original).unwrap();
    let crawler = Crawler::open_with(
        &other,
        &ResumeOptions {
            upgrade: Some(Upgrade::NoCopy),
            ..ResumeOptions::default()
        },
    )
    .unwrap();
    assert_eq!(crawler.opened().upgraded, Some((2, None)));
}

#[tokio::test]
async fn fr_037_newer_files_open_read_only_or_are_refused() {
    let site = tree_site();
    let dir = tempfile::tempdir().unwrap();
    let path = straight(&site, dir.path()).await;
    {
        let store = CrawlStore::open(&path).unwrap();
        store.set_meta("format_version", "99").unwrap();
    }
    let info = file::info(&path).unwrap();
    assert!(
        matches!(info.access, Access::NewerFormat { .. }),
        "{:?}",
        info.access
    );
    assert!(
        CrawlSummary::read(&path).is_ok(),
        "an additive newer file can be viewed"
    );
    assert!(Crawler::open(&path).is_err(), "but not changed");
    {
        let store = CrawlStore::open_read_only(&path).unwrap();
        drop(store);
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute("UPDATE meta SET value = '99' WHERE key = 'read_compat'", [])
            .unwrap();
    }
    let e = file::info(&path).unwrap_err();
    assert!(e.to_string().contains("newer SpiderLens"), "{e}");
}

#[tokio::test]
async fn fr_034_secrets_must_be_entered_again() {
    let site = tree_site();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("s.spiderlens");
    let mut c = config(Some(300.0));
    c.identity.headers.push(spiderlens_config::CustomHeader {
        name: "Authorization".into(),
        value: spiderlens_config::HeaderValue::secret("Bearer abc"),
        send_to: Default::default(),
    });
    let crawler = Crawler::create(&path, c, &input(&site)).unwrap();
    run_until(crawler, 20).await;
    let raw = std::fs::read(&path).unwrap();
    assert!(
        !raw.windows(10).any(|w| w == b"Bearer abc"),
        "the secret is never stored"
    );
    assert_eq!(file::info(&path).unwrap().missing_secrets, ["Authorization"]);
    match Crawler::open(&path) {
        Err(CrawlError::NeedsSecrets(names)) => assert_eq!(names, ["Authorization"]),
        Err(e) => panic!("{e}"),
        Ok(_) => panic!("resumed without the secret"),
    }
    let crawler = Crawler::open_with(
        &path,
        &ResumeOptions {
            secrets: vec![("authorization".into(), "Bearer abc".into())],
            ..ResumeOptions::default()
        },
    )
    .unwrap();
    let h = &crawler.config().identity.headers[0];
    assert_eq!(h.value.expose(), Some("Bearer abc"));
}

#[tokio::test]
async fn fr_038_damaged_files_are_reported_and_partly_recovered() {
    let site = tree_site();
    let dir = tempfile::tempdir().unwrap();
    let path = straight(&site, dir.path()).await;
    // Overwrite the last fifth of the file with garbage.
    let mut bytes = std::fs::read(&path).unwrap();
    let half = bytes.len() / 5 * 4;
    for (i, b) in bytes[half..].iter_mut().enumerate() {
        *b = (i * 7 % 251) as u8;
    }
    std::fs::write(&path, &bytes).unwrap();
    let full_read = rusqlite::Connection::open(&path).and_then(|c| {
        c.query_row("SELECT COUNT(*), SUM(LENGTH(raw)) FROM links", [], |r| {
            r.get::<_, i64>(0)
        })
    });
    assert!(full_read.is_err(), "the test really damaged the file");
    assert!(file::salvageable(&path));

    let dest = dir.path().join("recovered.spiderlens");
    let report = file::salvage(&path, &dest).unwrap();
    assert!(report.tables.iter().any(|(_, _, complete)| !complete));
    let info = file::info(&dest).unwrap();
    assert!(info.recovered);
    assert_eq!(info.start_url.as_deref(), Some(site.url(HOST, "/p/0").as_str()));
    assert!(info.counts.discovered > 0, "some URLs were saved");
}

#[test]
fn fr_038_a_file_that_is_not_a_crawl_is_refused_clearly() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("notes.spiderlens");
    std::fs::write(&path, b"just some text, not a database at all").unwrap();
    let e = file::info(&path).unwrap_err();
    assert!(
        e.to_string().contains("not a SpiderLens crawl file") || e.to_string().contains("damaged"),
        "{e}"
    );
    assert!(!file::salvageable(&path));
}

#[tokio::test]
async fn fr_021_read_only_files_are_viewed_and_resume_asks_for_a_copy() {
    let site = tree_site();
    let dir = tempfile::tempdir().unwrap();
    let path = straight(&site, dir.path()).await;
    let mut perms = std::fs::metadata(&path).unwrap().permissions();
    perms.set_readonly(true);
    std::fs::set_permissions(&path, perms.clone()).unwrap();
    if file::writable(&path) {
        // Running as root: permissions do not apply.
        return;
    }
    let info = file::info(&path).unwrap();
    assert!(matches!(info.access, Access::ReadOnlyMedia));
    assert!(matches!(Crawler::open(&path), Err(CrawlError::ReadOnly(_))));
    let copy = dir.path().join("copy.spiderlens");
    file::save_as(&path, &copy).unwrap();
    assert!(matches!(file::info(&copy).unwrap().access, Access::Writable));
    #[allow(clippy::permissions_set_readonly_false)]
    perms.set_readonly(false);
    std::fs::set_permissions(&path, perms).unwrap();
}
