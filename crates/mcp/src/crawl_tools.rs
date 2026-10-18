//! Crawl tools (spec 016 US6, FR-026, plan D10; contracts/mcp-tools.md
//! "Crawl tools"): an assistant starts, watches, pauses, resumes and stops
//! crawls, only when the user turned on Settings › AI assistants › Allow AI
//! assistants to start crawls.
//!
//! A crawl runs in this server process on its own thread and runtime, the
//! way `spiderlens crawl` and the desktop app run one: the same
//! `Crawler::create` and `Crawler::run`, into a new unsaved crawl file in the
//! app's crawls folder. The file is marked `started_by` =
//! `assistant:<client name>` in its `meta` table and added to the recent
//! list, which is how the app shows "Started by an AI assistant". When the
//! assistant closes the connection, every crawl it started is paused, so the
//! app can resume it.
//!
//! Safety: only profiles the user allowed, and only profiles that respect
//! robots.txt, are used; rendering profiles are refused (no browser is
//! started from here); one assistant crawl runs at a time; pause, resume and
//! stop work only on crawls this process started.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use serde_json::{Map, Value, json};
use spiderlens_config::{
    AppDirs, AssistantSettings, Mode, ModeInput, ProfileStore, RecentEntry, RecentStore, Severity,
    StorageStore, crawl_profile_problem, parse_list_text, validate,
};
use spiderlens_engine::crawl::{
    Control, CrawlError, CrawlEvent, Crawler, Progress, ResumeOptions, default_file_name, file,
};
use tokio::sync::{mpsc, watch};

use crate::ToolError;
use crate::access::{Access, crawl_id};
use crate::tools::{ToolDef, crawl_prop, obj};

pub(crate) const CRAWL_OFF: &str = "Crawling through SpiderLens is off. Turn on Settings › AI assistants › Allow AI assistants to start crawls.";
/// Most URLs `start_crawl` takes in List mode.
pub const MAX_LIST_URLS: usize = 10_000;
/// The `meta` key that marks who started a crawl; the app reads it.
pub const STARTED_BY_KEY: &str = "started_by";
const NOT_OURS: &str = "SpiderLens did not start this crawl in this session, so an assistant cannot control it. Use the crawl id that start_crawl returned.";

pub(crate) const TOOLS: &[ToolDef] = &[
    ToolDef {
        name: "start_crawl",
        description: "Start a crawl with one of the saved profiles the user allowed for assistants. Give `start_url` (Spider mode; default: the profile's start URL) or `urls` (List mode, at most 10,000). One assistant crawl runs at a time; it is saved as an unsaved crawl the user can open in SpiderLens. Returns the crawl id.",
        schema: || {
            obj(
                json!({
                    "profile": {"type": "string", "description": "Name of an allowed saved profile."},
                    "start_url": {"type": "string", "description": "Where to start, for Spider mode."},
                    "urls": {"type": "array", "items": {"type": "string"}, "maxItems": 10_000, "description": "The URLs to crawl, for List mode."}
                }),
                &["profile"],
            )
        },
    },
    ToolDef {
        name: "crawl_progress",
        description: "How a crawl started through this server is doing: its state (running, pausing, paused, stopping, stopped, completed, failed), URLs crawled, queued, errors, speed and time left.",
        schema: || obj(json!({"crawl": crawl_prop()}), &["crawl"]),
    },
    ToolDef {
        name: "pause_crawl",
        description: "Pause a crawl this server started. Requests in flight finish, the queue is kept and the crawl can be resumed.",
        schema: || obj(json!({"crawl": crawl_prop()}), &["crawl"]),
    },
    ToolDef {
        name: "resume_crawl",
        description: "Continue a crawl this server paused, with the settings stored in its file.",
        schema: || obj(json!({"crawl": crawl_prop()}), &["crawl"]),
    },
    ToolDef {
        name: "stop_crawl",
        description: "Stop a crawl this server started. Requests in flight finish; the rest of the queue is recorded as not crawled because the crawl was stopped.",
        schema: || obj(json!({"crawl": crawl_prop()}), &["crawl"]),
    },
];

pub(crate) fn is_crawl_tool(name: &str) -> bool {
    TOOLS.iter().any(|t| t.name == name)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Running,
    Pausing,
    Paused,
    Stopping,
    Stopped,
    Completed,
    Failed,
}

