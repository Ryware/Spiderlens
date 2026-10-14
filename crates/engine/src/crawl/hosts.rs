//! Per-host crawl state owned by the coordinator (spec 008): robots.txt,
//! requests open, crawl-delay and Retry-After, and the adaptive allowance.
//! Pages and, with rendering on, page resources share it (spec 014 D4).

use std::sync::Arc;
use std::time::{Duration, Instant};

use spiderlens_config::SpeedSettings;

use super::speed::SiteSpeed;
use crate::robots::RobotsPolicy;

#[derive(Debug, Clone)]
pub(crate) enum RobotsState {
    Unknown,
    Fetching,
    Known(Arc<RobotsPolicy>, Option<String>),
}

#[derive(Debug)]
pub(crate) struct Host {
    pub(crate) robots: RobotsState,
    pub(crate) in_flight: u32,
    /// No request before this: crawl-delay spacing or a Retry-After pause.
    pub(crate) next_start: Instant,
    pub(crate) delay: Option<Duration>,
    pub(crate) speed: SiteSpeed,
}

impl Host {
    pub(crate) fn new(speed: &SpeedSettings) -> Self {
        Self {
            robots: RobotsState::Unknown,
            in_flight: 0,
            next_start: Instant::now(),
            delay: None,
            speed: SiteSpeed::new(speed.max_per_host, speed.adaptive),
        }
    }

    /// Requests this host may have open now.
    pub(crate) fn limit(&self) -> u32 {
        if self.delay.is_some() {
            1
        } else {
            self.speed.allowance()
        }
    }
}
