//! Injects the build identifier at compile time (research R9).
//!
//! Commit: SPIDERLENS_BUILD_SHA (CI sets it to the pull request head, because
//! GITHUB_SHA is a temporary merge commit there), then GITHUB_SHA, then
//! `git rev-parse HEAD`.
//! Build id: SPIDERLENS_BUILD_ID override, else `<short sha>.<run number>` in
//! CI, `<short sha>` locally, or "dev" outside git.

use std::process::Command;

fn main() {
    for var in [
        "SPIDERLENS_BUILD_ID",
        "SPIDERLENS_BUILD_SHA",
        "GITHUB_SHA",
        "GITHUB_RUN_NUMBER",
    ] {
        println!("cargo:rerun-if-env-changed={var}");
    }

    let commit = ["SPIDERLENS_BUILD_SHA", "GITHUB_SHA"]
        .into_iter()
        .find_map(|var| std::env::var(var).ok().filter(|s| !s.is_empty()))
        .or_else(git_head);

    let build_id = std::env::var("SPIDERLENS_BUILD_ID")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| match (&commit, std::env::var("GITHUB_RUN_NUMBER")) {
            (Some(sha), Ok(run)) if !run.is_empty() => format!("{}.{run}", short(sha)),
            (Some(sha), _) => short(sha).to_string(),
            (None, _) => "dev".to_string(),
        });

    println!("cargo:rustc-env=SPIDERLENS_BUILD_ID={build_id}");
    println!("cargo:rustc-env=SPIDERLENS_COMMIT={}", commit.unwrap_or_default());
}

fn short(sha: &str) -> &str {
    &sha[..sha.len().min(7)]
}

fn git_head() -> Option<String> {
    let out = Command::new("git").args(["rev-parse", "HEAD"]).output().ok()?;
    if !out.status.success() {
        return None;
    }
    // Rebuild when HEAD moves, so local builds carry the right id. A pull or
    // commit on the same branch leaves HEAD alone and moves the branch ref
    // (or packed-refs), so watch those too.
    if let Some(dir) = git(&["rev-parse", "--git-dir"]) {
        println!("cargo:rerun-if-changed={dir}/HEAD");
    }
    if let Some(common) = git(&["rev-parse", "--git-common-dir"]) {
        let mut watch = vec![format!("{common}/packed-refs")];
        if let Some(branch) = git(&["symbolic-ref", "-q", "HEAD"]) {
            watch.push(format!("{common}/{branch}"));
        }
        // A missing path would make cargo rerun this script on every build.
        for path in watch.into_iter().filter(|p| std::path::Path::new(p).exists()) {
            println!("cargo:rerun-if-changed={path}");
        }
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
}
