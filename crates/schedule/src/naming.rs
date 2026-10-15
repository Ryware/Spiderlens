//! Crawl file names for runs (FR-015).

use std::path::{Path, PathBuf};

use jiff::Zoned;

pub const EXTENSION: &str = "spiderlens";

/// `<schedule name> YYYY-MM-DD HHmm` in the slot's local time, with
/// characters that are not allowed in file names on any OS replaced by `-`.
pub fn run_stem(name: &str, at: &Zoned) -> String {
    let clean: String = name
        .trim()
        .chars()
        .map(|c| {
            if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control() {
                '-'
            } else {
                c
            }
        })
        .collect();
    // Windows refuses names ending in a dot or space.
    let clean = clean.trim_end_matches(['.', ' ']);
    format!("{clean} {}", at.strftime("%Y-%m-%d %H%M"))
}

/// The first of `stem.spiderlens`, `stem (2).spiderlens`, ... that does not
/// exist yet in `dir`. Nothing is ever overwritten.
pub fn free_path(dir: &Path, stem: &str) -> PathBuf {
    let mut n = 1;
    loop {
        let name = if n == 1 {
            format!("{stem}.{EXTENSION}")
        } else {
            format!("{stem} ({n}).{EXTENSION}")
        };
        let path = dir.join(name);
        if !path.exists() {
            return path;
        }
        n += 1;
    }
}

/// The folder for a run's after-run outputs, next to its crawl file.
pub fn exports_dir(crawl_file: &Path) -> PathBuf {
    let stem = crawl_file.file_stem().unwrap_or_default().to_string_lossy();
    crawl_file.with_file_name(format!("{stem} exports"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::date;
    use jiff::tz::TimeZone;

    #[test]
    fn names_and_suffixes() {
        let at = date(2026, 10, 19).at(7, 0, 0, 0).to_zoned(TimeZone::UTC).unwrap();
        assert_eq!(
            run_stem("Client A: weekly", &at),
            "Client A- weekly 2026-10-19 0700"
        );
        assert_eq!(run_stem("a/b\\c.", &at), "a-b-c 2026-10-19 0700");
        let dir = tempfile::tempdir().unwrap();
        let first = free_path(dir.path(), "x");
        assert!(first.ends_with("x.spiderlens"));
        std::fs::write(&first, b"").unwrap();
        let second = free_path(dir.path(), "x");
        assert!(second.ends_with("x (2).spiderlens"));
        std::fs::write(&second, b"").unwrap();
        assert!(free_path(dir.path(), "x").ends_with("x (3).spiderlens"));
        assert!(exports_dir(&second).ends_with("x (2) exports"));
    }
}
