//! Near duplicate content (spec 020): the analysis pass, its stored
//! results, and the queries the Content tab, the Clusters view, the
//! inspector, the comparison view and the CLI read.

pub mod analyze;
pub mod diff;
pub mod export;
mod pass;
mod query;

pub use analyze::{Analysis, Cluster, Item, PageResult, Params, bands};
pub use diff::{CompareSide, Comparison, DiffOp, DiffParagraph, DiffSpan};
pub use export::Listing;
pub use pass::{
    PASS_ID, PASS_VERSION, PassSummary, Schema, analyse_file, effective_settings, run_pass, stale_reason,
    stored_settings,
};
pub use query::{
    ClusterRow, ClustersPage, MemberRow, MembersPage, NearDupState, NearDupStatus, PageSummaryRow,
    PairExportRow, PairRow, PairsPage, clusters, compare, members, page_rows, pair_rows, pairs, status,
};

pub use rusqlite::{Connection, OpenFlags};

/// Opens a crawl file read-only for near duplicate queries.
pub fn open_read_only(path: &std::path::Path) -> crate::Result<Connection> {
    let conn = Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    conn.pragma_update(None, "temp_store", "FILE")?;
    Ok(conn)
}
