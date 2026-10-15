//! CSV and JSON output of near duplicate results (FR-021, contracts/cli.md),
//! shared by the CLI and the app's exports.

use std::io::Write;

use rusqlite::Connection;
use serde::Serialize;

use super::query::{self, PageSummaryRow, PairExportRow};
use crate::export::csv_field;
use crate::{Error, Result};

/// Which list to write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Listing {
    Pages,
    Pairs { cluster: Option<u32> },
    Clusters,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClusterExportRow {
    pub cluster: u32,
    pub size: u32,
    pub min_similarity: u8,
    pub max_similarity: u8,
    pub representative: String,
    pub exact: bool,
}

fn rows_of(conn: &Connection, listing: Listing) -> Result<(Vec<&'static str>, Vec<serde_json::Value>)> {
    let to_json = |v: &dyn erased::Ser| v.json();
    Ok(match listing {
        Listing::Pages => (
            vec![
                "address",
                "status",
                "main_words",
                "closest_similarity",
                "closest_address",
                "near_duplicates",
                "near_duplicates_is_minimum",
                "cluster",
                "exact_duplicate",
            ],
            query::page_rows(conn)?
                .iter()
                .map(|r: &PageSummaryRow| to_json(r))
                .collect(),
        ),
        Listing::Pairs { cluster } => (
            vec!["address", "other_address", "similarity", "exact", "cluster"],
            query::pair_rows(conn, cluster)?
                .iter()
                .map(|r: &PairExportRow| to_json(r))
                .collect(),
        ),
        Listing::Clusters => {
            let page = query::clusters(conn, 0, u32::MAX)?;
            let rows: Vec<serde_json::Value> = page
                .rows
                .into_iter()
                .map(|c| {
                    to_json(&ClusterExportRow {
                        cluster: c.id,
                        size: c.size,
                        min_similarity: c.min_sim,
                        max_similarity: c.max_sim,
                        representative: c.representative,
                        exact: c.exact,
                    })
                })
                .collect();
            (
                vec![
                    "cluster",
                    "size",
                    "min_similarity",
                    "max_similarity",
                    "representative",
                    "exact",
                ],
                rows,
            )
        }
    })
}

/// Serialization behind a trait object, so the three row types share code.
mod erased {
    pub trait Ser {
        fn json(&self) -> serde_json::Value;
    }
    impl<T: serde::Serialize> Ser for T {
        fn json(&self) -> serde_json::Value {
            serde_json::to_value(self).expect("rows serialize")
        }
    }
}

/// Writes CSV (UTF-8 with BOM, CRLF, like the 007 export). Returns rows.
pub fn write_csv(conn: &Connection, listing: Listing, out: impl Write) -> Result<u64> {
    let (headers, rows) = rows_of(conn, listing)?;
    let mut out = std::io::BufWriter::new(out);
    let io = |e: std::io::Error| Error::Invalid(e.to_string());
    out.write_all(b"\xEF\xBB\xBF").map_err(io)?;
    out.write_all(headers.join(",").as_bytes()).map_err(io)?;
    out.write_all(b"\r\n").map_err(io)?;
    for row in &rows {
        for (n, h) in headers.iter().enumerate() {
            if n > 0 {
                out.write_all(b",").map_err(io)?;
            }
            match &row[*h] {
                serde_json::Value::Null => {}
                serde_json::Value::String(s) => csv_field(&mut out, s).map_err(io)?,
                other => out.write_all(other.to_string().as_bytes()).map_err(io)?,
            }
        }
        out.write_all(b"\r\n").map_err(io)?;
    }
    out.flush().map_err(io)?;
    Ok(rows.len() as u64)
}

/// Writes a JSON array of objects with the same keys as the CSV.
pub fn write_json(conn: &Connection, listing: Listing, mut out: impl Write) -> Result<u64> {
    let (_, rows) = rows_of(conn, listing)?;
    serde_json::to_writer_pretty(&mut out, &rows).map_err(|e| Error::Invalid(e.to_string()))?;
    out.write_all(b"\n").map_err(|e| Error::Invalid(e.to_string()))?;
    Ok(rows.len() as u64)
}
