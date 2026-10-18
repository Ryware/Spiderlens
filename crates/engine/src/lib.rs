//! SpiderLens crawl engine.
//!
//! This crate must never depend on Tauri or any UI code (constitution,
//! principle III). The CLI and the desktop app are thin layers on top of it.

pub mod accessibility;
mod build_info;
mod check;
mod config;
pub mod crawl;
pub mod custom;
mod error;
pub mod extract;
mod fetch;
pub mod net;
pub mod pdf;
mod result;
pub mod robots;
mod title;

pub use build_info::BuildInfo;
pub use check::{check_page, parse_input_url};
pub use config::{CheckOptions, RequestHeader};
pub use error::{CheckError, ErrorClass, ErrorReason, FinalClass};
pub use result::{FinalState, PageCheckResult, RedirectHop, Title};

/// Call first in `main` of every binary that can crawl with rendering on.
/// When this process was started as the browser's guard (`--render-guard`,
/// spec 014 research R12), it runs the guard and never returns; otherwise
/// it returns at once.
pub fn run_render_guard_if_asked() {
    let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    if let Some((parent, browser)) = spiderlens_render::parse_guard_args(&args) {
        spiderlens_render::run_guard(parent, browser);
    }
}
