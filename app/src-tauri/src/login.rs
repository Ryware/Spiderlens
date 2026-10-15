//! The login window (spec 025 plan D5, contracts/desktop-commands.md).
//!
//! The person signs in to the site in a private window of its own. On
//! Done, the app reads the window's cookies, keeps the ones for the
//! crawl's internal hosts, closes the window and deletes its data. The
//! values stay in this process as a [`LoginSession`]; the window gets only
//! names. The login window has no capability entry, so the site's pages
//! cannot call any command.
//!
//! Events: `login://navigated` with `{ url }` for the address strip in the
//! main window, `login://closed` when the person closes the window.

use std::path::PathBuf;
use std::sync::Mutex;

use serde::Serialize;
use spiderlens_config::{InternalHosts, SubdomainMode};
use spiderlens_engine::net::{ImportCounts, LoginSession, SessionCookie};
use tauri::{AppHandle, Emitter, Manager, Url, WebviewUrl, WebviewWindowBuilder};

type Cmd<T> = Result<T, String>;

const LABEL: &str = "login";

struct Pending {
    login_url: String,
    internal_hosts: Vec<String>,
    data_dir: PathBuf,
}

#[derive(Default)]
pub struct State {
    pending: Mutex<Option<Pending>>,
    session: Mutex<Option<LoginSession>>,
}

