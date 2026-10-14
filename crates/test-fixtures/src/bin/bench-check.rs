//! The reduced benchmark that runs in CI (spec 008, FR-034).
//!
//! `bench-check --cli PATH [--base-cli PATH] [--pages 20000] [--runs 3]`
//!
//! Starts `bench-site` (from the same folder as this binary) and crawls it
//! with the `spiderlens` CLI at `--cli`. Two kinds of check:
//!
//! - **Throughput.** A Fast crawl of `--pages` instant pages, `--runs` times.
//!   With `--base-cli` (the CLI built from the base branch), the two builds
//!   take turns on the same machine and the check fails when the best
//!   speed drops more than 10% below the base. Comparing against a build of
//!   the base branch, rather than a number stored in the repo, is what makes
//!   10% meaningful on shared CI machines whose speed varies far more.
//! - **Politeness.** Caps and rate-limit handling, checked from the site's
//!   own `/__stats` (SC-003, SC-004): the URLs-per-second cap, the overall
//!   and per-site request caps, and under 2% of requests rejected by a site
//!   that answers 429 above 4 open requests.
//!
//! Prints a Markdown table (for the CI job summary) and exits 1 on failure.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// Allowed throughput drop against the base build (FR-034).
const MAX_REGRESSION: f64 = 0.10;

