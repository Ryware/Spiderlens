//! Spec 019 (crawl data management) engine scenarios: stored-HTML size and
//! removal, helper files, backups, and the automatic cleanup decision.
//! Test names carry the scenario or success-criterion id.

use std::collections::HashSet;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use jiff::{SignedDuration, Timestamp};
use rusqlite::Connection;
use spiderlens_config::{CrawlConfig, ModeInput, SpeedSettings};
use spiderlens_engine::crawl::file::{self, FileError, WriterLock};
use spiderlens_engine::crawl::manage::{self, Candidate, CleanupRule, Kind, ScanContext, plan_cleanup};
use spiderlens_engine::crawl::{Control, Crawler};
use spiderlens_test_fixtures::{Site, SiteBuilder};
use tokio::sync::watch;

const HOST: &str = "127.0.0.1";
const PAGES: usize = 60;

fn site() -> Site {
    let mut b = SiteBuilder::new();
    let mut seed: u64 = 0x2545_f491_4f6c_dd1d;
    for i in 0..PAGES {
        let links: Vec<String> = (1..=5)
            .map(|k| i * 5 + k)
            .filter(|&c| c < PAGES)
            .map(|c| format!("/p/{c}"))
            .collect();
        let refs: Vec<&str> = links.iter().map(String::as_str).collect();
        // Text that does not compress away, so stored HTML has real weight.
        let words: Vec<String> = (0..1500)
            .map(|_| {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                format!("{:x}", seed % 0xffff_ffff)
            })
            .collect();
        b = b.page(
            HOST,
            &format!("/p/{i}"),
            &format!("<p>{}</p>", words.join(" ")),
            &refs,
        );
    }
    b.start()
}

async fn crawl(dir: &Path, name: &str) -> PathBuf {
    let site = site();
    let path = dir.join(name);
    let config = CrawlConfig {
        speed: SpeedSettings::FAST,
        ..CrawlConfig::default()
    };
    let input = ModeInput::Spider {
        start_url: site.url(HOST, "/p/0"),
    };
    let crawler = Crawler::create(&path, config, &input).unwrap();
    let (_tx, rx) = watch::channel(Control::Run);
    let s = crawler.run(rx, None).await.unwrap();
    assert_eq!(s.crawled as usize, PAGES);
    path
}

/// Every table except stored HTML, row by row, for "results unchanged"
/// (FR-013). `events` grows by the purge record and is compared without it.
fn dump(path: &Path) -> Vec<String> {
    let conn = Connection::open(path).unwrap();
    let tables: Vec<String> = conn
        .prepare(
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT IN ('bodies') ORDER BY name",
        )
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    let mut out = Vec::new();
    for t in tables {
        let filter = if t == "events" {
            " WHERE kind != 'html_purged'"
        } else {
            ""
        };
        let mut stmt = conn
            .prepare(&format!("SELECT * FROM \"{t}\"{filter} ORDER BY rowid"))
            .unwrap();
        let n = stmt.column_count();
        let names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
        let rows = stmt
            .query_map([], |r| {
                let mut row = Vec::with_capacity(n);
                for i in 0..n {
                    // html_stored is the one column a purge must change.
                    if t == "pages" && names[i] == "html_stored" {
                        continue;
                    }
                    row.push(format!("{:?}", r.get_ref(i)?));
                }
                Ok(format!("{t}: {}", row.join("|")))
            })
            .unwrap();
        for r in rows {
            out.push(r.unwrap());
        }
    }
    out
}

fn set_age(path: &Path, age: Duration) {
    let f = File::options().write(true).open(path).unwrap();
    f.set_modified(SystemTime::now() - age).unwrap();
}

const DAY: Duration = Duration::from_secs(24 * 3600);

#[tokio::test]
async fn us1_html_size_counts_stored_pages() {
    let dir = tempfile::tempdir().unwrap();
    let path = crawl(dir.path(), "a.spiderlens").await;
    let size = manage::html_size(&path).unwrap();
    assert_eq!(size.pages as usize, PAGES);
    assert!(size.bytes > 0);
    assert!(size.bytes < file::file_size(&path));
}

#[tokio::test]
async fn us3_2_purge_keeps_every_result_and_shrinks_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = crawl(dir.path(), "a.spiderlens").await;
    let before = dump(&path);
    let size_before = file::file_size(&path);
    let html = manage::html_size(&path).unwrap();

    let report = manage::purge_html(&path, true).unwrap();
    assert_eq!(report.html_pages as usize, PAGES);
    assert!(report.compacted);
    assert_eq!(manage::html_size(&path).unwrap().pages, 0);
    assert_eq!(dump(&path), before, "no result changed (FR-013)");
    let size_after = file::file_size(&path);
    assert!(
        size_before - size_after >= html.bytes * 9 / 10,
        "freed {} of {} HTML bytes",
        size_before - size_after,
        html.bytes
    );
    // The file is a single, healthy crawl afterwards.
    let ok: String = Connection::open(&path)
        .unwrap()
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .unwrap();
    assert_eq!(ok, "ok");
    assert!(!file::partial_path(&path).exists());
}

