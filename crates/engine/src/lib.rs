//! SpiderLens crawl engine.
//!
//! This crate must never depend on Tauri or any UI code (constitution,
//! principle III). The CLI and the desktop app are thin layers on top of it.

mod build_info;
mod check;
mod config;
pub mod crawl;
mod error;
pub mod extract;
mod fetch;
pub mod pdf;
mod result;
pub mod robots;
mod title;

pub use build_info::BuildInfo;
pub use check::{check_page, parse_input_url};
pub use config::{CheckOptions, RequestHeader};
pub use error::{CheckError, ErrorClass, ErrorReason, FinalClass};
pub use result::{FinalState, PageCheckResult, RedirectHop, Title};
