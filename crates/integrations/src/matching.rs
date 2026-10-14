//! Provider URLs to crawled URLs (plan D9, FR-012).
//!
//! A temporary table holds one key per internal crawled URL, normalized the
//! way the crawl normalized it. Secondary keys (other scheme, trailing slash
//! toggled) are added only where they point at exactly one crawled URL;
//! anything ambiguous is never matched. Lookups are indexed, so memory stays
//! flat however many rows a provider returns.

use rusqlite::{Connection, OptionalExtension, params};
use url::Url;

use crate::Result;

/// The match key of a URL: normalized like the crawl, without fragment.
pub fn primary_key(raw: &str) -> Option<String> {
    spiderlens_config::normalize_url(raw).ok().map(|u| u.to_string())
}

fn secondary_keys(primary: &str) -> Vec<String> {
    let Ok(u) = Url::parse(primary) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut other = u.clone();
    let _ = other.set_scheme(if u.scheme() == "https" { "http" } else { "https" });
    let toggled = |mut x: Url| {
        let p = x.path().to_string();
        if p.len() > 1 && p.ends_with('/') {
            x.set_path(p.trim_end_matches('/'));
        } else if !p.ends_with('/') {
            x.set_path(&format!("{p}/"));
        }
        x
    };
    out.push(other.to_string());
    out.push(toggled(u).to_string());
    out.push(toggled(other).to_string());
    out.retain(|k| k != primary);
    out
}

pub struct Matcher<'c> {
    conn: &'c Connection,
}

impl<'c> Matcher<'c> {
    /// Builds the key table from the crawl's internal URLs.
    pub fn build(conn: &'c Connection) -> Result<Self> {
        conn.execute_batch(
            "DROP TABLE IF EXISTS temp.match_key;
             CREATE TEMP TABLE match_key (key TEXT PRIMARY KEY, url_id INTEGER NOT NULL,
               is_primary INTEGER NOT NULL, ambiguous INTEGER NOT NULL DEFAULT 0);",
        )?;
        let mut read = conn.prepare("SELECT id, url FROM urls WHERE internal = 1")?;
        let mut primary = conn.prepare(
            "INSERT INTO temp.match_key (key, url_id, is_primary) VALUES (?1, ?2, 1)
             ON CONFLICT(key) DO UPDATE SET ambiguous = 1",
        )?;
        let mut secondary = conn.prepare(
            "INSERT INTO temp.match_key (key, url_id, is_primary) VALUES (?1, ?2, 0)
             ON CONFLICT(key) DO UPDATE SET ambiguous = CASE
               WHEN match_key.is_primary = 1 THEN match_key.ambiguous
               WHEN match_key.url_id = excluded.url_id THEN match_key.ambiguous
               ELSE 1 END",
        )?;
        let mut rows = read.query([])?;
        while let Some(r) = rows.next()? {
            let id: i64 = r.get(0)?;
            let url: String = r.get(1)?;
            if let Some(k) = primary_key(&url) {
                primary.execute(params![k, id])?;
            }
        }
        drop(rows);
        // Secondary keys go in after every primary key, so a variant never
        // shadows another URL's own address. Read from a copy: SQLite must
        // not insert into the table it is scanning.
        conn.execute_batch(
            "DROP TABLE IF EXISTS temp.match_primary;
             CREATE TEMP TABLE match_primary AS SELECT key, url_id FROM temp.match_key;",
        )?;
        let mut read = conn.prepare("SELECT key, url_id FROM temp.match_primary")?;
        let mut rows = read.query([])?;
        while let Some(r) = rows.next()? {
            let k: String = r.get(0)?;
            let id: i64 = r.get(1)?;
            for s in secondary_keys(&k) {
                secondary.execute(params![s, id])?;
            }
        }
        drop(rows);
        conn.execute_batch("DROP TABLE temp.match_primary;")?;
        Ok(Self { conn })
    }

    /// The crawled URL a provider URL stands for, if exactly one.
    pub fn lookup(&self, raw: &str) -> Result<Option<i64>> {
        let Some(k) = primary_key(raw) else {
            return Ok(None);
        };
        Ok(self
            .conn
            .query_row(
                "SELECT url_id FROM temp.match_key WHERE key = ?1 AND ambiguous = 0",
                [k],
                |r| r.get(0),
            )
            .optional()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn(urls: &[(&str, bool)]) -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("CREATE TABLE urls (id INTEGER PRIMARY KEY, url TEXT NOT NULL UNIQUE, internal INTEGER NOT NULL);")
            .unwrap();
        for (u, internal) in urls {
            c.execute("INSERT INTO urls (url, internal) VALUES (?1, ?2)", params![u, internal])
                .unwrap();
        }
        c
    }

    #[test]
    fn exact_scheme_and_slash_variants_match_when_unique() {
        let c = conn(&[("https://a.test/", true), ("https://a.test/blog", true), ("https://x.test/", false)]);
        let m = Matcher::build(&c).unwrap();
        assert_eq!(m.lookup("https://a.test/blog").unwrap(), Some(2));
        assert_eq!(m.lookup("https://a.test/blog/").unwrap(), Some(2));
        assert_eq!(m.lookup("http://a.test/blog").unwrap(), Some(2));
        assert_eq!(m.lookup("https://A.test/blog#top").unwrap(), Some(2));
        // External URLs are never matched; other hosts are never guessed.
        assert_eq!(m.lookup("https://x.test/").unwrap(), None);
        assert_eq!(m.lookup("https://b.test/blog").unwrap(), None);
    }

    #[test]
    fn a_url_s_own_address_beats_another_url_s_variant() {
        let c = conn(&[("https://a.test/p", true), ("https://a.test/p/", true)]);
        let m = Matcher::build(&c).unwrap();
        assert_eq!(m.lookup("https://a.test/p").unwrap(), Some(1));
        assert_eq!(m.lookup("https://a.test/p/").unwrap(), Some(2));
    }

    #[test]
    fn ambiguous_variants_are_not_matched() {
        // http://a.test/p/ is a variant of both crawled URLs.
        let c = conn(&[("https://a.test/p/", true), ("http://a.test/p", true)]);
        let m = Matcher::build(&c).unwrap();
        assert_eq!(m.lookup("http://a.test/p/").unwrap(), None);
        assert_eq!(m.lookup("https://a.test/p").unwrap(), None);
        assert_eq!(m.lookup("https://a.test/p/").unwrap(), Some(1));
    }
}