#[tokio::test]
async fn us3_purge_records_an_event_and_clears_the_page_flag() {
    let dir = tempfile::tempdir().unwrap();
    let path = crawl(dir.path(), "a.spiderlens").await;
    manage::purge_html(&path, false).unwrap();
    let info = file::info(&path).unwrap();
    let ev = info
        .events
        .iter()
        .find(|e| e.kind == "html_purged")
        .expect("purge event (FR-016)");
    assert!(ev.detail.as_deref().unwrap_or_default().contains("\"pages\":60"));
    let stored: i64 = Connection::open(&path)
        .unwrap()
        .query_row("SELECT COUNT(*) FROM pages WHERE html_stored = 1", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(stored, 0);
}

#[tokio::test]
async fn us3_4_purge_without_compaction_frees_pages_inside_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = crawl(dir.path(), "a.spiderlens").await;
    let before = dump(&path);
    let report = manage::purge_html(&path, false).unwrap();
    assert!(!report.compacted);
    assert_eq!(dump(&path), before);
    let free_pages: i64 = Connection::open(&path)
        .unwrap()
        .query_row("PRAGMA freelist_count", [], |r| r.get(0))
        .unwrap();
    assert!(free_pages > 0, "space is reusable by the crawl");
    // Compacting later still works and keeps the results.
    manage::purge_html(&path, true).unwrap();
    assert_eq!(dump(&path), before);
}

#[tokio::test]
async fn us3_5_purge_is_refused_while_another_writer_holds_the_crawl() {
    let dir = tempfile::tempdir().unwrap();
    let path = crawl(dir.path(), "a.spiderlens").await;
    let _held = WriterLock::acquire(&path).unwrap();
    // A second acquire from the same process is refused like another
    // process would be (the OS lock is per open file).
    let err = manage::purge_html(&path, true).unwrap_err();
    assert!(matches!(err, FileError::Lock(_)), "{err}");
    assert_eq!(manage::html_size(&path).unwrap().pages as usize, PAGES);
}

#[tokio::test]
async fn us3_3_a_crash_before_the_rename_leaves_the_original_valid() {
    let dir = tempfile::tempdir().unwrap();
    let path = crawl(dir.path(), "a.spiderlens").await;
    manage::purge_html(&path, false).unwrap();
    let before = dump(&path);
    // What an interrupted compaction leaves: a half-written hidden copy.
    std::fs::write(file::partial_path(&path), b"half a file").unwrap();
    assert_eq!(dump(&path), before);
    // The next compaction replaces the stale copy.
    manage::purge_html(&path, true).unwrap();
    assert_eq!(dump(&path), before);
    assert!(!file::partial_path(&path).exists());
}

#[tokio::test]
async fn us3_check_reports_space_and_compaction_room() {
    let dir = tempfile::tempdir().unwrap();
    let path = crawl(dir.path(), "a.spiderlens").await;
    let c = manage::purge_check(&path).unwrap();
    assert_eq!(c.html_pages as usize, PAGES);
    assert_eq!(c.new_size, c.file_bytes - c.html_bytes);
    assert!(c.can_compact);
}

#[test]
fn us2_crawl_files_lists_the_crawl_and_its_helpers() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.spiderlens");
    for p in [
        path.clone(),
        dir.path().join("a.spiderlens-wal"),
        dir.path().join("a.spiderlens-shm"),
        dir.path().join("a.spiderlens.lock"),
        dir.path().join(".a.spiderlens.partial"),
        dir.path().join("b.spiderlens.lock"),
    ] {
        std::fs::write(p, b"x").unwrap();
    }
    let files = manage::crawl_files(&path);
    assert_eq!(files.len(), 5);
    assert!(!files.iter().any(|p| p.ends_with("b.spiderlens.lock")));
    assert_eq!(manage::crawl_files_size(&path), 5);
}

#[test]
fn us5_1_backups_are_found_next_to_their_crawl() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("site.spiderlens");
    std::fs::write(&path, b"x").unwrap();
    std::fs::write(dir.path().join("site (format 2 backup).spiderlens"), b"xx").unwrap();
    std::fs::write(dir.path().join("site (format 2 backup 2).spiderlens"), b"xxx").unwrap();
    std::fs::write(dir.path().join("other (format 2 backup).spiderlens"), b"x").unwrap();
    let b = manage::backups_of(&path);
    assert_eq!(b.len(), 2);
    assert!(b.iter().all(|b| b.format_version == 2));
    assert_eq!(b.iter().map(|b| b.size_bytes).sum::<u64>(), 5);
}