impl State {
    /// The captured session, for the next crawl started or resumed.
    pub fn session(&self) -> Option<LoginSession> {
        self.session.lock().expect("login lock").clone()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SiteNames {
    pub domain: String,
    pub names: Vec<String>,
}

/// What the window shows after a capture or an import: names, never values.
#[derive(Debug, Clone, Serialize)]
pub struct Captured {
    pub sites: Vec<SiteNames>,
    pub captured_at: i64,
    pub login_url: String,
}

impl Captured {
    fn of(s: &LoginSession) -> Self {
        Self {
            sites: s
                .summary()
                .into_iter()
                .map(|(domain, names)| SiteNames { domain, names })
                .collect(),
            captured_at: s.captured_at,
            login_url: s.login_url.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct Navigated {
    url: String,
}

fn now_ms() -> i64 {
    jiff::Timestamp::now().as_millisecond()
}

/// The crawl's internal hosts as a test: each listed host and its
/// subdomains.
fn internal_test(hosts: &[String]) -> Box<dyn Fn(&str) -> bool + Send + Sync> {
    let sets: Vec<InternalHosts> = hosts
        .iter()
        .map(|h| InternalHosts::new(SubdomainMode::AllSubdomains, h.trim().trim_start_matches('.')))
        .collect();
    Box::new(move |host: &str| sets.iter().any(|s| s.is_internal(host.trim_start_matches('.'))))
}

fn remove_dir(dir: &PathBuf) {
    let _ = std::fs::remove_dir_all(dir);
}

/// Opens the login page in a private window.
#[tauri::command]
pub fn login_open(
    app: AppHandle,
    state: tauri::State<'_, State>,
    login_url: String,
    internal_hosts: Vec<String>,
    user_agent: Option<String>,
) -> Cmd<()> {
    let url = Url::parse(login_url.trim()).map_err(|_| "Enter the login page's full address.".to_string())?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("The login page must start with http:// or https://.".into());
    }
    if let Some(w) = app.get_webview_window(LABEL) {
        let _ = w.set_focus();
        return Ok(());
    }
    let data_dir = std::env::temp_dir().join(format!("spiderlens-login-{}-{}", std::process::id(), now_ms()));
    let events = app.clone();
    let mut builder = WebviewWindowBuilder::new(&app, LABEL, WebviewUrl::External(url))
        .title("Sign in")
        .inner_size(960.0, 720.0)
        .incognito(true)
        .data_directory(data_dir.clone())
        .on_navigation(move |u| {
            let _ = events.emit("login://navigated", Navigated { url: u.to_string() });
            true
        });
    // The site sees the same browser the crawl claims to be (plan risk 3).
    if let Some(ua) = user_agent.as_deref().filter(|u| !u.trim().is_empty()) {
        builder = builder.user_agent(ua);
    }
    let window = builder
        .build()
        .map_err(|e| format!("Could not open the login window: {e}"))?;
    let closed = app.clone();
    window.on_window_event(move |e| {
        if matches!(e, tauri::WindowEvent::Destroyed) {
            let _ = closed.emit("login://closed", ());
        }
    });
    *state.pending.lock().expect("login lock") = Some(Pending {
        login_url,
        internal_hosts,
        data_dir,
    });
    Ok(())
}

/// Reads the login window's cookies, keeps the internal hosts' ones and
/// closes the window. Async: reading cookies from a synchronous command
/// deadlocks on Windows (WebView2).
#[tauri::command]
pub async fn login_done(app: AppHandle) -> Cmd<Captured> {
    let state = app.state::<State>();
    let pending = state
        .pending
        .lock()
        .expect("login lock")
        .take()
        .ok_or("The login window is not open.")?;
    let window = app
        .get_webview_window(LABEL)
        .ok_or("The login window was closed.")?;
    let cookies = window.cookies();
    let _ = window.close();
    remove_dir(&pending.data_dir);
    let cookies = cookies.map_err(|e| format!("Could not read the login window's cookies: {e}"))?;
    let keep = internal_test(&pending.internal_hosts);
    let now = now_ms();
    let session = LoginSession {
        login_url: pending.login_url,
        captured_at: now,
        cookies: cookies
            .iter()
            .filter_map(|c| {
                let raw_domain = c.domain()?.to_string();
                let domain = raw_domain.trim_start_matches('.').to_ascii_lowercase();
                if !keep(&domain) {
                    return None;
                }
                let expires = c
                    .expires()
                    .and_then(|e| e.datetime())
                    .map(|t| t.unix_timestamp().saturating_mul(1000));
                if expires.is_some_and(|e| e <= now) {
                    return None;
                }
                Some(SessionCookie {
                    name: c.name().to_string(),
                    value: spiderlens_config::Secret::new(c.value()),
                    host_only: !raw_domain.starts_with('.'),
                    domain,
                    path: c.path().unwrap_or("/").to_string(),
                    secure: c.secure().unwrap_or(false),
                    http_only: c.http_only().unwrap_or(false),
                    expires,
                })
            })
            .collect(),
    };
    if session.cookies.is_empty() {
        return Err(
            "No cookies for this site were found. Sign in in the window first, then choose Signed in.".into(),
        );
    }
    let captured = Captured::of(&session);
    *state.session.lock().expect("login lock") = Some(session);
    Ok(captured)
}

/// Closes the login window without keeping anything.
#[tauri::command]
pub fn login_cancel(app: AppHandle, state: tauri::State<'_, State>) {
    if let Some(p) = state.pending.lock().expect("login lock").take() {
        if let Some(w) = app.get_webview_window(LABEL) {
            let _ = w.close();
        }
        remove_dir(&p.data_dir);
    }
}

/// Forgets the captured session.
#[tauri::command]
pub fn login_clear(state: tauri::State<'_, State>) {
    *state.session.lock().expect("login lock") = None;
}

/// The captured session's summary, if there is one.
#[tauri::command]
pub fn login_status(state: tauri::State<'_, State>) -> Option<Captured> {
    state
        .session
        .lock()
        .expect("login lock")
        .as_ref()
        .map(Captured::of)
}

#[derive(Debug, Clone, Serialize)]
pub struct Imported {
    pub imported: usize,
    pub skipped_expired: usize,
    pub skipped_external: usize,
    pub skipped_malformed: usize,
    pub captured: Option<Captured>,
}

impl From<(ImportCounts, Option<Captured>)> for Imported {
    fn from((c, captured): (ImportCounts, Option<Captured>)) -> Self {
        Self {
            imported: c.imported,
            skipped_expired: c.skipped_expired,
            skipped_external: c.skipped_external,
            skipped_malformed: c.skipped_malformed,
            captured,
        }
    }
}

/// Imports a browser's `cookies.txt` as the login session (FR-015): values
/// stay here, only names and counts go back.
#[tauri::command]
pub fn cookies_import(
    state: tauri::State<'_, State>,
    path: PathBuf,
    internal_hosts: Vec<String>,
) -> Cmd<Imported> {
    let text =
        std::fs::read_to_string(&path).map_err(|e| format!("Could not read {}: {e}", path.display()))?;
    let name = path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    );
    let (session, counts) =
        LoginSession::from_cookies_txt(&text, &name, now_ms(), internal_test(&internal_hosts));
    if session.cookies.is_empty() {
        return Ok((counts, None).into());
    }
    let captured = Captured::of(&session);
    *state.session.lock().expect("login lock") = Some(session);
    Ok((counts, Some(captured)).into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn internal_hosts_include_their_subdomains_only() {
        let keep = internal_test(&["example.com".into()]);
        assert!(keep("example.com"));
        assert!(keep(".shop.example.com"));
        assert!(!keep("tracker.test"));
        assert!(!keep("notexample.com"));
    }

    #[test]
    fn captured_summary_has_names_not_values() {
        let s = LoginSession {
            login_url: "https://example.com/login".into(),
            captured_at: 1,
            cookies: vec![SessionCookie {
                name: "sid".into(),
                value: spiderlens_config::Secret::new("secret-value"),
                domain: "example.com".into(),
                host_only: true,
                path: "/".into(),
                secure: true,
                http_only: true,
                expires: None,
            }],
        };
        let json = serde_json::to_string(&Captured::of(&s)).unwrap();
        assert!(json.contains("sid"));
        assert!(!json.contains("secret-value"));
    }
}
