//! The bounded PDF reader pool (spec 027 plan D4).
//!
//! A fetch task hands its PDF here after it has reported the response, so
//! its site slot is already free and PDF parsing never holds up page
//! fetching. At most [`IN_MEMORY`] PDFs wait in memory; past that they wait
//! in temporary files, so memory stays bounded by the size cap whatever the
//! site links to. Reads run on at most `readers` blocking threads; a read
//! that runs past its time limit is abandoned with what it had read, and its
//! thread keeps its reader slot until it really ends.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use tokio::sync::Semaphore;

use super::{PdfFacts, PdfStatus};
use crate::extract::body;

/// PDFs that may wait in memory at once.
pub const IN_MEMORY: usize = 8;

/// Extra time the watchdog allows past the reader's own deadline, which is
/// only checked between pages.
const GRACE: Duration = Duration::from_secs(1);

/// A PDF waiting to be read.
pub enum Held {
    Memory(Vec<u8>),
    File(PathBuf),
}

/// What a read produced.
pub struct Read {
    pub facts: PdfFacts,
    /// The text, compressed, and its size, when text is kept.
    pub text: Option<(Vec<u8>, usize)>,
}

#[derive(Clone)]
pub struct Pool {
    inner: Arc<Inner>,
}

struct Inner {
    readers: Arc<Semaphore>,
    in_memory: AtomicUsize,
    spilled: AtomicU64,
    time_limit: Duration,
    keep_text: bool,
    spill_dir: PathBuf,
}

/// Reader threads for this machine: a quarter of the cores, one to two.
pub fn default_readers() -> usize {
    let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
    (cores / 4).clamp(1, 2)
}

impl Pool {
    pub fn new(readers: usize, time_limit: Duration, keep_text: bool) -> Self {
        let spill_dir = std::env::temp_dir().join(format!("spiderlens-pdf-{}", std::process::id()));
        Self {
            inner: Arc::new(Inner {
                readers: Arc::new(Semaphore::new(readers.max(1))),
                in_memory: AtomicUsize::new(0),
                spilled: AtomicU64::new(0),
                time_limit,
                keep_text,
                spill_dir,
            }),
        }
    }

    /// PDFs written to temporary files so far.
    pub fn spilled(&self) -> u64 {
        self.inner.spilled.load(Ordering::Relaxed)
    }

    /// Keeps `bytes` in memory while there is room, else in a temporary
    /// file. Falls back to memory when the file cannot be written.
    pub fn hold(&self, bytes: Vec<u8>) -> Held {
        let i = &self.inner;
        if i.in_memory.fetch_add(1, Ordering::AcqRel) < IN_MEMORY {
            return Held::Memory(bytes);
        }
        i.in_memory.fetch_sub(1, Ordering::AcqRel);
        let n = i.spilled.fetch_add(1, Ordering::Relaxed);
        let path = i.spill_dir.join(format!("{n}.pdf"));
        let written = std::fs::create_dir_all(&i.spill_dir).and_then(|()| std::fs::write(&path, &bytes));
        match written {
            Ok(()) => Held::File(path),
            Err(_) => {
                i.in_memory.fetch_add(1, Ordering::AcqRel);
                Held::Memory(bytes)
            }
        }
    }

    /// Reads one held PDF under the time limit.
    pub async fn read(&self, held: Held) -> Read {
        let i = &self.inner;
        let permit = i.readers.clone().acquire_owned().await.expect("the pool is never closed");
        let from_memory = matches!(held, Held::Memory(_));
        let progress = super::progress();
        let deadline = Instant::now() + i.time_limit;
        let shared = progress.clone();
        let keep_text = i.keep_text;
        let job = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let bytes = match held {
                Held::Memory(b) => b,
                Held::File(path) => {
                    let b = std::fs::read(&path);
                    let _ = std::fs::remove_file(&path);
                    match b {
                        Ok(b) => b,
                        Err(e) => {
                            return Read {
                                facts: PdfFacts::failed(format!("The downloaded PDF could not be kept: {e}")),
                                text: None,
                            };
                        }
                    }
                }
            };
            let mut facts = super::read(&bytes, deadline, &shared);
            let text = take_text(&mut facts, keep_text);
            Read { facts, text }
        });
        let read = match tokio::time::timeout(i.time_limit + GRACE, job).await {
            Ok(Ok(r)) => r,
            Ok(Err(e)) => Read {
                facts: PdfFacts::failed(format!("PDF reading failed: {e}")),
                text: None,
            },
            Err(_) => {
                let mut facts = super::timed_out(&progress);
                let text = take_text(&mut facts, keep_text);
                Read { facts, text }
            }
        };
        if from_memory {
            i.in_memory.fetch_sub(1, Ordering::AcqRel);
        }
        read
    }
}

/// Moves the text out of `facts`, compressed when it is kept.
fn take_text(facts: &mut PdfFacts, keep: bool) -> Option<(Vec<u8>, usize)> {
    let text = std::mem::take(&mut facts.text);
    if !keep || text.is_empty() || facts.status == PdfStatus::Failed {
        return None;
    }
    body::compress(text.as_bytes()).ok().map(|packed| (packed, text.len()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn pdfs_past_the_memory_bound_wait_in_files() {
        let pool = Pool::new(1, Duration::from_secs(5), true);
        let pdf = spiderlens_test_fixtures::pdf::PdfSpec::titled("Held", 2).build();
        let held: Vec<Held> = (0..IN_MEMORY + 3).map(|_| pool.hold(pdf.clone())).collect();
        assert_eq!(held.iter().filter(|h| matches!(h, Held::File(_))).count(), 3);
        assert_eq!(pool.spilled(), 3);
        for h in held {
            let path = match &h {
                Held::File(p) => Some(p.clone()),
                Held::Memory(_) => None,
            };
            let r = pool.read(h).await;
            assert_eq!(r.facts.status, PdfStatus::Read);
            assert_eq!(r.facts.title.as_deref(), Some("Held"));
            assert!(r.text.is_some());
            if let Some(p) = path {
                assert!(!p.exists(), "spilled files are removed once read");
            }
        }
        // Every memory slot is free again.
        assert!(matches!(pool.hold(pdf), Held::Memory(_)));
    }

    #[tokio::test]
    async fn text_is_not_kept_unless_asked() {
        let pool = Pool::new(1, Duration::from_secs(5), false);
        let pdf = spiderlens_test_fixtures::pdf::PdfSpec::titled("No text kept", 1).build();
        let r = pool.read(pool.hold(pdf)).await;
        assert!(r.text.is_none());
        assert!(r.facts.words.unwrap_or(0) > 0);
    }
}
