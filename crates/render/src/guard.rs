//! The guard process (research R12 option 2, plan D12).
//!
//! Neither CDP client supports a pipe transport (research R16), so a browser
//! would outlive a SpiderLens that crashed or was killed. The `spiderlens`
//! and desktop binaries therefore accept a hidden `--render-guard <pid>`
//! argument: in that mode the process starts the browser, checks every
//! 500 ms whether SpiderLens (`<pid>`) is still alive, and when it is not,
//! kills the browser's process tree and deletes its temporary profile.
//! When the browser exits by itself, the guard cleans up and exits too.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

/// The hidden argument that starts guard mode.
pub const GUARD_ARG: &str = "--render-guard";

const POLL: Duration = Duration::from_millis(500);

/// Parses `--render-guard <pid> <browser> <browser args...>` from a
/// command line (without the program name). `None` when the arguments are
/// not a guard command line.
pub fn parse_guard_args(args: &[OsString]) -> Option<(u32, Vec<OsString>)> {
    let (first, rest) = args.split_first()?;
    if first != GUARD_ARG {
        return None;
    }
    let (pid, browser) = rest.split_first()?;
    let pid = pid.to_str()?.parse().ok()?;
    (!browser.is_empty()).then(|| (pid, browser.to_vec()))
}

/// The guard process entry point. `browser_args` is the browser executable
/// followed by its arguments. Never returns.
pub fn run_guard(parent_pid: u32, browser_args: Vec<OsString>) -> ! {
    let mut it = browser_args.into_iter();
    let Some(exe) = it.next() else {
        std::process::exit(2)
    };
    let args: Vec<OsString> = it.collect();
    let profile = profile_of(&args);
    let parent = Parent::new(parent_pid);
    let mut cmd = Command::new(exe);
    // The browser's stderr is the guard's, which SpiderLens reads for the
    // DevTools address.
    cmd.args(&args)
        .env("TZ", "UTC")
        .env("LANGUAGE", "en-US")
        .env("LC_ALL", "en_US.UTF-8")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit());
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("render guard: could not start the browser: {e}");
            std::process::exit(3)
        }
    };
    loop {
        if let Ok(Some(status)) = child.try_wait() {
            if let Some(p) = &profile {
                remove_profile(p);
            }
            std::process::exit(status.code().unwrap_or(1));
        }
        if !parent.alive() {
            kill_tree(child.id());
            let _ = child.kill();
            let _ = child.wait();
            if let Some(p) = &profile {
                remove_profile(p);
            }
            std::process::exit(0);
        }
        std::thread::sleep(POLL);
    }
}

/// The `--user-data-dir=` of a browser command line.
pub(crate) fn profile_of(args: &[OsString]) -> Option<PathBuf> {
    args.iter().find_map(|a| {
        a.to_str()
            .and_then(|s| s.strip_prefix("--user-data-dir="))
            .map(PathBuf::from)
    })
}

/// Deletes a temporary profile, retrying while the browser's last helper
/// processes let go of their files (needed on Windows).
pub(crate) fn remove_profile(dir: &Path) {
    for _ in 0..20 {
        match std::fs::remove_dir_all(dir) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                std::thread::sleep(Duration::from_millis(50))
            }
            _ => return,
        }
    }
}

/// SpiderLens, as seen from the guard.
struct Parent {
    pid: u32,
    /// Start time, so a reused process id is not taken for SpiderLens.
    #[cfg(not(unix))]
    started: Option<u64>,
}

impl Parent {
    fn new(pid: u32) -> Self {
        Self {
            pid,
            #[cfg(not(unix))]
            started: process_start(pid),
        }
    }

    #[cfg(unix)]
    fn alive(&self) -> bool {
        // When the parent dies, the guard is handed to another process.
        std::os::unix::process::parent_id() == self.pid
    }

    #[cfg(not(unix))]
    fn alive(&self) -> bool {
        self.started.is_some() && process_start(self.pid) == self.started
    }
}

#[cfg(not(unix))]
fn process_start(pid: u32) -> Option<u64> {
    let pid = Pid::from_u32(pid);
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[pid]),
        true,
        ProcessRefreshKind::nothing(),
    );
    sys.process(pid).map(|p| p.start_time())
}

/// Every process below `root` (not `root` itself), deepest first.
pub(crate) fn descendants(sys: &System, root: u32) -> Vec<Pid> {
    let root = Pid::from_u32(root);
    let procs = sys.processes();
    let mut out: Vec<(usize, Pid)> = Vec::new();
    for (pid, p) in procs {
        // On Linux sysinfo also lists threads.
        if p.thread_kind().is_some() || *pid == root {
            continue;
        }
        let mut cur = p.parent();
        let mut depth = 1;
        while let Some(c) = cur {
            if c == root {
                out.push((depth, *pid));
                break;
            }
            depth += 1;
            if depth > 32 {
                break;
            }
            cur = procs.get(&c).and_then(|p| p.parent());
        }
    }
    out.sort_by_key(|(depth, _)| std::cmp::Reverse(*depth));
    out.into_iter().map(|(_, p)| p).collect()
}

pub(crate) fn all_processes() -> System {
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_memory(),
    );
    sys
}

/// Kills `root` and every process below it.
pub(crate) fn kill_tree(root: u32) {
    let sys = all_processes();
    let mut pids = descendants(&sys, root);
    pids.push(Pid::from_u32(root));
    for pid in pids {
        if let Some(p) = sys.process(pid) {
            p.kill();
        }
    }
}

/// Resident memory of `root` and every process below it, in bytes.
pub(crate) fn tree_memory(root: u32) -> u64 {
    let sys = all_processes();
    let mut pids = descendants(&sys, root);
    pids.push(Pid::from_u32(root));
    pids.iter()
        .filter_map(|p| sys.process(*p))
        .map(|p| p.memory())
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn os(v: &[&str]) -> Vec<OsString> {
        v.iter().map(OsString::from).collect()
    }

    #[test]
    fn guard_arguments_are_parsed() {
        let got = parse_guard_args(&os(&["--render-guard", "42", "/b/chrome", "--headless"]));
        assert_eq!(got, Some((42, os(&["/b/chrome", "--headless"]))));
        assert_eq!(parse_guard_args(&os(&["--render-guard", "42"])), None);
        assert_eq!(parse_guard_args(&os(&["--render-guard", "x", "/b"])), None);
        assert_eq!(parse_guard_args(&os(&["crawl", "a.test"])), None);
        assert_eq!(parse_guard_args(&[]), None);
    }

    #[test]
    fn profile_is_read_from_the_browser_arguments() {
        let args = os(&["--headless", "--user-data-dir=/tmp/p-1", "--no-sandbox"]);
        assert_eq!(profile_of(&args), Some(PathBuf::from("/tmp/p-1")));
        assert_eq!(profile_of(&os(&["--headless"])), None);
    }

    #[test]
    fn this_process_has_memory_and_no_children_by_default() {
        assert!(tree_memory(std::process::id()) > 0);
    }
}
