//! Opens a link in the system browser, for Google sign-in (plan D3). The
//! desktop app and the CLI both use it; the CLI also prints the link.

/// Returns false when no browser could be started.
pub fn open(url: &str) -> bool {
    #[cfg(target_os = "macos")]
    let r = std::process::Command::new("open").arg(url).spawn();
    #[cfg(target_os = "windows")]
    let r = std::process::Command::new("rundll32")
        .args(["url.dll,FileProtocolHandler", url])
        .spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let r = std::process::Command::new("xdg-open").arg(url).spawn();
    r.is_ok()
}
