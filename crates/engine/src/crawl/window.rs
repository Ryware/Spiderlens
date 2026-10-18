//! A bounded in-memory window over the on-disk queue (research R6).
//!
//! Invariant: every queued row with a key up to `hi` is in the window, and
//! nothing above `hi` is. So the window's first entries are the queue's first
//! entries, and memory stays bounded however large the crawl gets.
//!
//! Entries are also indexed by host, so the scheduler can skip a host that is
//! at its limit in one step instead of walking all of its URLs (spec 008,
//! plan D1).

use std::collections::{BTreeMap, BTreeSet, HashMap};

use super::store::{CrawlStore, QueueEntry, QueueKey, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Hi {
    /// Nothing loaded yet.
    Empty,
    Upto(QueueKey),
    /// Everything queued on disk is in the window.
    All,
}

pub struct QueueWindow {
    entries: BTreeMap<QueueKey, QueueEntry>,
    /// The keys of `entries`, per host. A host with no entries has no key.
    by_host: HashMap<String, BTreeSet<QueueKey>>,
    hi: Hi,
    capacity: usize,
}

impl QueueWindow {
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: BTreeMap::new(),
            by_host: HashMap::new(),
            hi: Hi::Empty,
            capacity: capacity.max(2),
        }
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// True when the window holds the whole remaining queue.
    pub fn holds_everything(&self) -> bool {
        self.hi == Hi::All
    }

    fn covers(&self, key: QueueKey) -> bool {
        match self.hi {
            Hi::Empty => false,
            Hi::Upto(hi) => key <= hi,
            Hi::All => true,
        }
    }

    /// Loads more rows from disk when the window is under half full.
    pub fn refill(&mut self, store: &CrawlStore) -> Result<()> {
        if self.hi == Hi::All || self.entries.len() >= self.capacity / 2 {
            return Ok(());
        }
        let after = match self.hi {
            Hi::Upto(k) => Some(k),
            _ => None,
        };
        let want = self.capacity - self.entries.len();
        let rows = store.queued_after(after, want)?;
        let complete = rows.len() < want;
        for e in rows {
            self.hi = Hi::Upto(e.key());
            self.insert(e);
        }
        if complete {
            self.hi = Hi::All;
        }
        Ok(())
    }

    /// Raises the capacity to `capacity` and loads more rows from disk at
    /// once (spec 004 plan D11). Returns whether anything was loaded.
    pub fn grow(&mut self, store: &CrawlStore, capacity: usize) -> Result<bool> {
        self.capacity = self.capacity.max(capacity);
        if self.hi == Hi::All {
            return Ok(false);
        }
        let after = match self.hi {
            Hi::Upto(k) => Some(k),
            _ => None,
        };
        let want = self.capacity.saturating_sub(self.entries.len()).max(1);
        let rows = store.queued_after(after, want)?;
        let complete = rows.len() < want;
        let loaded = !rows.is_empty();
        for e in rows {
            self.hi = Hi::Upto(e.key());
            self.insert(e);
        }
        if complete {
            self.hi = Hi::All;
        }
        Ok(loaded)
    }

    /// Removes and returns every entry of one host.
    pub fn take_host(&mut self, host: &str) -> Vec<QueueEntry> {
        let Some(keys) = self.by_host.remove(host) else {
            return Vec::new();
        };
        keys.into_iter().filter_map(|k| self.entries.remove(&k)).collect()
    }

    /// A row was queued on disk; keep it here too if it falls in range.
    pub fn on_queued(&mut self, entry: QueueEntry) {
        if !self.covers(entry.key()) {
            return;
        }
        self.insert(entry);
        if self.entries.len() > self.capacity {
            // Drop the tail; those rows stay queued on disk.
            while self.entries.len() > self.capacity / 2 + self.capacity / 4 {
                if let Some((key, e)) = self.entries.pop_last() {
                    self.unindex(&e.host_key, key);
                }
            }
            let last = *self.entries.keys().next_back().expect("not empty");
            self.hi = Hi::Upto(last);
        }
    }

    fn insert(&mut self, entry: QueueEntry) {
        let key = entry.key();
        match self.by_host.get_mut(&entry.host_key) {
            Some(keys) => {
                keys.insert(key);
            }
            None => {
                self.by_host.insert(entry.host_key.clone(), BTreeSet::from([key]));
            }
        }
        // Keys are unique per row, so this never replaces another host's entry.
        self.entries.insert(key, entry);
    }

    fn unindex(&mut self, host: &str, key: QueueKey) {
        if let Some(keys) = self.by_host.get_mut(host) {
            keys.remove(&key);
            if keys.is_empty() {
                self.by_host.remove(host);
            }
        }
    }

    pub fn remove(&mut self, key: QueueKey) -> Option<QueueEntry> {
        let e = self.entries.remove(&key)?;
        self.unindex(&e.host_key, key);
        Some(e)
    }

    pub fn get(&self, key: QueueKey) -> Option<&QueueEntry> {
        self.entries.get(&key)
    }

    /// Entries in queue order.
    #[cfg(test)]
    pub fn iter(&self) -> impl Iterator<Item = &QueueEntry> {
        self.entries.values()
    }

    /// Each host with URLs in the window, and its earliest URL's key.
    /// How many entries each host has in the window (spec 008 FR-029).
    pub fn host_counts(&self) -> impl Iterator<Item = (&str, usize)> {
        self.by_host.iter().map(|(h, keys)| (h.as_str(), keys.len()))
    }

    pub fn host_heads(&self) -> impl Iterator<Item = (&str, QueueKey)> {
        self.by_host
            .iter()
            .filter_map(|(h, keys)| Some((h.as_str(), *keys.first()?)))
    }

    /// Each host's earliest external check in the window: what may still be
    /// requested while page fetches are held back (spec 014 FR-035).
    pub fn check_heads(&self) -> impl Iterator<Item = (&str, QueueKey)> {
        self.by_host.iter().filter_map(|(h, keys)| {
            keys.iter()
                .find(|k| self.entries.get(k).is_some_and(|e| e.check_only))
                .map(|k| (h.as_str(), *k))
        })
    }

    /// Removes every entry matching `pred` (used when the URL limit is hit).
    pub fn retain(&mut self, mut keep: impl FnMut(&QueueEntry) -> bool) {
        self.entries.retain(|_, e| keep(e));
        self.by_host.clear();
        for (key, e) in &self.entries {
            match self.by_host.get_mut(&e.host_key) {
                Some(keys) => {
                    keys.insert(*key);
                }
                None => {
                    self.by_host.insert(e.host_key.clone(), BTreeSet::from([*key]));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crawl::reason::DiscoverySource;
    use crate::crawl::store::{Discovery, Placement, Recorded};

    fn add(s: &mut CrawlStore, w: &mut QueueWindow, i: u32, depth: u32) {
        let url = format!("https://a.test/{i}");
        let r = s
            .record(&Discovery {
                url: &url,
                host_key: "h",
                internal: true,
                depth,
                source: DiscoverySource::Link,
                source_id: None,
                in_sitemap: false,
                placement: Placement::Queue {
                    check_only: false,
                    via_nofollow: false,
                },
            })
            .unwrap();
        if let Recorded::New { queued: Some(e), .. } = r {
            w.on_queued(e);
        }
    }

    #[test]
    fn window_stays_bounded_and_in_order() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = CrawlStore::create(&dir.path().join("w.spiderlens")).unwrap();
        let mut w = QueueWindow::new(64);
        w.refill(&s).unwrap();
        assert!(w.holds_everything());
        for i in 0..1000 {
            add(&mut s, &mut w, i, 1 + i % 3);
            assert!(w.len() <= 64);
        }
        // Drain in order, marking each as taken, and check global order.
        let mut last = None;
        let mut taken = 0;
        loop {
            w.refill(&s).unwrap();
            let Some(first) = w.iter().next().cloned() else {
                break;
            };
            if let Some(prev) = last {
                assert!(first.key() > prev, "queue order kept across refills");
            }
            last = Some(first.key());
            w.remove(first.key());
            s.mark_in_flight(first.id).unwrap();
            taken += 1;
        }
        assert_eq!(taken, 1000);
    }

    #[test]
    fn new_rows_below_hi_join_the_window() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = CrawlStore::create(&dir.path().join("w.spiderlens")).unwrap();
        let mut w = QueueWindow::new(8);
        for i in 0..20 {
            add(&mut s, &mut w, i, 3);
        }
        w.refill(&s).unwrap();
        assert!(!w.holds_everything());
        // A shallower URL sorts before everything loaded, so it must be here.
        add(&mut s, &mut w, 99, 1);
        assert_eq!(w.iter().next().unwrap().depth, 1);
    }

    #[test]
    fn host_heads_track_each_hosts_earliest_entry() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = CrawlStore::create(&dir.path().join("w.spiderlens")).unwrap();
        let mut w = QueueWindow::new(64);
        w.refill(&s).unwrap();
        for (i, host) in ["a", "b", "a", "b", "a"].iter().enumerate() {
            let url = format!("https://{host}.test/{i}");
            let host_key = format!("https://{host}.test:443");
            if let Recorded::New { queued: Some(e), .. } = s
                .record(&Discovery {
                    url: &url,
                    host_key: &host_key,
                    internal: true,
                    depth: 1,
                    source: DiscoverySource::Link,
                    source_id: None,
                    in_sitemap: false,
                    placement: Placement::Queue {
                        check_only: false,
                        via_nofollow: false,
                    },
                })
                .unwrap()
            {
                w.on_queued(e);
            }
        }
        let heads = |w: &QueueWindow| {
            let mut h: Vec<(String, String)> = w
                .host_heads()
                .map(|(host, key)| (host.to_string(), w.get(key).unwrap().url.clone()))
                .collect();
            h.sort();
            h
        };
        assert_eq!(
            heads(&w),
            [
                ("https://a.test:443".into(), "https://a.test/0".into()),
                ("https://b.test:443".into(), "https://b.test/1".into())
            ]
        );
        let first_b = w.host_heads().find(|(h, _)| h.contains("b.test")).unwrap().1;
        w.remove(first_b);
        assert_eq!(
            heads(&w)[1].1,
            "https://b.test/3",
            "next b entry becomes the head"
        );
        w.retain(|e| !e.host_key.contains("b.test"));
        assert_eq!(heads(&w).len(), 1, "a host with no entries left has no head");
    }
}
