//! Sitemaps on the read side (spec 011): the Sitemap files view,
//! generating XML sitemaps from a crawl, and the two Crawl Analysis passes
//! the Sitemaps rules and tab read (part b).

pub mod audit;
mod files;
mod generate;
mod orphans;
mod tab;
mod xml;

pub use audit::SitemapAudit;
pub use files::{SitemapFileRow, SitemapFiles};
pub use generate::{GenerateError, GenerateReport, GeneratedFile, Output, generate};
pub use orphans::{OrphanRow, OrphanSummary, Orphans, UrlSitemapEntry};
pub(crate) use tab::tab;
#[doc(hidden)]
pub use xml::check_structure;
