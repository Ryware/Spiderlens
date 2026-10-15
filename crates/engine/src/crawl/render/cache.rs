//! The resource cache shared by every render of one crawl (spec 014 FR-017,
//! plan D4 step 2).
//!
//! A byte-bounded, least recently used, in-memory cache keyed by URL. Only
//! responses the site allows to be cached are kept: no `no-store`, not
//! `private`, not `no-cache`, and with an explicit freshness lifetime from
//! `max-age` or `Expires`. Heuristic freshness is never used, so a response
//! without those headers is always fetched again.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// The default size of the cache.
pub const DEFAULT_CAPACITY: usize = 64 * 1024 * 1024;

/// One cached response.
#[derive(Debug, PartialEq, Eq)]
pub struct Cached {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Cached {
    fn bytes(&self, url: &str) -> usize {
        url.len() + self.body.len() + self.headers.iter().map(|(n, v)| n.len() + v.len()).sum::<usize>()
    }
}

struct Entry {
    value: Arc<Cached>,
    expires: Instant,
    bytes: usize,
    tick: u64,
}

pub struct ResourceCache {
    capacity: usize,
    used: usize,
    tick: u64,
    entries: HashMap<String, Entry>,
    /// Use order: oldest tick first.
    order: BTreeMap<u64, String>,
}

impl ResourceCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            used: 0,
            tick: 0,
            entries: HashMap::new(),
            order: BTreeMap::new(),
        }
    }

    /// Bytes held now.
    pub fn used(&self) -> usize {
        self.used
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// A fresh response for `url`, if any. A stale one is dropped.
    pub fn get(&mut self, url: &str, now: Instant) -> Option<Arc<Cached>> {
        let expires = self.entries.get(url)?.expires;
        if now >= expires {
            self.remove(url);
            return None;
        }
        self.tick += 1;
        let tick = self.tick;
        let entry = self.entries.get_mut(url)?;
        self.order.remove(&entry.tick);
        entry.tick = tick;
        self.order.insert(tick, url.to_owned());
        Some(entry.value.clone())
    }

    /// Stores a response when its headers allow it. Returns whether it was
    /// stored. One response may take at most an eighth of the cache, so a
    /// single large file cannot push everything else out.
    pub fn put(&mut self, url: &str, response: Cached, now: Instant, date_now: jiff::Timestamp) -> bool {
        let Some(fresh_for) = freshness(response.status, &response.headers, date_now) else {
            return false;
        };
        let bytes = response.bytes(url);
        if bytes > self.capacity / 8 {
            return false;
        }
        self.remove(url);
        while self.used + bytes > self.capacity {
            let Some((_, oldest)) = self.order.pop_first() else {
                break;
            };
            if let Some(e) = self.entries.remove(&oldest) {
                self.used -= e.bytes;
            }
        }
        self.tick += 1;
        self.order.insert(self.tick, url.to_owned());
        self.entries.insert(
            url.to_owned(),
            Entry {
                value: Arc::new(response),
                expires: now + fresh_for,
                bytes,
                tick: self.tick,
            },
        );
        self.used += bytes;
        true
    }

    fn remove(&mut self, url: &str) {
        if let Some(e) = self.entries.remove(url) {
            self.order.remove(&e.tick);
            self.used -= e.bytes;
        }
    }
}

