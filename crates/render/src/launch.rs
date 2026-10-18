//! Starting and stopping the browser (plan D1 "Launch flags", D12
//! "Cleanup"; research R12).
//!
//! Each browser gets a temporary profile, `spiderlens-render-<pid>-<n>` in
//! the system temporary folder, deleted when the browser closes, when the
//! [`Browser`] is dropped, by the guard process when SpiderLens dies, and,
//! as a last resort, by the startup sweep of the next launch.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::time::Duration;

use chromiumoxide::handler::HandlerConfig;
use futures_util::StreamExt;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::task::JoinHandle;

use crate::guard::{GUARD_ARG, kill_tree, remove_profile, tree_memory};
use crate::types::{BrowserInfo, BrowserName, LaunchError, LaunchOptions};

/// Prefix of every temporary profile folder.
const PROFILE_PREFIX: &str = "spiderlens-render-";

/// How long the browser may take to print its DevTools address.
const START_TIMEOUT: Duration = Duration::from_secs(30);

/// How long a polite close may take before the process tree is killed.
const CLOSE_TIMEOUT: Duration = Duration::from_secs(5);

static LAUNCHES: AtomicU64 = AtomicU64::new(0);
static PROFILES: AtomicU32 = AtomicU32::new(0);

/// How many browsers this process has launched. For tests that check a
/// crawl with rendering off starts no browser.
#[doc(hidden)]
pub fn launch_count() -> u64 {
    LAUNCHES.load(Ordering::Relaxed)
}

/// The launch flags of plan D1 for a browser using `profile`.
pub(crate) fn browser_args(info: &BrowserInfo, profile: &Path, heap_limit_mb: u32) -> Vec<OsString> {
    let mut args: Vec<OsString> = vec![
        if info.name == BrowserName::ChromeHeadlessShell {
            "--headless".into()
        } else {
            "--headless=new".into()
        },
        "--remote-debugging-port=0".into(),
        {
            let mut a = OsString::from("--user-data-dir=");
            a.push(profile);
            a
        },
        "--no-first-run".into(),
        "--no-default-browser-check".into(),
        "--disable-default-apps".into(),
        "--disable-background-networking".into(),
        "--disable-component-update".into(),
        "--disable-sync".into(),
        "--disable-extensions".into(),
        "--disable-breakpad".into(),
        "--disable-client-side-phishing-detection".into(),
        "--disable-domain-reliability".into(),
        "--disable-features=Translate,OptimizationHints,MediaRouter,DialMediaRouteProvider,\
         AutofillServerCommunication,CertificateTransparencyComponentUpdater"
            .into(),
        "--metrics-recording-only".into(),
        "--password-store=basic".into(),
        "--use-mock-keychain".into(),
        "--mute-audio".into(),
        "--hide-scrollbars".into(),
        "--lang=en-US".into(),
        format!("--js-flags=--max-old-space-size={heap_limit_mb}").into(),
        // The browser never reaches the network itself: every request is
        // answered by the engine (research R6). Names do not resolve, which
        // also refuses WebSockets to host names.
        "--host-resolver-rules=MAP * ~NOTFOUND".into(),
    ];
    if cfg!(target_os = "linux") {
        args.push("--disable-dev-shm-usage".into());
    }
    if running_as_root() {
        // Chromium refuses to start its sandbox as root (containers, CI).
        args.push("--no-sandbox".into());
    }
    args.push("about:blank".into());
    args
}

#[cfg(target_os = "linux")]
fn running_as_root() -> bool {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata("/proc/self").is_ok_and(|m| m.uid() == 0)
}

#[cfg(not(target_os = "linux"))]
fn running_as_root() -> bool {
    false
}

/// The process id in a profile folder name, if it is one of ours.
fn profile_owner(name: &str) -> Option<u32> {
    name.strip_prefix(PROFILE_PREFIX)?.split('-').next()?.parse().ok()
}

/// Deletes temporary profiles left in `root` by SpiderLens processes that
/// are no longer running. Returns how many were deleted.
pub fn sweep_stale_profiles(root: &Path) -> usize {
    let Ok(entries) = std::fs::read_dir(root) else {
        return 0;
    };
    let mut stale: Vec<(u32, PathBuf)> = entries
        .filter_map(Result::ok)
        .filter_map(|e| {
            let owner = profile_owner(&e.file_name().to_string_lossy())?;
            (owner != std::process::id()).then(|| (owner, e.path()))
        })
        .collect();
    if stale.is_empty() {
        return 0;
    }
    let pids: Vec<Pid> = stale.iter().map(|(p, _)| Pid::from_u32(*p)).collect();
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::Some(&pids),
        true,
        ProcessRefreshKind::nothing(),
    );
    stale.retain(|(p, _)| sys.process(Pid::from_u32(*p)).is_none());
    for (_, dir) in &stale {
        remove_profile(dir);
    }
    stale.len()
}