impl State {
    fn as_str(self) -> &'static str {
        match self {
            State::Running => "running",
            State::Pausing => "pausing",
            State::Paused => "paused",
            State::Stopping => "stopping",
            State::Stopped => "stopped",
            State::Completed => "completed",
            State::Failed => "failed",
        }
    }

    fn active(self) -> bool {
        matches!(self, State::Running | State::Pausing | State::Stopping)
    }

    fn from_status(s: &str) -> Self {
        match s {
            "paused" => State::Paused,
            "stopped" => State::Stopped,
            _ => State::Completed,
        }
    }
}

struct Shared {
    state: State,
    progress: Option<Progress>,
    /// What the engine reported when the run ended.
    summary: Option<Value>,
    /// A problem that ended or paused the run.
    problem: Option<String>,
}

struct Run {
    path: PathBuf,
    shared: Arc<Mutex<Shared>>,
    control: Option<watch::Sender<Control>>,
    thread: Option<JoinHandle<()>>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// The crawls this server process started.
#[derive(Default)]
pub(crate) struct CrawlTools {
    runs: Mutex<HashMap<String, Run>>,
}

type Args = Map<String, Value>;

impl CrawlTools {
    /// Runs one crawl tool. `crawl` is set to the crawl id for the activity
    /// log.
    pub fn call(
        &self,
        access: &Access,
        tool: &str,
        args: &Args,
        client: Option<&str>,
        crawl: &mut Option<String>,
    ) -> Result<Value, ToolError> {
        let settings = access.settings();
        if !settings.allow_crawl {
            return Err(ToolError::refused(CRAWL_OFF));
        }
        let dirs = access
            .dirs()
            .ok_or_else(|| ToolError::invalid("SpiderLens could not find its folders on this computer."))?;
        if tool == "start_crawl" {
            let started = self.start(dirs, &settings, args, client)?;
            *crawl = started["crawl"].as_str().map(str::to_string);
            return Ok(started);
        }
        let id = str_arg(args, "crawl")?.ok_or_else(|| ToolError::invalid("`crawl` is required."))?;
        *crawl = Some(id.to_string());
        match tool {
            "crawl_progress" => self.progress(id),
            "pause_crawl" => self.control(id, Control::Pause),
            "stop_crawl" => self.control(id, Control::Stop),
            "resume_crawl" => self.resume(dirs, id),
            other => Err(ToolError::invalid(format!("Unknown tool {other}."))),
        }
    }

