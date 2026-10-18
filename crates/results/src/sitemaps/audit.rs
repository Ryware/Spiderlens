//! The `sitemap_audit` Crawl Analysis pass (spec 011 plan D6): the sitemap
//! problems that need the whole crawl or Rust logic, into
//! `sitemap_problems(rule, file_id, entry_id, url_id, detail)`.
//!
//! Problems a rule can see from one URL's own data (a redirect in a
//! sitemap, an invalid `lastmod`, a file that could not be read) are live
//! rule conditions in `issues::rules::sitemaps` instead, so they update
//! while the crawl runs. Thresholds stay with the rules: the pass writes the
//! measured value in `detail` (days, entries) and the rule compares it with
//! `thr()`, so a settings change needs no rerun.

use std::collections::HashMap;
use std::time::Duration;

use rusqlite::{Connection, OptionalExtension};
use spiderlens_config::StoredCrawlConfig;
use url::Url;

use super::generate::last_modified_secs;
use crate::Result;
use crate::analysis::{Pass, PassContext};
use crate::tabs::Need;

/// The rule ids the pass writes, read by the rules of the same name.
pub mod problem {
    /// The same URL in more than one entry; `detail` is how many.
    pub const DUPLICATE: &str = "sitemap.duplicate";
    /// An entry outside its sitemap's folder or host; `detail` is the folder.
    pub const OUTSIDE: &str = "sitemap.outside_location";
    /// `lastmod` after the crawl started; `detail` is the days after.
    pub const LASTMOD_FUTURE: &str = "sitemap.lastmod_future";
    /// The page's `Last-Modified` is later than `lastmod`; `detail` is the
    /// days between them.
    pub const LASTMOD_OLD: &str = "sitemap.lastmod_old";
    /// A file whose entries all share one `lastmod`; `detail` is the entry
    /// count, `file_id` the file.
    pub const LASTMOD_IDENTICAL: &str = "sitemap.lastmod_identical";
}

pub struct SitemapAudit;

impl Pass for SitemapAudit {
    fn id(&self) -> &'static str {
        "sitemap_audit"
    }
    fn version(&self) -> u32 {
        1
    }
    fn needs(&self) -> &'static [Need] {
        &[Need::SitemapEntries]
    }
    /// Spec 011 plan D6: 2 s per 100,000 entries.
    fn budget(&self) -> Duration {
        Duration::from_secs(2)
    }
    fn run(&self, cx: &mut PassContext<'_>) -> Result<()> {
        let conn = cx.conn;
        let s = cx.schema;
        create(conn, s)?;
        conn.execute(&format!("DELETE FROM {s}.sitemap_problems"), [])?;
        const STEPS: u64 = 4;
        cx.progress(0, STEPS, "Sitemap checks")?;
        duplicates(conn, s)?;
        identical(conn, s)?;
        cx.progress(1, STEPS, "Sitemap checks")?;
        dates(conn, s)?;
        cx.progress(2, STEPS, "Sitemap checks")?;
        outside_location(conn, s, cx)?;
        cx.progress(STEPS, STEPS, "Sitemap checks")?;
        Ok(())
    }
}

/// The pass's table in `schema` (also created by crawl file step 18).
fn create(conn: &Connection, schema: &str) -> rusqlite::Result<()> {
    conn.execute_batch(&format!(
        "CREATE TABLE IF NOT EXISTS {schema}.sitemap_problems (
             rule TEXT NOT NULL, file_id INTEGER, entry_id INTEGER, url_id INTEGER, detail TEXT);
         CREATE INDEX IF NOT EXISTS {schema}.sitemap_problems_rule ON sitemap_problems(rule, url_id);
         CREATE INDEX IF NOT EXISTS {schema}.sitemap_problems_url ON sitemap_problems(url_id);"
    ))
}

/// Every entry of a URL listed more than once, in one file or across files
/// (US3 scenario 6).
fn duplicates(conn: &Connection, s: &str) -> rusqlite::Result<()> {
    conn.execute(
        &format!(
            "INSERT INTO {s}.sitemap_problems (rule, file_id, entry_id, url_id, detail)
             SELECT ?1, e.file_id, e.id, e.url_id, d.n
             FROM sitemap_entries e
             JOIN (SELECT url_id, COUNT(*) AS n FROM sitemap_entries
                   WHERE kind = 'url' AND url_id IS NOT NULL GROUP BY url_id HAVING COUNT(*) > 1) d
               ON d.url_id = e.url_id
             WHERE e.kind = 'url'"
        ),
        [problem::DUPLICATE],
    )?;
    Ok(())
}