/// A running browser process and the CDP connection to it.
pub(crate) struct Running {
    pub(crate) cdp: chromiumoxide::Browser,
    /// Out-of-memory reports seen on the browser's stderr.
    pub(crate) oom: Arc<AtomicU64>,
    handler: JoinHandle<()>,
    stderr: JoinHandle<()>,
    child: Child,
    /// The guard, or the browser itself without a guard.
    root_pid: u32,
    profile: PathBuf,
    closed: bool,
}

impl Running {
    pub(crate) async fn start(info: &BrowserInfo, opts: &LaunchOptions) -> Result<Self, LaunchError> {
        let tmp = std::env::temp_dir();
        sweep_stale_profiles(&tmp);
        let profile = tmp.join(format!(
            "{PROFILE_PREFIX}{}-{}",
            std::process::id(),
            PROFILES.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&profile)
            .map_err(|e| LaunchError::Start(format!("temporary profile: {e}")))?;
        let args = browser_args(info, &profile, opts.heap_limit_mb);
        let mut cmd = match &opts.guard {
            Some(g) => {
                let mut c = Command::new(&g.exe);
                c.args(&g.args)
                    .arg(GUARD_ARG)
                    .arg(std::process::id().to_string())
                    .arg(&info.path)
                    .args(&args);
                c
            }
            None => {
                let mut c = Command::new(&info.path);
                c.args(&args)
                    .env("TZ", "UTC")
                    .env("LANGUAGE", "en-US")
                    .env("LC_ALL", "en_US.UTF-8");
                c
            }
        };
        cmd.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        // Ctrl-C in a terminal must reach SpiderLens only: the crawl decides
        // whether open renders finish (plan D2 "Pause").
        #[cfg(unix)]
        cmd.process_group(0);
        #[cfg(windows)]
        cmd.creation_flags(0x0800_0000 | 0x0000_0200); // CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                remove_profile(&profile);
                return Err(LaunchError::Start(format!(
                    "could not start {}: {e}",
                    info.path.display()
                )));
            }
        };
        LAUNCHES.fetch_add(1, Ordering::Relaxed);
        let root_pid = child.id().unwrap_or(0);
        let fail = |child_pid: u32, profile: &Path, message: String| {
            kill_tree(child_pid);
            remove_profile(profile);
            LaunchError::Start(message)
        };
        let Some(stderr) = child.stderr.take() else {
            return Err(fail(root_pid, &profile, "no stderr".into()));
        };
        let mut lines = BufReader::new(stderr).lines();
        let mut seen: Vec<String> = Vec::new();
        let ws = tokio::time::timeout(START_TIMEOUT, async {
            while let Ok(Some(line)) = lines.next_line().await {
                if let Some(at) = line.find("ws://") {
                    return Some(line[at..].trim().to_owned());
                }
                if seen.len() == 20 {
                    seen.remove(0);
                }
                seen.push(line);
            }
            None
        })
        .await;
        let ws = match ws {
            Ok(Some(ws)) => ws,
            Ok(None) => {
                let msg = format!("the browser exited at start: {}", seen.join(" | "));
                return Err(fail(root_pid, &profile, msg));
            }
            Err(_) => {
                return Err(fail(
                    root_pid,
                    &profile,
                    "the browser did not start in 30 s".into(),
                ));
            }
        };
        // Keep reading stderr so the browser never blocks on a full pipe. V8
        // reports a page that ran out of heap here, just before its crash.
        let oom = Arc::new(AtomicU64::new(0));
        let stderr = {
            let oom = oom.clone();
            tokio::spawn(async move {
                while let Ok(Some(line)) = lines.next_line().await {
                    if line.contains("V8 javascript OOM") {
                        oom.fetch_add(1, Ordering::Relaxed);
                    }
                }
            })
        };
        let config = HandlerConfig {
            request_intercept: true,
            cache_enabled: false,
            viewport: None,
            request_timeout: Duration::from_secs(30),
            ..HandlerConfig::default()
        };
        let (cdp, mut handler) = match chromiumoxide::Browser::connect_with_config(ws, config).await {
            Ok(b) => b,
            Err(e) => {
                stderr.abort();
                return Err(fail(root_pid, &profile, format!("could not connect: {e}")));
            }
        };
        let handler = tokio::spawn(async move {
            // One unreadable message must not stop the connection.
            while let Some(event) = handler.next().await {
                let _ = event;
            }
        });
        Ok(Self {
            cdp,
            oom,
            handler,
            stderr,
            child,
            root_pid,
            profile,
            closed: false,
        })
    }

    pub(crate) fn memory_bytes(&self) -> u64 {
        tree_memory(self.root_pid)
    }

    /// Asks the browser to close, waits for it, then kills whatever is left
    /// and deletes the profile.
    pub(crate) async fn close(mut self) {
        let _ = tokio::time::timeout(CLOSE_TIMEOUT, self.cdp.close()).await;
        let _ = tokio::time::timeout(CLOSE_TIMEOUT, self.child.wait()).await;
        self.cleanup();
    }

    fn cleanup(&mut self) {
        if self.closed {
            return;
        }
        self.closed = true;
        self.handler.abort();
        self.stderr.abort();
        // Only while the process has not been reaped is its id still ours.
        if matches!(self.child.try_wait(), Ok(None)) {
            kill_tree(self.root_pid);
            let _ = self.child.start_kill();
        }
        remove_profile(&self.profile);
    }

    pub(crate) fn profile(&self) -> &Path {
        &self.profile
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        self.cleanup();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::BrowserSource;

    fn info(name: BrowserName) -> BrowserInfo {
        BrowserInfo {
            name,
            version: "155.0.8059.39".into(),
            major: 155,
            source: BrowserSource::Installed,
            path: PathBuf::from("/x/chrome"),
        }
    }

    fn has(args: &[OsString], a: &str) -> bool {
        args.iter().any(|x| x == a)
    }

    #[test]
    fn launch_flags_follow_plan_d1() {
        let p = Path::new("/tmp/spiderlens-render-1-0");
        let args = browser_args(&info(BrowserName::GoogleChrome), p, 512);
        for a in [
            "--headless=new",
            "--no-first-run",
            "--disable-default-apps",
            "--disable-background-networking",
            "--disable-component-update",
            "--disable-sync",
            "--lang=en-US",
            "--js-flags=--max-old-space-size=512",
            "--host-resolver-rules=MAP * ~NOTFOUND",
            "--user-data-dir=/tmp/spiderlens-render-1-0",
            "--remote-debugging-port=0",
        ] {
            assert!(has(&args, a), "missing {a}");
        }
        assert_eq!(args.last().map(|a| a.as_os_str()), Some("about:blank".as_ref()));
        let shell = browser_args(&info(BrowserName::ChromeHeadlessShell), p, 256);
        assert!(has(&shell, "--headless") && !has(&shell, "--headless=new"));
    }

    #[test]
    fn profile_names_carry_the_owner() {
        assert_eq!(profile_owner("spiderlens-render-123-4"), Some(123));
        assert_eq!(profile_owner("spiderlens-render-x-4"), None);
        assert_eq!(profile_owner("other-123-4"), None);
    }

    #[test]
    fn the_sweep_removes_only_profiles_of_dead_processes() {
        let root = tempfile::tempdir().unwrap();
        // A process id that cannot be running.
        let dead = root.path().join(format!("{PROFILE_PREFIX}{}-0", u32::MAX - 7));
        let mine = root
            .path()
            .join(format!("{PROFILE_PREFIX}{}-0", std::process::id()));
        let other = root.path().join("unrelated-folder");
        for d in [&dead, &mine, &other] {
            std::fs::create_dir_all(d.join("Default")).unwrap();
        }
        assert_eq!(sweep_stale_profiles(root.path()), 1);
        assert!(!dead.exists());
        assert!(mine.exists() && other.exists());
    }
}

/// Total memory of this machine in bytes (0 when unknown).
fn total_memory() -> u64 {
    let mut sys = sysinfo::System::new();
    sys.refresh_memory();
    sys.total_memory()
}

/// Total and available memory of this machine in bytes, for the memory
/// guard (plan D10). Zeros when unknown.
pub fn system_memory() -> (u64, u64) {
    let mut sys = sysinfo::System::new();
    sys.refresh_memory();
    (sys.total_memory(), sys.available_memory())
}

/// Default number of pages rendered at once: total memory in GB divided by
/// 4, between 1 and 16 (spec 014 plan D10).
pub fn default_concurrency() -> u32 {
    concurrency_for(total_memory())
}

fn concurrency_for(total_bytes: u64) -> u32 {
    ((total_bytes / (1 << 30)) / 4).clamp(1, 16) as u32
}

#[cfg(test)]
mod concurrency_tests {
    use super::concurrency_for;

    #[test]
    fn default_concurrency_follows_memory() {
        const GB: u64 = 1 << 30;
        assert_eq!(concurrency_for(0), 1);
        assert_eq!(concurrency_for(16 * GB), 4);
        assert_eq!(concurrency_for(256 * GB), 16);
    }
}
