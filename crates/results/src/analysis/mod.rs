//! Crawl Analysis: whole-crawl computations that run after fetching ends
//! (spec 009 plan D12, specs 022 and 023).
//!
//! Each computation writes its own table. Until the shared runner from spec
//! 009 lands, the reader builds these tables in its `temp` schema on demand,
//! like the other derived tables (spec 023 plan D6, "stage 1"). Columns name
//! the tables unqualified, so a stored copy in `main` and a fresh copy in
//! `temp` are read the same way (plan D5).

pub mod link_metrics;
