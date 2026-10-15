//! Storage for the spelling pass (spec 027 part D). The pass itself lives in
//! the `spiderlens-spell` crate, so the crawl never depends on dictionaries;
//! this module only reads what the pass checks and writes what it found.

use std::path::Path;

use rusqlite::{OptionalExtension, params};

use super::file::{FileError, WriterLock};
use super::migrate;
use super::store::{CrawlStore, Result, StoreError};

/// One page the pass checks.
#[derive(Debug, Clone)]
pub struct SpellPage {
    pub id: i64,
    pub title: Option<String>,
    pub description: Option<String>,
    pub headings: Vec<String>,
    /// The page's `lang`, as written.
    pub lang: Option<String>,
    /// The stored HTML, when the crawl kept it.
    pub html: Option<Vec<u8>>,
}

/// What the pass found on one page.
#[derive(Debug, Clone, PartialEq)]
pub struct SpellPageRow {
    /// The dictionary used, or None when none was installed for the page.
    pub lang: Option<String>,
    /// checked | partly_checked (no stored HTML) | no_dictionary | no_text
    pub status: &'static str,
    pub words: u32,
    pub errors: u32,
    pub lang_mismatch: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpellFindingRow {
    /// spelling (grammar later).
    pub kind: &'static str,
    pub word: String,
    /// title | description | heading | body
    pub source: &'static str,
    /// Character offset in that source.
    pub pos: u32,
    pub context: String,
    pub suggestions: Vec<String>,
    /// Hash of the body block the word is in, for site-wide repeats.
    pub block: Option<i64>,
}

/// id, title, meta description, lang.
type PageRow = (i64, Option<String>, Option<String>, Option<String>);

/// Internal HTML pages fetched successfully: the pages spelling covers.
const ELIGIBLE: &str = "u.internal = 1 AND u.state = 'crawled' AND u.error_class IS NULL \
     AND u.status BETWEEN 200 AND 299 AND u.resource_type = 'html'";

/// Opens a crawl file for the spelling pass: takes the writer lock and adds
/// the spelling tables to a file made before them. Only additive steps are
/// applied, so older SpiderLens versions can still read the file.
pub fn open_for_spelling(path: &Path) -> std::result::Result<(WriterLock, CrawlStore), FileError> {
    let lock = WriterLock::acquire(path)?;
    let store = CrawlStore::open(path)?;
    let from = migrate::version(store.conn())?;
    if from < migrate::LATEST {
        if migrate::read_compat_of(migrate::LATEST) > from.max(migrate::BASE) {
            return Err(StoreError::Format(
                "this crawl file needs a format upgrade first: resume it once in this version".into(),
            )
            .into());
        }
        migrate::upgrade(store.conn())?;
    }
    Ok((lock, store))
}

impl CrawlStore {
    pub fn spell_eligible(&self) -> Result<u64> {
        let n: i64 = self.conn().query_row(
            &format!("SELECT COUNT(*) FROM urls u WHERE {ELIGIBLE}"),
            [],
            |r| r.get(0),
        )?;
        Ok(n as u64)
    }

    /// Up to `limit` pages after `after`, in id order.
    pub fn spell_pages_after(&self, after: i64, limit: u32) -> Result<Vec<SpellPage>> {
        let sql = format!(
            "SELECT u.id, u.title, p.meta_description, p.lang FROM urls u LEFT JOIN pages p ON p.url_id = u.id
             WHERE {ELIGIBLE} AND u.id > ?1 ORDER BY u.id LIMIT ?2"
        );
        let rows: Vec<PageRow> = self
            .conn()
            .prepare_cached(&sql)?
            .query_map(params![after, limit], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
            })?
            .collect::<rusqlite::Result<_>>()?;
        let mut pages = Vec::with_capacity(rows.len());
        for (id, title, description, lang) in rows {
            let headings = self
                .conn()
                .prepare_cached(
                    "SELECT text FROM page_text WHERE url_id = ?1 AND kind = 'heading' ORDER BY pos",
                )?
                .query_map([id], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            pages.push(SpellPage {
                id,
                title,
                description,
                headings,
                lang,
                html: self.page_html(id)?,
            });
        }
        Ok(pages)
    }

    /// Forgets the previous pass's results.
    pub fn clear_spelling(&self) -> Result<()> {
        self.conn()
            .execute_batch("DELETE FROM spell_findings; DELETE FROM spell_pages;")?;
        Ok(())
    }

    pub fn record_spelling(&self, id: i64, page: &SpellPageRow, findings: &[SpellFindingRow]) -> Result<()> {
        self.conn()
            .prepare_cached(
                "INSERT OR REPLACE INTO spell_pages (url_id, lang, status, words, errors, lang_mismatch)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )?
            .execute(params![
                id,
                page.lang,
                page.status,
                page.words,
                page.errors,
                page.lang_mismatch
            ])?;
        let mut stmt = self.conn().prepare_cached(
            "INSERT INTO spell_findings (url_id, kind, word, source, pos, context, suggestions, block, site_wide)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0)",
        )?;
        for f in findings {
            let suggestions = (!f.suggestions.is_empty())
                .then(|| serde_json::to_string(&f.suggestions).expect("strings serialize"));
            stmt.execute(params![
                id,
                f.kind,
                f.word,
                f.source,
                f.pos,
                f.context,
                suggestions,
                f.block
            ])?;
        }
        Ok(())
    }

    /// Marks findings in text blocks that repeat across the site (FR-D09).
    pub fn mark_site_wide(&self, blocks: &[i64]) -> Result<()> {
        let mut stmt = self
            .conn()
            .prepare_cached("UPDATE spell_findings SET site_wide = 1 WHERE block = ?1")?;
        for b in blocks {
            stmt.execute([b])?;
        }
        Ok(())
    }

    /// When the last pass finished, as JSON written by the pass.
    pub fn spelling_summary(&self) -> Result<Option<String>> {
        Ok(self
            .conn()
            .query_row("SELECT value FROM meta WHERE key = 'spelling'", [], |r| r.get(0))
            .optional()?)
    }
}
