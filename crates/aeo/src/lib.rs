//! AI search checks (spec 015): can AI crawlers reach a site, can they read
//! it without JavaScript, and is it easy to quote and trust.
//!
//! Every check is a pure function of data the crawl already stored; only the
//! analysis entry point touches the crawl file. Nothing here runs in the
//! crawl's fetch path, and nothing depends on the desktop shell.

pub mod access;
pub mod content;
pub mod llms;
pub mod rules;
pub mod score;
pub mod signals;
