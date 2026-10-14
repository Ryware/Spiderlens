//! Near duplicate content (spec 020): the analysis pass, its stored
//! results, and the queries the Content tab, the Clusters view, the
//! inspector, the comparison view and the CLI read.

pub mod analyze;
pub mod diff;
mod pass;
mod query;

pub use analyze::{Analysis, Cluster, Item, PageResult, Params, bands};
pub use diff::{CompareSide, Comparison, DiffOp, DiffParagraph, DiffSpan};
pub use pass::{PASS_ID, PASS_VERSION, PassSummary, Schema, run_pass, stale_reason};
pub use query::{
    ClusterRow, ClustersPage, NearDupState, NearDupStatus, PageSummaryRow, PairExportRow, PairRow, PairsPage,
    clusters, compare, page_rows, pair_rows, pairs, status,
};
