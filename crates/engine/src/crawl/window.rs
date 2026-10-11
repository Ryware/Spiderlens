//! A bounded in-memory window over the on-disk queue (research R6).
//!
//! Invariant: every queued row with a key up to `hi` is in the window, and
//! nothing above `hi` is. So the window's first entries are the queue's first
//! entries, and memory stays bounded however large the crawl gets.

use std::collections::BTreeMap;

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
    hi: Hi,
    capacity: usize,
}

impl QueueWindow {
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: BTreeMap::new(),
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
            self.entries.insert(e.key(), e);
        }
        if complete {
            self.hi = Hi::All;
        }
        Ok(())
    }

    /// A row was queued on disk; keep it here too if it falls in range.
    pub fn on_queued(&mut self, entry: QueueEntry) {
        if !self.covers(entry.key()) {
            return;
        }
        self.entries.insert(entry.key(), entry);
        if self.entries.len() > self.capacity {
            // Drop the tail; those rows stay queued on disk.
            while self.entries.len() > self.capacity / 2 + self.capacity / 4 {
                self.entries.pop_last();
            }
            let last = *self.entries.keys().next_back().expect("not empty");
            self.hi = Hi::Upto(last);
        }
    }

    pub fn remove(&mut self, key: QueueKey) -> Option<QueueEntry> {
        self.entries.remove(&key)
    }

    /// Entries in queue order.
    pub fn iter(&self) -> impl Iterator<Item = &QueueEntry> {
        self.entries.values()
    }

    /// Removes every entry matching `pred` (used when the URL limit is hit).
    pub fn retain(&mut self, mut keep: impl FnMut(&QueueEntry) -> bool) {
        self.entries.retain(|_, e| keep(e));
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
}
