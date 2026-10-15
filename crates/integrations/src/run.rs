//! What a run needs while it works (plan D7): its HTTP client, limits,
//! cache, and the controls the UI or a Ctrl-C uses to pause or cancel it.
//!
//! Runs hold the crawl file's connection across awaits, so they run on one
//! thread with a current-thread Tokio runtime ([`block_on`]), as the CLI
//! already does for crawls.

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Duration;

use jiff::Timestamp;

use crate::Provider;
use crate::cache::{self, Cache};
use crate::http::{Http, HttpError};
use crate::limits::{Bucket, Limits};

/// Pause or cancel a run from another thread.
#[derive(Debug, Clone, Default)]
pub struct Control(Arc<AtomicU8>);

impl Control {
    const RUN: u8 = 0;
    const PAUSE: u8 = 1;
    const CANCEL: u8 = 2;

    pub fn pause(&self) {
        self.0.store(Self::PAUSE, Ordering::SeqCst);
    }

    pub fn cancel(&self) {
        self.0.store(Self::CANCEL, Ordering::SeqCst);
    }

    pub fn paused(&self) -> bool {
        self.0.load(Ordering::SeqCst) == Self::PAUSE
    }

    pub fn cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst) == Self::CANCEL
    }

    pub fn reset(&self) {
        self.0.store(Self::RUN, Ordering::SeqCst);
    }
}

/// Everything one provider run uses besides the crawl file.
pub struct Ctx {
    pub provider: Provider,
    pub http: Http,
    pub limits: Limits,
    pub bucket: Bucket,
    pub cache: Option<Cache>,
    /// Ignore cached answers (FR-008 "force a refresh"); new answers are
    /// still cached.
    pub refresh: bool,
}

impl Ctx {
    pub fn new(provider: Provider, limits: Limits, cache: Option<Cache>) -> Self {
        let read = if provider == Provider::LabTests { 120 } else { 30 };
        Self {
            provider,
            http: Http::new(Duration::from_secs(read)),
            bucket: Bucket::new(limits.per_minute),
            limits,
            cache,
            refresh: false,
        }
    }

    /// A cached answer, if one is fresh.
    pub fn cached(&self, key: &[u8; 16]) -> Option<serde_json::Value> {
        if self.refresh {
            return None;
        }
        self.cache.as_ref()?.get(key, Timestamp::now()).ok().flatten()
    }

    pub fn remember(&self, key: &[u8; 16], value: &serde_json::Value) {
        if let Some(c) = &self.cache {
            let now = Timestamp::now();
            // A cache that cannot be written only costs a repeat request.
            let _ = c.put(
                key,
                self.provider,
                cache::default_ttl(self.provider, now),
                value,
                now,
            );
        }
    }
}

/// Runs a run's future to completion on the current thread.
pub fn block_on<F: std::future::Future>(f: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime")
        .block_on(f)
}

/// How a run ended early, for the run's message and its state.
pub fn describe(e: &HttpError) -> String {
    e.to_string()
}
