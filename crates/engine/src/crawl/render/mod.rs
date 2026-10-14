//! The render stage of a crawl (spec 014 plan D2 to D4, D8).
//!
//! Exists only when the crawl's rendering mode is JavaScript: with
//! rendering off, none of this runs.

pub mod cache;
pub mod net;
