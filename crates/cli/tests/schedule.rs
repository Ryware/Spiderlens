//! CLI contract tests for spec 021 (specs/021-scheduled-crawls/contracts/cli.md).
//! The system scheduler is replaced by a JSON file and the clock can be set,
//! so nothing here touches the real launchd, Task Scheduler or crontab.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use assert_cmd::Command;
use predicates::str::contains;
use serde_json::Value;
use spiderlens_test_fixtures::{Site, SiteBuilder};

const HOST: &str = "127.0.0.1";

struct Env {
    dir: tempfile::TempDir,
}

impl Env {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("out")).unwrap();
        Self { dir }
    }

    fn out(&self) -> PathBuf {
        self.dir.path().join("out")
    }

    fn os_file(&self) -> PathBuf {
        self.dir.path().join("os.json")
    }

    fn cmd(&self) -> Command {
        let mut c = Command::cargo_bin("spiderlens").expect("binary built");
        c.env("SPIDERLENS_CONFIG_DIR", self.dir.path().join("config"))
            .env("SPIDERLENS_SCHEDULER", self.os_file())
            .env("SPIDERLENS_NOTIFY_LOG", self.dir.path().join("notes.log"))
            .env("SPIDERLENS_TZ", "UTC")
            .arg("schedule");
        c
    }

    fn json(&self, args: &[&str]) -> Value {
        let out = self.cmd().args(args).arg("--json").output().unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        serde_json::from_slice(&out.stdout).unwrap()
    }

    fn registrations(&self) -> Value {
        std::fs::read(self.os_file())
            .map(|b| serde_json::from_slice(&b).unwrap())
            .unwrap_or(Value::Null)
    }
}

fn site() -> Site {
    SiteBuilder::new()
        .page(HOST, "/", "", &["/a"])
        .page(HOST, "/a", "", &[])
        .start()
}

fn add(env: &Env, name: &str, url: &str, when: &[&str]) -> Value {
    let out = env.out();
    let mut args = vec!["add", "--name", name, url, "--out", out.to_str().unwrap()];
    args.extend_from_slice(when);
    env.json(&args)
}

#[test]
fn add_list_show_edit_enable_disable_remove() {
    let env = Env::new();
    let s = add(
        &env,
        "Client A",
        "https://example.com/",
        &["--weekly", "mon,thu@07:00", "--export", "internal"],
    );
    let id = s["id"].as_str().unwrap().to_string();
    assert_eq!(
        s["frequency"],
        serde_json::json!({"kind": "weekly", "days": [1, 4], "time": {"hour": 7, "minute": 0}})
    );
    assert_eq!(s["next_runs"].as_array().unwrap().len(), 5);
    assert_eq!(s["registration"]["state"], "registered");
    assert_eq!(
        env.registrations()[&id]["args"],
        serde_json::json!(["schedule", "tick", id])
    );

    let list = env.json(&["list"]);
    assert_eq!(list[0]["name"], "Client A");
    assert_eq!(list[0]["frequency"], "Every Mon and Thu at 07:00");

    // Names find schedules too, ignoring case; only given options change.
    let edited = env.json(&["edit", "client a", "--daily", "06:30", "--max-urls", "50"]);
    assert_eq!(edited["frequency"]["kind"], "daily");
    assert_eq!(edited["config"]["limits"]["max_urls"], 50);
    assert_eq!(edited["after"][0]["tabs"][0]["tab"], "internal");

    let off = env.json(&["disable", &id]);
    assert_eq!(off["registration"]["state"], "suspended");
    assert!(env.registrations().get(&id).is_none());
    env.json(&["enable", &id]);
    assert!(env.registrations().get(&id).is_some());

    let copy = env.json(&["duplicate", &id, "--name", "Client B"]);
    assert_eq!(copy["enabled"], false);

    env.cmd()
        .args(["remove", "Client A"])
        .assert()
        .success()
        .stderr(contains("crawl files"));
    assert_eq!(env.json(&["list"]).as_array().unwrap().len(), 1);
    env.cmd().args(["remove-all"]).assert().code(2);
    env.cmd().args(["remove-all", "--yes"]).assert().success();
    assert_eq!(env.json(&["list"]).as_array().unwrap().len(), 0);
    assert_eq!(env.registrations(), serde_json::json!({}));
}

#[test]
fn invalid_schedules_are_refused() {
    let env = Env::new();
    let out = env.out();
    let out = out.to_str().unwrap();
    env.cmd()
        .args([
            "add",
            "--name",
            "x",
            "https://example.com/",
            "--out",
            out,
            "--cron",
            "*/5 * * * *",
        ])
        .assert()
        .code(2)
        .stderr(contains("at most once an hour"));
    env.cmd()
        .args([
            "add",
            "--name",
            "x",
            "https://example.com/",
            "--out",
            out,
            "--daily",
            "07:00",
        ])
        .args(["--secret-header", "Authorization: Bearer abc"])
        .assert()
        .code(2)
        .stderr(contains("marked secret"));
    env.cmd()
        .args(["add", "https://example.com/", "--daily", "07:00"])
        .assert()
        .code(2)
        .stderr(contains("--name"));
    assert_eq!(env.json(&["list"]).as_array().unwrap().len(), 0);
}

