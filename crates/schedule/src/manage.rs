//! Saving, enabling and removing schedules together with their OS
//! registrations (FR-007, FR-024 to FR-026). The app and the CLI both use
//! this, so they behave the same (FR-029).

use std::path::PathBuf;

use jiff::Timestamp;
use serde::Serialize;
use spiderlens_config::Issue;
use ts_rs::TS;

use crate::model::Schedule;
use crate::os::{Registration, Scheduler};
use crate::runner::Launcher;
use crate::store::{Store, StoreError, new_id};
use crate::validate::{has_errors, normalize, validate_schedule};

#[derive(Debug, thiserror::Error)]
pub enum ManageError {
    #[error("The schedule has problems: {}", .0.iter().map(|i| i.message.as_str()).collect::<Vec<_>>().join(" "))]
    Invalid(Vec<Issue>),
    #[error(transparent)]
    Store(#[from] StoreError),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "state", rename_all = "snake_case")]
#[ts(export)]
pub enum Registered {
    Registered {
        program: PathBuf,
    },
    /// Disabled: no registration on purpose.
    Suspended,
    NotRegistered {
        reason: String,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct RepairReport {
    pub repaired: Vec<String>,
    pub failed: Vec<(String, String)>,
    /// Registrations with no schedule, removed.
    pub orphans: Vec<String>,
}

pub struct Manager<'a> {
    pub store: Store,
    pub os: &'a dyn Scheduler,
    pub launcher: Launcher,
}

impl Manager<'_> {
    pub fn registration(&self, s: &Schedule) -> Option<Registration> {
        let cron = s.frequency.cron().ok()?;
        Some(Registration {
            id: s.id.clone(),
            name: s.name.clone(),
            minute: cron.minute(),
            program: self.launcher.program.clone(),
            args: self.launcher.tick_args(&s.id),
        })
    }

    fn sync(&self, s: &Schedule) -> Option<String> {
        let result = if s.enabled {
            match self.registration(s) {
                Some(r) => self.os.register(&r),
                None => return Some("The frequency is not valid.".into()),
            }
        } else {
            self.os.remove(&s.id)
        };
        result.err().map(|e| e.0)
    }

    /// Validates, saves and registers. A new schedule (empty id) gets an id.
    /// Returns the saved schedule, its warnings, and the reason the OS
    /// refused the registration if it did (the schedule is saved anyway).
    pub fn save(&self, mut s: Schedule) -> Result<(Schedule, Vec<Issue>, Option<String>), ManageError> {
        normalize(&mut s);
        let issues = validate_schedule(&s);
        if has_errors(&issues) {
            return Err(ManageError::Invalid(issues));
        }
        let now = Timestamp::now();
        if s.id.is_empty() {
            s.id = new_id(&s.name, now);
            s.created_at = now;
        }
        s.updated_at = now;
        let s = self.store.put(s)?;
        let refused = self.sync(&s);
        Ok((s, issues, refused))
    }

    pub fn set_enabled(&self, key: &str, enabled: bool) -> Result<(Schedule, Option<String>), ManageError> {
        let mut s = self.store.get(key)?;
        s.enabled = enabled;
        s.updated_at = Timestamp::now();
        let s = self.store.put(s)?;
        let refused = self.sync(&s);
        Ok((s, refused))
    }

    /// A disabled copy with a new name.
    pub fn duplicate(&self, key: &str, name: &str) -> Result<Schedule, ManageError> {
        let mut s = self.store.get(key)?;
        s.id = String::new();
        s.name = name.to_string();
        s.enabled = false;
        Ok(self.save(s)?.0)
    }

    /// Removes the schedule and its registration; its crawl files and
    /// history stay.
    pub fn delete(&self, key: &str) -> Result<Schedule, ManageError> {
        let s = self.store.get(key)?;
        let _ = self.os.remove(&s.id);
        Ok(self.store.remove(&s.id)?)
    }

    pub fn state(&self, s: &Schedule) -> Registered {
        if !s.enabled {
            return Registered::Suspended;
        }
        match self.os.inspect(&s.id) {
            Ok(Some(program)) => Registered::Registered { program },
            Ok(None) => Registered::NotRegistered {
                reason: "It is not registered with the system scheduler. Use Repair.".into(),
            },
            Err(e) => Registered::NotRegistered { reason: e.0 },
        }
    }

    /// FR-025: re-registers enabled schedules whose registration is missing
    /// or starts another program, and removes registrations of schedules
    /// that no longer exist.
    pub fn repair(&self) -> Result<RepairReport, ManageError> {
        let mut report = RepairReport::default();
        let all = self.store.list()?;
        for s in all.iter().filter(|s| s.enabled) {
            let current = self.os.inspect(&s.id).ok().flatten();
            if current.as_ref() == Some(&self.launcher.program) {
                continue;
            }
            match self.sync(s) {
                None => report.repaired.push(s.name.clone()),
                Some(reason) => report.failed.push((s.name.clone(), reason)),
            }
        }
        for id in self.os.list_ours().unwrap_or_default() {
            let keep = all.iter().any(|s| s.id == id && s.enabled);
            if !keep && self.os.remove(&id).is_ok() {
                report.orphans.push(id);
            }
        }
        Ok(report)
    }

    /// FR-026: every schedule and every registration we made, orphans too.
    pub fn remove_all(&self) -> Result<usize, ManageError> {
        let removed = self.store.remove_all()?;
        for s in &removed {
            let _ = self.os.remove(&s.id);
        }
        for id in self.os.list_ours().unwrap_or_default() {
            let _ = self.os.remove(&id);
        }
        Ok(removed.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::os::Fake;
    use crate::store::Paths;
    use crate::store::tests::sample;

    fn manager<'a>(dir: &std::path::Path, os: &'a Fake, program: &str) -> Manager<'a> {
        Manager {
            store: Store::new(Paths::new(dir)),
            os,
            launcher: Launcher {
                program: program.into(),
                tick_prefix: vec!["schedule".into(), "tick".into()],
                run_prefix: vec!["schedule".into(), "run".into()],
            },
        }
    }

    #[test]
    fn save_registers_disable_suspends_delete_removes() {
        let dir = tempfile::tempdir().unwrap();
        let os = Fake::default();
        let m = manager(dir.path(), &os, "/bin/spiderlens");
        let mut s = sample("", "Weekly");
        s.output_dir = dir.path().to_path_buf();
        let (s, warnings, refused) = m.save(s).unwrap();
        assert!(warnings.is_empty() && refused.is_none());
        assert_eq!(s.id.len(), 8);
        let reg = os.entries.lock().unwrap()[&s.id].clone();
        assert_eq!(
            (reg.minute, reg.args.clone()),
            (0, vec!["schedule".into(), "tick".into(), s.id.clone()])
        );
        assert_eq!(
            m.state(&s),
            Registered::Registered {
                program: "/bin/spiderlens".into()
            }
        );

        let (s, _) = m.set_enabled("weekly", false).unwrap();
        assert!(os.entries.lock().unwrap().is_empty());
        assert_eq!(m.state(&s), Registered::Suspended);

        let copy = m.duplicate(&s.id, "Weekly copy").unwrap();
        assert!(!copy.enabled && copy.id != s.id);
        m.delete(&s.id).unwrap();
        assert_eq!(m.store.list().unwrap().len(), 1);
    }

    #[test]
    fn invalid_is_refused_and_not_saved() {
        let dir = tempfile::tempdir().unwrap();
        let os = Fake::default();
        let m = manager(dir.path(), &os, "/bin/spiderlens");
        let s = sample("", "");
        assert!(matches!(m.save(s), Err(ManageError::Invalid(_))));
        assert!(m.store.list().unwrap().is_empty());
    }

    #[test]
    fn repair_moves_and_orphans() {
        let dir = tempfile::tempdir().unwrap();
        let os = Fake::default();
        let old = manager(dir.path(), &os, "/old/spiderlens");
        let mut s = sample("", "A");
        s.output_dir = dir.path().to_path_buf();
        let (s, ..) = old.save(s).unwrap();
        os.register(&Registration {
            id: "orphan01".into(),
            name: "gone".into(),
            minute: 1,
            program: "/old/spiderlens".into(),
            args: vec![],
        })
        .unwrap();
        let new = manager(dir.path(), &os, "/Applications/SpiderLens.app/x");
        let report = new.repair().unwrap();
        assert_eq!(report.repaired, ["A"]);
        assert_eq!(report.orphans, ["orphan01"]);
        assert_eq!(
            os.inspect(&s.id).unwrap(),
            Some("/Applications/SpiderLens.app/x".into())
        );
        // Nothing to do the second time.
        assert_eq!(new.repair().unwrap(), RepairReport::default());
        assert_eq!(new.remove_all().unwrap(), 1);
        assert!(os.entries.lock().unwrap().is_empty());
    }
}
