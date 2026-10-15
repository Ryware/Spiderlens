//! The Cookies view (spec 025 FR-027): each distinct cookie the site set,
//! the URLs that set it, and the cookies of one URL. Values are never
//! stored, so none can be shown.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::reader::Reader;
use crate::tabs::Need;
use crate::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum CookieFilter {
    #[default]
    All,
    Session,
    Persistent,
    ThirdParty,
    /// Set over https without `Secure`.
    MissingSecure,
    MissingHttpOnly,
    SameSiteNone,
}

impl CookieFilter {
    pub const ALL: [Self; 7] = [
        Self::All,
        Self::Session,
        Self::Persistent,
        Self::ThirdParty,
        Self::MissingSecure,
        Self::MissingHttpOnly,
        Self::SameSiteNone,
    ];

    /// A condition on the grouped row (`g`).
    fn condition(self) -> &'static str {
        match self {
            Self::All => "1",
            Self::Session => "g.session",
            Self::Persistent => "NOT g.session",
            Self::ThirdParty => "g.party = 'third'",
            Self::MissingSecure => "g.https AND NOT g.secure",
            Self::MissingHttpOnly => "NOT g.http_only",
            Self::SameSiteNone => "g.same_site = 'none'",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Session => "Session",
            Self::Persistent => "Persistent",
            Self::ThirdParty => "Third party",
            Self::MissingSecure => "Missing Secure",
            Self::MissingHttpOnly => "Missing HttpOnly",
            Self::SameSiteNone => "SameSite=None",
        }
    }
}

