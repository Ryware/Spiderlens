//! Launch and cleanup with a real browser (spec 014 T010, research R12).
//!
//! This test binary has its own `main` because it plays three parts: the
//! test runner, the guard process (`--render-guard <pid> ...`, as the
//! `spiderlens` binary does), and a parent that launches a browser and is
//! then killed (`--fake-parent`). Without a browser the tests are skipped,
//! unless `SPIDERLENS_REQUIRE_BROWSER=1` (CI).

use std::ffi::OsString;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use spiderlens_render::{Browser, BrowserInfo, GuardCommand, LaunchOptions, browser_for_tests, launch_count};
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

mod render;

type TestFn = fn(&BrowserInfo) -> Result<(), String>;

fn main() {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    if let Some((pid, browser)) = spiderlens_render::parse_guard_args(&args) {
        spiderlens_render::run_guard(pid, browser);
    }
    if args.first().is_some_and(|a| a == "--fake-parent") {
        fake_parent();
    }
    let filter: Option<String> = args
        .iter()
        .filter_map(|a| a.to_str())
        .find(|a| !a.starts_with('-'))
        .map(str::to_owned);
    let tests: &[(&str, TestFn)] = &[
        (
            "launch_blank_close_without_guard",
            launch_blank_close_without_guard,
        ),
        ("launch_blank_close_with_guard", launch_blank_close_with_guard),
        ("drop_without_close_cleans_up", drop_without_close_cleans_up),
        ("restart_replaces_the_process", restart_replaces_the_process),
        (
            "guard_closes_the_browser_when_the_parent_is_killed",
            guard_closes_the_browser_when_the_parent_is_killed,
        ),
        (
            "render_pages_of_the_js_fixture",
            render::render_pages_of_the_js_fixture,
        ),
    ];
    let info = match browser_for_tests() {
        Ok(Some(b)) => b,
        Ok(None) => return,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    println!(
        "browser: {:?} {} at {}",
        info.name,
        info.version,
        info.path.display()
    );
    let mut failed = 0;
    for (name, test) in tests {
        if filter.as_deref().is_some_and(|f| !name.contains(f)) {
            continue;
        }
        let started = Instant::now();
        match test(&info) {
            Ok(()) => println!("test {name} ... ok ({:.1} s)", started.elapsed().as_secs_f64()),
            Err(e) => {
                failed += 1;
                println!("test {name} ... FAILED: {e}");
            }
        }
    }
    if failed > 0 {
        std::process::exit(1);
    }
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("runtime")
}

fn guard() -> LaunchOptions {
    LaunchOptions {
        guard: Some(GuardCommand::current_exe().expect("exe")),
        ..LaunchOptions::default()
    }
}

/// Processes whose command line names `profile`.
fn using(profile: &Path) -> Vec<u32> {
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_cmd(UpdateKind::Always),
    );
    let needle = profile.to_string_lossy().into_owned();
    sys.processes()
        .iter()
        .filter(|(_, p)| p.thread_kind().is_none())
        .filter(|(_, p)| p.cmd().iter().any(|a| a.to_string_lossy().contains(&needle)))
        .map(|(pid, _)| pid.as_u32())
        .collect()
}

/// Waits up to `within` for no process to use `profile` and for the folder
/// to be gone.
fn all_gone(profile: &Path, within: Duration) -> Result<(), String> {
    let deadline = Instant::now() + within;
    loop {
        let left = using(profile);
        if left.is_empty() && !profile.exists() {
            return Ok(());
        }
        if Instant::now() > deadline {
            return Err(format!(
                "after {within:?}: processes {left:?} still use {}, folder exists: {}",
                profile.display(),
                profile.exists()
            ));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn launch_blank_close(info: &BrowserInfo, opts: LaunchOptions) -> Result<(), String> {
    let before = launch_count();
    let profile = runtime().block_on(async {
        let browser = Browser::launch(info, opts)
            .await
            .map_err(|e| format!("launch: {e:?}"))?;
        let profile = browser.profile_dir().ok_or("no profile")?.to_path_buf();
        if using(&profile).is_empty() {
            return Err("no browser process found".to_string());
        }
        if browser.memory_bytes() == 0 {
            return Err("no memory measured".into());
        }
        let got = browser.smoke_test().await?;
        if got != "en-US 42" {
            return Err(format!("about:blank evaluated to {got:?}"));
        }
        browser.close().await;
        Ok::<PathBuf, String>(profile)
    })?;
    if launch_count() != before + 1 {
        return Err("launch counter".into());
    }
    all_gone(&profile, Duration::from_secs(5))
}

fn launch_blank_close_without_guard(info: &BrowserInfo) -> Result<(), String> {
    launch_blank_close(info, LaunchOptions::default())
}

fn launch_blank_close_with_guard(info: &BrowserInfo) -> Result<(), String> {
    launch_blank_close(info, guard())
}

fn drop_without_close_cleans_up(info: &BrowserInfo) -> Result<(), String> {
    let profile = runtime().block_on(async {
        let browser = Browser::launch(info, guard())
            .await
            .map_err(|e| format!("launch: {e:?}"))?;
        let profile = browser.profile_dir().ok_or("no profile")?.to_path_buf();
        drop(browser);
        Ok::<PathBuf, String>(profile)
    })?;
    all_gone(&profile, Duration::from_secs(5))
}

fn restart_replaces_the_process(info: &BrowserInfo) -> Result<(), String> {
    runtime().block_on(async {
        let mut browser = Browser::launch(info, LaunchOptions::default())
            .await
            .map_err(|e| format!("launch: {e:?}"))?;
        let first = browser.profile_dir().ok_or("no profile")?.to_path_buf();
        browser.restart().await.map_err(|e| format!("restart: {e:?}"))?;
        let second = browser.profile_dir().ok_or("no profile")?.to_path_buf();
        if first == second {
            return Err("same profile after restart".into());
        }
        all_gone(&first, Duration::from_secs(5))?;
        browser.smoke_test().await?;
        browser.close().await;
        all_gone(&second, Duration::from_secs(5))
    })
}

/// Launches a browser through the guard, prints its profile folder, and
/// waits to be killed.
fn fake_parent() -> ! {
    let info = match browser_for_tests() {
        Ok(Some(b)) => b,
        _ => std::process::exit(5),
    };
    let rt = runtime();
    let browser = rt.block_on(Browser::launch(&info, guard())).expect("launch");
    println!("{}", browser.profile_dir().expect("profile").display());
    loop {
        std::thread::sleep(Duration::from_secs(60));
    }
}

/// SC-009 at the process level: kill SpiderLens and the browser goes too.
fn guard_closes_the_browser_when_the_parent_is_killed(_: &BrowserInfo) -> Result<(), String> {
    let mut parent = Command::new(std::env::current_exe().map_err(|e| e.to_string())?)
        .arg("--fake-parent")
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    let mut line = String::new();
    BufReader::new(parent.stdout.take().ok_or("stdout")?)
        .read_line(&mut line)
        .map_err(|e| e.to_string())?;
    let profile = PathBuf::from(line.trim());
    if line.trim().is_empty() || using(&profile).is_empty() {
        let _ = parent.kill();
        return Err(format!("the fake parent did not start a browser: {line:?}"));
    }
    // SIGKILL or TerminateProcess: no cleanup code runs in the parent.
    parent.kill().map_err(|e| e.to_string())?;
    parent.wait().map_err(|e| e.to_string())?;
    all_gone(&profile, Duration::from_secs(10))
}
