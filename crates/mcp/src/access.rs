//! Which crawl files an assistant may read, and their ids (spec 016 FR-021,
//! FR-025; contracts/mcp-tools.md "Crawl ids").
//!
//! The settings file is read again on every call, so turning access off in
//! the app takes effect at the assistant's next request.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use spiderlens_config::{AppDirs, AssistantSettings, AssistantStore, RecentStore};

use crate::ToolError;

/// Command-line additions to `assistants.json`, for this process only.
#[derive(Debug, Clone, Default)]
pub struct Overrides {
    pub allow_read: bool,
    pub folders: Vec<PathBuf>,
}

pub(crate) const READ_OFF: &str = "SpiderLens read access is off. Turn on Settings › AI assistants › Allow AI assistants to read my crawls.";
pub(crate) const NOT_ALLOWED: &str = "This file is not in your recent crawls or an allowed folder.";

pub(crate) struct Access {
    dirs: Option<AppDirs>,
    overrides: Overrides,
}

/// A crawl file an assistant may read.
#[derive(Debug, Clone)]
pub(crate) struct Allowed {
    pub id: String,
    pub path: PathBuf,
    /// In the app's folder for unsaved crawls.
    pub unsaved: bool,
}

impl Access {
    pub fn new(dirs: Option<AppDirs>, overrides: Overrides) -> Self {
        Self { dirs, overrides }
    }

    pub fn dirs(&self) -> Option<&AppDirs> {
        self.dirs.as_ref()
    }

    pub fn settings(&self) -> AssistantSettings {
        let mut s = self
            .dirs
            .as_ref()
            .and_then(|d| AssistantStore::open(&d.config).ok())
            .map(|st| st.load())
            .unwrap_or_default();
        s.allow_read |= self.overrides.allow_read;
        s.allowed_folders.extend(self.overrides.folders.iter().cloned());
        s
    }

    /// Fails with the "read access is off" message when it is.
    pub fn check_read(&self) -> Result<AssistantSettings, ToolError> {
        let s = self.settings();
        if s.allow_read {
            Ok(s)
        } else {
            Err(ToolError::refused(READ_OFF))
        }
    }

    /// Recent crawls first (most recently used first), then crawl files in
    /// allowed folders, one level deep. Missing files are left out.
    pub fn list(&self, settings: &AssistantSettings) -> Vec<Allowed> {
        let unsaved_dir = self
            .dirs
            .as_ref()
            .and_then(|d| d.unsaved_crawls().canonicalize().ok());
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        let mut push = |p: &Path| {
            let Ok(canon) = p.canonicalize() else { return };
            if !canon.is_file() || !seen.insert(canon.clone()) {
                return;
            }
            out.push(Allowed {
                id: crawl_id(&canon),
                unsaved: unsaved_dir.as_deref().is_some_and(|d| canon.parent() == Some(d)),
                path: canon,
            });
        };
        if let Some(d) = &self.dirs
            && let Ok(recent) = RecentStore::open(&d.config)
        {
            for e in recent.list() {
                push(&e.path);
            }
        }
        for folder in &settings.allowed_folders {
            let Ok(entries) = std::fs::read_dir(folder) else {
                continue;
            };
            let mut files: Vec<PathBuf> = entries
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().is_some_and(|x| x == "spiderlens"))
                .collect();
            files.sort();
            for f in files {
                push(&f);
            }
        }
        out
    }

    /// A crawl id from `list_crawls`, or a path that is a recent entry or
    /// inside an allowed folder (FR-025).
    pub fn resolve(&self, settings: &AssistantSettings, crawl: &str) -> Result<Allowed, ToolError> {
        let allowed = self.list(settings);
        if let Some(a) = allowed.iter().find(|a| a.id == crawl) {
            return Ok(a.clone());
        }
        if crawl.starts_with("c_") && !crawl.contains(['/', '\\']) {
            return Err(ToolError::invalid(
                "No crawl has this id. Call list_crawls for the current ids.",
            ));
        }
        let Ok(canon) = Path::new(crawl).canonicalize() else {
            return Err(ToolError::refused(NOT_ALLOWED));
        };
        if let Some(a) = allowed.into_iter().find(|a| a.path == canon) {
            return Ok(a);
        }
        Err(ToolError::refused(NOT_ALLOWED))
    }
}

/// `c_` and 8 hex characters of a hash of the canonical path: stable for a
/// file, and it does not reveal the path.
pub(crate) fn crawl_id(canonical: &Path) -> String {
    format!(
        "c_{:08x}",
        (fnv1a(canonical.to_string_lossy().as_bytes()) >> 32) as u32
    )
}

pub(crate) fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}
