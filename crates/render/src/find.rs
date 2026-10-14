//! Finding a browser (plan D1, contracts/render-crate.md "Finding and
//! getting a browser").
//!
//! Order: a path the user gave, then a `chrome-headless-shell` SpiderLens
//! downloaded earlier, then installed Chrome, Chromium and Edge in the
//! standard places for each OS. Which of "use the installed browser" and
//! "offer the download first" wins is decided in one place,
//! [`should_offer_download`], so the owner's decision can flip it cheaply.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::types::{BrowserInfo, BrowserName, BrowserSource, FindOptions, NotUsable};

/// The `chrome-headless-shell` version this release downloads (research R14).
pub const PINNED_VERSION: &str = "155.0.8059.39";

/// How many major versions below the pinned one are still accepted
/// (FR-003, "comparable version").
pub const MAJOR_TOLERANCE: u32 = 4;

/// Browser source policy (research R16 note, owner decision pending):
/// `true` prefers a downloaded `chrome-headless-shell`, which rendered about
/// five times faster than full headless Chromium in the spike, and keeps an
/// installed browser as the fallback when the user declines the download or
/// is offline. `false` restores R2 option (a): installed browser first.
pub const PREFER_DOWNLOAD: bool = true;

/// The oldest major version accepted.
pub fn min_major() -> u32 {
    major_of(PINNED_VERSION)
        .unwrap_or(0)
        .saturating_sub(MAJOR_TOLERANCE)
}

/// Whether the caller should offer the one-time download (with consent)
/// before rendering with what [`find_browser`] returned. A user path or an
/// earlier download is always used as is; an installed browser is used
/// without asking only when [`PREFER_DOWNLOAD`] is off.
pub fn should_offer_download(found: &Result<BrowserInfo, NotUsable>, download_available: bool) -> bool {
    if !download_available {
        return false;
    }
    match found {
        Ok(b) => b.source == BrowserSource::Installed && PREFER_DOWNLOAD,
        Err(_) => true,
    }
}

/// User path, then a previous download, then installed Chrome, Chromium, Edge.
/// Runs `<browser> --version` (on Windows, reads the version folder next to
/// the executable, because `chrome.exe --version` prints nothing there).
pub fn find_browser(opts: &FindOptions) -> Result<BrowserInfo, NotUsable> {
    find_in(opts, &installed_candidates())
}

/// [`find_browser`] with the installed locations given, for tests.
pub(crate) fn find_in(
    opts: &FindOptions,
    installed: &[(BrowserName, PathBuf)],
) -> Result<BrowserInfo, NotUsable> {
    if let Some(path) = &opts.user_path {
        // A path the user gave is never skipped silently.
        if !is_file(path) {
            return Err(NotUsable::Broken {
                path: path.clone(),
                message: "No browser at this path.".into(),
            });
        }
        return check(path, None, BrowserSource::UserPath);
    }
    let mut too_old = None;
    let mut broken = None;
    let downloaded = downloaded_candidates(&opts.data_dir);
    let all = downloaded
        .iter()
        .map(|p| {
            (
                Some(BrowserName::ChromeHeadlessShell),
                p,
                BrowserSource::Downloaded,
            )
        })
        .chain(
            installed
                .iter()
                .map(|(n, p)| (Some(*n), p, BrowserSource::Installed)),
        );
    for (name, path, source) in all {
        if !is_file(path) {
            continue;
        }
        match check(path, name, source) {
            Ok(info) => return Ok(info),
            Err(e @ NotUsable::TooOld { .. }) => {
                too_old.get_or_insert(e);
            }
            Err(e) => {
                broken.get_or_insert(e);
            }
        }
    }
    Err(too_old.or(broken).unwrap_or(NotUsable::NotFound))
}

fn is_file(path: &Path) -> bool {
    std::fs::metadata(path).is_ok_and(|m| m.is_file())
}

