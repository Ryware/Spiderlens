//! Linux: a systemd user timer per schedule, or tagged lines in the user's
//! crontab when systemd user sessions are not available.

use std::fs;
use std::path::PathBuf;

use super::{OsError, Registration, Scheduler, run};

pub fn unit(id: &str) -> String {
    format!("spiderlens-schedule-{id}")
}

/// One argument for `ExecStart=`: quoted, with `%` doubled for systemd.
fn systemd_arg(a: &str) -> String {
    let escaped = a.replace('\\', "\\\\").replace('"', "\\\"").replace('%', "%%");
    format!("\"{escaped}\"")
}

/// `KillMode=process` keeps the run the tick starts alive after the tick exits.
pub fn service(r: &Registration) -> String {
    let mut exec = systemd_arg(&r.program.to_string_lossy());
    for a in &r.args {
        exec.push(' ');
        exec.push_str(&systemd_arg(a));
    }
    format!(
        "[Unit]\nDescription=SpiderLens scheduled crawl: {}\n\n[Service]\nType=oneshot\nExecStart={exec}\nKillMode=process\n",
        r.name.replace('\n', " ")
    )
}

pub fn timer(r: &Registration) -> String {
    format!(
        "[Unit]\nDescription=SpiderLens scheduled crawl: {}\n\n[Timer]\nOnCalendar=*-*-* *:{:02}:00\nOnStartupSec=1min\nPersistent=true\n\n[Install]\nWantedBy=timers.target\n",
        r.name.replace('\n', " "),
        r.minute
    )
}

/// The program in a service file we wrote.
pub fn program_of_service(text: &str) -> Option<PathBuf> {
    let exec = text.lines().find_map(|l| l.strip_prefix("ExecStart="))?;
    let rest = exec.strip_prefix('"')?;
    let mut out = String::new();
    let mut chars = rest.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => out.extend(chars.next()),
            '"' => break,
            c => out.push(c),
        }
    }
    Some(PathBuf::from(out.replace("%%", "%")))
}

fn tag(id: &str) -> String {
    format!("# spiderlens-schedule {id}")
}

fn shell_quote(a: &str) -> String {
    format!("'{}'", a.replace('\'', r"'\''"))
}

/// The crontab with this schedule's lines replaced (or removed when `r` is
/// `None`). Lines that are not ours are kept exactly.
pub fn merge_crontab(existing: &str, id: &str, r: Option<&Registration>) -> String {
    let tag = tag(id);
    let mut out: Vec<String> = existing
        .lines()
        .filter(|l| !l.trim_end().ends_with(&tag))
        .map(str::to_string)
        .collect();
    if let Some(r) = r {
        let mut cmd = shell_quote(&r.program.to_string_lossy());
        for a in &r.args {
            cmd.push(' ');
            cmd.push_str(&shell_quote(a));
        }
        // cron treats % as a newline unless escaped.
        let cmd = cmd.replace('%', "\\%");
        out.push(format!("{} * * * * {cmd} >/dev/null 2>&1 {tag}", r.minute));
        out.push(format!("@reboot {cmd} >/dev/null 2>&1 {tag}"));
    }
    let mut text = out.join("\n");
    if !text.is_empty() {
        text.push('\n');
    }
    text
}

pub fn program_of_cron(crontab: &str, id: &str) -> Option<PathBuf> {
    let tag = tag(id);
    let line = crontab.lines().find(|l| l.trim_end().ends_with(&tag))?;
    let start = line.find('\'')? + 1;
    let end = line[start..].find('\'')? + start;
    Some(PathBuf::from(&line[start..end]))
}

pub fn ids_in_crontab(crontab: &str) -> Vec<String> {
    let mut ids: Vec<String> = crontab
        .lines()
        .filter_map(|l| l.split("# spiderlens-schedule ").nth(1))
        .map(|id| id.trim().to_string())
        .collect();
    ids.sort();
    ids.dedup();
    ids
}

pub struct Systemd {
    dir: PathBuf,
}

impl Systemd {
    pub fn new() -> Self {
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .unwrap_or_default();
        Self {
            dir: config.join("systemd/user"),
        }
    }

    fn available() -> bool {
        run("systemctl", &["--user", "show-environment"], None).is_ok()
    }

    fn crontab() -> String {
        // "no crontab for user" is an error exit with nothing to keep.
        run("crontab", &["-l"], None).unwrap_or_default()
    }

    fn write_crontab(text: &str) -> Result<(), OsError> {
        run("crontab", &["-"], Some(text.as_bytes())).map(|_| ())
    }
}

impl Default for Systemd {
    fn default() -> Self {
        Self::new()
    }
}

