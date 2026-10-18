//! Commands for the crawl report, the report file and bulk exports (spec 010,
//! specs/010-crawl-report-exports/contracts/desktop-commands.md).
//!
//! The report is built on the results reader thread from stored data, never
//! on the crawl's thread. Bulk exports open their own read connection on a
//! thread of their own, so the table stays usable while they run.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use spiderlens_results::ExportError;
use spiderlens_results::bulk::{self, BulkOptions, BulkProgress, BulkResult};
use spiderlens_results::report::html::FileOptions;
use spiderlens_results::report::{Report, ReportOptions, Section};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::{compare, results};

type Cmd<T> = Result<T, String>;

/// Error text the UI matches to ask before replacing a file or folder.
const EXISTS: &str = "exists";

#[derive(Default)]
pub struct State {
    bulk: Mutex<Option<Arc<AtomicBool>>>,
}

#[derive(Serialize)]
pub struct Written {
    path: PathBuf,
    bytes: u64,
}

/// The open comparison's section, when the report is asked to carry one
/// (FR-033). The UI passes the comparison only for the report of its
/// current crawl.
async fn extra_sections(compare: &compare::State, comparison: Option<u32>) -> Cmd<Vec<Section>> {
    match comparison {
        Some(id) => Ok(vec![compare.report_section(id).await?]),
        None => Ok(Vec::new()),
    }
}

/// The report of the open crawl file (FR-001), with the comparison section
/// when `comparison` names an open comparison.
#[tauri::command]
pub async fn report(
    results: tauri::State<'_, results::State>,
    compare: tauri::State<'_, compare::State>,
    path: PathBuf,
    running: bool,
    profile: Option<String>,
    comparison: Option<u32>,
) -> Cmd<Report> {
    let extra_sections = extra_sections(&compare, comparison).await?;
    let opts = ReportOptions {
        running,
        profile,
        extra_sections,
    };
    results
        .with(&path, move |r| r.report(&opts).map_err(|e| e.to_string()))
        .await
}

async fn html(
    results: &results::State,
    compare: &compare::State,
    path: &Path,
    file: FileOptions,
    comparison: Option<u32>,
) -> Cmd<String> {
    file.validate()?;
    let opts = ReportOptions {
        extra_sections: extra_sections(compare, comparison).await?,
        ..ReportOptions::default()
    };
    results
        .with(path, move |r| {
            r.report_html(&opts, &file).map_err(|e| e.to_string())
        })
        .await
}

/// Writes the self-contained report file (FR-013 to FR-016).
#[tauri::command]
pub async fn export_report_html(
    results: tauri::State<'_, results::State>,
    compare: tauri::State<'_, compare::State>,
    path: PathBuf,
    dest: PathBuf,
    file: FileOptions,
    overwrite: bool,
    comparison: Option<u32>,
) -> Cmd<Written> {
    if dest.exists() && !overwrite {
        return Err(EXISTS.into());
    }
    let html = html(&results, &compare, &path, file, comparison).await?;
    write_atomic(&dest, html.as_bytes()).map_err(|e| format!("Could not write the report: {e}"))?;
    Ok(Written {
        bytes: html.len() as u64,
        path: dest,
    })
}

fn write_atomic(out: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let name = out
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tmp = out.with_file_name(format!(".{name}.partial"));
    let r = std::fs::write(&tmp, bytes).and_then(|()| std::fs::rename(&tmp, out));
    if r.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    r
}

/// Opens the report file in its own window and the system print dialog,
/// where "Save as PDF" makes the PDF (FR-017, plan D7). The window has no
/// app permissions and the page has no scripts.
#[tauri::command]
pub async fn print_report(
    app: AppHandle,
    results: tauri::State<'_, results::State>,
    compare: tauri::State<'_, compare::State>,
    path: PathBuf,
    file: FileOptions,
    comparison: Option<u32>,
) -> Cmd<()> {
    let html = html(&results, &compare, &path, file, comparison).await?;
    let dir = app.path().app_cache_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let page = dir.join("report-print.html");
    write_atomic(&page, html.as_bytes()).map_err(|e| format!("Could not prepare the report: {e}"))?;
    let url = tauri::Url::from_file_path(&page).map_err(|()| "Could not prepare the report.".to_string())?;
    const LABEL: &str = "report-print";
    if let Some(w) = app.get_webview_window(LABEL) {
        let _ = w.close();
    }
    WebviewWindowBuilder::new(&app, LABEL, WebviewUrl::External(url))
        .title("Print report")
        .inner_size(900.0, 1000.0)
        .on_page_load(|w, p| {
            if p.event() == tauri::webview::PageLoadEvent::Finished {
                let _ = w.print();
            }
        })
        .build()
        .map_err(|e| format!("Could not open the report: {e}"))?;
    Ok(())
}

/// One bulk export at a time; progress arrives as `bulk-export-progress`
/// events (FR-018 to FR-025).
#[tauri::command]
pub async fn bulk_export(
    app: AppHandle,
    results: tauri::State<'_, results::State>,
    state: tauri::State<'_, State>,
    path: PathBuf,
    options: BulkOptions,
) -> Cmd<BulkResult> {
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut slot = state.bulk.lock().expect("bulk lock");
        if slot.is_some() {
            return Err("An export is already running.".into());
        }
        *slot = Some(cancel.clone());
    }
    let settings = results.with(&path, |r| Ok(r.issue_settings().clone())).await;
    let (reply, answer) = tokio::sync::oneshot::channel();
    let started = settings.and_then(|settings| {
        let app = app.clone();
        std::thread::Builder::new()
            .name("bulk-export".into())
            .spawn(move || {
                // Progress comes once per table and every 10,000 rows.
                let r = bulk::run_bulk(
                    &path,
                    &settings,
                    &options,
                    &mut |p: BulkProgress| {
                        let _ = app.emit("bulk-export-progress", p);
                    },
                    &cancel,
                );
                let _ = reply.send(r);
            })
            .map_err(|e| format!("Could not start the export: {e}"))
    });
    let result = match started {
        Ok(_) => answer
            .await
            .map_err(|_| "The export stopped.".to_string())
            .and_then(|r| {
                r.map_err(|e| match e {
                    ExportError::Exists(_) => EXISTS.into(),
                    e => e.to_string(),
                })
            }),
        Err(e) => Err(e),
    };
    *state.bulk.lock().expect("bulk lock") = None;
    result
}

#[tauri::command]
pub fn bulk_export_cancel(state: tauri::State<'_, State>) {
    if let Some(c) = state.bulk.lock().expect("bulk lock").as_ref() {
        c.store(true, Ordering::Relaxed);
    }
}
