//! SpiderLens results: the read side of a crawl file (spec 007).
//!
//! The engine writes the crawl file; this crate reads it for the results
//! table, its filters and counts, the URL detail pane and CSV export. The
//! desktop app and the CLI both use it, so they always agree (FR-031).
//!
//! Nothing here loads a crawl into memory: views keep their row order in a
//! temporary table on disk and hand out one page of rows at a time (plan D4,
//! constitution principle VI).

pub mod analysis;
mod cookies;
mod counts;
mod detail;
mod export;
mod features;
mod reader;
mod sql;
#[doc(hidden)]
pub mod synth;
mod tabs;
mod view;
pub mod viz;

pub use analysis::link_positions::TestedLink;
pub use analysis::{AnalysisProgress, AnalysisRun};
pub use cookies::{CookieFilter, CookiePage, CookieRow, CookieUrl, CookieUrlPage};
pub use counts::{FilterCounts, TabCounts};
pub use detail::{
    HeadingRow, HtmlMissing, LinkMetrics, LinkRow, ListKind, ListPage, PageHtml, RedirectHop, UrlDetail,
};
pub use export::{COPY_LIMIT, ExportError, export_file, write_csv, write_tsv_row};
pub use features::Features;
pub use reader::Reader;
pub use rusqlite::InterruptHandle;
pub use sql::{ci_contains, error_class_label, natural_cmp, norm_ws};
pub use tabs::{ColumnDef, FilterDef, Need, TabDef, ValueKind, tab, tabs};
pub use view::{Refresh, RowPage, Sort, UrlScope, ViewQuery};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("crawl file error: {0}")]
    Sql(#[from] rusqlite::Error),
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    Unavailable(String),
}

impl Error {
    pub(crate) fn invalid(msg: impl Into<String>) -> Self {
        Self::Invalid(msg.into())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
