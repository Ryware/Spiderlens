//! Checks before a schedule is saved (FR-005, FR-006).
//!
//! Issues use the same shape as crawl configuration issues (spec 002), so the
//! UI and the command line show them the same way.

use spiderlens_config::{Issue, Mode, ModeInput, Severity, validate};

use crate::model::{Schedule, ScheduleInput};

fn issue(path: &str, severity: Severity, code: &str, message: impl Into<String>) -> Issue {
    Issue {
        path: path.into(),
        severity,
        code: code.into(),
        message: message.into(),
    }
}

/// Makes the configuration's mode follow the input, so the two cannot disagree.
pub fn normalize(s: &mut Schedule) {
    s.name = s.name.trim().to_string();
    s.config.mode = match s.input {
        ScheduleInput::Spider { .. } => Mode::Spider,
        ScheduleInput::List { .. } => Mode::List,
    };
}

pub fn validate_schedule(s: &Schedule) -> Vec<Issue> {
    let mut out = Vec::new();
    let name = s.name.trim();
    if name.is_empty() {
        out.push(issue(
            "name",
            Severity::Error,
            "name_empty",
            "Give the schedule a name.",
        ));
    } else if name.chars().count() > 80 {
        out.push(issue(
            "name",
            Severity::Error,
            "name_long",
            "Keep the name to 80 characters or fewer.",
        ));
    }
    if let Err(e) = s.frequency.cron() {
        out.push(issue(
            "frequency",
            Severity::Error,
            "frequency_invalid",
            e.to_string(),
        ));
    }
    if !s.output_dir.is_absolute() {
        out.push(issue(
            "output_dir",
            Severity::Error,
            "output_relative",
            "Choose a full folder path for the crawl files.",
        ));
    } else if !s.output_dir.is_dir() {
        out.push(issue(
            "output_dir",
            Severity::Warning,
            "output_missing",
            format!(
                "The folder {} does not exist now; runs fail until it does.",
                s.output_dir.display()
            ),
        ));
    }
    for (i, h) in s.config.identity.headers.iter().enumerate() {
        if h.value.is_secret() {
            out.push(issue(
                &format!("config.identity.headers[{i}]"),
                Severity::Error,
                "secret_header",
                format!(
                    "The header \"{}\" is marked secret. Secret values are never stored, so a run without \
                     you could not send it. Remove it or unmark it to schedule this crawl.",
                    h.name
                ),
            ));
        }
    }
    match &s.input {
        ScheduleInput::Spider { start_url } => {
            let input = ModeInput::Spider {
                start_url: start_url.clone(),
            };
            out.extend(validate(&s.config, &input));
        }
        ScheduleInput::List { path } => {
            out.extend(spiderlens_config::validate_config(&s.config));
            if !path.is_file() {
                out.push(issue(
                    "input.path",
                    Severity::Warning,
                    "list_missing",
                    format!(
                        "The list file {} does not exist now; runs fail until it does.",
                        path.display()
                    ),
                ));
            }
        }
    }
    out
}

pub fn has_errors(issues: &[Issue]) -> bool {
    issues.iter().any(|i| i.severity == Severity::Error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frequency::Frequency;
    use crate::store::tests::sample;
    use spiderlens_config::{CustomHeader, HeaderValue};

    fn codes(s: &Schedule) -> Vec<String> {
        validate_schedule(s).into_iter().map(|i| i.code).collect()
    }

    #[test]
    fn checks() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = sample("a", "A");
        s.output_dir = dir.path().to_path_buf();
        assert!(codes(&s).is_empty(), "{:?}", validate_schedule(&s));

        s.name = " ".into();
        s.output_dir = "relative".into();
        s.frequency = Frequency::Advanced {
            cron: "*/5 * * * *".into(),
        };
        assert_eq!(codes(&s), ["name_empty", "frequency_invalid", "output_relative"]);

        let mut s = sample("a", "A");
        s.output_dir = dir.path().join("missing");
        s.config.identity.headers.push(CustomHeader {
            name: "Authorization".into(),
            value: HeaderValue::secret("x"),
            send_to: Default::default(),
        });
        let issues = validate_schedule(&s);
        assert!(has_errors(&issues));
        assert_eq!(codes(&s), ["output_missing", "secret_header"]);
    }

    #[test]
    fn mode_follows_input() {
        let mut s = sample("a", " A ");
        s.input = ScheduleInput::List {
            path: "/x.txt".into(),
        };
        normalize(&mut s);
        assert_eq!((s.config.mode, s.name.as_str()), (Mode::List, "A"));
    }
}
