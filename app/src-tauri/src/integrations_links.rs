//! Tauri commands for link metrics (spec 024 US5) and the Google Sheets and
//! Looker Studio export (US6). Same rules as `integrations.rs`: the hub runs
//! on its own thread, a run is refused while a crawl holds the file.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use serde::Serialize;
use spiderlens_config::Segment;
use spiderlens_integrations::links::{LinkEstimate, LinkRequest, MetricInfo};
use spiderlens_integrations::run::Control;
use spiderlens_integrations::sheets::{Overflow, SheetExport, SheetsPlan, SpreadsheetRef};
use spiderlens_integrations::store::RunProgress;
use spiderlens_integrations::{Error, browser};
use spiderlens_results::{ViewQuery, export_file_in};
use tauri::{AppHandle, Emitter, Manager};

use crate::integrations::{Cmd, State, blocking, err, provider, refused_while_crawling, run};

/// Rows sent to the spreadsheet so far, at most every 200 ms.
pub const SHEETS_PROGRESS: &str = "integrations://sheets-progress";

#[derive(Serialize, Clone)]
struct SheetsProgress {
    sent: u64,
    total: u64,
}

#[tauri::command]
pub fn integrations_link_metrics(
    state: tauri::State<'_, State>,
    provider_id: String,
) -> Cmd<Vec<MetricInfo>> {
    Ok(state.hub.link_metrics_catalogue(provider(&provider_id)?))
}

/// What a run would cost, in the provider's units, before anything is sent.
#[tauri::command]
pub async fn integrations_link_estimate(
    state: tauri::State<'_, State>,
    path: PathBuf,
    label: Option<String>,
    request: LinkRequest,
) -> Cmd<LinkEstimate> {
    blocking(&state, move |h| {
        h.estimate_links(&path, label.as_deref(), &request)
    })
    .await
}

/// Fetches link metrics into the crawl file (US5).
#[tauri::command]
pub async fn integrations_run_links(
    app: AppHandle,
    state: tauri::State<'_, State>,
    path: PathBuf,
    label: Option<String>,
    request: LinkRequest,
) -> Cmd<RunProgress> {
    let file = path.clone();
    run(app, &state, path, move |h, c, on| {
        h.fetch_links(&file, label.as_deref(), &request, c, on)
    })
    .await
}

/// Writes the view to a temporary CSV with the same rows and columns as the
/// CSV export, so the sheet matches what the person sees.
fn export_temp_csv(
    app: &AppHandle,
    path: &Path,
    query: &ViewQuery,
    columns: &[String],
    segments: Option<&[Segment]>,
) -> spiderlens_integrations::Result<PathBuf> {
    let issues = app
        .state::<crate::results::State>()
        .issues
        .lock()
        .expect("issues lock")
        .clone();
    let settings = spiderlens_results::issues::resolve(&issues).0;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let dest = std::env::temp_dir().join(format!("spiderlens-sheets-{}-{nanos}.csv", std::process::id()));
    let never = AtomicBool::new(false);
    export_file_in(
        path,
        query,
        columns,
        None,
        None,
        &settings,
        segments,
        &dest,
        &never,
        &mut |_, _| {},
    )
    .map_err(|e| Error::Invalid(e.to_string()))?;
    Ok(dest)
}

/// How big an export would be, and whether it fits one spreadsheet (FR-030).
#[tauri::command]
pub async fn integrations_sheets_plan(
    app: AppHandle,
    state: tauri::State<'_, State>,
    path: PathBuf,
    query: ViewQuery,
    columns: Vec<String>,
    segments: Option<Vec<Segment>>,
) -> Cmd<SheetsPlan> {
    blocking(&state, move |h| {
        let csv = export_temp_csv(&app, &path, &query, &columns, segments.as_deref())?;
        let plan = h.sheets_plan(&csv);
        let _ = std::fs::remove_file(&csv);
        plan
    })
    .await
}

/// Creates a new spreadsheet from the view (US6).
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn integrations_export_sheets(
    app: AppHandle,
    state: tauri::State<'_, State>,
    path: PathBuf,
    query: ViewQuery,
    columns: Vec<String>,
    segments: Option<Vec<Segment>>,
    title: String,
    worksheet: String,
    account: Option<String>,
    overflow: Overflow,
) -> Cmd<SheetExport> {
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
    let r = blocking(&state, move |h| {
        let csv = export_temp_csv(&app, &path, &query, &columns, segments.as_deref())?;
        let mut last = Instant::now() - Duration::from_secs(1);
        let out = h.export_sheets(
            &csv,
            &title,
            &worksheet,
            account.as_deref(),
            overflow,
            &control,
            &mut |sent, total| {
                if last.elapsed() >= Duration::from_millis(200) {
                    last = Instant::now();
                    let _ = app.emit(SHEETS_PROGRESS, SheetsProgress { sent, total });
                }
            },
        );
        let _ = std::fs::remove_file(&csv);
        out
    })
    .await;
    *state.run.lock().expect("lock") = None;
    r
}

/// Opens the Looker Studio report that starts on an exported sheet.
#[tauri::command]
pub async fn integrations_open_looker(state: tauri::State<'_, State>, sheet: SpreadsheetRef) -> Cmd<String> {
    let link = blocking(&state, move |h| h.looker_link(&sheet)).await?;
    if !browser::open(&link) {
        return Err(err("Could not open a browser. Copy the link instead."));
    }
    Ok(link)
}

/// The same link, for copying.
#[tauri::command]
pub async fn integrations_looker_link(state: tauri::State<'_, State>, sheet: SpreadsheetRef) -> Cmd<String> {
    blocking(&state, move |h| h.looker_link(&sheet)).await
}
