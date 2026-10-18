//! The memory guard of the render stage (spec 014 FR-037, FR-038, plan D10).
//!
//! Every second the stage reads the machine's free memory and the browser
//! process tree's resident memory and asks [`MemoryGuard::tick`] what to do:
//!
//! - Free memory under 10% of the total, or the browser over its budget:
//!   lower the render limit by one (never below 1) and report
//!   `memory_guard` as the limiting reason.
//! - Free memory above 20% and the browser under 80% of its budget for
//!   10 s: raise the limit by one, up to what the settings allow.
//! - Still over budget at a limit of 1: restart the browser between pages.
//! - The browser is also restarted every 1,000 pages to cap leaks.
//!
//! The guard is plain arithmetic on readings it is given, so tests feed it
//! a fake memory source and a fake clock.

use std::time::{Duration, Instant};

/// Pages rendered between browser restarts.
pub(crate) const RESTART_EVERY_PAGES: u32 = 1_000;
/// How long memory must stay calm before the limit goes up by one.
pub(crate) const RAISE_AFTER: Duration = Duration::from_secs(10);
/// How often the stage reads memory.
pub(crate) const TICK_EVERY: Duration = Duration::from_secs(1);

const MB: u64 = 1024 * 1024;

/// One reading, in bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MemoryReading {
    /// Total memory of the machine.
    pub total: u64,
    /// Memory available without swapping.
    pub free: u64,
    /// Resident memory of the browser process tree.
    pub browser: u64,
}

/// Where readings come from; a fake in tests.
pub(crate) trait MemorySource: Send {
    fn read(&mut self, browser_bytes: u64) -> MemoryReading;
}

/// The machine's memory, with the browser's given by the caller.
pub(crate) struct SystemMemory;

impl MemorySource for SystemMemory {
    fn read(&mut self, browser_bytes: u64) -> MemoryReading {
        let (total, free) = spiderlens_render::system_memory();
        MemoryReading {
            total,
            free,
            browser: browser_bytes,
        }
    }
}

/// What a tick decided.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Decision {
    /// Renders allowed at once from now on.
    pub limit: u32,
    /// The limit is below the configured one because of memory.
    pub limited: bool,
    /// Restart the browser between pages now.
    pub restart: bool,
}

pub(crate) struct MemoryGuard {
    /// What the settings allow.
    configured: u32,
    /// Explicit budget in MB, or one eighth of total memory.
    budget_mb: Option<u32>,
    limit: u32,
    calm_since: Option<Instant>,
    pages: u32,
}

impl MemoryGuard {
    pub(crate) fn new(configured: u32, budget_mb: Option<u32>) -> Self {
        let configured = configured.max(1);
        Self {
            configured,
            budget_mb,
            limit: configured,
            calm_since: None,
            pages: 0,
        }
    }

    /// The browser's budget: the setting, or one eighth of total memory.
    pub(crate) fn budget(&self, total: u64) -> u64 {
        self.budget_mb.map_or(total / 8, |mb| u64::from(mb) * MB)
    }

    pub(crate) fn limit(&self) -> u32 {
        self.limit
    }

    /// Whether memory holds the limit below the configured one.
    pub(crate) fn limited(&self) -> bool {
        self.limit < self.configured
    }

    /// A page finished. True every [`RESTART_EVERY_PAGES`] pages: restart
    /// the browser between pages to cap leaks.
    pub(crate) fn page_done(&mut self) -> bool {
        self.pages += 1;
        if self.pages >= RESTART_EVERY_PAGES {
            self.pages = 0;
            return true;
        }
        false
    }

