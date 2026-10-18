//! SpiderLens external data integrations (spec 024).
//!
//! Everything here runs after a crawl, from the URLs stored in the crawl
//! file, with the user's own credentials (constitution principle II). The
//! crawl engine never depends on this crate (plan D1), so nothing here can
//! slow a crawl down.
//!
//! - `credentials`: the OS keychain behind a small trait (plan D2).
//! - `oauth`: Google sign-in through the system browser, loopback and PKCE (D3).
//! - `http`: one client per run, separate from the crawler's (D4).
//! - `limits`: per-provider rate, concurrency and daily allowance (D5).
//! - `cache`: provider responses on disk, outside crawl files (D6).
//! - `run` and `store`: resumable runs inside the crawl file (D7, D8).
//! - `matching`: provider URLs to crawled URLs (D9).
//! - `gsc`: Search Console (D10).
//! - `links`: Ahrefs, Majestic and Moz link metrics with cost control (D10).
//! - `sheets` and `looker`: export a results view to Google Sheets, open it in Looker Studio (D10).
//! - `hub`: the one entry point the app and the CLI share.

pub mod browser;
pub mod cache;
pub mod credentials;
mod error;
pub mod gsc;
pub mod http;
pub mod hub;
pub mod limits;
pub mod links;
pub mod looker;
pub mod matching;
pub mod oauth;
mod provider;
pub mod run;
pub mod settings;
pub mod sheets;
pub mod store;

pub use error::{Error, Result};
pub use provider::Provider;
