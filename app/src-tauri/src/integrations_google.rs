//! Commands for URL Inspection and Google Analytics 4 (spec 024 slices 2
//! and 4): thin wrappers over `Hub`, like `integrations.rs`, which declares
//! this file as a child module and shares its run and worker helpers.

use std::path::PathBuf;

use spiderlens_integrations::ga4::{Ga4Request, Ga4Stream};
use spiderlens_integrations::inspect::{InspectPlan, InspectRequest};
use spiderlens_integrations::store::RunProgress;
use tauri::AppHandle;

use super::{Cmd, State, blocking, run};

/// The default inspection request for a property: the indexable pages.
#[tauri::command]
pub fn integrations_default_inspect_request(property: String) -> InspectRequest {
    InspectRequest::indexable(property)
}

/// How many URLs the request selects against today's remaining allowance,
/// and whether the run needs more than one day (US2.1, US2.4).
#[tauri::command]
pub async fn integrations_inspection_plan(
    state: tauri::State<'_, State>,
    path: PathBuf,
    account: Option<String>,
    request: InspectRequest,
) -> Cmd<InspectPlan> {
    blocking(&state, move |h| {
        h.inspection_plan(&path, account.as_deref(), &request)
    })
    .await
}

/// Inspects the chosen crawled URLs with Google (US2).
#[tauri::command]
pub async fn integrations_run_inspection(
    app: AppHandle,
    state: tauri::State<'_, State>,
    path: PathBuf,
    account: Option<String>,
    request: InspectRequest,
) -> Cmd<RunProgress> {
    let file = path.clone();
    run(app, &state, path, move |h, c, on| {
        h.fetch_inspection(&file, account.as_deref(), &request, c, on)
    })
    .await
}

/// The web data streams the account can read, the one for the open crawl's
/// site first (US4.1).
#[tauri::command]
pub async fn integrations_analytics_streams(
    state: tauri::State<'_, State>,
    account: Option<String>,
    path: Option<PathBuf>,
) -> Cmd<Vec<Ga4Stream>> {
    blocking(&state, move |h| {
        h.analytics_streams(account.as_deref(), path.as_deref())
    })
    .await
}

/// The default Analytics request for a stream: the last 28 days, query
/// strings combined.
#[tauri::command]
pub fn integrations_default_analytics_request(property: String, stream: String) -> Ga4Request {
    Ga4Request::last_28_days(property, stream)
}

/// Fetches sessions per landing page into the crawl file (US4).
#[tauri::command]
pub async fn integrations_run_analytics(
    app: AppHandle,
    state: tauri::State<'_, State>,
    path: PathBuf,
    account: Option<String>,
    request: Ga4Request,
) -> Cmd<RunProgress> {
    let file = path.clone();
    run(app, &state, path, move |h, c, on| {
        h.fetch_analytics(&file, account.as_deref(), &request, c, on)
    })
    .await
}
