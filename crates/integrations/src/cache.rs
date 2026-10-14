//! Provider responses cached on the user's machine, outside crawl files
//! (plan D6, FR-008). One SQLite file in the app cache folder, shared by the
//! app and the CLI. Keys name the credential by label, never by secret.

use std::path::Path;
use std::time::Duration;

use jiff::Timestamp;
use rusqlite::{Connection, OptionalExtension, params};

use crate::{Provider, Result};

pub struct Cache {
    conn: Connection,
}

/// Default cache period per provider (FR-008).
pub fn default_ttl(p: Provider, now: Timestamp) -> Duration {
    match p {
        Provider::Ahrefs | Provider::Majestic | Provider::Moz => Duration::from_secs(7 * 24 * 3600),
        // Field data changes once a day, around 04:00 UTC.
        Provider::FieldData => {
            let secs = now.as_second().rem_euclid(86_400);
            let until = (4 * 3600 - secs).rem_euclid(86_400);
            Duration::from_secs(if until == 0 { 86_400 } else { until as u64 })
        }
        _ => Duration::from_secs(24 * 3600),
    }
}

/// The cache key: xxh3-128 of the request's parts joined with an unlikely
/// separator.
pub fn key(parts: &[&str]) -> [u8; 16] {
    xxhash_rust::xxh3::xxh3_128(parts.join("\u{1f}").as_bytes()).to_le_bytes()
}

impl Cache {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let conn = Connection::open(path)?;
        conn.busy_timeout(Duration::from_secs(5))?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             CREATE TABLE IF NOT EXISTS cache (key BLOB PRIMARY KEY, provider TEXT NOT NULL,
               expires_at INTEGER NOT NULL, body BLOB NOT NULL);
             CREATE INDEX IF NOT EXISTS cache_expiry ON cache(expires_at);",
        )?;
        conn.execute(
            "DELETE FROM cache WHERE expires_at <= ?1",
            [Timestamp::now().as_second()],
        )?;
        Ok(Self { conn })
    }

    /// The app's cache file, for the CLI.
    pub fn default_path() -> Option<std::path::PathBuf> {
        spiderlens_config::dirs::app_cache_dir().map(|d| d.join("integrations-cache.sqlite"))
    }

    pub fn get(&self, key: &[u8; 16], now: Timestamp) -> Result<Option<serde_json::Value>> {
        let body: Option<Vec<u8>> = self
            .conn
            .query_row(
                "SELECT body FROM cache WHERE key = ?1 AND expires_at > ?2",
                params![&key[..], now.as_second()],
                |r| r.get(0),
            )
            .optional()?;
        Ok(body
            .and_then(|b| zstd::decode_all(&b[..]).ok())
            .and_then(|b| serde_json::from_slice(&b).ok()))
    }

    pub fn put(
        &self,
        key: &[u8; 16],
        provider: Provider,
        ttl: Duration,
        value: &serde_json::Value,
        now: Timestamp,
    ) -> Result<()> {
        let raw = serde_json::to_vec(value).expect("JSON serializes");
        let body = zstd::encode_all(&raw[..], 3)?;
        let expires = now.as_second() + i64::try_from(ttl.as_secs()).unwrap_or(i64::MAX / 2);
        self.conn.execute(
            "INSERT OR REPLACE INTO cache (key, provider, expires_at, body) VALUES (?1, ?2, ?3, ?4)",
            params![&key[..], provider.id(), expires, body],
        )?;
        Ok(())
    }

    /// Clears one provider's entries, or all.
    pub fn clear(&self, provider: Option<Provider>) -> Result<usize> {
        Ok(match provider {
            Some(p) => self
                .conn
                .execute("DELETE FROM cache WHERE provider = ?1", [p.id()])?,
            None => self.conn.execute("DELETE FROM cache", [])?,
        })
    }

    /// Entries and stored bytes.
    pub fn size(&self) -> Result<(u64, u64)> {
        Ok(self.conn.query_row(
            "SELECT COUNT(*), COALESCE(SUM(length(body)), 0) FROM cache",
            [],
            |r| Ok((r.get::<_, i64>(0)? as u64, r.get::<_, i64>(1)? as u64)),
        )?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn put_get_expire_and_clear() {
        let dir = tempfile::tempdir().unwrap();
        let c = Cache::open(&dir.path().join("c.sqlite")).unwrap();
        let now: Timestamp = "2026-10-14T10:00:00Z".parse().unwrap();
        let k = key(&["search_console", "a@b.c", "sc-domain:a.test", "page=0"]);
        let v = serde_json::json!({"rows": [1, 2, 3]});
        c.put(&k, Provider::SearchConsole, Duration::from_secs(60), &v, now)
            .unwrap();
        assert_eq!(c.get(&k, now).unwrap(), Some(v));
        assert_eq!(
            c.get(&k, now + jiff::SignedDuration::from_secs(61)).unwrap(),
            None
        );
        assert_eq!(c.size().unwrap().0, 1);
        assert_eq!(c.clear(Some(Provider::Moz)).unwrap(), 0);
        assert_eq!(c.clear(None).unwrap(), 1);
    }

    #[test]
    fn different_parts_give_different_keys() {
        assert_ne!(key(&["a", "bc"]), key(&["ab", "c"]));
    }

    #[test]
    fn field_data_expires_at_the_next_daily_update() {
        let now: Timestamp = "2026-10-14T03:00:00Z".parse().unwrap();
        assert_eq!(default_ttl(Provider::FieldData, now), Duration::from_secs(3600));
        let now: Timestamp = "2026-10-14T05:00:00Z".parse().unwrap();
        assert_eq!(
            default_ttl(Provider::FieldData, now),
            Duration::from_secs(23 * 3600)
        );
    }
}
