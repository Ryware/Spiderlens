//! Sitemaps on the read side (spec 011): generating XML sitemaps from a
//! crawl. The audit passes and views join this module in later tasks.

mod generate;
mod xml;

pub use generate::{GenerateError, GenerateReport, GeneratedFile, Output, generate};
#[doc(hidden)]
pub use xml::check_structure;
