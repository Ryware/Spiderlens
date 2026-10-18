//! End-to-end runs against the local fixture site (US1, US2, US3).

use std::path::Path;
use std::sync::Arc;

use jiff::Timestamp;
use jiff::civil::date;
use jiff::tz::TimeZone;
use spiderlens_schedule::notify::Recorder;
use spiderlens_schedule::runner::{
    Context, Outcome, TickAction, exit_code, history_view, request_stop, run, tick,
};
use spiderlens_schedule::{
    AfterStep, ExportTab, Frequency, NotifyLevel, Paths, RunStatus, Schedule, ScheduleInput, StepStatus,
    Store, TimeOfDay, Trigger,
};
use spiderlens_test_fixtures::{Fixture, Site, SiteBuilder, closed_port_url};

fn at(d: i8, h: i8, m: i8) -> Timestamp {
    date(2026, 10, d)
        .at(h, m, 0, 0)
        .to_zoned(TimeZone::UTC)
        .unwrap()
        .timestamp()
}

fn schedule(out: &Path, start_url: String) -> Schedule {
    Schedule {
        id: "sched001".into(),
        name: "Client A".into(),
        enabled: true,
        config: Default::default(),
        source_profile: None,
        source_hash: None,
        input: ScheduleInput::Spider { start_url },
        frequency: Frequency::Daily {
            time: TimeOfDay { hour: 7, minute: 0 },
        },
        output_dir: out.to_path_buf(),
        catch_up: true,
        notify: NotifyLevel::Problems,
        after: vec![],
        keep_last: None,
        created_at: at(13, 12, 0),
        updated_at: at(13, 12, 0),
    }
}

const E: &str = "example.test";

fn site() -> Site {
    SiteBuilder::new()
        .page(E, "/", "", &["/a", "/b"])
        .page(E, "/a", "", &["/c"])
        .page(E, "/b", "", &[])
        .page(E, "/c", "", &[])
        .start()
}

fn setup_with(s: Schedule, config: &Path, site: Option<&Site>) -> (Context, Arc<Recorder>) {
    let paths = Paths::new(config);
    Store::new(paths.clone()).put(s).unwrap();
    let rec = Arc::new(Recorder::default());
    let resolve = site.map(|s| s.resolve()).unwrap_or_default();
    (
        Context {
            paths,
            tz: TimeZone::UTC,
            notifier: rec.clone(),
            resolve,
            disk_reserve: 0,
        },
        rec,
    )
}

fn setup(s: Schedule, config: &Path) -> (Context, Arc<Recorder>) {
    setup_with(s, config, None)
}