    fn start(
        &self,
        dirs: &AppDirs,
        settings: &AssistantSettings,
        args: &Args,
        client: Option<&str>,
    ) -> Result<Value, ToolError> {
        let name = str_arg(args, "profile")?.ok_or_else(|| ToolError::invalid("`profile` is required."))?;
        if !settings.allowed_profiles.iter().any(|p| p == name) {
            return Err(ToolError::refused(format!(
                "The profile \"{name}\" is not allowed for AI assistants. The user can allow it in Settings › AI assistants."
            )));
        }
        let store = ProfileStore::open(dirs.config.join("profiles"))
            .map_err(|e| ToolError::invalid(e.to_string()))?;
        if let Some(problem) = crawl_profile_problem(&store, name) {
            return Err(ToolError::refused(problem));
        }
        let profile = store.load(name).map_err(|e| ToolError::invalid(e.to_string()))?;
        let mut config = profile.config;
        if config.rendering.is_on() {
            return Err(ToolError::refused(format!(
                "The profile \"{name}\" renders JavaScript, which an assistant cannot start. Allow a profile without rendering."
            )));
        }
        let urls = args.get("urls").filter(|v| !v.is_null());
        let start_url = str_arg(args, "start_url")?;
        let input = match (urls, start_url) {
            (Some(_), Some(_)) => {
                return Err(ToolError::invalid("Give `start_url` or `urls`, not both."));
            }
            (Some(Value::Array(list)), None) => {
                if list.is_empty() {
                    return Err(ToolError::invalid("`urls` is empty."));
                }
                if list.len() > MAX_LIST_URLS {
                    return Err(ToolError::invalid(format!(
                        "`urls` has {} entries; List mode takes at most {MAX_LIST_URLS}.",
                        list.len()
                    )));
                }
                let mut text = String::new();
                for u in list {
                    let u = u
                        .as_str()
                        .ok_or_else(|| ToolError::invalid("`urls` must be a list of strings."))?;
                    text.push_str(&u.replace(['\n', '\r'], " "));
                    text.push('\n');
                }
                let parsed = parse_list_text(&text, &mut |_| {});
                if parsed.urls.is_empty() {
                    return Err(ToolError::invalid(
                        "None of the `urls` is a web address SpiderLens can crawl.",
                    ));
                }
                config.mode = Mode::List;
                ModeInput::List(parsed)
            }
            (Some(_), None) => return Err(ToolError::invalid("`urls` must be a list of strings.")),
            (None, start) => {
                let start = start.map(str::to_string).or(profile.start_url).ok_or_else(|| {
                    ToolError::invalid("Give `start_url` or `urls`; the profile has no start URL.")
                })?;
                config.mode = Mode::Spider;
                ModeInput::Spider { start_url: start }
            }
        };
        if let Some(e) = validate(&config, &input)
            .into_iter()
            .find(|i| i.severity == Severity::Error)
        {
            return Err(ToolError::invalid(e.message));
        }
        let mut runs = lock(&self.runs);
        if let Some((id, _)) = runs.iter().find(|(_, r)| lock(&r.shared).state.active()) {
            return Err(ToolError::invalid(format!(
                "The crawl {id} is still running. Pause or stop it before starting another."
            )));
        }
        let crawls = dirs.unsaved_crawls();
        std::fs::create_dir_all(&crawls)
            .map_err(|e| ToolError::invalid(format!("Could not create {}: {e}", crawls.display())))?;
        let path = free_path(&crawls, &default_file_name(&input));
        let who = client_name(client);
        let crawler = Crawler::create(&path, config, &input)
            .map_err(|e| ToolError::invalid(e.to_string()))?
            .with_disk_reserve(Some(reserve(dirs)));
        crawler
            .set_meta(STARTED_BY_KEY, &format!("assistant:{who}"))
            .map_err(|e| ToolError::invalid(e.to_string()))?;
        let path = path.canonicalize().unwrap_or(path);
        let id = crawl_id(&path);
        note_recent(dirs, &path, "running", 0);
        let shared = Arc::new(Mutex::new(Shared {
            state: State::Running,
            progress: None,
            summary: None,
            problem: None,
        }));
        let (control, rx) = watch::channel(Control::Run);
        let thread = spawn(crawler, path.clone(), shared.clone(), rx, dirs.config.clone())?;
        runs.insert(
            id.clone(),
            Run {
                path: path.clone(),
                shared,
                control: Some(control),
                thread: Some(thread),
            },
        );
        Ok(json!({
            "crawl": id,
            "file": path.file_name().map(|n| n.to_string_lossy().to_string()),
            "state": State::Running.as_str(),
            "started_by": format!("assistant:{who}"),
            "note": "The crawl runs in the background. Call crawl_progress to watch it. It is saved as an unsaved crawl the user can open in SpiderLens."
        }))
    }

    fn progress(&self, id: &str) -> Result<Value, ToolError> {
        let runs = lock(&self.runs);
        let run = runs.get(id).ok_or_else(|| ToolError::invalid(NOT_OURS))?;
        let s = lock(&run.shared);
        let mut v = json!({"crawl": id, "state": s.state.as_str()});
        if let Some(p) = &s.progress {
            v["progress"] = serde_json::to_value(p).unwrap_or(Value::Null);
        }
        if let Some(sum) = &s.summary {
            v["summary"] = sum.clone();
        }
        if let Some(p) = &s.problem {
            v["problem"] = Value::String(p.clone());
        }
        Ok(v)
    }