fn header<'a>(headers: &'a [(String, String)], name: &str) -> impl Iterator<Item = &'a str> {
    headers
        .iter()
        .filter(move |(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
}

/// How long a response may be reused, or `None` when it must not be stored.
pub fn freshness(status: u16, headers: &[(String, String)], now: jiff::Timestamp) -> Option<Duration> {
    if status != 200 {
        return None;
    }
    if header(headers, "vary").any(|v| v.split(',').any(|f| f.trim() == "*")) {
        return None;
    }
    let mut max_age = None;
    for directive in header(headers, "cache-control").flat_map(|v| v.split(',')) {
        let (name, value) = match directive.split_once('=') {
            Some((n, v)) => (n.trim(), Some(v.trim().trim_matches('"'))),
            None => (directive.trim(), None),
        };
        match name.to_ascii_lowercase().as_str() {
            "no-store" | "private" | "no-cache" => return None,
            // The first max-age wins (RFC 9111 4.2.1 leaves duplicates invalid).
            // An unreadable max-age makes the response stale.
            "max-age" if max_age.is_none() => {
                max_age = Some(Duration::from_secs(value?.parse::<u64>().ok()?));
            }
            _ => {}
        }
    }
    let lifetime = match max_age {
        Some(d) => d,
        None => {
            let expires = header(headers, "expires").next()?;
            let expires = jiff::fmt::rfc2822::parse(expires.trim()).ok()?.timestamp();
            let date = header(headers, "date")
                .next()
                .and_then(|d| jiff::fmt::rfc2822::parse(d.trim()).ok())
                .map(|z| z.timestamp())
                .unwrap_or(now);
            Duration::try_from(expires.duration_since(date)).ok()?
        }
    };
    (!lifetime.is_zero()).then_some(lifetime)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(n, v)| (n.to_string(), v.to_string()))
            .collect()
    }

    fn resp(headers: &[(&str, &str)], size: usize) -> Cached {
        Cached {
            status: 200,
            headers: h(headers),
            body: vec![b'x'; size],
        }
    }

    fn ts(s: &str) -> jiff::Timestamp {
        jiff::fmt::rfc2822::parse(s).unwrap().timestamp()
    }

    const NOW: &str = "Wed, 14 Oct 2026 10:00:00 GMT";

    #[test]
    fn freshness_comes_from_max_age_or_expires_only() {
        let now = ts(NOW);
        assert_eq!(
            freshness(200, &h(&[("Cache-Control", "public, max-age=600")]), now),
            Some(Duration::from_secs(600))
        );
        assert_eq!(
            freshness(200, &h(&[("cache-control", "MAX-AGE=\"60\"")]), now),
            Some(Duration::from_secs(60))
        );
        // max-age wins over Expires.
        assert_eq!(
            freshness(
                200,
                &h(&[
                    ("Cache-Control", "max-age=5"),
                    ("Expires", "Wed, 14 Oct 2026 11:00:00 GMT")
                ]),
                now
            ),
            Some(Duration::from_secs(5))
        );
        // Expires against the response's Date, else against now.
        assert_eq!(
            freshness(
                200,
                &h(&[
                    ("Date", "Wed, 14 Oct 2026 09:00:00 GMT"),
                    ("Expires", "Wed, 14 Oct 2026 09:30:00 GMT")
                ]),
                now
            ),
            Some(Duration::from_secs(1800))
        );
        assert_eq!(
            freshness(200, &h(&[("Expires", "Wed, 14 Oct 2026 10:01:00 GMT")]), now),
            Some(Duration::from_secs(60))
        );
        // No heuristic freshness: Last-Modified alone is not enough.
        assert_eq!(
            freshness(
                200,
                &h(&[("Last-Modified", "Mon, 01 Jan 2024 00:00:00 GMT")]),
                now
            ),
            None
        );
        assert_eq!(freshness(200, &[], now), None);
    }

    #[test]
    fn uncacheable_responses_are_not_stored() {
        let now = ts(NOW);
        for cc in [
            "no-store",
            "private, max-age=600",
            "no-cache",
            "max-age=0",
            "max-age=abc",
        ] {
            assert_eq!(freshness(200, &h(&[("Cache-Control", cc)]), now), None, "{cc}");
        }
        // Expires in the past, or not a date ("0" is common).
        assert_eq!(
            freshness(200, &h(&[("Expires", "Wed, 14 Oct 2026 09:00:00 GMT")]), now),
            None
        );
        assert_eq!(freshness(200, &h(&[("Expires", "0")]), now), None);
        assert_eq!(freshness(404, &h(&[("Cache-Control", "max-age=60")]), now), None);
        assert_eq!(
            freshness(200, &h(&[("Cache-Control", "max-age=60"), ("Vary", "*")]), now),
            None
        );
    }

    #[test]
    fn a_hit_until_it_expires() {
        let mut c = ResourceCache::new(DEFAULT_CAPACITY);
        let t0 = Instant::now();
        assert!(c.put(
            "https://a.test/app.js",
            resp(&[("Cache-Control", "max-age=10")], 100),
            t0,
            ts(NOW)
        ));
        let hit = c
            .get("https://a.test/app.js", t0 + Duration::from_secs(9))
            .expect("hit");
        assert_eq!(hit.body.len(), 100);
        assert!(
            c.get("https://a.test/app.js", t0 + Duration::from_secs(10))
                .is_none()
        );
        assert!(c.is_empty());
        assert_eq!(c.used(), 0);
        // A response that may not be cached is not stored.
        assert!(!c.put(
            "https://a.test/x.js",
            resp(&[("Cache-Control", "no-store")], 10),
            t0,
            ts(NOW)
        ));
        assert!(c.get("https://a.test/x.js", t0).is_none());
    }

    #[test]
    fn bytes_are_bounded_and_the_least_recently_used_goes_first() {
        let mut c = ResourceCache::new(8_000);
        let t0 = Instant::now();
        let cc = [("Cache-Control", "max-age=600")];
        for name in ["a", "b", "c"] {
            assert!(c.put(&format!("u/{name}"), resp(&cc, 900), t0, ts(NOW)));
        }
        let three = c.used();
        // Touch "a" so that "b" is the least recently used.
        assert!(c.get("u/a", t0).is_some());
        for name in ["d", "e", "f", "g", "h", "i"] {
            c.put(&format!("u/{name}"), resp(&cc, 900), t0, ts(NOW));
        }
        assert!(c.used() <= 8_000, "{}", c.used());
        assert!(c.get("u/b", t0).is_none(), "least recently used is evicted");
        assert!(c.get("u/i", t0).is_some());
        assert!(three < 8_000);
        // A response over an eighth of the cache is never stored.
        assert!(!c.put("u/big", resp(&cc, 1_001), t0, ts(NOW)));
    }

    #[test]
    fn replacing_an_entry_keeps_the_byte_count_right() {
        let mut c = ResourceCache::new(DEFAULT_CAPACITY);
        let t0 = Instant::now();
        let cc = [("Cache-Control", "max-age=60")];
        c.put("u", resp(&cc, 500), t0, ts(NOW));
        c.put("u", resp(&cc, 200), t0, ts(NOW));
        assert_eq!(c.len(), 1);
        assert_eq!(c.used(), resp(&cc, 200).bytes("u"));
    }
}
