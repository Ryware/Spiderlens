//! Sitemaps on the read side (spec 011): the Sitemap files view and
//! generating XML sitemaps from a crawl. The audit passes join this module
//! in part b.

mod files;
mod generate;
mod xml;

pub use files::{SitemapFileRow, SitemapFiles};
pub use generate::{GenerateError, GenerateReport, GeneratedFile, Output, generate};
#[doc(hidden)]
pub use xml::check_structure;