/// Files where every entry has a `lastmod` and all are the same (US4
/// scenario 3); the rule applies its entry threshold to `detail`.
fn identical(conn: &Connection, s: &str) -> rusqlite::Result<()> {
    conn.execute(
        &format!(
            "INSERT INTO {s}.sitemap_problems (rule, file_id, entry_id, url_id, detail)
             SELECT ?1, e.file_id, NULL, NULL, COUNT(*)
             FROM sitemap_entries e WHERE e.kind = 'url'
             GROUP BY e.file_id
             HAVING COUNT(*) > 1 AND COUNT(e.lastmod) = COUNT(*) AND MIN(e.lastmod) = MAX(e.lastmod)"
        ),
        [problem::LASTMOD_IDENTICAL],
    )?;
    Ok(())
}

/// When the crawl started, in UTC seconds: from the stored configuration,
/// else the first sitemap read or fetch.
fn crawl_start(conn: &Connection) -> rusqlite::Result<Option<i64>> {
    let config: Option<String> = conn
        .query_row("SELECT value FROM meta WHERE key = 'config'", [], |r| r.get(0))
        .optional()?;
    if let Some(stored) = config.and_then(|c| serde_json::from_str::<StoredCrawlConfig>(&c).ok()) {
        return Ok(Some(stored.started_at.as_second()));
    }
    conn.query_row(
        "SELECT MIN(t) / 1000 FROM (SELECT MIN(read_at) AS t FROM sitemap_files
                                    UNION ALL SELECT MIN(fetched_at) FROM urls)",
        [],
        |r| r.get(0),
    )
}

const DAY: f64 = 86_400.0;

/// `lastmod` in the future (US4 scenario 2) and older than the page's own
/// `Last-Modified` (scenario 4).
fn dates(conn: &Connection, s: &str) -> rusqlite::Result<()> {
    if let Some(start) = crawl_start(conn)? {
        conn.execute(
            &format!(
                "INSERT INTO {s}.sitemap_problems (rule, file_id, entry_id, url_id, detail)
                 SELECT ?1, e.file_id, e.id, e.url_id, round((e.lastmod - ?2) / 86400.0, 1)
                 FROM sitemap_entries e
                 WHERE e.kind = 'url' AND e.lastmod IS NOT NULL AND e.lastmod > ?2"
            ),
            (problem::LASTMOD_FUTURE, start),
        )?;
    }
    let mut read = conn.prepare(
        "SELECT e.file_id, e.id, e.url_id, e.lastmod, h.headers
         FROM sitemap_entries e JOIN headers h ON h.url_id = e.url_id
         WHERE e.kind = 'url' AND e.lastmod IS NOT NULL",
    )?;
    let mut write = conn.prepare(&format!(
        "INSERT INTO {s}.sitemap_problems (rule, file_id, entry_id, url_id, detail)
         VALUES (?1, ?2, ?3, ?4, ?5)"
    ))?;
    let mut rows = read.query([])?;
    while let Some(r) = rows.next()? {
        let headers: String = r.get(4)?;
        let Some(page) = last_modified_secs(&headers) else {
            continue;
        };
        let lastmod: i64 = r.get(3)?;
        let days = ((page - lastmod) as f64 / DAY).floor();
        if days >= 1.0 {
            write.execute((
                problem::LASTMOD_OLD,
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
                days as i64,
            ))?;
        }
    }
    Ok(())
}

/// Where a file's entries may point: the folder of the address it was read
/// from (the base URL for a local file), research R4.
fn folder_of(address: &str) -> Option<Url> {
    let mut u = Url::parse(address).ok()?;
    if !matches!(u.scheme(), "http" | "https") {
        return None;
    }
    u.set_query(None);
    u.set_fragment(None);
    if !u.path().ends_with('/') {
        let path = u.path();
        let dir = &path[..path.rfind('/').map_or(0, |i| i + 1)];
        let dir = if dir.is_empty() { "/" } else { dir }.to_string();
        u.set_path(&dir);
    }
    Some(u)
}

