//! Registering schedules with the operating system's own scheduler (plan D1).
//!
//! Every schedule registers the same shape: start the program at minute MM of
//! every hour, and once at login. The text of each registration is built by
//! pure functions, tested on every OS; only `register`, `remove`, `inspect`
//! and `list_ours` touch the system.

pub mod launchd;
pub mod systemd;
pub mod taskschd;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Mutex;

/// Prefix of every registration name we create.
pub const LABEL: &str = "com.ryware.spiderlens.schedule";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Registration {
    pub id: String,
    pub name: String,
    pub minute: u8,
    pub program: PathBuf,
    /// Arguments after the program, for example `["schedule", "tick", id]`.
    pub args: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct OsError(pub String);

impl From<std::io::Error> for OsError {
    fn from(e: std::io::Error) -> Self {
        Self(e.to_string())
    }
}

pub trait Scheduler: Send + Sync {
    /// Creates or replaces the registration.
    fn register(&self, r: &Registration) -> Result<(), OsError>;
    /// Removes it; removing a registration that does not exist is not an error.
    fn remove(&self, id: &str) -> Result<(), OsError>;
    /// The program a registration starts, or `None` when there is none.
    fn inspect(&self, id: &str) -> Result<Option<PathBuf>, OsError>;
    /// Ids of every registration we made, including ones without a schedule.
    fn list_ours(&self) -> Result<Vec<String>, OsError>;
}

/// The scheduler of the OS this was built for.
pub fn system() -> Box<dyn Scheduler> {
    #[cfg(target_os = "macos")]
    {
        Box::new(launchd::Launchd::new())
    }
    #[cfg(windows)]
    {
        Box::new(taskschd::TaskScheduler)
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        Box::new(systemd::Systemd::new())
    }
}

/// The system scheduler, or a file-backed fake when `SPIDERLENS_SCHEDULER`
/// names a JSON file (tests of the command line and the app).
pub fn from_env() -> Box<dyn Scheduler> {
    match std::env::var_os("SPIDERLENS_SCHEDULER") {
        Some(path) => Box::new(FileFake(PathBuf::from(path))),
        None => system(),
    }
}

/// Registrations kept in a JSON file: `{id: {program, minute, args}}`.
pub struct FileFake(pub PathBuf);

impl FileFake {
    fn load(&self) -> BTreeMap<String, serde_json::Value> {
        std::fs::read(&self.0)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    fn save(&self, m: &BTreeMap<String, serde_json::Value>) -> Result<(), OsError> {
        std::fs::write(&self.0, serde_json::to_vec_pretty(m).expect("map serializes"))?;
        Ok(())
    }
}

impl Scheduler for FileFake {
    fn register(&self, r: &Registration) -> Result<(), OsError> {
        let mut m = self.load();
        m.insert(
            r.id.clone(),
            serde_json::json!({"program": r.program, "minute": r.minute, "args": r.args}),
        );
        self.save(&m)
    }
    fn remove(&self, id: &str) -> Result<(), OsError> {
        let mut m = self.load();
        m.remove(id);
        self.save(&m)
    }
    fn inspect(&self, id: &str) -> Result<Option<PathBuf>, OsError> {
        Ok(self
            .load()
            .get(id)
            .and_then(|v| v["program"].as_str())
            .map(PathBuf::from))
    }
    fn list_ours(&self) -> Result<Vec<String>, OsError> {
        Ok(self.load().into_keys().collect())
    }
}

/// In-memory scheduler for tests.
#[derive(Default)]
pub struct Fake {
    pub entries: Mutex<BTreeMap<String, Registration>>,
}

impl Scheduler for Fake {
    fn register(&self, r: &Registration) -> Result<(), OsError> {
        self.entries.lock().unwrap().insert(r.id.clone(), r.clone());
        Ok(())
    }
    fn remove(&self, id: &str) -> Result<(), OsError> {
        self.entries.lock().unwrap().remove(id);
        Ok(())
    }
    fn inspect(&self, id: &str) -> Result<Option<PathBuf>, OsError> {
        Ok(self.entries.lock().unwrap().get(id).map(|r| r.program.clone()))
    }
    fn list_ours(&self) -> Result<Vec<String>, OsError> {
        Ok(self.entries.lock().unwrap().keys().cloned().collect())
    }
}

/// Runs a system command; a non-zero exit is an error carrying its message.
pub(crate) fn run(program: &str, args: &[&str], stdin: Option<&[u8]>) -> Result<String, OsError> {
    use std::io::Write;
    let mut child = Command::new(program)
        .args(args)
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| OsError(format!("could not run {program}: {e}")))?;
    if let (Some(data), Some(mut pipe)) = (stdin, child.stdin.take()) {
        pipe.write_all(data)?;
    }
    let out = child.wait_with_output()?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        let msg = String::from_utf8_lossy(&out.stderr);
        let msg = if msg.trim().is_empty() {
            String::from_utf8_lossy(&out.stdout).into_owned()
        } else {
            msg.into_owned()
        };
        Err(OsError(format!("{program} {}: {}", args.join(" "), msg.trim())))
    }
}

pub(crate) fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

pub(crate) fn xml_unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// The text between the first `<tag>` and `</tag>`.
pub(crate) fn xml_text<'a>(doc: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("<{tag}>");
    let start = doc.find(&open)? + open.len();
    let end = doc[start..].find(&format!("</{tag}>"))? + start;
    Some(&doc[start..end])
}

#[cfg(test)]
pub(crate) fn sample() -> Registration {
    Registration {
        id: "k3f9a2qd".into(),
        name: "Client & \"A\"".into(),
        minute: 5,
        program: PathBuf::from("/Applications/SpiderLens.app/Contents/MacOS/spiderlens-desktop"),
        args: vec!["--schedule-tick".into(), "k3f9a2qd".into()],
    }
}
