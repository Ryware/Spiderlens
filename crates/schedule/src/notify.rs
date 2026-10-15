//! Desktop notifications when a run ends (FR-019).
//!
//! The run is a plain background process, so notifications go through what
//! each OS offers to any program: `osascript` on macOS, a PowerShell toast on
//! Windows, `notify-send` on Linux. A notification that cannot be shown is
//! not an error; the run history has the same information.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Mutex;

use crate::model::{NotifyLevel, Run};

pub trait Notifier: Send + Sync {
    fn notify(&self, title: &str, body: &str);
}

/// Whether a run with this outcome is announced at this level.
pub fn wanted(level: NotifyLevel, run: &Run) -> bool {
    match level {
        NotifyLevel::Always => true,
        NotifyLevel::Problems => {
            run.status.is_problem() || run.steps.iter().any(|s| s.status == crate::StepStatus::Failed)
        }
        NotifyLevel::Never => false,
    }
}

pub fn text(schedule_name: &str, run: &Run) -> (String, String) {
    let title = format!("SpiderLens: {schedule_name}");
    let mut body = match (&run.reason, &run.counts) {
        (Some(reason), _) => format!("{}: {reason}", run.status.label()),
        (None, Some(c)) => format!("{}: {} URLs crawled", run.status.label(), c.crawled),
        (None, None) => run.status.label().to_string(),
    };
    if let Some(step) = run.steps.iter().find(|s| s.status == crate::StepStatus::Failed) {
        body.push_str(&format!(
            " ({} failed: {})",
            step.kind,
            step.reason.as_deref().unwrap_or("unknown")
        ));
    }
    (title, body)
}

/// The OS notifier, or a file log when `SPIDERLENS_NOTIFY_LOG` names one
/// (used by tests).
pub fn system() -> Box<dyn Notifier> {
    match std::env::var_os("SPIDERLENS_NOTIFY_LOG") {
        Some(path) => Box::new(FileLog(PathBuf::from(path))),
        None => Box::new(System),
    }
}

struct System;

fn quiet(cmd: &mut Command) {
    let _ = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

impl Notifier for System {
    fn notify(&self, title: &str, body: &str) {
        if cfg!(target_os = "macos") {
            let esc = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
            let script = format!(
                "display notification \"{}\" with title \"{}\"",
                esc(body),
                esc(title)
            );
            quiet(Command::new("osascript").args(["-e", &script]));
        } else if cfg!(windows) {
            let esc = |s: &str| {
                s.replace('&', "&amp;")
                    .replace('<', "&lt;")
                    .replace('>', "&gt;")
                    .replace('\'', "''")
            };
            let script = format!(
                "[Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] > $null; \
                 $x = New-Object Windows.Data.Xml.Dom.XmlDocument; \
                 $x.LoadXml('<toast><visual><binding template=\"ToastGeneric\"><text>{}</text><text>{}</text></binding></visual></toast>'); \
                 [Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier('{{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}}\\WindowsPowerShell\\v1.0\\powershell.exe').Show([Windows.UI.Notifications.ToastNotification]::new($x))",
                esc(title),
                esc(body)
            );
            quiet(Command::new("powershell").args([
                "-NoProfile",
                "-WindowStyle",
                "Hidden",
                "-Command",
                &script,
            ]));
        } else {
            quiet(Command::new("notify-send").args(["--app-name=SpiderLens", title, body]));
        }
    }
}

struct FileLog(PathBuf);

impl Notifier for FileLog {
    fn notify(&self, title: &str, body: &str) {
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.0)
        {
            let _ = writeln!(f, "{title}\t{body}");
        }
    }
}

/// Records notifications in memory (tests).
#[derive(Default)]
pub struct Recorder(pub Mutex<Vec<(String, String)>>);

impl Notifier for Recorder {
    fn notify(&self, title: &str, body: &str) {
        self.0.lock().unwrap().push((title.into(), body.into()));
    }
}
