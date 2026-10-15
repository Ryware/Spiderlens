//! Commands for the settings screen (spec 002, contracts/desktop-commands.md).
//!
//! Each command is a thin call into `spiderlens-config`; no setting is
//! defined or validated here. A parsed URL list stays in Rust and only its
//! summary crosses to the UI, so a million-URL list never travels over IPC.

use std::path::PathBuf;
use std::sync::Mutex;

use serde::Serialize;
use spiderlens_config::{
    CrawlConfig, Issue, ListFormat, ListInput, Mode, ModeInput, PatternTest, ProfileEntry, ProfileStore,
    SpeedPreset, SpeedSettings, StoreError, UaPreset, export_profile, import_profile, parse_list_bytes,
    parse_list_text, test_patterns, validate, validate_config,
};
use tauri::{AppHandle, Emitter};

pub struct State {
    store: ProfileStore,
    list: Mutex<Option<ListInput>>,
}

impl State {
    pub fn open(dir: PathBuf) -> Result<Self, StoreError> {
        Ok(Self {
            store: ProfileStore::open(dir)?,
            list: Mutex::new(None),
        })
    }

    /// A stored profile by name, for schedules (spec 021).
    pub fn load_profile(&self, name: &str) -> Option<spiderlens_config::ImportedProfile> {
        self.store.load(name).ok()
    }

    /// The list most recently parsed in this window, for List mode crawls.
    pub fn list(&self) -> Option<ListInput> {
        self.list.lock().expect("list lock").clone()
    }
}

type Cmd<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// The configuration new crawls start with: the default profile, else the
/// built-in defaults (FR-032).
#[tauri::command]
pub fn config_defaults(state: tauri::State<State>, builtin: bool) -> CrawlConfig {
    if builtin {
        CrawlConfig::default()
    } else {
        state.store.default_config()
    }
}

#[derive(Serialize)]
pub struct UaPresetInfo {
    preset: UaPreset,
    label: &'static str,
    user_agent: Option<String>,
    robots_token: Option<&'static str>,
}

#[derive(Serialize)]
pub struct Presets {
    user_agents: Vec<UaPresetInfo>,
    polite: SpeedSettings,
    fast: SpeedSettings,
}

#[tauri::command]
pub fn config_presets() -> Presets {
    Presets {
        user_agents: UaPreset::ALL
            .into_iter()
            .map(|p| UaPresetInfo {
                preset: p,
                label: p.label(),
                user_agent: p.user_agent(),
                robots_token: p.robots_token(),
            })
            .collect(),
        polite: SpeedSettings::POLITE,
        fast: SpeedSettings::FAST,
    }
}

#[derive(Serialize)]
pub struct Validation {
    issues: Vec<Issue>,
    /// Derived from the values, so the UI never decides what "Custom" means.
    speed_preset: SpeedPreset,
    /// The start URL as it will be crawled, e.g. `https://example.com/` for
    /// `example.com` (US1-2).
    normalized_start_url: Option<String>,
}

/// Validates the configuration with its input. In List mode the input is the
/// list most recently parsed in this window.
#[tauri::command]
pub fn config_validate(
    state: tauri::State<State>,
    config: CrawlConfig,
    start_url: Option<String>,
) -> Validation {
    let normalized_start_url = start_url
        .as_deref()
        .and_then(|u| spiderlens_config::normalize_url(u).ok())
        .map(String::from);
    let issues = match (config.mode, start_url) {
        (Mode::Spider, Some(url)) => validate(&config, &ModeInput::Spider { start_url: url }),
        (Mode::List, _) => {
            let list = state.list.lock().expect("list lock").clone().unwrap_or_default();
            validate(&config, &ModeInput::List(list))
        }
        (Mode::Spider, None) => validate_config(&config),
    };
    Validation {
        issues,
        speed_preset: config.speed.preset(),
        normalized_start_url,
    }
}

#[tauri::command]
pub fn patterns_test(include: Vec<String>, exclude: Vec<String>, samples: Vec<String>) -> Vec<PatternTest> {
    test_patterns(&include, &exclude, &samples)
}

/// One URL's answer from the scope tester, with the reason's label so the UI
/// keeps no copy of the wording.
#[derive(Serialize, Clone)]
pub struct ScopeVerdict {
    input: String,
    url: Option<String>,
    would_crawl: bool,
    reason: Option<spiderlens_engine::crawl::NotCrawledReason>,
    label: Option<&'static str>,
    detail: Option<String>,
    robots_checked: bool,
    /// The segment the URL would belong to by its address alone (spec 027
    /// FR-B08); page facts are unknown before the crawl.
    segment: Option<String>,
}

/// What the crawler would do with each URL under these settings (spec 003
/// FR-016, spec 007 T041). Requests nothing unless `fetch_robots`, and then
/// only each host's robots.txt.
#[tauri::command]
pub async fn scope_test(
    config: CrawlConfig,
    start_url: String,
    urls: Vec<String>,
    fetch_robots: bool,
) -> Result<Vec<ScopeVerdict>, String> {
    let start = spiderlens_config::normalize_url(&start_url).map_err(|e| format!("Start URL: {e}"))?;
    let urls: Vec<String> = urls.into_iter().filter(|u| !u.trim().is_empty()).collect();
    let tests = spiderlens_engine::crawl::scope_test(&config, &start, &urls, fetch_robots).await?;
    let matcher = spiderlens_config::SegmentMatcher::new(&config.segments);
    let segment_of = |url: &Option<String>| -> Option<String> {
        let url = url.as_deref()?;
        let id = matcher.classify(&spiderlens_config::UrlFacts {
            url,
            ..Default::default()
        })?;
        config
            .segments
            .iter()
            .find(|s| s.id == id)
            .map(|s| s.name.trim().to_string())
    };
    Ok(tests
        .into_iter()
        .map(|t| ScopeVerdict {
            segment: segment_of(&t.url),
            label: t.reason.map(|r| r.label()),
            input: t.input,
            url: t.url,
            would_crawl: t.would_crawl,
            reason: t.reason,
            detail: t.detail,
            robots_checked: t.robots_checked,
        })
        .collect())
}