impl Scheduler for Systemd {
    fn register(&self, r: &Registration) -> Result<(), OsError> {
        let name = unit(&r.id);
        if Self::available() {
            fs::create_dir_all(&self.dir)?;
            fs::write(self.dir.join(format!("{name}.service")), service(r))?;
            fs::write(self.dir.join(format!("{name}.timer")), timer(r))?;
            run("systemctl", &["--user", "daemon-reload"], None)?;
            run(
                "systemctl",
                &["--user", "enable", "--now", &format!("{name}.timer")],
                None,
            )?;
            // A crontab entry from an earlier fallback would run it twice.
            let existing = Self::crontab();
            if !ids_in_crontab(&existing).is_empty() {
                Self::write_crontab(&merge_crontab(&existing, &r.id, None))?;
            }
            Ok(())
        } else {
            Self::write_crontab(&merge_crontab(&Self::crontab(), &r.id, Some(r)))
        }
    }

    fn remove(&self, id: &str) -> Result<(), OsError> {
        let name = unit(id);
        let timer = self.dir.join(format!("{name}.timer"));
        if timer.exists() {
            let _ = run(
                "systemctl",
                &["--user", "disable", "--now", &format!("{name}.timer")],
                None,
            );
            fs::remove_file(&timer)?;
            let _ = fs::remove_file(self.dir.join(format!("{name}.service")));
            let _ = run("systemctl", &["--user", "daemon-reload"], None);
        }
        let existing = Self::crontab();
        if ids_in_crontab(&existing).iter().any(|i| i == id) {
            Self::write_crontab(&merge_crontab(&existing, id, None))?;
        }
        Ok(())
    }

    fn inspect(&self, id: &str) -> Result<Option<PathBuf>, OsError> {
        let name = unit(id);
        if self.dir.join(format!("{name}.timer")).exists()
            && let Ok(text) = fs::read_to_string(self.dir.join(format!("{name}.service")))
        {
            return Ok(program_of_service(&text));
        }
        Ok(program_of_cron(&Self::crontab(), id))
    }

    fn list_ours(&self) -> Result<Vec<String>, OsError> {
        let prefix = "spiderlens-schedule-";
        let mut ids = ids_in_crontab(&Self::crontab());
        if let Ok(entries) = fs::read_dir(&self.dir) {
            for e in entries.flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                if let Some(id) = name.strip_prefix(prefix).and_then(|n| n.strip_suffix(".timer")) {
                    ids.push(id.to_string());
                }
            }
        }
        ids.sort();
        ids.dedup();
        Ok(ids)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::os::sample;

    #[test]
    fn unit_files() {
        let mut r = sample();
        r.program = PathBuf::from("/opt/Spider Lens/spiderlens");
        r.args = vec!["schedule".into(), "tick".into(), "100%".into()];
        let s = service(&r);
        assert!(s.contains("ExecStart=\"/opt/Spider Lens/spiderlens\" \"schedule\" \"tick\" \"100%%\""));
        assert!(s.contains("KillMode=process"));
        assert_eq!(program_of_service(&s), Some(r.program.clone()));
        let t = timer(&r);
        assert!(t.contains("OnCalendar=*-*-* *:05:00"));
        assert!(t.contains("Persistent=true"));
        assert!(t.contains("OnStartupSec=1min"));
    }

    #[test]
    fn crontab_merge_keeps_other_lines() {
        let mut r = sample();
        r.program = PathBuf::from("/usr/bin/spiderlens");
        r.args = vec!["schedule".into(), "tick".into(), r.id.clone()];
        let existing = "MAILTO=me\n0 3 * * * backup.sh\n";
        let with = merge_crontab(existing, &r.id, Some(&r));
        assert_eq!(
            with,
            "MAILTO=me\n0 3 * * * backup.sh\n\
             5 * * * * '/usr/bin/spiderlens' 'schedule' 'tick' 'k3f9a2qd' >/dev/null 2>&1 # spiderlens-schedule k3f9a2qd\n\
             @reboot '/usr/bin/spiderlens' 'schedule' 'tick' 'k3f9a2qd' >/dev/null 2>&1 # spiderlens-schedule k3f9a2qd\n"
        );
        // Registering again replaces, never duplicates.
        assert_eq!(merge_crontab(&with, &r.id, Some(&r)), with);
        assert_eq!(ids_in_crontab(&with), ["k3f9a2qd"]);
        assert_eq!(program_of_cron(&with, &r.id), Some(r.program.clone()));
        assert_eq!(merge_crontab(&with, &r.id, None), existing);
        assert_eq!(merge_crontab("", &r.id, None), "");
    }
}