    fn control(&self, id: &str, what: Control) -> Result<Value, ToolError> {
        let mut runs = lock(&self.runs);
        let run = runs.get_mut(id).ok_or_else(|| ToolError::invalid(NOT_OURS))?;
        let mut s = lock(&run.shared);
        let (verb, next) = match what {
            Control::Pause => ("paused", State::Pausing),
            _ => ("stopped", State::Stopping),
        };
        match (s.state, what) {
            (State::Running, _) => {}
            (State::Pausing, Control::Pause) | (State::Stopping, Control::Stop) => {
                return Ok(json!({"crawl": id, "state": s.state.as_str()}));
            }
            (State::Paused, Control::Stop) => {
                return Err(ToolError::invalid(
                    "The crawl is paused. Resume it first if it should be stopped, or leave it paused.",
                ));
            }
            (st, _) => {
                return Err(ToolError::invalid(format!(
                    "The crawl is {}; it cannot be {verb}.",
                    st.as_str()
                )));
            }
        }
        s.state = next;
        drop(s);
        if let Some(tx) = &run.control {
            let _ = tx.send(what);
        }
        Ok(json!({"crawl": id, "state": next.as_str()}))
    }

    fn resume(&self, dirs: &AppDirs, id: &str) -> Result<Value, ToolError> {
        let mut runs = lock(&self.runs);
        let run = runs.get_mut(id).ok_or_else(|| ToolError::invalid(NOT_OURS))?;
        {
            let s = lock(&run.shared);
            if s.state != State::Paused {
                return Err(ToolError::invalid(format!(
                    "The crawl is {}; only a paused crawl can be resumed.",
                    s.state.as_str()
                )));
            }
        }
        if let Some(t) = run.thread.take() {
            let _ = t.join();
        }
        let crawler = Crawler::open_with(&run.path, &ResumeOptions::default())
            .map_err(|e| match e {
                CrawlError::NeedsSecrets(names) => ToolError::invalid(format!(
                    "The crawl needs secret values only the user can give: {}. Resume it in SpiderLens.",
                    names.join(", ")
                )),
                other => ToolError::invalid(other.to_string()),
            })?
            .with_disk_reserve(Some(reserve(dirs)));
        {
            let mut s = lock(&run.shared);
            s.state = State::Running;
            s.progress = None;
            s.summary = None;
            s.problem = None;
        }
        note_recent(dirs, &run.path, "running", 0);
        let (control, rx) = watch::channel(Control::Run);
        run.thread = Some(spawn(
            crawler,
            run.path.clone(),
            run.shared.clone(),
            rx,
            dirs.config.clone(),
        )?);
        run.control = Some(control);
        Ok(json!({"crawl": id, "state": State::Running.as_str()}))
    }

    /// The assistant closed the connection: pause every running crawl and
    /// wait until each has saved its state, so the app can resume them.
    pub fn pause_all(&self) {
        let mut runs = lock(&self.runs);
        for run in runs.values_mut() {
            {
                let mut s = lock(&run.shared);
                if s.state == State::Running {
                    s.state = State::Pausing;
                }
            }
            if let Some(tx) = &run.control {
                let _ = tx.send(Control::Pause);
            }
        }
        for run in runs.values_mut() {
            if let Some(t) = run.thread.take() {
                let _ = t.join();
            }
        }
    }
}

/// The crawl's own thread and runtime, as in the CLI and the app, so a busy
/// crawl never competes with the connection that serves the assistant.
fn spawn(
    crawler: Crawler,
    path: PathBuf,
    shared: Arc<Mutex<Shared>>,
    control: watch::Receiver<Control>,
    config_dir: PathBuf,
) -> Result<JoinHandle<()>, ToolError> {
    std::thread::Builder::new()
        .name("assistant-crawl".into())
        .spawn(move || {
            let result = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
                Ok(rt) => rt.block_on(async {
                    let (tx, mut rx) = mpsc::unbounded_channel();
                    let sh = shared.clone();
                    let watcher = tokio::spawn(async move {
                        while let Some(e) = rx.recv().await {
                            if let CrawlEvent::Progress(p) = e {
                                lock(&sh).progress = Some(p);
                            }
                        }
                    });
                    let r = crawler.run(control, Some(tx)).await;
                    let _ = watcher.await;
                    r
                }),
                Err(e) => Err(CrawlError::Config(e.to_string())),
            };
            let mut s = lock(&shared);
            let discovered = match result {
                Ok(sum) => {
                    s.state = State::from_status(&sum.status);
                    s.summary = Some(json!({
                        "status": sum.status,
                        "discovered": sum.discovered,
                        "crawled": sum.crawled,
                        "queued": sum.queued,
                        "not_crawled": sum.not_crawled,
                        "url_limit_reached": sum.url_limit_reached,
                        "active_ms": sum.active_ms,
                    }));
                    Some(sum.discovered)
                }
                // A disk or write problem: the file is consistent and can
                // be resumed (spec 006 FR-040).
                Err(CrawlError::Local(e)) => {
                    s.state = State::Paused;
                    s.problem = Some(e.to_string());
                    None
                }
                Err(e) => {
                    s.state = State::Failed;
                    s.problem = Some(e.to_string());
                    None
                }
            };
            let status = match s.state {
                State::Stopped => "stopped",
                State::Completed => "completed",
                _ => "paused",
            };
            drop(s);
            if let Ok(recent) = RecentStore::open(&config_dir) {
                let urls = discovered
                    .or_else(|| file::info(&path).ok().map(|i| i.counts.discovered))
                    .unwrap_or(0);
                let _ = recent.touch(recent_entry(&path, status, urls));
            }
        })
        .map_err(|e| ToolError::invalid(format!("Could not start the crawl: {e}")))
}