fn same_origin(a: &Url, b: &Url) -> bool {
    a.scheme() == b.scheme()
        && a.host_str() == b.host_str()
        && a.port_or_known_default() == b.port_or_known_default()
}

/// Entries on another host or outside their sitemap's folder (US3
/// scenario 5), unless the robots.txt of the entry's host listed the
/// sitemap: robots.txt is read for the start host, so a file found through
/// it (or an index it listed) may list the start host's URLs from anywhere.
fn outside_location(conn: &Connection, s: &str, cx: &mut PassContext<'_>) -> Result<()> {
    struct File {
        parent: Option<i64>,
        source: String,
        folder: Option<Url>,
    }
    let mut files: HashMap<i64, File> = HashMap::new();
    {
        let mut stmt = conn.prepare(
            "SELECT id, parent_id, source, COALESCE(base_url, final_url, location) FROM sitemap_files",
        )?;
        let mut rows = stmt.query([])?;
        while let Some(r) = rows.next()? {
            let address: String = r.get(3)?;
            files.insert(
                r.get(0)?,
                File {
                    parent: r.get(1)?,
                    source: r.get(2)?,
                    folder: folder_of(&address),
                },
            );
        }
    }
    // The source of the file at the top of each file's index chain.
    let root_source = |mut id: i64| -> Option<&str> {
        for _ in 0..=files.len() {
            let f = files.get(&id)?;
            match f.parent {
                Some(p) if p != id && files.contains_key(&p) => id = p,
                _ => return Some(f.source.as_str()),
            }
        }
        None
    };
    let start: Option<Url> = conn
        .query_row("SELECT value FROM meta WHERE key = 'start_url'", [], |r| {
            r.get::<_, String>(0)
        })
        .optional()?
        .and_then(|u| Url::parse(&u).ok());

    let mut read = conn.prepare(
        "SELECT id, file_id, url_id, loc FROM sitemap_entries WHERE kind = 'url' AND invalid IS NULL ORDER BY id",
    )?;
    let mut write = conn.prepare(&format!(
        "INSERT INTO {s}.sitemap_problems (rule, file_id, entry_id, url_id, detail)
         VALUES (?1, ?2, ?3, ?4, ?5)"
    ))?;
    let mut rows = read.query([])?;
    let mut done = 0u64;
    while let Some(r) = rows.next()? {
        done += 1;
        if done.is_multiple_of(50_000) {
            cx.progress(2, 4, "Sitemap checks")?;
        }
        let file_id: i64 = r.get(1)?;
        let Some(folder) = files.get(&file_id).and_then(|f| f.folder.as_ref()) else {
            continue;
        };
        let loc: String = r.get(3)?;
        let Ok(entry) = Url::parse(loc.trim()) else {
            continue;
        };
        if same_origin(&entry, folder) && entry.path().starts_with(folder.path()) {
            continue;
        }
        let cross_listed = root_source(file_id) == Some("robots")
            && start.as_ref().is_some_and(|st| same_origin(st, &entry));
        if cross_listed {
            continue;
        }
        write.execute((
            problem::OUTSIDE,
            file_id,
            r.get::<_, i64>(0)?,
            r.get::<_, Option<i64>>(2)?,
            folder.as_str(),
        ))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folders_follow_the_location_rule() {
        let f = folder_of("https://a.test/blog/sitemap.xml").unwrap();
        assert_eq!(f.as_str(), "https://a.test/blog/");
        let f = folder_of("https://a.test/sitemap.xml?x=1").unwrap();
        assert_eq!(f.as_str(), "https://a.test/");
        let f = folder_of("https://a.test/sitemaps/").unwrap();
        assert_eq!(f.as_str(), "https://a.test/sitemaps/");
        assert!(folder_of("file:/tmp/s.xml").is_none());
        let a = Url::parse("https://a.test/x").unwrap();
        assert!(same_origin(&a, &Url::parse("https://a.test:443/y").unwrap()));
        assert!(!same_origin(&a, &Url::parse("https://www.a.test/x").unwrap()));
        assert!(!same_origin(&a, &Url::parse("http://a.test/x").unwrap()));
    }
}