fn check(path: &Path, name: Option<BrowserName>, source: BrowserSource) -> Result<BrowserInfo, NotUsable> {
    let (version_text, version) = read_version(path).map_err(|message| NotUsable::Broken {
        path: path.to_path_buf(),
        message,
    })?;
    let major = major_of(&version).ok_or_else(|| NotUsable::Broken {
        path: path.to_path_buf(),
        message: format!("Unexpected version: {version}"),
    })?;
    let info = BrowserInfo {
        name: name.unwrap_or_else(|| name_of(path, &version_text)),
        version,
        major,
        source,
        path: path.to_path_buf(),
    };
    let min = min_major();
    if major < min {
        return Err(NotUsable::TooOld {
            found: info,
            min_major: min,
        });
    }
    Ok(info)
}

/// The browser family, from the version text or else the file name.
pub(crate) fn name_of(path: &Path, version_text: &str) -> BrowserName {
    let file = path
        .file_name()
        .map(|f| f.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let text = version_text.to_ascii_lowercase();
    if text.contains("headless shell") || file.contains("headless") {
        BrowserName::ChromeHeadlessShell
    } else if text.contains("edge") || file.contains("msedge") || file.contains("edge") {
        BrowserName::MicrosoftEdge
    } else if text.contains("chromium") || file.contains("chromium") {
        BrowserName::Chromium
    } else {
        BrowserName::GoogleChrome
    }
}

/// The first `a.b.c.d` (or `a.b.c`) number in a version text.
pub(crate) fn parse_version(text: &str) -> Option<String> {
    text.split(|c: char| c.is_whitespace() || c == ',')
        .map(|w| w.trim_start_matches(['v', 'V']))
        .find(|w| {
            let parts: Vec<_> = w.split('.').collect();
            parts.len() >= 3
                && parts
                    .iter()
                    .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
        })
        .map(str::to_owned)
}

fn major_of(version: &str) -> Option<u32> {
    version.split('.').next()?.parse().ok()
}

/// Returns the raw version text and the parsed version.
fn read_version(path: &Path) -> Result<(String, String), String> {
    if cfg!(windows)
        && let Some(v) = version_folder(path)
    {
        return Ok((String::new(), v));
    }
    // A file just written can be briefly "busy" while another thread forks.
    let mut tries = 0;
    let out = loop {
        match Command::new(path).arg("--version").output() {
            Err(e) if e.kind() == std::io::ErrorKind::ExecutableFileBusy && tries < 5 => {
                tries += 1;
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            other => break other.map_err(|e| format!("Could not run the browser: {e}"))?,
        }
    };
    let text = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    match parse_version(&text) {
        Some(v) => Ok((text, v)),
        None if !out.status.success() => Err(format!("The browser exited with {}", out.status)),
        None => Err(format!("No version in \"{text}\"")),
    }
}

/// Chrome and Edge on Windows keep each installed version in a folder named
/// after it next to the executable. The highest one is the version in use.
pub(crate) fn version_folder(exe: &Path) -> Option<String> {
    let dir = exe.parent()?;
    std::fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            (parse_version(&name).as_deref() == Some(name.as_str())).then_some(name)
        })
        .max_by_key(|v| version_key(v))
}

fn version_key(v: &str) -> Vec<u32> {
    v.split('.').filter_map(|p| p.parse().ok()).collect()
}

/// The executable name of `chrome-headless-shell` on this OS.
fn shell_exe() -> &'static str {
    if cfg!(windows) {
        "chrome-headless-shell.exe"
    } else {
        "chrome-headless-shell"
    }
}

/// Downloads live in `<data_dir>/browsers/<version>/chrome-headless-shell-<platform>/`.
/// Newest version first.
pub(crate) fn downloaded_candidates(data_dir: &Path) -> Vec<PathBuf> {
    let root = data_dir.join("browsers");
    let Ok(versions) = std::fs::read_dir(&root) else {
        return Vec::new();
    };
    let mut found: Vec<(Vec<u32>, PathBuf)> = Vec::new();
    for v in versions.filter_map(Result::ok) {
        let name = v.file_name().to_string_lossy().into_owned();
        let Ok(platforms) = std::fs::read_dir(v.path()) else {
            continue;
        };
        for p in platforms.filter_map(Result::ok) {
            let exe = p.path().join(shell_exe());
            if is_file(&exe) {
                found.push((version_key(&name), exe));
            }
        }
    }
    found.sort_by(|a, b| b.0.cmp(&a.0));
    found.into_iter().map(|(_, p)| p).collect()
}

