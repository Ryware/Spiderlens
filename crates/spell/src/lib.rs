//! Spelling checks for SpiderLens (spec 027 part D).
//!
//! Kept out of the engine and results crates, so the crawl never depends on
//! dictionaries: the app and the CLI run [`run::run`] over a finished crawl
//! file, and the results crate reads what it wrote with plain SQL.

pub mod check;
pub mod dict;
pub mod html;
pub mod run;
pub mod tokenize;

pub use check::Checker;
pub use dict::{Available, DictError, Installed, Origin, Store, default_dir, manifest};
pub use run::{RunError, SpellProgress, SpellSummary, run};