/// What the UI sees of a parsed list: counts and the invalid lines.
#[derive(Serialize, Clone)]
pub struct ListSummary {
    urls: usize,
    duplicates: u64,
    invalid_count: u64,
    invalid_sample: Vec<spiderlens_config::InvalidLine>,
    notices: Vec<String>,
    first_urls: Vec<String>,
}

fn keep(state: &State, list: ListInput) -> ListSummary {
    let summary = ListSummary {
        urls: list.urls.len(),
        duplicates: list.duplicates,
        invalid_count: list.invalid_count,
        invalid_sample: list.invalid_sample.iter().take(1000).cloned().collect(),
        notices: list.notices.clone(),
        first_urls: list.urls.iter().take(20).cloned().collect(),
    };
    *state.list.lock().expect("list lock") = Some(list);
    summary
}

#[tauri::command]
pub async fn list_parse_text(
    app: AppHandle,
    state: tauri::State<'_, State>,
    text: String,
) -> Cmd<ListSummary> {
    let list = tauri::async_runtime::spawn_blocking(move || {
        parse_list_text(&text, &mut |n| {
            let _ = app.emit("list-progress", n);
        })
    })
    .await
    .map_err(err)?;
    Ok(keep(&state, list))
}

/// Parses a file chosen in a dialog, off the UI thread, with progress events.
#[tauri::command]
pub async fn list_parse_file(
    app: AppHandle,
    state: tauri::State<'_, State>,
    path: PathBuf,
) -> Cmd<ListSummary> {
    let list = tauri::async_runtime::spawn_blocking(move || -> Cmd<ListInput> {
        let bytes = std::fs::read(&path).map_err(|e| format!("Could not read {}: {e}", path.display()))?;
        let format = ListFormat::from_file_name(&path.to_string_lossy());
        Ok(parse_list_bytes(&bytes, format, &mut |n| {
            let _ = app.emit("list-progress", n);
        }))
    })
    .await
    .map_err(err)??;
    Ok(keep(&state, list))
}

#[tauri::command]
pub fn profiles_list(state: tauri::State<State>) -> Cmd<Vec<ProfileEntry>> {
    state.store.list().map_err(err)
}

#[derive(Serialize)]
pub struct LoadedProfile {
    name: String,
    config: CrawlConfig,
    start_url: Option<String>,
    ignored: Vec<String>,
}

#[tauri::command]
pub fn profile_load(state: tauri::State<State>, name: String) -> Cmd<LoadedProfile> {
    let p = state.store.load(&name).map_err(err)?;
    Ok(LoadedProfile {
        name: p.name,
        config: p.config,
        start_url: p.start_url,
        ignored: p.ignored,
    })
}

#[derive(Serialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum SaveResult {
    Saved,
    NameTaken,
}

#[tauri::command]
pub fn profile_save(
    state: tauri::State<State>,
    name: String,
    config: CrawlConfig,
    start_url: Option<String>,
    overwrite: bool,
) -> Cmd<SaveResult> {
    match state.store.save(&name, &config, start_url.as_deref(), overwrite) {
        Ok(()) => Ok(SaveResult::Saved),
        Err(StoreError::NameTaken(_)) => Ok(SaveResult::NameTaken),
        Err(e) => Err(err(e)),
    }
}

#[tauri::command]
pub fn profile_rename(state: tauri::State<State>, from: String, to: String) -> Cmd<()> {
    state.store.rename(&from, &to).map_err(err)
}

#[tauri::command]
pub fn profile_duplicate(state: tauri::State<State>, from: String, to: String) -> Cmd<()> {
    state.store.duplicate(&from, &to).map_err(err)
}

#[tauri::command]
pub fn profile_delete(state: tauri::State<State>, name: String) -> Cmd<()> {
    state.store.delete(&name).map_err(err)
}

#[tauri::command]
pub fn profile_set_default(state: tauri::State<State>, name: Option<String>) -> Cmd<()> {
    state.store.set_default(name.as_deref()).map_err(err)
}

#[tauri::command]
pub fn profile_export(state: tauri::State<State>, name: String, path: PathBuf) -> Cmd<()> {
    let p = state.store.load(&name).map_err(err)?;
    std::fs::write(&path, export_profile(&p.name, &p.config, p.start_url.as_deref())).map_err(err)
}

/// Imports into the store. Returns the profile, including any settings this
/// version ignored, so the UI can list them (FR-035).
#[tauri::command]
pub fn profile_import(state: tauri::State<State>, path: PathBuf) -> Cmd<LoadedProfile> {
    let text = std::fs::read_to_string(&path).map_err(err)?;
    let p = import_profile(&text).map_err(err)?;
    let mut name = p.name.clone();
    let mut n = 2;
    while state.store.load(&name).is_ok() {
        name = format!("{} ({n})", p.name);
        n += 1;
    }
    state
        .store
        .save(&name, &p.config, p.start_url.as_deref(), false)
        .map_err(err)?;
    Ok(LoadedProfile {
        name,
        config: p.config,
        start_url: p.start_url,
        ignored: p.ignored,
    })
}