/// Standard install locations, in the contract's order: Chrome, Chromium, Edge.
pub(crate) fn installed_candidates() -> Vec<(BrowserName, PathBuf)> {
    use BrowserName::*;
    let mut out = Vec::new();
    if cfg!(target_os = "macos") {
        let mut roots = vec![PathBuf::from("/Applications")];
        if let Some(home) = std::env::var_os("HOME") {
            roots.push(PathBuf::from(home).join("Applications"));
        }
        for (name, app) in [
            (GoogleChrome, "Google Chrome.app/Contents/MacOS/Google Chrome"),
            (Chromium, "Chromium.app/Contents/MacOS/Chromium"),
            (MicrosoftEdge, "Microsoft Edge.app/Contents/MacOS/Microsoft Edge"),
        ] {
            out.extend(roots.iter().map(|r| (name, r.join(app))));
        }
    } else if cfg!(windows) {
        let roots: Vec<PathBuf> = ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"]
            .iter()
            .filter_map(std::env::var_os)
            .map(PathBuf::from)
            .collect();
        for (name, exe) in [
            (GoogleChrome, r"Google\Chrome\Application\chrome.exe"),
            (Chromium, r"Chromium\Application\chrome.exe"),
            (MicrosoftEdge, r"Microsoft\Edge\Application\msedge.exe"),
        ] {
            out.extend(roots.iter().map(|r| (name, r.join(exe))));
        }
    } else {
        for (name, cmds, fixed) in [
            (
                GoogleChrome,
                &["google-chrome-stable", "google-chrome"][..],
                &["/opt/google/chrome/chrome"][..],
            ),
            (
                Chromium,
                &["chromium", "chromium-browser"][..],
                &["/snap/bin/chromium"][..],
            ),
            (
                MicrosoftEdge,
                &["microsoft-edge-stable", "microsoft-edge"][..],
                &["/opt/microsoft/msedge/msedge"][..],
            ),
        ] {
            out.extend(cmds.iter().filter_map(|c| on_path(c)).map(|p| (name, p)));
            out.extend(fixed.iter().map(|p| (name, PathBuf::from(p))));
        }
    }
    out
}

fn on_path(cmd: &str) -> Option<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|d| d.join(cmd))
        .find(|p| is_file(p))
}

/// The SpiderLens data folder shared by the CLI and the desktop app. The
/// same folder as Tauri's `app_data_dir` for `com.ryware.spiderlens`:
/// Windows `%APPDATA%\com.ryware.spiderlens`, macOS
/// `~/Library/Application Support/com.ryware.spiderlens`, Linux
/// `$XDG_DATA_HOME/com.ryware.spiderlens` (default `~/.local/share`).
pub fn data_dir() -> Option<PathBuf> {
    data_dir_from(|k| std::env::var_os(k))
}

pub(crate) const APP_ID: &str = "com.ryware.spiderlens";

