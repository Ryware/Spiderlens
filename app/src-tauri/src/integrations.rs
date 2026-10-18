//! Commands for external data (spec 024,
//! specs/024-external-integrations/contracts/desktop-commands.md): thin
//! wrappers over `spiderlens_integrations::hub::Hub`, shared with the CLI.
//!
//! Hub calls block (keychain, network on their own runtime), so each runs
//! on its own thread and the command awaits the answer. One run at a time.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use spiderlens_integrations::gsc::{PageRequest, Property, QueryRow};
use spiderlens_integrations::hub::{CacheInfo, Hub, OrphanPage, ProviderStatus, Status};
use spiderlens_integrations::run::Control;
use spiderlens_integrations::settings::{Connection, ProviderSettings};
use spiderlens_integrations::store::RunProgress;
use spiderlens_integrations::{Provider, browser};
use tauri::{AppHandle, Emitter, Manager};

pub(crate) type Cmd<T> = Result<T, String>;

/// The progress event, at most every 200 ms and once at the end.
pub const PROGRESS: &str = "integrations://progress";

pub struct State {
    pub(crate) hub: Arc<Hub>,
    /// The running run's control, if any.
    pub(crate) run: Mutex<Option<Control>>,
    /// The sign-in waiting for the browser, if any.
    sign_in: Mutex<Option<Control>>,
}

impl State {
    pub fn open() -> Result<Self, String> {
        Ok(Self {
            hub: Arc::new(Hub::open_default().map_err(err)?),
            run: Mutex::new(None),
            sign_in: Mutex::new(None),
        })
    }
}

pub(crate) fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Runs `f` with the hub on its own thread.
pub(crate) async fn blocking<T: Send + 'static>(
    state: &State,
    f: impl FnOnce(&Hub) -> spiderlens_integrations::Result<T> + Send + 'static,
) -> Cmd<T> {
    let hub = state.hub.clone();
    let (tx, rx) = tokio::sync::oneshot::channel();
    std::thread::Builder::new()
        .name("integrations".into())
        .spawn(move || {
            let _ = tx.send(f(&hub).map_err(err));
        })
        .map_err(err)?;
    rx.await
        .map_err(|_| "The integrations worker stopped.".to_string())?
}

pub(crate) fn provider(id: &str) -> Cmd<Provider> {
    Provider::from_id(id).ok_or_else(|| format!("Unknown provider {id}"))
}

#[tauri::command]
pub async fn integrations_status(state: tauri::State<'_, State>) -> Cmd<Status> {
    blocking(&state, |h| Ok(h.status())).await
}

#[tauri::command]
pub async fn integrations_settings_save(
    state: tauri::State<'_, State>,
    provider: String,
    settings: ProviderSettings,
) -> Cmd<ProviderStatus> {
    let p = self::provider(&provider)?;
    blocking(&state, move |h| h.save_settings(p, settings)).await
}

/// Saves the user's own Google OAuth client, or goes back to the built-in
/// one when `client_id` is empty (FR-016).
#[tauri::command]
pub async fn integrations_google_client(
    state: tauri::State<'_, State>,
    client_id: String,
    client_secret: Option<String>,
) -> Cmd<()> {
    blocking(&state, move |h| {
        let id = client_id.trim().to_string();
        h.save_google_client((!id.is_empty()).then(|| {
            (
                id,
                client_secret
                    .filter(|s| !s.trim().is_empty())
                    .map(|s| s.trim().to_string().into()),
            )
        }))
    })
    .await
}

/// Opens Google's consent page in the system browser and waits for the
/// answer (or `integrations_connect_cancel`).
#[tauri::command]
pub async fn integrations_connect_google(
    state: tauri::State<'_, State>,
    features: Vec<String>,
) -> Cmd<Connection> {
    let control = Control::default();
    *state.sign_in.lock().expect("lock") = Some(control.clone());
    let r = blocking(&state, move |h| {
        let f: Vec<&str> = features.iter().map(String::as_str).collect();
        h.connect_google(&f, Duration::from_secs(300), &control, |url| {
            browser::open(url);
        })
    })
    .await;
    *state.sign_in.lock().expect("lock") = None;
    r
}

#[tauri::command]
pub fn integrations_connect_cancel(state: tauri::State<'_, State>) {
    if let Some(c) = state.sign_in.lock().expect("lock").as_ref() {
        c.cancel();
    }
}

#[tauri::command]
pub async fn integrations_save_key(
    state: tauri::State<'_, State>,
    provider: String,
    label: String,
    key: String,
) -> Cmd<Connection> {
    let p = self::provider(&provider)?;
    blocking(&state, move |h| h.save_key(p, &label, key.into())).await
}

/// Revokes and removes a connection; true when Google confirmed the revoke.
#[tauri::command]
pub async fn integrations_forget(state: tauri::State<'_, State>, kind: String, label: String) -> Cmd<bool> {
    blocking(&state, move |h| h.forget(&kind, &label)).await
}

#[tauri::command]
pub async fn integrations_properties(
    state: tauri::State<'_, State>,
    account: Option<String>,
    path: Option<PathBuf>,
) -> Cmd<Vec<Property>> {
    blocking(&state, move |h| h.properties(account.as_deref(), path.as_deref())).await
}

