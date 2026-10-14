//! Synthetic crawl files for scale tests and benchmarks (T003).
//!
//! Writes the real crawl file schema (created by the engine's store) with
//! deterministic pseudo-random rows: titles with known duplicates, missing
//! and long titles, a mix of status codes, external URLs, redirect chains,
//! and about `links_per_page` links per page. No network.
//!
//! With `rendered`, it also writes what a JavaScript rendering crawl (spec
//! 014) stores for each internal 200 HTML page: a `rendered_pages` row
//! (a changed title on a third of them, a noindex on one in twenty), one
//! link only after rendering and, on one page in four, a raw link gone
//! after rendering.

use std::path::Path;

use rusqlite::{Connection, params};
use spiderlens_engine::crawl::CrawlStore;

pub struct Synth {
    pub urls: u64,
    pub links_per_page: u32,
    pub seed: u64,
    /// Adds spec 014 rendered data.
    pub rendered: bool,
}

impl Default for Synth {
    fn default() -> Self {
        Self {
            urls: 10_000,
            links_per_page: 10,
            seed: 7,
            rendered: false,
        }
    }
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        // xorshift64*
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
}

const WORDS: &[&str] = &[
    "guide",
    "price",
    "shoes",
    "red",
    "blue",
    "home",
    "about",
    "contact",
    "blog",
    "review",
    "2026",
    "best",
    "cheap",
    "sale",
    "new",
    "חנות",
    "מבצע",
    "Ελλάδα",
    "café",
    "Straße",
];

impl Synth {
    /// Creates `path` (it must not exist). Returns the number of URLs.
    pub fn write(&self, path: &Path) -> rusqlite::Result<u64> {
        drop(CrawlStore::create(path).map_err(|e| rusqlite::Error::InvalidPath(format!("{e}").into()))?);
        let mut conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "OFF")?;
        let tx = conn.transaction()?;
        let mut rng = Rng(self.seed.max(1));
        let mut html_pages: Vec<(i64, Option<String>)> = Vec::new();
        {
            let mut ins = tx.prepare(
                "INSERT INTO urls (id, url, host_key, internal, depth, state, check_only, reason, reason_detail,
                  source, source_id, status, content_type, redirect_to, title, error_class, error_message, fetched_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
            )?;
            for id in 1..=self.urls as i64 {
                let r = rng.below(1000);
                let external = r < 100;
                let host = if external {
                    "https://other.test:443"
                } else {
                    "https://example.test:443"
                };
                let w = WORDS[rng.below(WORDS.len() as u64) as usize];
                let url = format!("{}/{w}/{id}", host.trim_end_matches(":443"));
                let depth = (rng.below(8) + u64::from(id > 1)) as i64;
                let (status, ctype): (Option<i64>, Option<&str>) = match rng.below(100) {
                    0..=79 => (Some(200), Some("text/html; charset=utf-8")),
                    80..=84 => (Some(200), Some("image/png")),
                    85..=89 => (Some(301), None),
                    90..=94 => (Some(404), Some("text/html")),
                    95..=97 => (Some(500), Some("text/html")),
                    _ => (None, None),
                };
                let error = (status.is_none()).then_some("timeout");
                let title = match (ctype, rng.below(100)) {
                    (Some(c), _) if !c.starts_with("text/html") => None,
                    (_, 0..=1) => None,
                    (_, 2..=11) => Some("Home".to_string()),
                    (_, 12..=13) => Some(
                        "A very long title that goes well past the sixty character limit for titles".into(),
                    ),
                    (_, n) => {
                        let k = 2 + (n % 6);
                        Some(
                            (0..k)
                                .map(|_| WORDS[rng.below(WORDS.len() as u64) as usize])
                                .collect::<Vec<_>>()
                                .join(" "),
                        )
                    }
                };
                let redirect_to = (status == Some(301)).then(|| 1 + rng.below(self.urls) as i64);
                let (state, check_only, reason) = if external {
                    ("not_crawled", 1, Some("external"))
                } else {
                    ("crawled", 0, None)
                };
                if self.rendered
                    && !external
                    && status == Some(200)
                    && ctype.is_some_and(|c| c.starts_with("text/html"))
                {
                    html_pages.push((id, title.clone()));
                }
                ins.execute(params![
                    id,
                    url,
                    host,
                    !external,
                    depth,
                    state,
                    check_only,
                    reason,
                    Option::<String>::None,
                    if id == 1 { "start" } else { "link" },
                    (id > 1).then(|| 1 + rng.below((id - 1) as u64) as i64),
                    status,
                    ctype,
                    redirect_to,
                    title,
                    error,
                    error.map(|_| "no response within 30 s"),
                    1_760_000_000_000i64 + id,
                ])?;
            }
        }
        {
            let mut ins = tx.prepare(
                "INSERT INTO links (from_id, to_id, raw, kind, nofollow, page_nofollow, followed, pos) VALUES (?1, ?2, ?3, ?4, 0, 0, 1, ?5)",
            )?;
            for from in 1..=self.urls as i64 {
                for k in 0..self.links_per_page {
                    let to = 1 + rng.below(self.urls) as i64;
                    let kind = if k == 0 { "image" } else { "anchor" };
                    ins.execute(params![from, to, format!("/x/{to}"), kind, k])?;
                }
            }
        }
        if self.rendered {
            let mut page = tx.prepare(
                "INSERT INTO rendered_pages (url_id, title_count, meta_description_count, h1_count, h2_count,
                  heading_counts, word_count, no_content, title, noindex, nofollow, canonical_count,
                  canonical_conflict, rendered_html_stored)
                 VALUES (?1, 1, 0, 1, 0, '[1,0,0,0,0,0]', 300, 0, ?2, ?3, 0, 0, 0, 0)",
            )?;
            let mut render = tx.prepare(
                "INSERT INTO renders (url_id, state, attempts, render_ms, rendered_at) VALUES (?1, 'rendered', 1, ?2, ?3)",
            )?;
            let mut link = tx.prepare(
                "INSERT INTO rendered_links (from_id, to_id, raw, kind, nofollow, page_nofollow, followed, pos, rpos)
                 VALUES (?1, ?2, ?3, 'anchor', 0, 0, 1, ?4, 0)",
            )?;
            let mut gone = tx.prepare("INSERT INTO raw_only_links (from_id, pos) VALUES (?1, ?2)")?;
            for (id, title) in &html_pages {
                let title = match (title, rng.below(3)) {
                    (Some(t), 0) => Some(format!("{t} | Shop")),
                    (t, _) => t.clone(),
                };
                page.execute(params![id, title, i64::from(rng.below(20) == 0)])?;
                render.execute(params![
                    id,
                    200 + rng.below(800) as i64,
                    1_760_000_000_000i64 + id
                ])?;
                let to = 1 + rng.below(self.urls) as i64;
                link.execute(params![
                    id,
                    to,
                    format!("/js/{to}"),
                    i64::from(self.links_per_page)
                ])?;
                if self.links_per_page > 1 && rng.below(4) == 0 {
                    gone.execute(params![id, 1])?;
                }
            }
        }
        tx.execute("UPDATE meta SET value = 'completed' WHERE key = 'status'", [])?;
        // Base data only (plus rendering when asked): no page content, timing or HTML.
        tx.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES ('data_features', ?1)",
            [if self.rendered { "rendering" } else { "" }],
        )?;
        tx.commit()?;
        conn.pragma_update(None, "wal_checkpoint", "TRUNCATE")?;
        Ok(self.urls)
    }
}
