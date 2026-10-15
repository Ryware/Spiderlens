//! Synthetic crawl files for scale tests and benchmarks (T003).
//!
//! Writes the real crawl file schema (created by the engine's store) with
//! deterministic pseudo-random rows: titles with known duplicates, missing
//! and long titles, a mix of status codes, external URLs, redirect chains,
//! and about `links_per_page` links per page. No network.

use std::path::Path;

use rusqlite::{Connection, params};
use spiderlens_engine::crawl::CrawlStore;

pub struct Synth {
    pub urls: u64,
    pub links_per_page: u32,
    pub seed: u64,
    /// Store each internal page's HTML (header, menu, content, footer
    /// around its links), for the link positions benchmark.
    pub html: bool,
    /// Also writes page content (spec 005 `pages`, `page_text`) for the
    /// HTML pages, marks the file as having it, and builds the indexes a
    /// finished run leaves (`pages_title_hash`, `pages_md_hash`) when
    /// `result_indexes` is set; a live or resumed crawl has none.
    pub pages: bool,
    pub result_indexes: bool,
}

impl Default for Synth {
    fn default() -> Self {
        Self {
            urls: 10_000,
            links_per_page: 10,
            seed: 7,
            html: false,
            pages: false,
            result_indexes: false,
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

/// A page of about 20 KB: the image, then the anchors split between a
/// header menu, the content and the footer, so `pos` matches the links.
fn page_html(targets: &[i64]) -> String {
    use std::fmt::Write;
    let mut h = String::from("<!doctype html><html><head><title>Page</title></head><body><header>");
    let _ = write!(h, r#"<img src="/x/{}" alt="logo"><nav><ul>"#, targets[0]);
    let anchors = &targets[1..];
    let menu = anchors.len() / 3;
    let footer = anchors.len() / 6;
    for (i, t) in anchors.iter().enumerate() {
        if i == menu {
            h.push_str("</ul></nav></header><main><article>");
        }
        if i == anchors.len() - footer {
            h.push_str("</article></main><footer>");
        }
        if i < menu {
            let _ = write!(h, r#"<li><a href="/x/{t}">Section {i}</a></li>"#);
        } else {
            let _ = write!(
                h,
                r#"<p>Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. <a href="/x/{t}">Read more about {i}</a> ut enim ad minim veniam.</p>"#
            );
        }
    }
    if menu == anchors.len() {
        h.push_str("</ul></nav></header>");
    }
    h.push_str("</footer></body></html>");
    h
}

impl Synth {
    /// Creates `path` (it must not exist). Returns the number of URLs.
    pub fn write(&self, path: &Path) -> rusqlite::Result<u64> {
        drop(CrawlStore::create(path).map_err(|e| rusqlite::Error::InvalidPath(format!("{e}").into()))?);
        let mut conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "OFF")?;
        let tx = conn.transaction()?;
        let mut rng = Rng(self.seed.max(1));
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
                "INSERT INTO links (from_id, to_id, raw, kind, nofollow, page_nofollow, followed, pos)
                 VALUES (?1, ?2, ?3, ?4, 0, 0, 1, ?5)",
            )?;
            let mut body =
                tx.prepare("INSERT INTO bodies (url_id, codec, size, body) VALUES (?1, ?2, ?3, ?4)")?;
            let internal: Vec<bool> = if self.html {
                let mut q = tx.prepare("SELECT internal FROM urls ORDER BY id")?;
                q.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?
            } else {
                Vec::new()
            };
            let mut targets = Vec::with_capacity(self.links_per_page as usize);
            for from in 1..=self.urls as i64 {
                targets.clear();
                for k in 0..self.links_per_page {
                    let to = 1 + rng.below(self.urls) as i64;
                    let kind = if k == 0 { "image" } else { "anchor" };
                    ins.execute(params![from, to, format!("/x/{to}"), kind, k])?;
                    targets.push(to);
                }
                if self.html && internal[from as usize - 1] {
                    let html = page_html(&targets);
                    let packed = spiderlens_engine::extract::body::compress(html.as_bytes())
                        .map_err(|e| rusqlite::Error::InvalidPath(e.to_string().into()))?;
                    body.execute(params![
                        from,
                        spiderlens_engine::extract::body::CODEC,
                        html.len() as i64,
                        packed
                    ])?;
                }
            }
        }
        if self.pages {
            self.write_pages(&tx)?;
        }
        tx.execute("UPDATE meta SET value = 'completed' WHERE key = 'status'", [])?;
        // Base data, plus page content when asked: no timing or HTML.
        tx.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES ('data_features', ?1)",
            [if self.pages { "extraction" } else { "" }],
        )?;
        tx.commit()?;
        if self.result_indexes {
            conn.execute_batch(
                "CREATE INDEX pages_title_hash ON pages(title_hash) WHERE title_hash IS NOT NULL;
                 CREATE INDEX pages_md_hash ON pages(meta_description_hash) WHERE meta_description_hash IS NOT NULL;",
            )?;
        }
        conn.pragma_update(None, "wal_checkpoint", "TRUNCATE")?;
        Ok(self.urls)
    }

    /// A `pages` row for every HTML page: meta descriptions with known
    /// duplicates, H1s that sometimes repeat the title, a few pages with a
    /// second title (also in `page_text`).
    fn write_pages(&self, tx: &rusqlite::Transaction<'_>) -> rusqlite::Result<()> {
        let mut rng = Rng(self.seed.wrapping_mul(31).max(1));
        let mut ins = tx.prepare(
            "INSERT INTO pages (url_id, title_count, meta_description, meta_description_count, h1_1, h1_2, h1_count,
              h2_1, h2_2, h2_count, heading_counts, word_count, text_hash, title_hash, meta_description_hash,
              partial, encoding_errors, no_content, html_stored)
             VALUES (?1, ?2, ?3, ?4, ?5, NULL, ?6, ?7, NULL, ?8, '[]', ?9, ?10, ?11, ?12, 0, 0, 0, 0)",
        )?;
        let mut second = tx.prepare(
            "INSERT INTO page_text (url_id, kind, level, pos, text, in_head) VALUES (?1, 'title', NULL, 1, ?2, 1)",
        )?;
        let mut pages =
            tx.prepare("SELECT id, title FROM urls WHERE content_type LIKE 'text/html%' ORDER BY id")?;
        let rows: Vec<(i64, Option<String>)> = pages
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        let hash = |s: &str| {
            s.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| {
                (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3)
            }) as i64
        };
        for (id, title) in rows {
            let title_count = i64::from(title.is_some()) + i64::from(rng.below(50) == 0);
            let md = match rng.below(100) {
                0..=4 => None,
                5..=14 => Some("Welcome to our store".to_string()),
                n => Some(format!(
                    "{} page {id} about {}",
                    WORDS[(n % WORDS.len() as u64) as usize],
                    WORDS[rng.below(WORDS.len() as u64) as usize]
                )),
            };
            let h1 = match (rng.below(100), &title) {
                (0..=9, Some(t)) => Some(t.to_uppercase()),
                (10..=14, _) => None,
                (n, _) => Some(format!(
                    "{} {}",
                    WORDS[(n % WORDS.len() as u64) as usize],
                    rng.below(1000)
                )),
            };
            ins.execute(params![
                id,
                title_count,
                md,
                i64::from(md.is_some()),
                h1,
                i64::from(h1.is_some()) + i64::from(rng.below(30) == 0),
                Some(format!("Section {}", rng.below(50))),
                1 + rng.below(4) as i64,
                50 + rng.below(2000) as i64,
                rng.next() as i64,
                title.as_deref().map(hash),
                md.as_deref().map(hash),
            ])?;
            if title_count > 1 {
                second.execute(params![id, format!("Second title {}", rng.below(100))])?;
            }
        }
        Ok(())
    }
}

/// Stores `config` as the crawl's configuration in a synthetic file, so
/// tests can give it segments (spec 027 part B).
pub fn store_config(path: &Path, config: spiderlens_config::CrawlConfig) -> rusqlite::Result<()> {
    let stored = spiderlens_config::StoredCrawlConfig::new(
        config,
        spiderlens_config::ModeInput::Spider {
            start_url: "https://example.test/".into(),
        },
        vec!["example.test".into()],
        jiff::Timestamp::UNIX_EPOCH,
    );
    let json =
        serde_json::to_string(&stored).map_err(|e| rusqlite::Error::ToSqlConversionFailure(e.into()))?;
    Connection::open(path)?.execute(
        "INSERT OR REPLACE INTO meta (key, value) VALUES ('config', ?1)",
        [json],
    )?;
    Ok(())
}