    /// Applies one reading taken at `now`.
    pub(crate) fn tick(&mut self, r: MemoryReading, now: Instant) -> Decision {
        let budget = self.budget(r.total);
        // A machine that reports no memory is left alone.
        let known = r.total > 0;
        let low_memory = known && r.free.saturating_mul(10) < r.total;
        let over_budget = known && r.browser > budget;
        let mut restart = false;
        if low_memory || over_budget {
            self.calm_since = None;
            if self.limit > 1 {
                self.limit -= 1;
            } else if over_budget {
                restart = true;
            }
        } else if known
            && r.free.saturating_mul(5) > r.total
            && r.browser.saturating_mul(10) < budget.saturating_mul(8)
        {
            let since = *self.calm_since.get_or_insert(now);
            if now.duration_since(since) >= RAISE_AFTER && self.limit < self.configured {
                self.limit += 1;
                self.calm_since = Some(now);
            }
        } else {
            self.calm_since = None;
        }
        Decision {
            limit: self.limit,
            limited: self.limited(),
            restart,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GB: u64 = 1 << 30;

    /// 16 GB machine: budget 2 GB.
    fn reading(free_gb: f64, browser_mb: u64) -> MemoryReading {
        MemoryReading {
            total: 16 * GB,
            free: (free_gb * GB as f64) as u64,
            browser: browser_mb * MB,
        }
    }

    struct Fake(Vec<MemoryReading>);

    impl MemorySource for Fake {
        fn read(&mut self, _browser_bytes: u64) -> MemoryReading {
            self.0.remove(0)
        }
    }

    #[test]
    fn the_default_budget_is_one_eighth_of_total_memory() {
        let g = MemoryGuard::new(4, None);
        assert_eq!(g.budget(16 * GB), 2 * GB);
        assert_eq!(MemoryGuard::new(4, Some(512)).budget(16 * GB), 512 * MB);
    }

    #[test]
    fn low_free_memory_lowers_the_limit_one_step_at_a_time_but_never_below_one() {
        let mut g = MemoryGuard::new(4, None);
        let t = Instant::now();
        // 1 GB free of 16 is under 10%.
        let low = reading(1.0, 500);
        assert_eq!(g.tick(low, t).limit, 3);
        assert!(g.limited());
        assert_eq!(g.tick(low, t).limit, 2);
        assert_eq!(g.tick(low, t).limit, 1);
        let d = g.tick(low, t);
        assert_eq!(d.limit, 1);
        assert!(d.limited);
        // Low free memory alone does not restart the browser.
        assert!(!d.restart);
    }

    #[test]
    fn a_browser_over_budget_lowers_then_restarts_at_limit_one() {
        let mut g = MemoryGuard::new(2, None);
        let t = Instant::now();
        let over = reading(8.0, 2500);
        assert_eq!(g.tick(over, t).limit, 1);
        let d = g.tick(over, t);
        assert_eq!(d.limit, 1);
        assert!(d.restart, "over budget at limit 1 restarts the browser");
    }

    #[test]
    fn the_limit_goes_back_up_after_ten_calm_seconds_and_no_further_than_configured() {
        let mut g = MemoryGuard::new(3, None);
        let t0 = Instant::now();
        g.tick(reading(1.0, 500), t0);
        g.tick(reading(1.0, 500), t0);
        assert_eq!(g.limit(), 1);
        // Free above 20% and the browser under 80% of 2 GB (1,638 MB).
        let calm = reading(8.0, 1000);
        assert_eq!(g.tick(calm, t0).limit, 1, "the calm period starts");
        assert_eq!(g.tick(calm, t0 + Duration::from_secs(9)).limit, 1);
        assert_eq!(g.tick(calm, t0 + Duration::from_secs(10)).limit, 2);
        // Each step needs its own ten seconds.
        assert_eq!(g.tick(calm, t0 + Duration::from_secs(15)).limit, 2);
        let d = g.tick(calm, t0 + Duration::from_secs(20));
        assert_eq!(d.limit, 3);
        assert!(!d.limited);
        assert_eq!(g.tick(calm, t0 + Duration::from_secs(40)).limit, 3);
    }

    #[test]
    fn a_reading_in_between_resets_the_calm_period() {
        let mut g = MemoryGuard::new(2, None);
        let t0 = Instant::now();
        g.tick(reading(1.0, 500), t0);
        assert_eq!(g.limit(), 1);
        g.tick(reading(8.0, 1000), t0);
        // Free memory 12% (between 10% and 20%): neither lowers nor counts.
        g.tick(reading(1.9, 1000), t0 + Duration::from_secs(8));
        assert_eq!(g.tick(reading(8.0, 1000), t0 + Duration::from_secs(12)).limit, 1);
        assert_eq!(g.tick(reading(8.0, 1000), t0 + Duration::from_secs(22)).limit, 2);
    }

    #[test]
    fn the_browser_restarts_every_thousand_pages() {
        let mut g = MemoryGuard::new(4, None);
        for _ in 0..999 {
            assert!(!g.page_done());
        }
        assert!(g.page_done());
        for _ in 0..999 {
            assert!(!g.page_done());
        }
        assert!(g.page_done());
    }

    #[test]
    fn unknown_memory_changes_nothing() {
        let mut g = MemoryGuard::new(4, None);
        let d = g.tick(
            MemoryReading {
                total: 0,
                free: 0,
                browser: 0,
            },
            Instant::now(),
        );
        assert_eq!((d.limit, d.limited, d.restart), (4, false, false));
    }

    #[test]
    fn a_fake_source_drives_the_guard() {
        let mut src = Fake(vec![reading(1.0, 100), reading(8.0, 100)]);
        let mut g = MemoryGuard::new(2, None);
        let t = Instant::now();
        assert!(g.tick(src.read(0), t).limited);
        assert!(
            g.tick(src.read(0), t).limited,
            "still limited until ten calm seconds pass"
        );
    }
}