pub(crate) fn data_dir_from(env: impl Fn(&str) -> Option<std::ffi::OsString>) -> Option<PathBuf> {
    let abs = |k: &str| env(k).map(PathBuf::from).filter(|p| p.is_absolute());
    let base = if cfg!(windows) {
        abs("APPDATA")?
    } else if cfg!(target_os = "macos") {
        abs("HOME")?.join("Library/Application Support")
    } else {
        abs("XDG_DATA_HOME").or_else(|| abs("HOME").map(|h| h.join(".local/share")))?
    };
    Some(base.join(APP_ID))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(dir: &Path) -> FindOptions {
        FindOptions {
            user_path: None,
            data_dir: dir.to_path_buf(),
        }
    }

    /// A fake browser: a shell script printing a version.
    #[cfg(unix)]
    fn fake(path: &Path, output: &str) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, format!("#!/bin/sh\necho '{output}'\n")).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[test]
    fn versions_are_parsed() {
        assert_eq!(
            parse_version("Google Chrome 155.0.8059.39 ").as_deref(),
            Some("155.0.8059.39")
        );
        assert_eq!(
            parse_version("Chromium 141.0.7390.37 snap").as_deref(),
            Some("141.0.7390.37")
        );
        assert_eq!(
            parse_version("Microsoft Edge 154.0.3100.12").as_deref(),
            Some("154.0.3100.12")
        );
        assert_eq!(parse_version("Chrome Headless Shell"), None);
        assert_eq!(major_of("155.0.8059.39"), Some(155));
        assert_eq!(min_major(), 151);
    }

    #[test]
    fn names_come_from_the_version_text_or_file() {
        let p = Path::new("/x/headless_shell");
        assert_eq!(name_of(p, "Chromium 141.0"), BrowserName::ChromeHeadlessShell);
        assert_eq!(
            name_of(Path::new("/x/chrome"), "Google Chrome 155.0"),
            BrowserName::GoogleChrome
        );
        assert_eq!(name_of(Path::new("/x/msedge"), ""), BrowserName::MicrosoftEdge);
        assert_eq!(name_of(Path::new("/x/chromium"), ""), BrowserName::Chromium);
    }

    #[test]
    fn windows_version_folder_is_the_highest() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("chrome.exe");
        std::fs::write(&exe, b"").unwrap();
        for v in ["154.0.1.2", "155.0.8059.39", "155.0.10.1", "Locales"] {
            std::fs::create_dir(dir.path().join(v)).unwrap();
        }
        assert_eq!(version_folder(&exe).as_deref(), Some("155.0.8059.39"));
    }

    #[test]
    fn data_dir_matches_tauri_app_data_dir() {
        let env = |pairs: &'static [(&'static str, &'static str)]| {
            move |k: &str| {
                pairs
                    .iter()
                    .find(|(key, _)| *key == k)
                    .map(|(_, v)| std::ffi::OsString::from(v))
            }
        };
        if cfg!(windows) {
            let got = data_dir_from(env(&[("APPDATA", r"C:\Users\a\AppData\Roaming")]));
            assert_eq!(
                got,
                Some(PathBuf::from(r"C:\Users\a\AppData\Roaming\com.ryware.spiderlens"))
            );
        } else if cfg!(target_os = "macos") {
            let got = data_dir_from(env(&[("HOME", "/Users/a")]));
            assert_eq!(
                got,
                Some(PathBuf::from(
                    "/Users/a/Library/Application Support/com.ryware.spiderlens"
                ))
            );
        } else {
            let got = data_dir_from(env(&[("HOME", "/home/a")]));
            assert_eq!(
                got,
                Some(PathBuf::from("/home/a/.local/share/com.ryware.spiderlens"))
            );
            let got = data_dir_from(env(&[("HOME", "/home/a"), ("XDG_DATA_HOME", "/data")]));
            assert_eq!(got, Some(PathBuf::from("/data/com.ryware.spiderlens")));
            // A relative XDG_DATA_HOME is ignored, as the XDG spec says.
            let got = data_dir_from(env(&[("HOME", "/home/a"), ("XDG_DATA_HOME", "rel")]));
            assert_eq!(
                got,
                Some(PathBuf::from("/home/a/.local/share/com.ryware.spiderlens"))
            );
            assert_eq!(data_dir_from(env(&[])), None);
        }
    }

    #[test]
    fn nothing_found_is_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let installed = vec![(BrowserName::GoogleChrome, dir.path().join("missing"))];
        assert_eq!(find_in(&opts(dir.path()), &installed), Err(NotUsable::NotFound));
    }

    #[test]
    fn a_missing_user_path_is_broken_not_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let mut o = opts(dir.path());
        o.user_path = Some(dir.path().join("nope"));
        assert!(matches!(find_in(&o, &[]), Err(NotUsable::Broken { .. })));
    }

    #[test]
    fn download_offer_follows_the_policy() {
        let installed = BrowserInfo {
            name: BrowserName::GoogleChrome,
            version: "155.0.1.1".into(),
            major: 155,
            source: BrowserSource::Installed,
            path: PathBuf::from("/x/chrome"),
        };
        let downloaded = BrowserInfo {
            source: BrowserSource::Downloaded,
            ..installed.clone()
        };
        assert_eq!(
            should_offer_download(&Ok(installed.clone()), true),
            PREFER_DOWNLOAD
        );
        assert!(!should_offer_download(&Ok(downloaded), true));
        assert!(should_offer_download(&Err(NotUsable::NotFound), true));
        assert!(!should_offer_download(&Err(NotUsable::NotFound), false));
        assert!(!should_offer_download(&Ok(installed), false));
    }

    #[cfg(unix)]
    #[test]
    fn order_is_user_path_then_download_then_installed() {
        let dir = tempfile::tempdir().unwrap();
        let chrome = dir.path().join("inst/google-chrome");
        let edge = dir.path().join("inst/msedge");
        fake(&chrome, "Google Chrome 155.0.8059.39");
        fake(&edge, "Microsoft Edge 154.0.1.1");
        let installed = vec![
            (BrowserName::GoogleChrome, dir.path().join("inst/absent")),
            (BrowserName::GoogleChrome, chrome.clone()),
            (BrowserName::MicrosoftEdge, edge.clone()),
        ];
        let found = find_in(&opts(dir.path()), &installed).unwrap();
        assert_eq!(found.path, chrome);
        assert_eq!(found.name, BrowserName::GoogleChrome);
        assert_eq!(found.source, BrowserSource::Installed);
        assert_eq!((found.version.as_str(), found.major), ("155.0.8059.39", 155));

        // A previous download wins over an installed browser; the newest one.
        let data = dir.path().join("data");
        let old = data.join("browsers/153.0.1.1/chrome-headless-shell-linux64/chrome-headless-shell");
        let new = data.join("browsers/155.0.8059.39/chrome-headless-shell-linux64/chrome-headless-shell");
        fake(&old, "Chrome Headless Shell 153.0.1.1");
        fake(&new, "Chrome Headless Shell 155.0.8059.39");
        let found = find_in(&opts(&data), &installed).unwrap();
        assert_eq!(found.path, new);
        assert_eq!(found.name, BrowserName::ChromeHeadlessShell);
        assert_eq!(found.source, BrowserSource::Downloaded);

        // A user path wins over everything, and its name is read from it.
        let mut o = opts(&data);
        o.user_path = Some(edge.clone());
        let found = find_in(&o, &installed).unwrap();
        assert_eq!(found.source, BrowserSource::UserPath);
        assert_eq!(found.name, BrowserName::MicrosoftEdge);
    }

    #[cfg(unix)]
    #[test]
    fn too_old_is_reported_with_what_was_found() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("chromium");
        fake(&old, "Chromium 141.0.7390.37");
        let installed = vec![(BrowserName::Chromium, old.clone())];
        match find_in(&opts(dir.path()), &installed) {
            Err(NotUsable::TooOld { found, min_major }) => {
                assert_eq!(found.major, 141);
                assert_eq!(found.path, old);
                assert_eq!(min_major, 151);
            }
            other => panic!("expected TooOld, got {other:?}"),
        }
        // A newer browser later in the list is still used.
        let newer = dir.path().join("msedge");
        fake(&newer, "Microsoft Edge 152.0.1.1");
        let installed = vec![
            (BrowserName::Chromium, old),
            (BrowserName::MicrosoftEdge, newer.clone()),
        ];
        assert_eq!(find_in(&opts(dir.path()), &installed).unwrap().path, newer);
        // The oldest accepted major is exactly the pin minus four.
        let edge = dir.path().join("edge151");
        fake(&edge, "Microsoft Edge 151.0.0.1");
        assert!(find_in(&opts(dir.path()), &[(BrowserName::MicrosoftEdge, edge)]).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn a_browser_without_a_version_is_broken() {
        let dir = tempfile::tempdir().unwrap();
        let bad = dir.path().join("chrome");
        fake(&bad, "hello");
        match find_in(&opts(dir.path()), &[(BrowserName::GoogleChrome, bad.clone())]) {
            Err(NotUsable::Broken { path, .. }) => assert_eq!(path, bad),
            other => panic!("expected Broken, got {other:?}"),
        }
    }
}
