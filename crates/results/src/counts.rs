//! Filter counts for one tab in one pass (plan D6, FR-009).

use std::collections::BTreeMap;

use serde::Serialize;
use ts_rs::TS;

use crate::reader::Reader;
use crate::tabs::{self, Derived};
use crate::view::UrlScope;
use crate::{Error, Result};

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct FilterCounts {
    pub tab: String,
    /// Matching URLs per filter id; null when the filter is unavailable.
    #[ts(type = "Record<string, number | null>")]
    pub counts: BTreeMap<String, Option<u64>>,
    /// Why each unavailable filter is unavailable.
    #[ts(type = "Record<string, string>")]
    pub unavailable: BTreeMap<String, String>,
    /// Labels of filters whose threshold is an issue setting, with the value
    /// in force ("Over 70 characters"; spec 009 US4 scenario 1).
    #[ts(type = "Record<string, string>")]
    pub labels: BTreeMap<String, String>,
    pub version: u32,
}

impl Reader {
    pub fn counts(&mut self, tab: &str, scope: UrlScope) -> Result<FilterCounts> {
        let t = tabs::tab(tab).ok_or_else(|| Error::invalid(format!("unknown tab {tab:?}")))?;
        let available: Vec<_> = t.filters.iter().filter(|f| self.features.has(f.need)).collect();
        let derived: Vec<Derived> = available.iter().filter_map(|f| f.derived).collect();
        self.ensure_derived(derived)?;
        let version = self.version()? as u32;
        let mut sql = String::from("SELECT ");
        for (i, f) in available.iter().enumerate() {
            if i > 0 {
                sql.push_str(", ");
            }
            sql.push_str(&format!(
                "COALESCE(SUM(CASE WHEN ({}) THEN 1 ELSE 0 END), 0)",
                f.cond
            ));
        }
        sql.push_str(&format!(" FROM urls u NOT INDEXED WHERE ({})", t.eligible));
        match (t.scoped, scope) {
            (true, UrlScope::Internal) => sql.push_str(" AND u.internal = 1"),
            (true, UrlScope::External) => sql.push_str(" AND u.internal = 0"),
            _ => {}
        }
        let values: Vec<i64> = self.conn.query_row(&sql, [], |r| {
            (0..available.len()).map(|i| r.get::<_, i64>(i)).collect()
        })?;
        let mut counts = BTreeMap::new();
        let mut unavailable = BTreeMap::new();
        for f in &t.filters {
            if self.features.has(f.need) {
                let i = available.iter().position(|a| a.id == f.id).expect("listed");
                counts.insert(f.id.to_string(), Some(values[i] as u64));
            } else {
                counts.insert(f.id.to_string(), None);
                unavailable.insert(f.id.to_string(), f.need.unavailable_reason().to_string());
            }
        }
        let values = self.thresholds.read().unwrap_or_else(|e| e.into_inner()).clone();
        let labels = t
            .filters
            .iter()
            .filter(|f| f.threshold.is_some())
            .map(|f| (f.id.to_string(), f.label_with(&values)))
            .collect();
        Ok(FilterCounts {
            tab: t.id.to_string(),
            counts,
            unavailable,
            labels,
            version,
        })
    }
}
