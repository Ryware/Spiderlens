//! Commands for custom extraction and custom search (spec 013,
//! specs/013-custom-extraction-search/contracts/desktop-commands.md).
//!
//! Testing rules runs on a stored page (no request) or one typed URL;
//! running rules on a finished crawl reads its stored HTML on a thread of
//! its own and reports progress as events.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use serde::Serialize;
use spiderlens_config::custom::check_rules;
use spiderlens_config::{CrawlConfig, CustomRules, RobotsMode, export_rules, import_rules};
use spiderlens_engine::CheckOptions;
use spiderlens_engine::crawl::CrawlStore;
use spiderlens_engine::crawl::store::custom::read_rules;
use spiderlens_engine::custom::rerun::{RerunError, RerunState, Which, rerun, test_stored_page};
use spiderlens_engine::custom::{CompiledRules, PageRules, compile, flags, tester};
use tauri::{AppHandle, Emitter};

type Cmd<T> = Result<T, String>;

#[derive(Default)]
pub struct State {
    runs: Mutex<HashMap<u32, Arc<AtomicBool>>>,
    next_run: AtomicU32,
}

/// A problem with one rule (FR-008).
#[derive(Serialize)]
pub struct RuleIssue {
    /// For example `custom.extractors[2].expr`.
    path: String,
    message: String,
    warning: bool,
}

/// What one rule found on the tested page (FR-018).
#[derive(Serialize)]
pub struct RuleTestResult {
    name: String,
    kind: &'static str,
    count: u32,
    flags: Vec<&'static str>,
    values: Vec<String>,
    error: Option<String>,
}

#[derive(Serialize)]
pub struct TestOutcome {
    /// The page the rules ran on, after redirects.
    url: String,
    status: Option<u16>,
    results: Vec<RuleTestResult>,
    ms: u32,
}

/// A rule as stored with a crawl, with when and how it last ran.
#[derive(Serialize)]
pub struct CrawlRule {
    name: String,
    kind: &'static str,
    last_run_at: Option<String>,
    run_pages: Option<i64>,
    /// `crawl`, `complete`, `partial` or `stale`.
    run_state: Option<String>,
}

#[derive(Serialize, Clone)]
struct RunProgress {
    run: u32,
    done: u64,
    total: u64,
    per_sec: u32,
}

#[derive(Serialize, Clone)]
struct RunDone {
    run: u32,
    path: PathBuf,
    /// `complete`, `partial` or `failed`.
    state: &'static str,
    pages: u64,
    error: Option<String>,
}

fn compiled(rules: &CustomRules) -> Cmd<CompiledRules> {
    match compile(rules) {
        Ok(Some(c)) => Ok(c),
        Ok(None) => Err("Add a rule to test.".into()),
        Err(errors) => Err(errors
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("; ")),
    }
}

fn results(rules: &CustomRules, out: PageRules) -> Vec<RuleTestResult> {
    let named = rules
        .extractors
        .iter()
        .map(|e| (e.name.clone(), "extractor"))
        .chain(rules.searches.iter().map(|s| (s.name.clone(), "search")));
    named
        .zip(out.extractors.into_iter().chain(out.searches))
        .map(|((name, kind), o)| RuleTestResult {
            name,
            kind,
            count: o.count,
            flags: flags::names(o.flags),
            values: o.values,
            error: o.error,
        })
        .collect()
}

/// Problems with a rule set: names, limits and expression syntax (FR-008).
#[tauri::command]
pub fn custom_validate(rules: CustomRules) -> Vec<RuleIssue> {
    check_rules(&rules)
        .into_iter()
        .map(|p| RuleIssue {
            path: p.path,
            message: p.message,
            warning: p.warning,
        })
        .collect()
}

