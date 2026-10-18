//! The render stage of a crawl (spec 014 plan D2 to D4, D8).
//!
//! Exists only when the crawl's rendering mode is JavaScript: with
//! rendering off, none of this runs.

pub(crate) mod a11y_probe;
pub mod cache;
pub(crate) mod guard;
pub mod net;
pub mod progress;
pub(crate) mod stage;