fn arg(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

/// A `bench-site` child process, killed when dropped.
struct Site {
    child: Child,
    port: u16,
}

impl Site {
    fn start(port: u16, extra: &[&str]) -> Site {
        let exe = std::env::current_exe().expect("own path");
        let bin = exe.with_file_name(format!("bench-site{}", std::env::consts::EXE_SUFFIX));
        let child = Command::new(&bin)
            .args(["--port", &port.to_string()])
            .args(extra)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap_or_else(|e| panic!("start {}: {e}", bin.display()));
        let site = Site { child, port };
        let deadline = Instant::now() + Duration::from_secs(20);
        while site.get("/__stats").is_none() {
            assert!(Instant::now() < deadline, "bench-site did not start");
            std::thread::sleep(Duration::from_millis(100));
        }
        site
    }

    fn url(&self) -> String {
        format!("http://127.0.0.1:{}/p/0", self.port)
    }

    /// A minimal HTTP GET, enough for `/__stats` and `/__reset`.
    fn get(&self, path: &str) -> Option<String> {
        let mut s = TcpStream::connect(("127.0.0.1", self.port)).ok()?;
        s.set_read_timeout(Some(Duration::from_secs(5))).ok()?;
        write!(
            s,
            "GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
        )
        .ok()?;
        let mut out = String::new();
        s.read_to_string(&mut out).ok()?;
        out.split_once("\r\n\r\n").map(|(_, body)| body.to_string())
    }

    fn stat(&self, key: &str) -> u64 {
        let body = self.get("/__stats").expect("stats");
        let pat = format!("\"{key}\":");
        let start = body.find(&pat).expect("stat key") + pat.len();
        body[start..]
            .chars()
            .take_while(char::is_ascii_digit)
            .collect::<String>()
            .parse()
            .expect("stat value")
    }
}

impl Drop for Site {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

struct Crawl {
    took: Duration,
    /// Largest size the crawl file's write-ahead log reached.
    peak_wal: u64,
}

/// Crawls `site` with `cli` and the given options.
fn crawl(cli: &Path, site: &Site, opts: &[&str], dir: &Path) -> Crawl {
    site.get("/__reset");
    let out = dir.join("bench.spiderlens");
    let wal = dir.join("bench.spiderlens-wal");
    let _ = std::fs::remove_file(&out);
    let started = Instant::now();
    let mut child = Command::new(cli)
        .args(["crawl", &site.url(), "--quiet", "--json", "--out"])
        .arg(&out)
        .args(opts)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| panic!("run {}: {e}", cli.display()));
    let mut peak_wal = 0;
    let status = loop {
        if let Some(status) = child.try_wait().expect("crawl status") {
            break status;
        }
        peak_wal = peak_wal.max(std::fs::metadata(&wal).map_or(0, |m| m.len()));
        std::thread::sleep(Duration::from_millis(20));
    };
    let took = started.elapsed();
    if !status.success() {
        let mut err = String::new();
        let _ = child.stderr.take().expect("piped").read_to_string(&mut err);
        panic!("crawl {opts:?} failed: {status}\n{err}");
    }
    Crawl { took, peak_wal }
}

/// The fastest run: a busy machine only ever slows a run down, so the best
/// of several is far steadier than the median.
fn best(v: Vec<f64>) -> f64 {
    v.into_iter().fold(0.0, f64::max)
}

struct Report {
    rows: Vec<(String, String, bool)>,
}

impl Report {
    fn check(&mut self, name: &str, value: String, ok: bool) {
        eprintln!("{} {name}: {value}", if ok { "ok  " } else { "FAIL" });
        self.rows.push((name.to_string(), value, ok));
    }
}

fn main() {
    let cli = PathBuf::from(arg("--cli").expect("--cli PATH"));
    let base = arg("--base-cli").map(PathBuf::from);
    let pages: u64 = arg("--pages").map_or(20_000, |v| v.parse().expect("--pages N"));
    let runs: usize = arg("--runs").map_or(3, |v| v.parse().expect("--runs N"));
    let dir = std::env::temp_dir().join(format!("bench-check-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let mut report = Report { rows: Vec::new() };

    // Throughput. The builds alternate so a noisy minute hurts both alike.
    let site = Site::start(8761, &["--pages", &pages.to_string()]);
    let rate = |bin: &Path| {
        let took = crawl(bin, &site, &["--speed", "fast"], &dir).took;
        site.stat("requests") as f64 / took.as_secs_f64()
    };
    let mut new_rates = Vec::new();
    let mut base_rates = Vec::new();
    for _ in 0..runs {
        new_rates.push(rate(&cli));
        if let Some(base) = &base {
            base_rates.push(rate(base));
        }
    }
    drop(site);
    let new_rate = best(new_rates);
    if base_rates.is_empty() {
        let name = format!("{pages} pages, Fast");
        report.check(&name, format!("{new_rate:.0} URLs/s"), true);
    } else {
        let base_rate = best(base_rates);
        let change = new_rate / base_rate - 1.0;
        report.check(
            &format!("{pages} pages, Fast, against base"),
            format!("{new_rate:.0} URLs/s vs {base_rate:.0} ({:+.1}%)", change * 100.0),
            change >= -MAX_REGRESSION,
        );
    }

    // The write-ahead log starts over while the crawl runs instead of
    // growing with every page saved (T018: 3.6 GB at 250,000 pages before).
    let site = Site::start(8765, &["--pages", "50000"]);
    let peak = crawl(&cli, &site, &["--speed", "fast"], &dir).peak_wal;
    let mb = peak / 1_000_000;
    report.check("Save log, 50000 pages", format!("peak {mb} MB"), mb < 200);
    drop(site);

    // URLs-per-second cap (SC-003).
    let site = Site::start(8762, &["--pages", "1500"]);
    crawl(&cli, &site, &["--speed", "fast", "--rate", "100"], &dir);
    let peak = site.stat("peak_per_second");
    report.check("Cap 100 URLs/s", format!("server peak {peak}/s"), peak <= 100);
    drop(site);

    // Overall and per-site caps, with slow answers so requests overlap.
    let site = Site::start(8763, &["--pages", "600", "--delay-ms", "50"]);
    crawl(
        &cli,
        &site,
        &["--speed", "fast", "--concurrency", "10", "--per-host", "10"],
        &dir,
    );
    let peak = site.stat("peak_open");
    report.check("Cap 10 at once", format!("server peak {peak} open"), peak <= 10);
    crawl(
        &cli,
        &site,
        &["--speed", "fast", "--per-host", "6", "--no-adaptive"],
        &dir,
    );
    let peak = site.stat("peak_open");
    report.check("Cap 6 per site", format!("server peak {peak} open"), peak <= 6);
    drop(site);

    // A site that rejects above 4 open requests (SC-004: under 2%).
    let site = Site::start(8764, &["--pages", "3000", "--max-open", "4"]);
    crawl(&cli, &site, &["--speed", "fast"], &dir);
    let (rejected, requests) = (site.stat("too_many"), site.stat("requests"));
    let share = rejected as f64 / requests.max(1) as f64;
    report.check(
        "429 above 4 open",
        format!("{rejected} of {requests} rejected ({:.1}%)", share * 100.0),
        share < 0.02,
    );
    drop(site);
    let _ = std::fs::remove_dir_all(&dir);

    println!("| Check | Result | |\n|---|---|---|");
    for (name, value, ok) in &report.rows {
        println!("| {name} | {value} | {} |", if *ok { "pass" } else { "**fail**" });
    }
    if report.rows.iter().any(|(_, _, ok)| !ok) {
        std::process::exit(1);
    }
}
