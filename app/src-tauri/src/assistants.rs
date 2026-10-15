//! Settings › AI assistants (spec 016 FR-020 to FR-022, FR-028;
//! contracts/desktop-commands.md). The MCP server runs in its own process
//! (this executable with `mcp`), so the app and the server share only
//! `assistants.json` and the activity log.

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::Serialize;
use spiderlens_config::{AppDirs, AssistantSettings, AssistantStore};
use spiderlens_mcp::activity::{Entry, Log};
use spiderlens_mcp::snippets::{Snippet, snippets};

type Cmd<T> = Result<T, String>;

const TEST_TIMEOUT: Duration = Duration::from_secs(10);

pub struct State {
    dirs: AppDirs,
}

impl State {
    pub fn new(dirs: AppDirs) -> Self {
        Self { dirs }
    }

    fn store(&self) -> Cmd<AssistantStore> {
        AssistantStore::open(&self.dirs.config).map_err(|e| e.to_string())
    }

    fn log(&self) -> Log {
        Log::new(&self.dirs.data)
    }
}

#[derive(Serialize)]
pub struct AssistantsView {
    settings: AssistantSettings,
    executable: PathBuf,
    snippets: Vec<Snippet>,
}

fn executable() -> Cmd<PathBuf> {
    std::env::current_exe().map_err(|e| format!("SpiderLens could not find its own program file: {e}"))
}

#[tauri::command]
pub fn assistants_get(state: tauri::State<'_, State>) -> Cmd<AssistantsView> {
    let exe = executable()?;
    Ok(AssistantsView {
        settings: state.store()?.load(),
        snippets: snippets(&exe),
        executable: exe,
    })
}

#[tauri::command]
pub fn assistants_set(state: tauri::State<'_, State>, settings: AssistantSettings) -> Cmd<AssistantSettings> {
    let settings = settings.checked()?;
    state.store()?.save(&settings).map_err(|e| e.to_string())?;
    Ok(settings)
}

/// Runs `<this executable> mcp --self-test` and returns what it printed:
/// the same tool list an assistant would get.
#[tauri::command]
pub async fn assistants_test() -> Cmd<serde_json::Value> {
    let exe = executable()?;
    tauri::async_runtime::spawn_blocking(move || {
        let mut child = Command::new(&exe)
            .args(["mcp", "--self-test"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("The MCP server did not start: {e}"))?;
        let start = Instant::now();
        loop {
            if child.try_wait().map_err(|e| e.to_string())?.is_some() {
                break;
            }
            if start.elapsed() > TEST_TIMEOUT {
                let _ = child.kill();
                return Err("The MCP server did not answer within 10 seconds.".into());
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let out = child.wait_with_output().map_err(|e| e.to_string())?;
        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr);
            return Err(format!("The MCP server failed: {}", err.trim()));
        }
        serde_json::from_slice(&out.stdout)
            .map_err(|_| "The MCP server answered with something unexpected.".to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn assistants_activity(state: tauri::State<'_, State>, limit: usize) -> Vec<Entry> {
    state.log().read(limit.min(500))
}

#[tauri::command]
pub fn assistants_activity_clear(state: tauri::State<'_, State>) -> Cmd<()> {
    state.log().clear().map_err(|e| e.to_string())
}
