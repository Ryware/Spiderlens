//! Windows: one Task Scheduler task per schedule in the `\SpiderLens\` folder.

use std::path::PathBuf;

use super::{OsError, Registration, Scheduler, run, xml_escape, xml_text, xml_unescape};

const FOLDER: &str = r"\SpiderLens\";

pub fn task_name(id: &str) -> String {
    format!("{FOLDER}{id}")
}

/// Quotes one argument the way `CommandLineToArgvW` reads it back.
pub fn quote_arg(a: &str) -> String {
    if !a.is_empty() && !a.contains([' ', '\t', '"']) {
        return a.to_string();
    }
    let mut out = String::from('"');
    let mut slashes = 0;
    for c in a.chars() {
        match c {
            '\\' => slashes += 1,
            '"' => {
                out.push_str(&"\\".repeat(slashes * 2 + 1));
                out.push('"');
                slashes = 0;
            }
            _ => {
                out.push_str(&"\\".repeat(slashes));
                out.push(c);
                slashes = 0;
            }
        }
    }
    out.push_str(&"\\".repeat(slashes * 2));
    out.push('"');
    out
}

/// Every hour at the schedule's minute and at the user's logon, only while
/// the user is logged on (no stored password), on battery too, and starting
/// late when the computer was off (`StartWhenAvailable`). `Parallel` lets a
/// tick start while a run it launched is still going, so the overlap is
/// recorded as Skipped by SpiderLens (FR-013).
pub fn task_xml(r: &Registration, user: &str) -> String {
    let args: Vec<String> = r.args.iter().map(|a| quote_arg(a)).collect();
    format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo>
    <Description>SpiderLens scheduled crawl: {name}</Description>
  </RegistrationInfo>
  <Triggers>
    <CalendarTrigger>
      <Repetition>
        <Interval>PT1H</Interval>
        <Duration>P1D</Duration>
        <StopAtDurationEnd>false</StopAtDurationEnd>
      </Repetition>
      <StartBoundary>2026-01-01T00:{minute:02}:00</StartBoundary>
      <Enabled>true</Enabled>
      <ScheduleByDay>
        <DaysInterval>1</DaysInterval>
      </ScheduleByDay>
    </CalendarTrigger>
    <LogonTrigger>
      <Enabled>true</Enabled>
      <UserId>{user}</UserId>
      <Delay>PT1M</Delay>
    </LogonTrigger>
  </Triggers>
  <Principals>
    <Principal id="Author">
      <LogonType>InteractiveToken</LogonType>
      <RunLevel>LeastPrivilege</RunLevel>
    </Principal>
  </Principals>
  <Settings>
    <MultipleInstancesPolicy>Parallel</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <StartWhenAvailable>true</StartWhenAvailable>
    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>
    <Enabled>true</Enabled>
    <Hidden>false</Hidden>
  </Settings>
  <Actions Context="Author">
    <Exec>
      <Command>{command}</Command>
      <Arguments>{args}</Arguments>
    </Exec>
  </Actions>
</Task>
"#,
        name = xml_escape(&r.name),
        minute = r.minute,
        user = xml_escape(user),
        command = xml_escape(&r.program.to_string_lossy()),
        args = xml_escape(&args.join(" ")),
    )
}

/// UTF-16 little-endian with a byte order mark, as `schtasks /XML` expects.
pub fn utf16(text: &str) -> Vec<u8> {
    let mut out = vec![0xFF, 0xFE];
    for unit in text.encode_utf16() {
        out.extend_from_slice(&unit.to_le_bytes());
    }
    out
}

pub fn program_of(xml: &str) -> Option<PathBuf> {
    xml_text(xml, "Command").map(|c| PathBuf::from(xml_unescape(c.trim().trim_matches('"'))))
}

fn current_user() -> String {
    let user = std::env::var("USERNAME").unwrap_or_default();
    match std::env::var("USERDOMAIN") {
        Ok(d) if !d.is_empty() => format!("{d}\\{user}"),
        _ => user,
    }
}

pub struct TaskScheduler;

impl Scheduler for TaskScheduler {
    fn register(&self, r: &Registration) -> Result<(), OsError> {
        let dir = tempfile_dir()?;
        let file = dir.join(format!("spiderlens-task-{}.xml", r.id));
        std::fs::write(&file, utf16(&task_xml(r, &current_user())))?;
        let result = run(
            "schtasks",
            &[
                "/Create",
                "/TN",
                &task_name(&r.id),
                "/XML",
                &file.to_string_lossy(),
                "/F",
            ],
            None,
        );
        let _ = std::fs::remove_file(&file);
        result.map(|_| ())
    }

    fn remove(&self, id: &str) -> Result<(), OsError> {
        if self.inspect(id)?.is_none() {
            return Ok(());
        }
        run("schtasks", &["/Delete", "/TN", &task_name(id), "/F"], None).map(|_| ())
    }

    fn inspect(&self, id: &str) -> Result<Option<PathBuf>, OsError> {
        match run("schtasks", &["/Query", "/TN", &task_name(id), "/XML"], None) {
            Ok(xml) => Ok(program_of(&xml)),
            // schtasks reports a missing task as an error; there is no
            // separate exit code for it.
            Err(_) => Ok(None),
        }
    }

    fn list_ours(&self) -> Result<Vec<String>, OsError> {
        let out = match run("schtasks", &["/Query", "/FO", "CSV", "/NH"], None) {
            Ok(o) => o,
            Err(_) => return Ok(Vec::new()),
        };
        let mut ids: Vec<String> = out
            .lines()
            .filter_map(|l| l.split(',').next())
            .map(|n| n.trim_matches('"'))
            .filter_map(|n| n.strip_prefix(FOLDER))
            .map(str::to_string)
            .collect();
        ids.sort();
        ids.dedup();
        Ok(ids)
    }
}

fn tempfile_dir() -> Result<PathBuf, OsError> {
    Ok(std::env::temp_dir())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::os::sample;

    #[test]
    fn task_text() {
        let mut r = sample();
        r.program = PathBuf::from(r"C:\Program Files\SpiderLens\spiderlens-desktop.exe");
        let xml = task_xml(&r, r"PC\avi");
        assert!(xml.contains("<StartBoundary>2026-01-01T00:05:00</StartBoundary>"));
        assert!(xml.contains("<Interval>PT1H</Interval>"));
        assert!(xml.contains(r"<UserId>PC\avi</UserId>"));
        assert!(xml.contains("<LogonType>InteractiveToken</LogonType>"));
        assert!(xml.contains("<StartWhenAvailable>true</StartWhenAvailable>"));
        assert!(xml.contains("<Arguments>--schedule-tick k3f9a2qd</Arguments>"));
        assert!(xml.contains("Client &amp; &quot;A&quot;"));
        assert_eq!(program_of(&xml), Some(r.program));
        let bytes = utf16(&xml);
        assert_eq!(&bytes[..4], &[0xFF, 0xFE, b'<', 0]);
    }

    #[test]
    fn argument_quoting() {
        assert_eq!(quote_arg("plain"), "plain");
        assert_eq!(quote_arg("two words"), "\"two words\"");
        assert_eq!(quote_arg(r#"say "hi""#), r#""say \"hi\"""#);
        assert_eq!(quote_arg(r"C:\dir with space\"), r#""C:\dir with space\\""#);
        assert_eq!(quote_arg(""), "\"\"");
    }
}