/// Runs rules on one crawled page from its stored HTML; no request.
#[tauri::command]
pub async fn custom_test_page(rules: CustomRules, crawl_path: PathBuf, url_id: i64) -> Cmd<TestOutcome> {
    tauri::async_runtime::spawn_blocking(move || {
        let started = Instant::now();
        let c = compiled(&rules)?;
        let store = CrawlStore::open(&crawl_path).map_err(|e| e.to_string())?;
        let url = store.url_of(url_id).map_err(|e| e.to_string())?;
        let out = test_stored_page(&store, url_id, &c)
            .map_err(|e| e.to_string())?
            .ok_or("This crawl did not store this page's HTML, so it can only be tested by URL.")?;
        Ok(TestOutcome {
            url,
            status: None,
            results: results(&rules, out),
            ms: started.elapsed().as_millis() as u32,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Fetches one URL with the profile's identity and robots setting and runs
/// the rules on it (FR-018).
#[tauri::command]
pub async fn custom_test_url(rules: CustomRules, url: String, config: CrawlConfig) -> Cmd<TestOutcome> {
    let started = Instant::now();
    let c = compiled(&rules)?;
    let opts = CheckOptions::from(&config);
    let respect = config.robots.mode == RobotsMode::Respect;
    let t = tester::test_url(&url, &opts, respect, &c)
        .await
        .map_err(|e| e.message)?;
    Ok(TestOutcome {
        url: t.final_url.to_string(),
        status: Some(t.status),
        results: results(&rules, t.rules),
        ms: started.elapsed().as_millis() as u32,
    })
}

/// The rules stored with a crawl and their last run (FR-013).
#[tauri::command]
pub async fn custom_rules_of_crawl(crawl_path: PathBuf) -> Cmd<Vec<CrawlRule>> {
    tauri::async_runtime::spawn_blocking(move || {
        let conn =
            spiderlens_engine::crawl::store::connect_read_only(&crawl_path).map_err(|e| e.to_string())?;
        let rules = read_rules(&conn).map_err(|e| e.to_string())?;
        Ok(rules
            .into_iter()
            .map(|r| CrawlRule {
                name: r.name,
                kind: r.kind.as_str(),
                last_run_at: r.last_run_at,
                run_pages: r.run_pages,
                run_state: r.run_state,
            })
            .collect())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Runs `rules` (or the rules stored with the crawl) on every page with
/// stored HTML. Progress arrives as `custom-run-progress`, the end as
/// `custom-run-done` (FR-012, FR-014).
#[tauri::command]
pub fn custom_run(
    app: AppHandle,
    state: tauri::State<'_, State>,
    crawl_path: PathBuf,
    rules: Option<CustomRules>,
    only: Option<Vec<String>>,
) -> Cmd<u32> {
    let run = state.next_run.fetch_add(1, Ordering::Relaxed) + 1;
    let cancel = Arc::new(AtomicBool::new(false));
    state.runs.lock().expect("runs lock").insert(run, cancel.clone());
    std::thread::Builder::new()
        .name("custom-run".into())
        .spawn(move || {
            let only = only.unwrap_or_default();
            let which = match &rules {
                Some(r) => Which::These(r),
                None => Which::Stored,
            };
            let mut last = Instant::now();
            let result = rerun(&crawl_path, which, &only, &cancel, |p| {
                if last.elapsed().as_millis() >= 200 || p.done == p.total {
                    last = Instant::now();
                    let _ = app.emit(
                        "custom-run-progress",
                        RunProgress {
                            run,
                            done: p.done,
                            total: p.total,
                            per_sec: p.per_sec,
                        },
                    );
                }
            });
            let done = match result {
                Ok(r) => RunDone {
                    run,
                    path: crawl_path,
                    state: match r.state {
                        RerunState::Complete => "complete",
                        RerunState::Partial => "partial",
                    },
                    pages: r.pages,
                    error: None,
                },
                Err(e) => RunDone {
                    run,
                    path: crawl_path,
                    state: "failed",
                    pages: 0,
                    error: Some(match e {
                        RerunError::NotPossible(m) => m,
                        other => other.to_string(),
                    }),
                },
            };
            let _ = app.emit("custom-run-done", done);
        })
        .map_err(|e| format!("Could not start the run: {e}"))?;
    Ok(run)
}

/// Stops a run after its current batch; it is recorded as partial.
#[tauri::command]
pub fn custom_cancel(state: tauri::State<'_, State>, run: u32) {
    if let Some(c) = state.runs.lock().expect("runs lock").remove(&run) {
        c.store(true, Ordering::Relaxed);
    }
}

/// Reads a rules file (FR-010).
#[tauri::command]
pub fn custom_import(path: PathBuf) -> Cmd<CustomRules> {
    let text =
        std::fs::read_to_string(&path).map_err(|e| format!("Could not read {}: {e}", path.display()))?;
    import_rules(&text).map_err(|e| format!("{}: {e}", path.display()))
}

/// Writes a rules file (FR-010).
#[tauri::command]
pub fn custom_export(path: PathBuf, rules: CustomRules) -> Cmd<()> {
    std::fs::write(&path, export_rules(&rules))
        .map_err(|e| format!("Could not write {}: {e}", path.display()))
}
