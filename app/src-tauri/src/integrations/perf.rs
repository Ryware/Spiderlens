//! Commands for field data and lab tests (spec 024 US3): the Google API key,
//! estimates and runs. Thin wrappers over `Hub`, in a child module of
//! `integrations` so they share its state, worker threads and run control.
//! Pausing, resuming and cancelling use the commands of `integrations`.

use std::path::PathBuf;

use spiderlens_integrations::crux::FieldRequest;
use spiderlens_integrations::perf::PerfEstimate;
use spiderlens_integrations::psi::LabRequest;
use spiderlens_integrations::settings::Connection;
use spiderlens_integrations::store::RunProgress;
use tauri::AppHandle;

use super::{Cmd, State, blocking, run};

/// Saves one Google API key in the keychain for field data and lab tests.
#[tauri::command]
pub async fn integrations_save_google_key(
    state: tauri::State<'_, State>,
    label: String,
    key: String,
) -> Cmd<Vec<Connection>> {
    blocking(&state, move |h| h.save_google_api_key(&label, key.into())).await
}

#[tauri::command]
pub async fn integrations_estimate_field_data(
    state: tauri::State<'_, State>,
    path: PathBuf,
    request: FieldRequest,
) -> Cmd<PerfEstimate> {
    blocking(&state, move |h| h.estimate_field_data(&path, &request)).await
}

#[tauri::command]
pub async fn integrations_estimate_lab_tests(
    state: tauri::State<'_, State>,
    path: PathBuf,
    request: LabRequest,
) -> Cmd<PerfEstimate> {
    blocking(&state, move |h| h.estimate_lab_tests(&path, &request)).await
}

/// Fetches field data into the crawl file (US3.1, US3.2).
#[tauri::command]
pub async fn integrations_run_field_data(
    app: AppHandle,
    state: tauri::State<'_, State>,
    path: PathBuf,
    key_label: Option<String>,
    request: FieldRequest,
) -> Cmd<RunProgress> {
    let file = path.clone();
    run(app, &state, path, move |h, c, on| {
        h.fetch_field_data(&file, key_label.as_deref(), &request, c, on)
    })
    .await
}

/// Runs lab tests for the chosen pages (US3.3).
#[tauri::command]
pub async fn integrations_run_lab_tests(
    app: AppHandle,
    state: tauri::State<'_, State>,
    path: PathBuf,
    key_label: Option<String>,
    request: LabRequest,
) -> Cmd<RunProgress> {
    let file = path.clone();
    run(app, &state, path, move |h, c, on| {
        h.fetch_lab_tests(&file, key_label.as_deref(), &request, c, on)
    })
    .await
}
