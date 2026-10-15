//! The Sitemap files view (spec 011 US3): every sitemap file the crawl read,
//! as a tree in index order.

use std::collections::HashMap;

use serde::Serialize;
use ts_rs::TS;

use crate::tabs::Need;
use crate::{Reader, Result};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct SitemapFileRow {
    #[ts(type = "number")]
    pub id: i64,
    /// The URL, or `file:` and the path for a file on this computer.
    pub location: String,
    /// `robots`, `default`, `user`, `local` or `list`; None in older files.
    pub source: Option<String>,
    #[ts(type = "number | null")]
    pub parent_id: Option<i64>,
    pub level: u32,
    /// `urlset` or `index`; None when the file could not be read.
    pub kind: Option<String>,
    pub http_status: Option<u16>,
    pub final_url: Option<String>,
    pub redirected: bool,
    pub compressed: bool,
    #[ts(type = "number | null")]
    pub bytes: Option<u64>,
    #[ts(type = "number")]
    pub entries: u64,
    pub over_limit: bool,
    pub error_class: Option<String>,
    pub error: Option<String>,
    pub error_line: Option<u32>,
    /// Entries whose `loc` is not a usable address.
    #[ts(type = "number")]
    pub invalid_entries: u64,
    /// Files this index listed that were read.
    pub children: u32,
    /// The address a local file will have.
    pub base_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct SitemapFiles {
    /// False for crawl files made before entries were kept: the rows then
    /// come from the older summary table, without sources or details.
    pub available: bool,
    pub files: Vec<SitemapFileRow>,
}

impl Reader {
    /// Every sitemap file, parents before their children.
    pub fn sitemap_files(&self) -> Result<SitemapFiles> {
        let available = self.features.has(Need::SitemapEntries);
        let rows = if available {
            self.new_rows()?
        } else {
            self.old_rows()?
        };
        Ok(SitemapFiles {
            available,
            files: tree_order(rows),
        })
    }

    fn new_rows(&self) -> Result<Vec<SitemapFileRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT f.id, f.location, f.source, f.parent_id, f.level, f.kind, f.http_status, f.final_url,
                    coalesce(f.redirected, 0), coalesce(f.compressed, 0), f.bytes, f.entry_count,
                    coalesce(f.over_limit, 0), f.error_class, f.error, f.error_line,
                    (SELECT count(*) FROM sitemap_entries e WHERE e.file_id = f.id AND e.invalid IS NOT NULL),
                    f.base_url
             FROM sitemap_files f ORDER BY f.id",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(SitemapFileRow {
                id: r.get(0)?,
                location: r.get(1)?,
                source: r.get(2)?,
                parent_id: r.get(3)?,
                level: r.get(4)?,
                kind: r.get(5)?,
                http_status: r.get(6)?,
                final_url: r.get(7)?,
                redirected: r.get(8)?,
                compressed: r.get(9)?,
                bytes: r.get::<_, Option<i64>>(10)?.map(|b| b as u64),
                entries: r.get::<_, i64>(11)? as u64,
                over_limit: r.get(12)?,
                error_class: r.get(13)?,
                error: r.get(14)?,
                error_line: r.get(15)?,
                invalid_entries: r.get::<_, i64>(16)? as u64,
                children: 0,
                base_url: r.get(17)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Older files: the base `sitemaps` table (location, parent, count, error).
    fn old_rows(&self) -> Result<Vec<SitemapFileRow>> {
        let mut stmt = self
            .conn
            .prepare("SELECT rowid, url, parent, url_count, error FROM sitemaps ORDER BY rowid")?;
        type Old = (i64, String, Option<String>, i64, Option<String>);
        let raw: Vec<Old> = stmt
            .query_map([], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
            })?
            .collect::<rusqlite::Result<_>>()?;
        let ids: HashMap<&str, i64> = raw.iter().map(|(id, url, ..)| (url.as_str(), *id)).collect();
        Ok(raw
            .iter()
            .map(|(id, url, parent, count, error)| SitemapFileRow {
                id: *id,
                location: url.clone(),
                source: None,
                parent_id: parent.as_deref().and_then(|p| ids.get(p).copied()),
                level: 0,
                kind: None,
                http_status: None,
                final_url: None,
                redirected: false,
                compressed: false,
                bytes: None,
                entries: *count as u64,
                over_limit: false,
                error_class: None,
                error: error.clone(),
                error_line: None,
                invalid_entries: 0,
                children: 0,
                base_url: None,
            })
            .collect())
    }
}

/// Depth-first, children in the order they were read; levels and child
/// counts recomputed from the parent links.
fn tree_order(rows: Vec<SitemapFileRow>) -> Vec<SitemapFileRow> {
    let known: std::collections::HashSet<i64> = rows.iter().map(|r| r.id).collect();
    let mut children: HashMap<Option<i64>, Vec<usize>> = HashMap::new();
    for (i, r) in rows.iter().enumerate() {
        let parent = r.parent_id.filter(|p| known.contains(p) && *p != r.id);
        children.entry(parent).or_default().push(i);
    }
    let mut out = Vec::with_capacity(rows.len());
    let mut stack: Vec<(usize, u32)> = children
        .get(&None)
        .map(|v| v.iter().rev().map(|&i| (i, 0)).collect())
        .unwrap_or_default();
    let mut placed = vec![false; rows.len()];
    while let Some((i, level)) = stack.pop() {
        if std::mem::replace(&mut placed[i], true) {
            continue;
        }
        let mut row = rows[i].clone();
        row.level = level;
        let kids = children.get(&Some(row.id)).map_or(&[][..], Vec::as_slice);
        row.children = kids.len() as u32;
        out.push(row);
        stack.extend(kids.iter().rev().map(|&k| (k, level + 1)));
    }
    // Rows in a parent cycle have no root; list them last at their level.
    for (i, r) in rows.into_iter().enumerate() {
        if !placed[i] {
            out.push(r);
        }
    }
    out
}