fn done(o: Outcome) -> spiderlens_schedule::Run {
    match o {
        Outcome::Done(r) => r,
        Outcome::AlreadyHandled => panic!("expected a run"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn scheduled_run_writes_a_named_complete_crawl() {
    let site = site();
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("out");
    std::fs::create_dir(&out).unwrap();
    let mut s = schedule(&out, site.url(E, "/"));
    s.after = vec![
        AfterStep::Export {
            tabs: vec![ExportTab {
                tab: "internal".into(),
                filter: "all".into(),
            }],
        },
        AfterStep::Compare,
    ];
    let (ctx, rec) = setup_with(s, &dir.path().join("config"), Some(&site));

    // Before 07:00 nothing is due; at 07:01 the 07:00 run is.
    assert_eq!(
        tick(&ctx, "sched001", at(14, 6, 59)).unwrap(),
        TickAction::Nothing
    );
    let TickAction::Run { slot, trigger } = tick(&ctx, "sched001", at(14, 7, 1)).unwrap() else {
        panic!("due");
    };
    assert_eq!((slot, trigger), (at(14, 7, 0), Trigger::Scheduled));

    let r = done(run(&ctx, "sched001", Some(slot), trigger).await.unwrap());
    assert_eq!(r.status, RunStatus::Finished, "{:?}", r.reason);
    let file = r.file.clone().unwrap();
    assert_eq!(file, out.join("Client A 2026-10-14 0700.spiderlens"));
    let summary = spiderlens_engine::crawl::CrawlSummary::read(&file).unwrap();
    assert_eq!(summary.status, "completed");
    assert_eq!(r.counts.as_ref().unwrap().crawled, 4);
    let store = spiderlens_engine::crawl::CrawlStore::open(&file).unwrap();
    let meta: serde_json::Value = serde_json::from_str(&store.meta("schedule").unwrap().unwrap()).unwrap();
    assert_eq!(meta["name"], "Client A");
    assert_eq!(meta["trigger"], "scheduled");

    // Steps: the export is written next to the crawl; compare is not available yet.
    assert_eq!(r.steps[0].status, StepStatus::Done);
    assert!(
        out.join("Client A 2026-10-14 0700 exports/internal-all.csv")
            .is_file()
    );
    assert_eq!(r.steps[1].status, StepStatus::Skipped);
    assert!(
        rec.0.lock().unwrap().is_empty(),
        "no notification for a success at Problems only"
    );

    // The same slot is not run twice (a second tick in the same hour).
    assert_eq!(
        tick(&ctx, "sched001", at(14, 7, 30)).unwrap(),
        TickAction::Nothing
    );
    assert_eq!(
        run(&ctx, "sched001", Some(slot), Trigger::Scheduled)
            .await
            .unwrap(),
        Outcome::AlreadyHandled
    );

    // Run now: a second file the same minute gets a suffix, the schedule is unchanged.
    let now = done(run(&ctx, "sched001", None, Trigger::RunNow).await.unwrap());
    assert_eq!(now.status, RunStatus::Finished);
    assert_ne!(now.file, r.file);
    assert_eq!(tick(&ctx, "sched001", at(14, 9, 0)).unwrap(), TickAction::Nothing);

    let hist = history_view(&ctx.paths, "sched001").unwrap();
    assert_eq!(hist.iter().map(|r| r.run).collect::<Vec<_>>(), [2, 1]);
}

#[tokio::test(flavor = "multi_thread")]
async fn unreachable_site_fails_with_reason_and_notifies() {
    let dir = tempfile::tempdir().unwrap();
    let (ctx, rec) = setup(
        schedule(dir.path(), closed_port_url()),
        &dir.path().join("config"),
    );
    let r = done(
        run(&ctx, "sched001", Some(at(14, 7, 0)), Trigger::Scheduled)
            .await
            .unwrap(),
    );
    assert_eq!(r.status, RunStatus::Failed);
    assert!(
        r.reason.as_deref().unwrap().contains("refused the connection"),
        "{:?}",
        r.reason
    );
    assert!(r.file.as_deref().unwrap().is_file(), "the crawl file is kept");
    assert_eq!(exit_code(&Outcome::Done(r)), 4);
    let notes = rec.0.lock().unwrap();
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].0, "SpiderLens: Client A");
    assert!(notes[0].1.starts_with("Failed: "));
}

#[tokio::test(flavor = "multi_thread")]
async fn missing_output_folder_fails_before_any_request() {
    let site = Fixture::start();
    let dir = tempfile::tempdir().unwrap();
    let (ctx, _) = setup(
        schedule(&dir.path().join("nope"), site.url("/")),
        &dir.path().join("config"),
    );
    let r = done(
        run(&ctx, "sched001", Some(at(14, 7, 0)), Trigger::Scheduled)
            .await
            .unwrap(),
    );
    assert_eq!(r.status, RunStatus::Failed);
    assert!(r.reason.unwrap().starts_with("The output folder"));
    assert!(r.file.is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn low_disk_space_pauses_before_any_request() {
    let site = Fixture::start();
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("out");
    std::fs::create_dir(&out).unwrap();
    let (mut ctx, _) = setup(schedule(&out, site.url("/")), &dir.path().join("config"));
    ctx.disk_reserve = u64::MAX;
    let o = run(&ctx, "sched001", Some(at(14, 7, 0)), Trigger::Scheduled)
        .await
        .unwrap();
    assert_eq!(exit_code(&o), 6);
    let r = done(o);
    assert_eq!(r.status, RunStatus::PausedDisk);
    assert!(r.reason.unwrap().starts_with("Not enough free space"));
    assert!(r.file.is_none());
    assert_eq!(std::fs::read_dir(&out).unwrap().count(), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn missed_and_caught_up() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = schedule(dir.path(), "https://example.com/".into());
    s.catch_up = false;
    let (ctx, rec) = setup(s.clone(), &dir.path().join("config"));
    // Off over three days: one Missed record, then nothing until the next slot.
    assert_eq!(tick(&ctx, "sched001", at(17, 9, 0)).unwrap(), TickAction::Missed);
    assert_eq!(
        tick(&ctx, "sched001", at(17, 10, 0)).unwrap(),
        TickAction::Nothing
    );
    let hist = history_view(&ctx.paths, "sched001").unwrap();
    assert_eq!(hist.len(), 1);
    assert_eq!(
        (hist[0].status, hist[0].slot),
        (RunStatus::Missed, Some(at(17, 7, 0)))
    );
    assert_eq!(rec.0.lock().unwrap().len(), 1);

    s.catch_up = true;
    Store::new(ctx.paths.clone()).put(s).unwrap();
    assert_eq!(
        tick(&ctx, "sched001", at(18, 9, 0)).unwrap(),
        TickAction::Run {
            slot: at(18, 7, 0),
            trigger: Trigger::CaughtUp
        }
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn overlap_is_skipped_and_stop_file_stops() {
    let site = site();
    let dir = tempfile::tempdir().unwrap();
    let mut s = schedule(dir.path(), site.url(E, "/"));
    // Slow enough to still be running when the second request comes.
    s.config.speed.max_urls_per_sec = Some(1.0);
    s.config.speed.max_concurrent = 1;
    s.config.speed.max_per_host = 1;
    let (ctx, _) = setup_with(s, &dir.path().join("config"), Some(&site));
    let ctx = Arc::new(ctx);

    let first = {
        let ctx = ctx.clone();
        tokio::spawn(async move { run(&ctx, "sched001", Some(at(14, 7, 0)), Trigger::Scheduled).await })
    };
    tokio::time::sleep(std::time::Duration::from_millis(800)).await;
    assert!(spiderlens_schedule::runner::is_running(&ctx.paths, "sched001"));
    let second = done(
        run(&ctx, "sched001", Some(at(15, 7, 0)), Trigger::Scheduled)
            .await
            .unwrap(),
    );
    assert_eq!(second.status, RunStatus::Skipped);
    assert_eq!(exit_code(&Outcome::Done(second)), 7);

    request_stop(&ctx.paths, "sched001").unwrap();
    let first = done(first.await.unwrap().unwrap());
    assert_eq!(first.status, RunStatus::StoppedByUser);
    assert!(!spiderlens_schedule::runner::is_running(&ctx.paths, "sched001"));
}