#[test]
fn keep_last_defaults_to_ten_and_can_change() {
    let env = Env::new();
    let s = add(&env, "Client A", "https://example.com/", &["--daily", "07:00"]);
    let id = s["id"].as_str().unwrap().to_string();
    assert_eq!(s["keep_last"], 10);

    let three = env.json(&["edit", &id, "--keep-last", "3"]);
    assert_eq!(three["keep_last"], 3);
    let all = env.json(&["edit", &id, "--keep-last", "all"]);
    assert!(all["keep_last"].is_null());
    env.cmd()
        .args(["show", id.as_str()])
        .assert()
        .success()
        .stdout(contains("all runs"));

    env.cmd()
        .args(["edit", id.as_str(), "--keep-last", "0"])
        .assert()
        .code(2)
        .stderr(contains("between 1 and 999"));
    env.cmd()
        .args(["edit", id.as_str(), "--keep-last", "many"])
        .assert()
        .code(2)
        .stderr(contains("not a number of runs"));
    assert!(env.json(&["show", &id])["keep_last"].is_null());
}

fn wait_for_history(env: &Env, id: &str) -> Value {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let h = env.json(&["history", id]);
        if h.as_array()
            .is_some_and(|a| a.first().is_some_and(|r| r["status"] != "running"))
        {
            return h;
        }
        assert!(Instant::now() < deadline, "no finished run: {h}");
        std::thread::sleep(Duration::from_millis(200));
    }
}

#[test]
fn tick_starts_a_run_in_the_background() {
    let site = site();
    let env = Env::new();
    let s = add(&env, "Nightly", &site.url(HOST, "/"), &["--daily", "07:00"]);
    let id = s["id"].as_str().unwrap();

    // The clock says tomorrow 07:01: the 07:00 run is due.
    let tomorrow = jiff::Timestamp::now()
        .to_zoned(jiff::tz::TimeZone::UTC)
        .date()
        .tomorrow()
        .unwrap()
        .at(7, 1, 0, 0)
        .to_zoned(jiff::tz::TimeZone::UTC)
        .unwrap()
        .timestamp()
        .to_string();
    env.cmd()
        .args(["tick", id])
        .env("SPIDERLENS_TEST", "1")
        .env("SPIDERLENS_NOW", &tomorrow)
        .assert()
        .success();
    let h = wait_for_history(&env, id);
    assert_eq!(h[0]["status"], "finished", "{h}");
    assert_eq!(h[0]["trigger"], "scheduled");
    let file = Path::new(h[0]["file"].as_str().unwrap());
    assert!(file.starts_with(env.out()));
    assert!(
        file.file_name()
            .unwrap()
            .to_string_lossy()
            .ends_with(" 0700.spiderlens")
    );
    assert!(file.is_file());

    // A second tick in the same hour does nothing.
    env.cmd()
        .args(["tick", id])
        .env("SPIDERLENS_TEST", "1")
        .env("SPIDERLENS_NOW", &tomorrow)
        .assert()
        .success();
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(env.json(&["history", id]).as_array().unwrap().len(), 1);
}

#[test]
fn run_now_in_the_foreground_and_exit_codes() {
    let site = site();
    let env = Env::new();
    let s = add(
        &env,
        "Now",
        &site.url(HOST, "/"),
        &["--hourly", "6", "--minute", "15", "--export", "internal"],
    );
    let id = s["id"].as_str().unwrap();
    let run = env.json(&["run-now", id]);
    assert_eq!(run["status"], "finished");
    assert_eq!(run["trigger"], "run_now");
    assert_eq!(run["counts"]["crawled"], 2);
    assert!(Path::new(run["steps"][0]["outputs"][0].as_str().unwrap()).is_file());

    // A site that cannot be reached: exit 4, reason recorded, notification logged.
    let bad = add(
        &env,
        "Bad",
        &spiderlens_test_fixtures::closed_port_url(),
        &["--daily", "07:00"],
    );
    let bad_id = bad["id"].as_str().unwrap();
    env.cmd().args(["run-now", bad_id]).assert().code(4);
    let h = env.json(&["history", "bad"]);
    assert_eq!(h[0]["status"], "failed");
    let notes = std::fs::read_to_string(env.dir.path().join("notes.log")).unwrap();
    assert!(notes.starts_with("SpiderLens: Bad\tFailed: "), "{notes}");
}

#[test]
fn repair_points_registrations_at_this_program() {
    let env = Env::new();
    let s = add(&env, "Moved", "https://example.com/", &["--daily", "07:00"]);
    let id = s["id"].as_str().unwrap().to_string();
    // Simulate an older install location and an orphan.
    let mut regs = env.registrations();
    regs[&id]["program"] = "/old/place/spiderlens".into();
    regs["orphan01"] = serde_json::json!({"program": "/old/place/spiderlens", "minute": 0, "args": []});
    std::fs::write(env.os_file(), serde_json::to_vec(&regs).unwrap()).unwrap();

    let report = env.json(&["repair"]);
    assert_eq!(report["repaired"], serde_json::json!(["Moved"]));
    assert_eq!(report["orphans"], serde_json::json!(["orphan01"]));
    assert_ne!(env.registrations()[&id]["program"], "/old/place/spiderlens");
}