/// One distinct cookie (name, domain and path), with the attributes of its
/// most recent observation.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct CookieRow {
    pub name: String,
    pub domain: String,
    pub path: String,
    pub session: bool,
    /// Milliseconds since the epoch, for persistent cookies.
    #[ts(type = "number | null")]
    pub expires: Option<i64>,
    pub secure: bool,
    pub http_only: bool,
    pub same_site: Option<String>,
    pub third_party: bool,
    /// `response` or `script`.
    pub source: String,
    #[ts(type = "number")]
    pub size: i64,
    /// URLs that set it; 1 for the cookies of one URL.
    #[ts(type = "number")]
    pub urls: i64,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct CookiePage {
    pub rows: Vec<CookieRow>,
    #[ts(type = "number")]
    pub total: i64,
    /// Rows per filter, for the filter strip.
    #[ts(type = "Record<string, number>")]
    pub counts: std::collections::BTreeMap<String, i64>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct CookieUrl {
    #[ts(type = "number")]
    pub id: i64,
    pub url: String,
    pub status: Option<u16>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct CookieUrlPage {
    pub rows: Vec<CookieUrl>,
    #[ts(type = "number")]
    pub total: i64,
}

/// One row per (name, domain, path) with the latest observation's
/// attributes (the highest rowid wins).
const GROUPED: &str = "
    SELECT c.name, c.domain, c.path, c.expires IS NULL AS session, c.expires, c.secure, c.http_only,
           c.same_site, c.party, c.source, c.size, n.urls, n.https
    FROM (SELECT k.name, k.domain, k.path, MAX(k.rowid) AS last, COUNT(DISTINCT k.url_id) AS urls,
                 MAX(u.url LIKE 'https:%') AS https
          FROM cookies k JOIN urls u ON u.id = k.url_id
          GROUP BY k.name, k.domain, k.path) n
    JOIN cookies c ON c.rowid = n.last";

const SORTS: &[&str] = &[
    "name",
    "domain",
    "path",
    "session",
    "expires",
    "secure",
    "http_only",
    "same_site",
    "party",
    "size",
    "urls",
];

impl Reader {
    fn require_cookies(&self) -> Result<()> {
        if self.features.has(Need::Cookies) {
            Ok(())
        } else {
            Err(Error::Unavailable(Need::Cookies.unavailable_reason().into()))
        }
    }

    /// Distinct cookies matching `filter`, sorted by `sort` (a column name,
    /// default `urls` descending), one page at a time.
    pub fn cookies(
        &self,
        filter: CookieFilter,
        sort: Option<&str>,
        descending: bool,
        offset: u32,
        limit: u32,
    ) -> Result<CookiePage> {
        self.require_cookies()?;
        let column = match sort {
            Some(s) if SORTS.contains(&s) => s,
            Some(s) => return Err(Error::invalid(format!("cannot sort cookies by {s}"))),
            None => "urls",
        };
        let dir = if descending || sort.is_none() {
            "DESC"
        } else {
            "ASC"
        };
        let grouped = format!(
            "WITH g AS ({GROUPED}) SELECT g.name, g.domain, g.path, g.session, g.expires, g.secure, g.http_only, \
             g.same_site, g.party, g.source, g.size, g.urls FROM g WHERE {} \
             ORDER BY g.{column} {dir}, g.name, g.domain, g.path LIMIT ?1 OFFSET ?2",
            filter.condition()
        );
        let rows = self
            .conn
            .prepare(&grouped)?
            .query_map([i64::from(limit), i64::from(offset)], |r| {
                Ok(CookieRow {
                    name: r.get(0)?,
                    domain: r.get(1)?,
                    path: r.get(2)?,
                    session: r.get(3)?,
                    expires: r.get(4)?,
                    secure: r.get(5)?,
                    http_only: r.get(6)?,
                    same_site: r.get(7)?,
                    third_party: r.get::<_, String>(8)? == "third",
                    source: r.get(9)?,
                    size: r.get(10)?,
                    urls: r.get(11)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut counts = std::collections::BTreeMap::new();
        let mut total = 0;
        for f in CookieFilter::ALL {
            let n: i64 = self.conn.query_row(
                &format!(
                    "WITH g AS ({GROUPED}) SELECT COUNT(*) FROM g WHERE {}",
                    f.condition()
                ),
                [],
                |r| r.get(0),
            )?;
            if f == filter {
                total = n;
            }
            counts.insert(
                serde_json::to_value(f)
                    .expect("filters serialize")
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
                n,
            );
        }
        Ok(CookiePage { rows, total, counts })
    }

    /// The URLs that set one cookie, in crawl order.
    pub fn cookie_urls(
        &self,
        name: &str,
        domain: &str,
        path: &str,
        offset: u32,
        limit: u32,
    ) -> Result<CookieUrlPage> {
        self.require_cookies()?;
        let rows = self
            .conn
            .prepare(
                "SELECT DISTINCT u.id, u.url, u.status FROM cookies c JOIN urls u ON u.id = c.url_id \
                 WHERE c.name = ?1 AND c.domain = ?2 AND c.path = ?3 ORDER BY u.id LIMIT ?4 OFFSET ?5",
            )?
            .query_map(
                rusqlite::params![name, domain, path, i64::from(limit), i64::from(offset)],
                |r| {
                    Ok(CookieUrl {
                        id: r.get(0)?,
                        url: r.get(1)?,
                        status: r.get(2)?,
                    })
                },
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let total = self.conn.query_row(
            "SELECT COUNT(DISTINCT url_id) FROM cookies WHERE name = ?1 AND domain = ?2 AND path = ?3",
            [name, domain, path],
            |r| r.get(0),
        )?;
        Ok(CookieUrlPage { rows, total })
    }

    /// The cookies one URL's response set, for the detail pane.
    pub fn url_cookies(&self, id: i64) -> Result<Vec<CookieRow>> {
        if !self.features.has(Need::Cookies) {
            return Ok(Vec::new());
        }
        Ok(self
            .conn
            .prepare(
                "SELECT name, domain, path, expires IS NULL, expires, secure, http_only, same_site, party, source, size \
                 FROM cookies WHERE url_id = ?1 ORDER BY rowid",
            )?
            .query_map([id], |r| {
                Ok(CookieRow {
                    name: r.get(0)?,
                    domain: r.get(1)?,
                    path: r.get(2)?,
                    session: r.get(3)?,
                    expires: r.get(4)?,
                    secure: r.get(5)?,
                    http_only: r.get(6)?,
                    same_site: r.get(7)?,
                    third_party: r.get::<_, String>(8)? == "third",
                    source: r.get(9)?,
                    size: r.get(10)?,
                    urls: 1,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    }
}
