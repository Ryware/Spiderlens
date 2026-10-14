//! The spelling pass over a crawl file (spec 027 FR-D07 to FR-D09).
//!
//! It reads internal HTML pages in batches of 200: title, meta description
//! and headings from the crawl, body text from the stored HTML when the
//! crawl kept it. Results replace any earlier pass. A text block that is on
//! most pages, like a footer, is checked once and its findings are marked
//! site-wide, so one typo in a template is one thing to fix.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use serde::Serialize;
use spiderlens_config::SpellingSettings;
use spiderlens_engine::crawl::{SpellFindingRow, SpellPage, SpellPageRow, open_for_spelling};
use ts_rs::TS;

use crate::check::{Checker, Counts, Misspelling, context};
use crate::dict::{Store, normalize_lang};
use crate::html::body_blocks;

const BATCH: u32 = 200;

/// Pages a block must be on, as a share of checked pages, to be site-wide.
const SITE_WIDE_SHARE: f64 = 0.5;
/// Fewer checked pages than this and nothing is site-wide.
const SITE_WIDE_MIN_PAGES: u64 = 4;

/// A page whose words are mostly in another script than its language,
/// counted only with at least this many words.
const MISMATCH_MIN_WORDS: u32 = 20;

#[derive(Debug, thiserror::Error)]
pub enum RunError {
    #[error(transparent)]
    File(#[from] spiderlens_engine::crawl::file::FileError),
    #[error(transparent)]
    Store(#[from] spiderlens_engine::crawl::store::StoreError),
}

#[derive(Debug, Clone, Copy, Serialize, TS)]
#[ts(export)]
pub struct SpellProgress {
    #[ts(type = "number")]
    pub checked: u64,
    #[ts(type = "number")]
    pub total: u64,
}

/// What a pass did, also stored in the crawl file's meta `spelling`.
#[derive(Debug, Clone, Default, Serialize, serde::Deserialize, TS)]
#[ts(export)]
pub struct SpellSummary {
    #[ts(type = "number")]
    pub pages: u64,
    /// Checked without body text, because the crawl did not keep HTML.
    #[ts(type = "number")]
    pub partly_checked: u64,
    #[ts(type = "number")]
    pub no_dictionary: u64,
    #[ts(type = "number")]
    pub errors: u64,
    #[ts(type = "number")]
    pub distinct_words: u64,
    #[ts(type = "number")]
    pub site_wide_blocks: u64,
    /// Languages pages declared that have no installed dictionary, with
    /// their page counts.
    #[ts(type = "Record<string, number>")]
    pub missing: BTreeMap<String, u64>,
    /// The dictionaries used.
    pub dictionaries: Vec<String>,
    pub cancelled: bool,
    #[ts(type = "number")]
    pub ms: u64,
}

/// Runs the pass, calling `progress` after each batch. Stops between
/// batches once `cancel` is set, keeping what was checked.
pub fn run(
    path: &Path,
    settings: &SpellingSettings,
    dictionaries: &Store,
    cancel: &AtomicBool,
    mut progress: impl FnMut(SpellProgress),
) -> Result<SpellSummary, RunError> {
    let started = Instant::now();
    let (_lock, crawl) = open_for_spelling(path)?;
    let checker = Checker::new(dictionaries, &settings.ignore, settings.skip_all_caps);
    let total = crawl.spell_eligible()?;
    let mut pass = Pass {
        checker: &checker,
        default_lang: &settings.default_lang,
        cache: HashMap::new(),
        block_pages: HashMap::new(),
        summary: SpellSummary::default(),
        distinct: HashSet::new(),
        used: HashSet::new(),
    };
    crawl.begin()?;
    crawl.clear_spelling()?;
    let mut after = 0;
    let mut checked = 0;
    loop {
        if cancel.load(Ordering::Relaxed) {
            pass.summary.cancelled = true;
            break;
        }
        let batch = crawl.spell_pages_after(after, BATCH)?;
        let Some(last) = batch.last() else { break };
        after = last.id;
        for page in &batch {
            let (row, findings) = pass.page(page);
            crawl.record_spelling(page.id, &row, &findings)?;
        }
        checked += batch.len() as u64;
        crawl.commit()?;
        progress(SpellProgress { checked, total });
        crawl.begin()?;
    }
    let pages = pass.summary.pages - pass.summary.no_dictionary;
    if pages >= SITE_WIDE_MIN_PAGES {
        let site_wide: Vec<i64> = pass
            .block_pages
            .iter()
            .filter(|(_, n)| **n as f64 > pages as f64 * SITE_WIDE_SHARE)
            .map(|(h, _)| *h)
            .collect();
        crawl.mark_site_wide(&site_wide)?;
        pass.summary.site_wide_blocks = site_wide.len() as u64;
    }
    let mut summary = pass.summary;
    summary.distinct_words = pass.distinct.len() as u64;
    summary.dictionaries = {
        let mut d: Vec<String> = pass.used.into_iter().collect();
        d.sort();
        d
    };
    summary.ms = started.elapsed().as_millis() as u64;
    crawl.set_meta("spelling", &serde_json::to_string(&summary).expect("summary serializes"))?;
    crawl.commit()?;
    Ok(summary)
}

struct Pass<'a> {
    checker: &'a Checker,
    default_lang: &'a str,
    /// Misspellings and counts of a block already checked, by dictionary
    /// and text hash.
    cache: HashMap<(String, i64), (Vec<Misspelling>, Counts)>,
    /// Checked pages each body block is on.
    block_pages: HashMap<i64, u32>,
    summary: SpellSummary,
    distinct: HashSet<(String, String)>,
    used: HashSet<String>,
}

impl Pass<'_> {
    fn page(&mut self, page: &SpellPage) -> (SpellPageRow, Vec<SpellFindingRow>) {
        self.summary.pages += 1;
        let Some(lang) = self.checker.pick(page.lang.as_deref(), self.default_lang) else {
            self.summary.no_dictionary += 1;
            let declared = page
                .lang
                .as_deref()
                .and_then(normalize_lang)
                .or_else(|| normalize_lang(self.default_lang))
                .unwrap_or_else(|| "unknown".into());
            *self.summary.missing.entry(declared.clone()).or_default() += 1;
            let row = SpellPageRow {
                lang: Some(declared),
                status: "no_dictionary",
                words: 0,
                errors: 0,
                lang_mismatch: false,
            };
            return (row, Vec::new());
        };
        self.used.insert(lang.clone());
        let mut counts = Counts::default();
        let mut findings = Vec::new();
        let mut add = |found: Vec<Misspelling>, source: &'static str, text: &str, offset: usize, block: Option<i64>| {
            for m in found {
                let suggestions = self.checker.suggest(&m.lang, &m.word);
                self.distinct.insert((m.lang.clone(), m.word.clone()));
                findings.push(SpellFindingRow {
                    kind: "spelling",
                    context: context(text, m.pos, m.len),
                    pos: (offset + m.pos) as u32,
                    word: m.word,
                    source,
                    suggestions,
                    block,
                });
            }
        };
        if let Some(t) = &page.title {
            add(self.checker.check(&lang, t, &mut counts), "title", t, 0, None);
        }
        if let Some(d) = &page.description {
            add(self.checker.check(&lang, d, &mut counts), "description", d, 0, None);
        }
        let mut offset = 0;
        for h in &page.headings {
            add(self.checker.check(&lang, h, &mut counts), "heading", h, offset, None);
            offset += h.chars().count() + 1;
        }
        let status = match &page.html {
            None => "partly_checked",
            Some(bytes) => {
                let html = String::from_utf8_lossy(bytes);
                let mut seen = HashSet::new();
                let mut offset = 0;
                for block in body_blocks(&html) {
                    let hash = xxhash_rust::xxh3::xxh3_64(block.as_bytes()) as i64;
                    if seen.insert(hash) {
                        *self.block_pages.entry(hash).or_default() += 1;
                    }
                    let key = (lang.clone(), hash);
                    let (found, c) = match self.cache.get(&key) {
                        Some(hit) => hit.clone(),
                        None => {
                            let mut c = Counts::default();
                            let found = self.checker.check(&lang, &block, &mut c);
                            self.cache.insert(key, (found.clone(), c));
                            (found, c)
                        }
                    };
                    counts.checked += c.checked;
                    counts.unchecked += c.unchecked;
                    counts.own_script += c.own_script;
                    counts.all += c.all;
                    add(found, "body", &block, offset, Some(hash));
                    offset += block.chars().count() + 1;
                }
                "checked"
            }
        };
        if page.html.is_none() {
            self.summary.partly_checked += 1;
        }
        let status = if counts.all == 0 { "no_text" } else { status };
        self.summary.errors += findings.len() as u64;
        let row = SpellPageRow {
            lang: Some(lang),
            status,
            words: counts.checked,
            errors: findings.len() as u32,
            lang_mismatch: counts.all >= MISMATCH_MIN_WORDS && counts.own_script * 2 < counts.all,
        };
        (row, findings)
    }
}
