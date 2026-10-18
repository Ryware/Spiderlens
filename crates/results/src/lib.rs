//! SpiderLens results: the read side of a crawl file (spec 007).
//!
//! The engine writes the crawl file; this crate reads it for the results
//! table, its filters and counts, the URL detail pane and CSV export. The
//! desktop app and the CLI both use it, so they always agree (FR-031).
//!
//! Nothing here loads a crawl into memory: views keep their row order in a
//! temporary table on disk and hand out one page of rows at a time (plan D4,
//! constitution principle VI).

pub mod agents;
pub mod analysis;
pub mod bulk;
mod cookies;
mod counts;
mod custom;
mod detail;
mod export;
mod features;
pub mod issues;
pub mod near_dup;
mod reader;
pub mod report;
mod segments;
pub mod serp;
mod sitemaps;
mod sql;
#[doc(hidden)]
pub mod synth;
pub(crate) mod tabs;
mod values;
pub(crate) mod view;
pub mod viz;

pub use analysis::link_positions::TestedLink;
pub use analysis::{AnalysisProgress, AnalysisRun};
pub use cookies::{CookieFilter, CookiePage, CookieRow, CookieUrl, CookieUrlPage};
pub use counts::{FilterCounts, TabCounts};
pub use custom::CustomResult;
pub use detail::{
    A11Y_DISCLAIMER, A11yCheckDetail, A11yDetail, A11yNodeDetail, AmpDetail, AmpFailure, HeadingRow,
    HtmlMissing, LinkMetrics, LinkRow, ListKind, ListPage, MobileDetail, MobileExample, PageHtml, PdfDetail,
    RedirectHop, Snippet, SpellDetail, SpellFinding, UrlDetail,
};
pub use export::{
    COPY_LIMIT, ExportError, csv_field, export_file, export_file_in, export_file_with, write_csv,
    write_csv_in, write_csv_with, write_tsv_row,
};
pub use features::Features;
pub use reader::{Reader, connect_attachable};
pub use rusqlite::InterruptHandle;
pub use segments::SegmentCounts;
pub use sitemaps::{
    GenerateError, GenerateReport, GeneratedFile, OrphanRow, OrphanSummary, Output as SitemapOutput,
    SitemapFileRow, SitemapFiles, UrlSitemapEntry, check_structure as check_sitemap_structure,
    generate as generate_sitemaps,
};
pub use sql::{ci_contains, error_class_label, natural_cmp, norm_ws};
pub use tabs::{
    ColumnDef, Derived, FilterDef, Need, TabDef, ValueKind, build_derived, c as columns, issue_tabs, tab,
    tabs,
};
pub use values::Values;
pub use view::{Refresh, RowPage, Sort, UrlScope, ViewQuery};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("crawl file error: {0}")]
    Sql(#[from] rusqlite::Error),
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    Unavailable(String),
    #[error("cancelled")]
    Cancelled,
}

impl Error {
    pub(crate) fn invalid(msg: impl Into<String>) -> Self {
        Self::Invalid(msg.into())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
