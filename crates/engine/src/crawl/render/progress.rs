//! Rendering progress (spec 014 plan D13, FR-040) and the render queue's
//! back-pressure marks (FR-035).

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use serde::Serialize;

/// Fetching stops starting pages while more renders than this wait.
pub const QUEUE_HIGH: u64 = 50_000;
/// Fetching starts pages again once fewer renders than this wait.
pub const QUEUE_LOW: u64 = 45_000;

/// The window of the "current" rendering rate.
const RATE_WINDOW: Duration = Duration::from_secs(5);

/// What keeps rendering from going faster, when it is the slower stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RenderLimiting {
    /// Every render slot is busy and pages wait.
    Concurrency,
    /// The memory guard lowered the number of renders at once.
    MemoryGuard,
    /// Slow pages occupy the render slots.
    SlowPages,
    /// The sites' own limits hold resource requests back.
    SiteLimits,
    /// So many pages wait that fetching paused until the queue shrinks.
    QueueFull,
}

/// `Progress.render`: present only when the crawl renders, so the progress
/// payload of a raw crawl is unchanged.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RenderProgress {
    /// Pages that reached a final rendering state after rendering
    /// (rendered, timed out or failed), over every run of this crawl file.
    pub rendered: u64,
    /// Pages waiting to be rendered.
    pub queued: u64,
    /// Pages being rendered now.
    pub rendering: u32,
    /// Pages finished per second over the last few seconds.
    pub pages_per_sec: f64,
    /// Pages finished per second since this run started.
    pub avg_pages_per_sec: f64,
    /// Pages that may be rendered at once now.
    pub limit: u32,
    pub limiting: Option<RenderLimiting>,
    /// The fetch stage has nothing left to request; renders go on.
    pub fetching_done: bool,
}

/// The counters behind [`RenderProgress`], kept by the render stage. The
/// queue itself lives in the crawl file; these are only its size, so the
/// fetch stage never has to count rows.
#[derive(Debug)]
pub(crate) struct Meter {
    pub queued: u64,
    pub done: u64,
    done_at_start: u64,
    started: Instant,
    samples: VecDeque<(Instant, u64)>,
    /// Back-pressure is on (hysteresis between the two marks).
    held: bool,
    high: u64,
    low: u64,
}

impl Meter {
    pub fn new(queued: u64, done: u64) -> Self {
        Self {
            queued,
            done,
            done_at_start: done,
            started: Instant::now(),
            samples: VecDeque::new(),
            held: false,
            high: QUEUE_HIGH,
            low: QUEUE_LOW,
        }
    }

    pub fn set_marks(&mut self, high: u64, low: u64) {
        self.high = high;
        self.low = low.min(high);
    }

    /// Whether fetching must not start page requests now: on above the high
    /// mark, off below the low one.
    pub fn queue_full(&mut self) -> bool {
        if self.held {
            if self.queued < self.low {
                self.held = false;
            }
        } else if self.queued > self.high {
            self.held = true;
        }
        self.held
    }

    fn rates(&mut self, now: Instant) -> (f64, f64) {
        let finished = self.done - self.done_at_start;
        self.samples.push_back((now, finished));
        while self
            .samples
            .get(1)
            .is_some_and(|(t, _)| now.duration_since(*t) >= RATE_WINDOW)
        {
            self.samples.pop_front();
        }
        let per_sec = |n: u64, d: Duration| {
            let secs = d.as_secs_f64();
            if secs >= 0.5 { n as f64 / secs } else { 0.0 }
        };
        let (t0, n0) = self.samples[0];
        (
            per_sec(finished - n0, now.duration_since(t0)),
            per_sec(finished, now.duration_since(self.started)),
        )
    }

    pub fn progress(&mut self, open: u32, limit: u32, fetching_done: bool) -> RenderProgress {
        let (pages_per_sec, avg_pages_per_sec) = self.rates(Instant::now());
        let limiting = if self.held {
            Some(RenderLimiting::QueueFull)
        } else if self.queued > 0 && open >= limit {
            Some(RenderLimiting::Concurrency)
        } else {
            None
        };
        RenderProgress {
            rendered: self.done,
            queued: self.queued,
            rendering: open,
            pages_per_sec,
            avg_pages_per_sec,
            limit,
            limiting,
            fetching_done,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn back_pressure_turns_on_above_the_high_mark_and_off_below_the_low_one() {
        let mut m = Meter::new(0, 0);
        m.set_marks(20, 15);
        m.queued = 20;
        assert!(!m.queue_full(), "the high mark itself is allowed");
        m.queued = 21;
        assert!(m.queue_full());
        m.queued = 16;
        assert!(m.queue_full(), "stays on between the marks");
        m.queued = 15;
        assert!(m.queue_full(), "the low mark itself still holds");
        m.queued = 14;
        assert!(!m.queue_full());
        m.queued = 18;
        assert!(!m.queue_full(), "stays off between the marks");
    }

    #[test]
    fn the_limiting_reason_is_named() {
        let mut m = Meter::new(5, 2);
        assert_eq!(
            m.progress(2, 2, false).limiting,
            Some(RenderLimiting::Concurrency)
        );
        assert_eq!(m.progress(1, 2, false).limiting, None);
        m.set_marks(4, 2);
        assert!(m.queue_full());
        let p = m.progress(2, 2, true);
        assert_eq!(p.limiting, Some(RenderLimiting::QueueFull));
        assert_eq!(
            (p.rendered, p.queued, p.rendering, p.fetching_done),
            (2, 5, 2, true)
        );
    }
}
