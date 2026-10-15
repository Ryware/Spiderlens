//! macOS: one LaunchAgent per schedule in `~/Library/LaunchAgents`.

use std::fs;
use std::path::{Path, PathBuf};

use super::{LABEL, OsError, Registration, Scheduler, run, xml_escape, xml_text, xml_unescape};

pub fn label(id: &str) -> String {
    format!("{LABEL}.{id}")
}

/// Hourly at the schedule's minute, at load (login), and launchd does not
/// kill a run the tick started when the tick exits.
pub fn plist(r: &Registration) -> String {
    let mut args = format!(
        "        <string>{}</string>\n",
        xml_escape(&r.program.to_string_lossy())
    );
    for a in &r.args {
        args.push_str(&format!("        <string>{}</string>\n", xml_escape(a)));
    }
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{label}</string>
    <key>Comment</key>
    <string>SpiderLens scheduled crawl: {name}</string>
    <key>ProgramArguments</key>
    <array>
{args}    </array>
    <key>StartCalendarInterval</key>
    <dict>
        <key>Minute</key>
        <integer>{minute}</integer>
    </dict>
    <key>RunAtLoad</key>
    <true/>
    <key>AbandonProcessGroup</key>
    <true/>
    <key>ProcessType</key>
    <string>Background</string>
    <key>StandardOutPath</key>
    <string>/dev/null</string>
    <key>StandardErrorPath</key>
    <string>/dev/null</string>
</dict>
</plist>
"#,
        label = label(&r.id),
        name = xml_escape(&r.name),
        minute = r.minute,
    )
}

/// The program in a plist we wrote: the first `<string>` of ProgramArguments.
pub fn program_of(plist: &str) -> Option<PathBuf> {
    let after = &plist[plist.find("<key>ProgramArguments</key>")?..];
    xml_text(after, "string").map(|s| PathBuf::from(xml_unescape(s)))
}

pub struct Launchd {
    dir: PathBuf,
}

impl Launchd {
    pub fn new() -> Self {
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
        Self {
            dir: home.join("Library/LaunchAgents"),
        }
    }

    fn path(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{}.plist", label(id)))
    }

    fn domain() -> Result<String, OsError> {
        let uid = run("id", &["-u"], None)?;
        Ok(format!("gui/{}", uid.trim()))
    }
}

impl Default for Launchd {
    fn default() -> Self {
        Self::new()
    }
}

fn bootout(domain: &str, path: &Path) {
    // Not loaded is fine.
    let _ = run("launchctl", &["bootout", domain, &path.to_string_lossy()], None);
}

impl Scheduler for Launchd {
    fn register(&self, r: &Registration) -> Result<(), OsError> {
        fs::create_dir_all(&self.dir)?;
        let path = self.path(&r.id);
        let domain = Self::domain()?;
        bootout(&domain, &path);
        fs::write(&path, plist(r))?;
        run(
            "launchctl",
            &["bootstrap", &domain, &path.to_string_lossy()],
            None,
        )
        .map(|_| ())
    }

    fn remove(&self, id: &str) -> Result<(), OsError> {
        let path = self.path(id);
        if let Ok(domain) = Self::domain() {
            bootout(&domain, &path);
        }
        match fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
            _ => Ok(()),
        }
    }

    fn inspect(&self, id: &str) -> Result<Option<PathBuf>, OsError> {
        match fs::read_to_string(self.path(id)) {
            Ok(text) => Ok(program_of(&text)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    fn list_ours(&self) -> Result<Vec<String>, OsError> {
        let prefix = format!("{LABEL}.");
        let mut ids = Vec::new();
        let Ok(entries) = fs::read_dir(&self.dir) else {
            return Ok(ids);
        };
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if let Some(id) = name.strip_prefix(&prefix).and_then(|n| n.strip_suffix(".plist")) {
                ids.push(id.to_string());
            }
        }
        Ok(ids)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::os::sample;

    #[test]
    fn plist_text() {
        let text = plist(&sample());
        assert!(text.contains("<string>com.ryware.spiderlens.schedule.k3f9a2qd</string>"));
        assert!(text.contains("Client &amp; &quot;A&quot;"));
        assert!(text.contains("<key>Minute</key>\n        <integer>5</integer>"));
        assert!(text.contains("<string>--schedule-tick</string>\n        <string>k3f9a2qd</string>"));
        assert!(text.contains("<key>RunAtLoad</key>\n    <true/>"));
        assert!(text.contains("<key>AbandonProcessGroup</key>\n    <true/>"));
        assert_eq!(program_of(&text), Some(sample().program));
    }
}