/// The default Search Console request for a property: the last 28 days of
/// final data.
#[tauri::command]
pub fn integrations_default_request(property: String) -> PageRequest {
    PageRequest::last_28_days(property)
}

pub(crate) fn refused_while_crawling(app: &AppHandle, path: &std::path::Path) -> Cmd<()> {
    if app.state::<crate::crawl::State>().active_path().as_deref() == Some(path) {
        return Err(
            "This crawl is still running. Fetch external data when it has finished or paused.".into(),
        );
    }
    Ok(())
}

pub(crate) async fn run(
    app: AppHandle,
    state: &State,
    path: PathBuf,
    go: impl FnOnce(&Hub, &Control, &mut dyn FnMut(&RunProgress)) -> spiderlens_integrations::Result<RunProgress>
    + Send
    + 'static,
) -> Cmd<RunProgress> {
    refused_while_crawling(&app, &path)?;
    let control = {
        let mut run = state.run.lock().expect("lock");
        if run.is_some() {
            return Err("Another external data run is in progress.".into());
        }
        let c = Control::default();
        *run = Some(c.clone());
        c
    };
    let events = app.clone();
    let r = blocking(state, move |h| {
        let mut last = Instant::now() - Duration::from_secs(1);
        go(h, &control, &mut |p| {
            if last.elapsed() >= Duration::from_millis(200) {
                last = Instant::now();
                let _ = events.emit(PROGRESS, p);
            }
        })
    })
    .await;
    *state.run.lock().expect("lock") = None;
    // The results reader re-reads which data the file has.
    app.state::<crate::results::State>().close(&path);
    if let Ok(p) = &r {
        let _ = app.emit(PROGRESS, p);
    }
    r
}

/// Fetches Search Console page data into the crawl file (US1).
#[tauri::command]
pub async fn integrations_run_search_console(
    app: AppHandle,
    state: tauri::State<'_, State>,
    path: PathBuf,
    account: Option<String>,
    request: PageRequest,
) -> Cmd<RunProgress> {
    let file = path.clone();
    run(app, &state, path, move |h, c, on| {
        h.fetch_search_console(&file, account.as_deref(), &request, c, on)
    })
    .await
}

#[tauri::command]
pub async fn integrations_run_resume(
    app: AppHandle,
    state: tauri::State<'_, State>,
    path: PathBuf,
    run_id: Option<i64>,
) -> Cmd<RunProgress> {
    let file = path.clone();
    run(app, &state, path, move |h, c, on| h.resume(&file, run_id, c, on)).await
}

#[tauri::command]
pub fn integrations_run_pause(state: tauri::State<'_, State>) {
    if let Some(c) = state.run.lock().expect("lock").as_ref() {
        c.pause();
    }
}

#[tauri::command]
pub fn integrations_run_cancel(state: tauri::State<'_, State>) {
    if let Some(c) = state.run.lock().expect("lock").as_ref() {
        c.cancel();
    }
}

#[tauri::command]
pub async fn integrations_runs(state: tauri::State<'_, State>, path: PathBuf) -> Cmd<Vec<RunProgress>> {
    blocking(&state, move |h| h.runs(&path)).await
}

#[tauri::command]
pub async fn integrations_queries(
    state: tauri::State<'_, State>,
    path: PathBuf,
    url_id: i64,
) -> Cmd<Vec<QueryRow>> {
    blocking(&state, move |h| h.queries(&path, url_id)).await
}

#[tauri::command]
pub async fn integrations_orphans(
    state: tauri::State<'_, State>,
    path: PathBuf,
    source: Option<String>,
    offset: i64,
    limit: i64,
) -> Cmd<OrphanPage> {
    blocking(&state, move |h| {
        h.orphans(&path, source.as_deref(), offset, limit.clamp(1, 1000))
    })
    .await
}

#[tauri::command]
pub async fn integrations_remove(
    app: AppHandle,
    state: tauri::State<'_, State>,
    path: PathBuf,
    provider: Option<String>,
) -> Cmd<()> {
    refused_while_crawling(&app, &path)?;
    let p = provider.as_deref().map(self::provider).transpose()?;
    let file = path.clone();
    let r = blocking(&state, move |h| h.remove(&file, p)).await;
    app.state::<crate::results::State>().close(&path);
    r
}

#[tauri::command]
pub async fn integrations_cache_info(state: tauri::State<'_, State>) -> Cmd<CacheInfo> {
    blocking(&state, |h| h.cache_info()).await
}

#[tauri::command]
pub async fn integrations_cache_clear(
    state: tauri::State<'_, State>,
    provider: Option<String>,
) -> Cmd<usize> {
    let p = provider.as_deref().map(self::provider).transpose()?;
    blocking(&state, move |h| h.cache_clear(p)).await
}

/// Opens a provider's terms page.
#[tauri::command]
pub fn integrations_open_terms(provider: String) -> Cmd<()> {
    let p = self::provider(&provider)?;
    browser::open(p.terms_url());
    Ok(())
}