#[test]
fn last_used_prefers_the_recent_list_and_never_lies_about_the_future() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.spiderlens");
    std::fs::write(&path, b"not a database").unwrap();
    set_age(&path, 40 * DAY);
    let now = Timestamp::now();
    let from_mtime = manage::last_used(&path, None, now).unwrap();
    assert!(now.duration_since(from_mtime) > SignedDuration::from_hours(39 * 24));
    let recent = (now - SignedDuration::from_hours(2)).to_string();
    let from_recent = manage::last_used(&path, Some(&recent), now).unwrap();
    assert_eq!(from_recent.to_string(), recent);
    let future = (now + SignedDuration::from_hours(48)).to_string();
    assert_eq!(manage::last_used(&path, Some(&future), now), Some(now));
}

// ---------------------------------------------------------------------------
// The cleanup decision (plan D5): the rule that must never fail.

fn now() -> Timestamp {
    "2026-10-13T12:00:00Z".parse().unwrap()
}

fn ago(days: i64) -> Option<Timestamp> {
    Some(now() - SignedDuration::from_hours(24 * days))
}

/// SC-003: every combination of the facts the decision reads.
#[test]
fn sc003_cleanup_never_plans_a_protected_file() {
    let kinds = [
        Kind::Crawl,
        Kind::Backup,
        Kind::Other,
        Kind::Helper { orphan: true },
        Kind::Helper { orphan: false },
    ];
    let ages = [None, ago(-3), ago(0), ago(1), ago(29), ago(31), ago(400)];
    let rules = [
        CleanupRule {
            days: Some(30),
            announced: true,
        },
        CleanupRule {
            days: Some(30),
            announced: false,
        },
        CleanupRule {
            days: Some(7),
            announced: true,
        },
        CleanupRule {
            days: None,
            announced: true,
        },
    ];
    let mut cases = 0;
    for kind in kinds {
        for in_unsaved_dir in [true, false] {
            for locked in [true, false] {
                for open in [true, false] {
                    for auto_removed_before in [true, false] {
                        for age in ages {
                            for rule in rules {
                                cases += 1;
                                let c = Candidate {
                                    path: PathBuf::from(format!("/x/{cases}")),
                                    kind,
                                    in_unsaved_dir,
                                    last_used: age,
                                    modified: age,
                                    locked,
                                    open,
                                    auto_removed_before,
                                    size_bytes: 1,
                                };
                                let plan = plan_cleanup(std::slice::from_ref(&c), rule, now());
                                let planned = !plan.trash.is_empty() || !plan.announce.is_empty();
                                let removed = planned || !plan.leftovers.is_empty();
                                if locked || open || matches!(kind, Kind::Backup | Kind::Other) {
                                    assert!(!removed, "{c:?} {rule:?}");
                                }
                                if planned {
                                    assert_eq!(kind, Kind::Crawl);
                                    assert!(in_unsaved_dir, "a saved crawl was planned: {c:?}");
                                    assert!(!auto_removed_before);
                                    let days = rule.days.expect("cleanup on");
                                    let old = now().duration_since(age.unwrap());
                                    assert!(old > SignedDuration::from_hours(24 * i64::from(days)));
                                    assert_eq!(plan.trash.is_empty(), !rule.announced);
                                }
                                if !plan.leftovers.is_empty() {
                                    assert_eq!(kind, Kind::Helper { orphan: true });
                                    assert!(age.unwrap() < now() - SignedDuration::from_hours(24));
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    assert_eq!(cases, 5 * 2 * 2 * 2 * 2 * 7 * 4);
}

#[test]
fn us4_1_old_unsaved_crawls_are_planned_after_the_notice() {
    let c = |name: &str, days: i64| Candidate {
        path: PathBuf::from(name),
        kind: Kind::Crawl,
        in_unsaved_dir: true,
        last_used: ago(days),
        modified: ago(days),
        locked: false,
        open: false,
        auto_removed_before: false,
        size_bytes: 1,
    };
    let all = [c("10", 10), c("31", 31), c("90", 90)];
    let plan = plan_cleanup(
        &all,
        CleanupRule {
            days: Some(30),
            announced: true,
        },
        now(),
    );
    assert_eq!(plan.trash, [PathBuf::from("31"), PathBuf::from("90")]);
    // US4.5: before the one-time notice nothing is removed, only announced.
    let first = plan_cleanup(
        &all,
        CleanupRule {
            days: Some(30),
            announced: false,
        },
        now(),
    );
    assert!(first.trash.is_empty());
    assert_eq!(first.announce.len(), 2);
    // US4.4: off means no crawl is ever planned.
    assert!(
        plan_cleanup(
            &all,
            CleanupRule {
                days: None,
                announced: true
            },
            now()
        )
        .is_empty()
    );
}

#[test]
fn us4_scan_finds_leftovers_and_spares_live_helpers_and_saved_crawls() {
    let unsaved = tempfile::tempdir().unwrap();
    let saved = tempfile::tempdir().unwrap();
    let u = unsaved.path();
    let s = saved.path();
    let write = |p: PathBuf, age: Duration| {
        std::fs::write(&p, b"x").unwrap();
        set_age(&p, age);
        p
    };
    let old_unsaved = write(u.join("old.spiderlens"), 60 * DAY);
    let new_unsaved = write(u.join("new.spiderlens"), DAY);
    let live_wal = write(u.join("new.spiderlens-wal"), 3 * DAY);
    let orphan_lock = write(u.join("gone.spiderlens.lock"), 3 * DAY);
    let fresh_orphan = write(u.join("gone2.spiderlens-wal"), Duration::from_secs(60));
    let stale_partial = write(u.join(".copy.spiderlens.partial"), 2 * DAY);
    let backup = write(u.join("old (format 2 backup).spiderlens"), 400 * DAY);
    let own_file = write(u.join("notes.txt"), 400 * DAY);
    let old_saved = write(s.join("saved.spiderlens"), 400 * DAY);
    let saved_orphan = write(s.join("deleted.spiderlens-shm"), 5 * DAY);

    let open = HashSet::new();
    let removed = HashSet::new();
    let recent = |_: &Path| None;
    let dirs = [s.to_path_buf()];
    let candidates = manage::scan(&ScanContext {
        unsaved_dir: u,
        other_dirs: &dirs,
        recent_last_used: &recent,
        open: &open,
        auto_removed: &removed,
        now: Timestamp::now(),
    });
    let plan = plan_cleanup(
        &candidates,
        CleanupRule {
            days: Some(30),
            announced: true,
        },
        Timestamp::now(),
    );
    let mut trash = plan.trash.clone();
    trash.sort();
    assert_eq!(trash, [old_unsaved]);
    let mut leftovers = plan.leftovers.clone();
    leftovers.sort();
    let mut expected = vec![orphan_lock, stale_partial, saved_orphan];
    expected.sort();
    assert_eq!(leftovers, expected);
    for kept in [
        &new_unsaved,
        &live_wal,
        &fresh_orphan,
        &backup,
        &own_file,
        &old_saved,
    ] {
        assert!(
            !plan.trash.contains(kept) && !plan.leftovers.contains(kept),
            "{kept:?}"
        );
    }
}

#[test]
fn us4_2_open_and_restored_crawls_are_kept() {
    let unsaved = tempfile::tempdir().unwrap();
    let u = unsaved.path();
    let open_one = u.join("open.spiderlens");
    let restored = u.join("restored.spiderlens");
    for p in [&open_one, &restored] {
        std::fs::write(p, b"x").unwrap();
        set_age(p, 90 * DAY);
    }
    let open: HashSet<PathBuf> = [open_one.clone()].into();
    let removed: HashSet<PathBuf> = [restored.clone()].into();
    let recent = |_: &Path| None;
    let candidates = manage::scan(&ScanContext {
        unsaved_dir: u,
        other_dirs: &[],
        recent_last_used: &recent,
        open: &open,
        auto_removed: &removed,
        now: Timestamp::now(),
    });
    let plan = plan_cleanup(
        &candidates,
        CleanupRule {
            days: Some(7),
            announced: true,
        },
        Timestamp::now(),
    );
    assert!(plan.is_empty(), "{plan:?}");
}

#[test]
fn sc004_a_hundred_crash_leftovers_are_all_found() {
    let unsaved = tempfile::tempdir().unwrap();
    let u = unsaved.path();
    let mut made = Vec::new();
    for i in 0..100 {
        let p = match i % 4 {
            0 => u.join(format!("c{i}.spiderlens.lock")),
            1 => u.join(format!("c{i}.spiderlens-wal")),
            2 => u.join(format!("c{i}.spiderlens-shm")),
            _ => u.join(format!(".c{i}.spiderlens.partial")),
        };
        std::fs::write(&p, b"x").unwrap();
        set_age(&p, 2 * DAY);
        made.push(p);
    }
    let open = HashSet::new();
    let removed = HashSet::new();
    let recent = |_: &Path| None;
    let candidates = manage::scan(&ScanContext {
        unsaved_dir: u,
        other_dirs: &[],
        recent_last_used: &recent,
        open: &open,
        auto_removed: &removed,
        now: Timestamp::now(),
    });
    let plan = plan_cleanup(
        &candidates,
        CleanupRule {
            days: None,
            announced: false,
        },
        Timestamp::now(),
    );
    assert_eq!(plan.leftovers.len(), 100);
}