fn recent_entry(path: &Path, status: &str, urls: u64) -> RecentEntry {
    RecentEntry {
        path: path.to_path_buf(),
        name: file::read_meta(path, "name")
            .unwrap_or_else(|| path.file_stem().unwrap_or_default().to_string_lossy().to_string()),
        start_url: file::read_meta(path, "start_url"),
        urls,
        status: status.to_string(),
        recovered: false,
        last_used: file::now_string(),
    }
}

fn note_recent(dirs: &AppDirs, path: &Path, status: &str, urls: u64) {
    if let Ok(recent) = RecentStore::open(&dirs.config) {
        let _ = recent.touch(recent_entry(path, status, urls));
    }
}

/// Free space kept on the drive, from Settings › Storage.
fn reserve(dirs: &AppDirs) -> u64 {
    StorageStore::open(&dirs.config)
        .map(|s| s.prefs().reserve_bytes)
        .unwrap_or(0)
}

/// A name that does not exist yet in `dir`: files are never overwritten.
fn free_path(dir: &Path, name: &str) -> PathBuf {
    let first = dir.join(name);
    if !first.exists() {
        return first;
    }
    let stem = name.trim_end_matches(".spiderlens");
    (2..)
        .map(|n| dir.join(format!("{stem}-{n}.spiderlens")))
        .find(|p| !p.exists())
        .expect("an unused name exists")
}

/// The assistant's name without its version ("claude-code 2.4" becomes
/// "claude-code").
pub(crate) fn client_name(client: Option<&str>) -> String {
    let c = client
        .map(str::trim)
        .filter(|c| !c.is_empty())
        .unwrap_or("unknown");
    let name = match c.rsplit_once(' ') {
        Some((n, v)) if v.chars().next().is_some_and(|ch| ch.is_ascii_digit()) => n,
        _ => c,
    };
    name.chars().filter(|ch| !ch.is_control()).take(64).collect()
}

fn str_arg<'a>(args: &'a Args, key: &str) -> Result<Option<&'a str>, ToolError> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s)),
        Some(_) => Err(ToolError::invalid(format!("`{key}` must be a string."))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_client_name_drops_its_version() {
        assert_eq!(client_name(Some("claude-code 2.4")), "claude-code");
        assert_eq!(client_name(Some("Claude Desktop 1.0.3")), "Claude Desktop");
        assert_eq!(client_name(Some("cursor")), "cursor");
        assert_eq!(client_name(None), "unknown");
    }

    #[test]
    fn free_path_never_overwrites() {
        let dir = tempfile::tempdir().unwrap();
        let a = free_path(dir.path(), "a.spiderlens");
        std::fs::write(&a, b"x").unwrap();
        assert_eq!(
            free_path(dir.path(), "a.spiderlens"),
            dir.path().join("a-2.spiderlens")
        );
    }

    #[test]
    fn five_crawl_tools_are_defined() {
        let names: Vec<&str> = TOOLS.iter().map(|t| t.name).collect();
        assert_eq!(
            names,
            [
                "start_crawl",
                "crawl_progress",
                "pause_crawl",
                "resume_crawl",
                "stop_crawl"
            ]
        );
        assert!(is_crawl_tool("stop_crawl") && !is_crawl_tool("list_crawls"));
    }
}
