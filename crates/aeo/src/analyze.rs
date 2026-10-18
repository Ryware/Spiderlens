//! The analysis run (spec 015 plan D1, D4): reads a finished crawl file and
//! writes the AI checks' tables.
//!
//! Three passes: hosts (robots.txt per AI crawler, signals, llms.txt), pages
//! (in id-ordered chunks; parsing and checks spread over worker threads, one
//! writer) and site (aggregates, site checks, score, top fixes). Nothing is
//! loaded whole: memory stays at a few chunks of pages plus one score per page.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use jiff::civil::Date;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use spiderlens_config::ai_bots::{self, AiBot, BotPurpose};
use spiderlens_engine::extract::{self, body};
use spiderlens_engine::robots::RobotsTxt;
use ts_rs::TS;
use url::Url;

use crate::access::{BotAccess, HostTally, RobotsSource, Verdict, host_policies};
use crate::checks::{self, Directive, PageInput, PurposeMasks, RenderFacts, Thresholds};
use crate::content::{self, ContentFacts, ContentLimits};
use crate::llms::{self, LlmsReport};
use crate::rules::{AeoRule, Part, RULES};
use crate::score::{self, Fix, Outcome, Weights};
use crate::signals::{Signal, robots_signals};
use crate::store;
use crate::structured::{self, Entity, StructuredFacts};

/// Version of the analysis; a stored result with another version is stale.
pub const ANALYSIS_VERSION: u32 = 1;

const CHUNK: i64 = 1_000;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("crawl file: {0}")]
    Sql(#[from] rusqlite::Error),
    #[error("custom AI crawlers: {0}")]
    Bots(String),
    #[error("the analysis was cancelled")]
    Cancelled,
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, Default)]
pub struct AnalyzeOptions {
    pub weights: Weights,
    pub thresholds: Thresholds,
    pub custom_bots: Vec<AiBot>,
    /// Worker threads for the page pass; default: cores minus one.
    pub workers: Option<usize>,
    pub cancel: Option<Arc<AtomicBool>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "pass", rename_all = "snake_case")]
#[ts(export, rename = "AiProgress")]
pub enum Progress {
    Hosts {
        done: u32,
        total: u32,
    },
    Pages {
        #[ts(type = "number")]
        done: u64,
        #[ts(type = "number")]
        total: u64,
    },
    Site,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, rename = "AiHostSummary")]
pub struct HostSummary {
    pub host_key: String,
    /// `ok`, `missing`, `unavailable`, `ignored_not_fetched`.
    pub robots: String,
    pub access: Vec<BotAccess>,
    pub signals: Vec<Signal>,
    /// `None` when site files were not fetched for this host.
    pub llms_txt: Option<LlmsFile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, rename = "AiLlmsFile")]
pub struct LlmsFile {
    /// `ok`, `not_found`, `error`, `too_large`, `skipped`.
    pub outcome: String,
    pub report: Option<LlmsReport>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, rename = "AiPartAverage")]
pub struct PartAverage {
    pub part: Part,
    /// Mean over pages where the part counted, scaled to `max`.
    pub score: Option<u32>,
    pub max: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, rename = "AiSiteOutcome")]
pub struct SiteOutcome {
    pub rule_id: String,
    pub outcome: Outcome,
    pub evidence: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, rename = "AiSummary")]
pub struct Summary {
    pub score: u32,
    /// Mean page score (70% of the site score).
    pub pages_score: u32,
    /// Site checks score (30% of the site score).
    pub site_checks: u32,
    pub parts: Vec<PartAverage>,
    /// Pages scoring 0-19, 20-49, 50-79, 80-100.
    #[ts(type = "[number, number, number, number]")]
    pub distribution: [u64; 4],
    pub top_fixes: Vec<Fix>,
    pub site_outcomes: Vec<SiteOutcome>,
    pub hosts: Vec<HostSummary>,
    pub entity: Option<Entity>,
    pub about_url: Option<String>,
    pub contact_url: Option<String>,
    #[ts(type = "number")]
    pub pages_analyzed: u64,
    #[ts(type = "number")]
    pub provisional_pages: u64,
    pub versions: Versions,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, rename = "AiVersions")]
pub struct Versions {
    pub analysis: u32,
    pub app: String,
    pub bots: u32,
    pub rules: String,
}

/// Opens `file` and runs [`analyze_open`].
pub fn analyze(file: &Path, opts: &AnalyzeOptions, progress: &dyn Fn(Progress)) -> Result<Summary> {
    let mut conn = Connection::open(file)?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    analyze_open(&mut conn, opts, progress)
}

/// The analysis on an open crawl file (the crawl's finish step and 009's
/// analysis phase call this). Replaces any earlier results.
pub fn analyze_open(
    conn: &mut Connection,
    opts: &AnalyzeOptions,
    progress: &dyn Fn(Progress),
) -> Result<Summary> {
    let started = Instant::now();
    let (bots, _clashes) = ai_bots::merged(&opts.custom_bots).map_err(|e| Error::Bots(e.to_string()))?;
    let masks = PurposeMasks::of(&bots);
    let cancelled = || opts.cancel.as_ref().is_some_and(|c| c.load(Ordering::Relaxed));

    store::ensure_schema(conn)?;
    let tx = conn.transaction()?;
    store::clear_results(&tx)?;
    store::drop_finding_indexes(&tx)?;
    let today = crawl_day(&tx)?;
    let has_render = store::has_table(&tx, "render_comparisons")?;

    // Hosts.
    let hosts = read_hosts(&tx)?;
    let mut host_state: HashMap<String, HostState> = HashMap::new();
    let mut summaries: BTreeMap<String, HostSummary> = BTreeMap::new();
    for (i, h) in hosts.iter().enumerate() {
        let source = match h.robots_status.as_str() {
            "ok" | "custom" => RobotsSource::Ok(h.robots_txt.as_deref().unwrap_or("")),
            "unavailable" => RobotsSource::Unavailable,
            "ignored" => RobotsSource::NotFetched,
            _ => RobotsSource::Missing,
        };
        let robots = match source {
            RobotsSource::Ok(_) => "ok",
            RobotsSource::Missing => "missing",
            RobotsSource::Unavailable => "unavailable",
            RobotsSource::NotFetched => "ignored_not_fetched",
        };
        let signals = match source {
            RobotsSource::Ok(t) => robots_signals(RobotsTxt::parse(t.as_bytes()).other_lines()),
            _ => vec![],
        };
        let llms_txt = read_llms(&tx, &h.host_key)?;
        summaries.insert(
            h.host_key.clone(),
            HostSummary {
                host_key: h.host_key.clone(),
                robots: robots.into(),
                access: vec![],
                signals,
                llms_txt,
            },
        );
        host_state.insert(
            h.host_key.clone(),
            HostState {
                robots_known: !matches!(source, RobotsSource::NotFetched),
                tally: HostTally::new(host_policies(source, &bots)),
            },
        );
        progress(Progress::Hosts {
            done: i as u32 + 1,
            total: hosts.len() as u32,
        });
    }

    // Pages.
    tx.execute_batch(
        "DROP TABLE IF EXISTS temp.aeo_title_counts;
         CREATE TEMP TABLE aeo_title_counts (title TEXT PRIMARY KEY, n INTEGER NOT NULL);
         INSERT INTO aeo_title_counts
             SELECT title, COUNT(*) FROM urls
             WHERE internal = 1 AND resource_type = 'html' AND title IS NOT NULL AND title != ''
             GROUP BY title;",
    )?;
    let total: u64 = tx.query_row(
        "SELECT COUNT(*) FROM urls WHERE internal = 1 AND state = 'crawled' AND resource_type = 'html'
         AND status BETWEEN 200 AND 299",
        [],
        |r| r.get::<_, i64>(0).map(|n| n as u64),
    )?;
    // Every core checks pages; this thread only reads and writes the file,
    // so it reads the next chunk while the workers check the last one.
    let workers = opts
        .workers
        .unwrap_or_else(|| std::thread::available_parallelism().map_or(1, |n| n.get()))
        .max(1);
    let mut agg = Aggregate::default();
    let mut done = 0u64;
    let th = opts.thresholds;
    let check = |(row, url, blocked, known): PageJob| {
        page(row, &url, blocked, known, &bots, masks, today, &th, &opts.weights)
    };
    std::thread::scope(|scope| -> Result<()> {
        let pool = Pool::start(scope, workers, &check);
        let mut after = 0i64;
        let mut in_flight: Option<usize> = None;
        loop {
            if cancelled() {
                return Err(Error::Cancelled);
            }
            let rows = read_chunk(&tx, after, has_render)?;
            let next = rows.last().map(|last| last.id);
            let mut inputs = Vec::new();
            for row in rows {
                let Ok(url) = Url::parse(&row.url) else { continue };
                let (blocked, known) = match host_state.get_mut(&row.host_key) {
                    Some(h) => (h.tally.add(&url), h.robots_known),
                    None => (0, false),
                };
                if row.page.is_some() {
                    inputs.push((row, url, blocked, known));
                }
            }
            // Results of the chunk before, in page order, while this one runs.
            let previous = in_flight.take().map(|n| pool.collect(n));
            if next.is_some() {
                in_flight = Some(pool.submit(inputs));
            }
            if let Some(results) = previous {
                write_pages(&tx, &results)?;
                for r in &results {
                    agg.add(r);
                }
                done += results.len() as u64;
                progress(Progress::Pages { done, total });
            }
            match next {
                Some(id) => after = id,
                None => break,
            }
        }
        // Dropping the pool closes the job queue; the scope joins the workers.
        drop(pool);
        Ok(())
    })?;

    // Site.
    progress(Progress::Site);
    for (key, state) in host_state {
        let home = Url::parse(&format!("{key}/")).ok();
        let access = match home {
            Some(h) => state.tally.finish(&h),
            None => vec![],
        };
        if let Some(s) = summaries.get_mut(&key) {
            s.access = access;
        }
    }
    let start_host = store::meta(&tx, "start_url")?
        .and_then(|u| Url::parse(&u).ok())
        .map(|u| spiderlens_engine::crawl::host_key(&u));
    let main_host = start_host
        .filter(|h| summaries.contains_key(h))
        .or_else(|| summaries.keys().next().cloned());
    let (about_url, contact_url) = about_contact(&tx)?;
    let entity = agg.entity();
    let mut site_outcomes = site_checks(
        &summaries,
        main_host.as_deref(),
        &bots,
        &entity,
        about_url.is_some(),
        contact_url.is_some(),
    );
    site_outcomes.sort_by_key(|(r, _, _)| RULES.iter().position(|x| x.rule.id == r.rule.id));
    let pairs: Vec<(&'static AeoRule, Outcome)> = site_outcomes.iter().map(|(r, o, _)| (*r, *o)).collect();
    let (site_score, pages_score, site_part) = score::site_score(&agg.scores, &pairs);
    let failed_site: Vec<&'static AeoRule> = pairs
        .iter()
        .filter(|(_, o)| *o == Outcome::Fail)
        .map(|(r, _)| *r)
        .collect();
    let top_fixes = score::top_fixes(&agg.losses, agg.scores.len() as u64, &failed_site, 10);

    let summary = Summary {
        score: site_score,
        pages_score,
        site_checks: site_part,
        parts: agg.parts(&opts.weights),
        distribution: agg.distribution,
        top_fixes,
        site_outcomes: site_outcomes
            .iter()
            .map(|(r, o, e)| SiteOutcome {
                rule_id: r.rule.id.into(),
                outcome: *o,
                evidence: e.clone(),
            })
            .collect(),
        hosts: summaries.into_values().collect(),
        entity,
        about_url,
        contact_url,
        pages_analyzed: agg.scores.len() as u64,
        provisional_pages: agg.provisional,
        versions: Versions {
            analysis: ANALYSIS_VERSION,
            app: env!("CARGO_PKG_VERSION").into(),
            bots: ai_bots::version(),
            rules: structured::rules_version().into(),
        },
    };
    write_site(&tx, &summary, opts, started.elapsed().as_millis() as i64)?;
    store::create_finding_indexes(&tx)?;
    store::set_meta(&tx, "aeo_analysis", &ANALYSIS_VERSION.to_string())?;
    tx.execute_batch("DROP TABLE IF EXISTS temp.aeo_title_counts;")?;
    tx.commit()?;
    Ok(summary)
}

/// The stored summary, without running anything; `None` when the checks
/// have not run on this file.
pub fn summary(conn: &Connection) -> Result<Option<Summary>> {
    if !store::has_table(conn, "aeo_site")? {
        return Ok(None);
    }
    let json: Option<String> = conn
        .query_row("SELECT site_outcomes FROM aeo_site WHERE id = 1", [], |r| {
            r.get(0)
        })
        .optional()?;
    Ok(json.and_then(|j| serde_json::from_str(&j).ok()))
}

struct HostRow {
    host_key: String,
    robots_status: String,
    robots_txt: Option<String>,
}

struct HostState<'b> {
    robots_known: bool,
    tally: HostTally<'b>,
}

fn read_hosts(conn: &Connection) -> Result<Vec<HostRow>> {
    let mut st = conn.prepare(
        "SELECT h.host_key, h.robots_status, h.robots_txt FROM hosts h
         WHERE EXISTS (SELECT 1 FROM urls u WHERE u.host_key = h.host_key AND u.internal = 1)
         ORDER BY h.host_key",
    )?;
    let rows = st
        .query_map([], |r| {
            Ok(HostRow {
                host_key: r.get(0)?,
                robots_status: r.get(1)?,
                robots_txt: r.get(2)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

/// outcome, final URL, content type, size, zstd body.
type SiteFileRow = (
    String,
    Option<String>,
    Option<String>,
    Option<i64>,
    Option<Vec<u8>>,
);

fn read_llms(conn: &Connection, host_key: &str) -> Result<Option<LlmsFile>> {
    let row: Option<SiteFileRow> = conn
        .query_row(
            "SELECT outcome, final_url, content_type, size, body FROM site_files
             WHERE host_key = ?1 AND path = '/llms.txt'",
            [host_key],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .optional()?;
    let Some((outcome, final_url, content_type, size, packed)) = row else {
        return Ok(None);
    };
    let report = match (outcome.as_str(), packed) {
        ("ok", Some(packed)) => {
            let bytes = body::decompress(&packed, size.unwrap_or(0).max(0) as usize).unwrap_or_default();
            let text = String::from_utf8_lossy(&bytes);
            let url = final_url
                .and_then(|u| Url::parse(&u).ok())
                .or_else(|| Url::parse(&format!("{host_key}/llms.txt")).ok());
            url.map(|u| llms::validate(&text, &u, content_type.as_deref()))
        }
        _ => None,
    };
    Ok(Some(LlmsFile { outcome, report }))
}

/// The day the crawl ran: the last fetch time, so a reopened file gives the
/// same staleness verdicts (SC-006).
fn crawl_day(conn: &Connection) -> Result<Date> {
    let ms: Option<i64> = conn.query_row("SELECT MAX(fetched_at) FROM urls", [], |r| r.get(0))?;
    let ts = ms
        .and_then(|ms| jiff::Timestamp::from_millisecond(ms).ok())
        .unwrap_or_else(jiff::Timestamp::now);
    Ok(ts.to_zoned(jiff::tz::TimeZone::UTC).date())
}

struct PageRaw {
    noindex: bool,
    title: Option<String>,
    title_pages: u32,
    h1: Option<String>,
    h1_count: u32,
    lang: Option<String>,
    encoding: Option<String>,
    partial: bool,
    /// The stored body, still compressed (a worker unpacks it), and its size.
    /// Not read when the crawl stored content facts for the page.
    html: Option<(Vec<u8>, usize)>,
    /// Content facts the crawl read while it had the page parsed.
    facts: Option<Vec<u8>>,
    directives: Vec<Directive>,
    json_ld: Vec<String>,
    sd_blocks: u32,
    render: Option<RenderFacts>,
}

struct UrlRaw {
    id: i64,
    url: String,
    host_key: String,
    /// Set for crawled internal HTML pages with a 2xx status.
    page: Option<PageRaw>,
}

fn read_chunk(conn: &Connection, after: i64, has_render: bool) -> Result<Vec<UrlRaw>> {
    let mut st = conn.prepare_cached(
        "SELECT u.id, u.url, u.host_key,
                (u.state = 'crawled' AND u.resource_type = 'html' AND u.status BETWEEN 200 AND 299) AS is_page,
                COALESCE(u.noindex, 0), u.title, COALESCE(tc.n, 1),
                p.url_id IS NOT NULL, p.h1_1, COALESCE(p.h1_count, 0), p.lang, p.encoding,
                COALESCE(p.partial, 0), COALESCE(p.html_stored, 0)
         FROM urls u
         LEFT JOIN pages p ON p.url_id = u.id
         LEFT JOIN temp.aeo_title_counts tc ON tc.title = u.title
         WHERE u.internal = 1 AND u.id > ?1
         ORDER BY u.id LIMIT ?2",
    )?;
    let mut out = Vec::new();
    let mut rows = st.query(params![after, CHUNK])?;
    while let Some(r) = rows.next()? {
        let id: i64 = r.get(0)?;
        let is_page: bool = r.get(3)?;
        let page = if is_page {
            let extracted: bool = r.get(7)?;
            let html_stored: bool = r.get(13)?;
            Some((
                extracted,
                html_stored,
                PageRaw {
                    noindex: r.get(4)?,
                    title: r.get(5)?,
                    title_pages: r.get(6)?,
                    h1: r.get(8)?,
                    h1_count: r.get(9)?,
                    lang: r.get(10)?,
                    encoding: r.get(11)?,
                    partial: r.get(12)?,
                    html: None,
                    facts: None,
                    directives: vec![],
                    json_ld: vec![],
                    sd_blocks: 0,
                    render: None,
                },
            ))
        } else {
            None
        };
        out.push((id, r.get::<_, String>(1)?, r.get::<_, String>(2)?, page));
    }
    drop(rows);
    // One range query per table for the whole chunk, not one per URL.
    let hi = out.last().map_or(after, |(id, ..)| *id);
    let mut dirs = directives(conn, after, hi)?;
    let mut sd = structured_blocks(conn, after, hi)?;
    let mut facts = page_facts(conn, after, hi)?;
    let mut bodies = html(conn, after, hi)?;
    let mut renders = if has_render {
        render(conn, after, hi)?
    } else {
        HashMap::new()
    };
    let mut result = Vec::with_capacity(out.len());
    for (id, url, host_key, page) in out {
        let page = match page {
            Some((extracted, html_stored, mut p)) => {
                if extracted {
                    p.directives = dirs.remove(&id).unwrap_or_default();
                    let (blocks, json_ld) = sd.remove(&id).unwrap_or_default();
                    p.sd_blocks = blocks;
                    p.json_ld = json_ld;
                }
                if html_stored {
                    p.facts = facts.remove(&id);
                    p.html = bodies.remove(&id);
                }
                p.render = renders.remove(&id);
                // `pages` row present ⇔ extraction ran; kept on the raw row.
                if !extracted {
                    p.h1_count = u32::MAX;
                }
                Some(p)
            }
            None => None,
        };
        result.push(UrlRaw {
            id,
            url,
            host_key,
            page,
        });
    }
    Ok(result)
}

/// Directives of the URLs with ids in `(after, hi]`, in page order.
fn directives(conn: &Connection, after: i64, hi: i64) -> Result<HashMap<i64, Vec<Directive>>> {
    let mut st = conn.prepare_cached(
        "SELECT url_id, directive, value, crawler, source FROM directives
         WHERE url_id > ?1 AND url_id <= ?2 ORDER BY url_id, pos",
    )?;
    let mut out: HashMap<i64, Vec<Directive>> = HashMap::new();
    let mut rows = st.query([after, hi])?;
    while let Some(r) = rows.next()? {
        out.entry(r.get(0)?).or_default().push(Directive {
            directive: r.get(1)?,
            value: r.get(2)?,
            crawler: r.get(3)?,
            source: r.get(4)?,
        });
    }
    Ok(out)
}

/// Structured data blocks per URL in `(after, hi]`: their count and the
/// JSON-LD ones.
fn structured_blocks(conn: &Connection, after: i64, hi: i64) -> Result<HashMap<i64, (u32, Vec<String>)>> {
    let mut st = conn.prepare_cached(
        "SELECT url_id, kind, raw FROM structured_data
         WHERE url_id > ?1 AND url_id <= ?2 ORDER BY url_id, pos",
    )?;
    let mut out: HashMap<i64, (u32, Vec<String>)> = HashMap::new();
    let mut rows = st.query([after, hi])?;
    while let Some(r) = rows.next()? {
        let e = out.entry(r.get(0)?).or_default();
        e.0 += 1;
        let kind: String = r.get(1)?;
        if kind == "json_ld" {
            e.1.push(r.get(2)?);
        }
    }
    Ok(out)
}

/// Content facts the crawl stored for the URLs in `(after, hi]`, in this
/// build's version.
fn page_facts(conn: &Connection, after: i64, hi: i64) -> Result<HashMap<i64, Vec<u8>>> {
    let mut st = conn.prepare_cached(
        "SELECT url_id, data FROM page_facts WHERE url_id > ?1 AND url_id <= ?2 AND kind = ?3 AND version = ?4",
    )?;
    let rows = st
        .query_map(
            params![after, hi, content::FACTS_KIND, content::FACTS_VERSION],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

/// Stored bodies of the URLs in `(after, hi]` that have no content facts,
/// still compressed, with their sizes. Bodies in another codec are left out.
fn html(conn: &Connection, after: i64, hi: i64) -> Result<HashMap<i64, (Vec<u8>, usize)>> {
    let mut st = conn.prepare_cached(
        "SELECT b.url_id, b.codec, b.size, b.body FROM bodies b
         WHERE b.url_id > ?1 AND b.url_id <= ?2
         AND NOT EXISTS (SELECT 1 FROM page_facts f WHERE f.url_id = b.url_id AND f.kind = ?3 AND f.version = ?4)",
    )?;
    let mut out = HashMap::new();
    let mut rows = st.query(params![after, hi, content::FACTS_KIND, content::FACTS_VERSION])?;
    while let Some(r) = rows.next()? {
        let codec: String = r.get(1)?;
        if codec == body::CODEC {
            let size: i64 = r.get(2)?;
            out.insert(r.get(0)?, (r.get(3)?, size.max(0) as usize));
        }
    }
    Ok(out)
}

/// Raw-versus-rendered facts per URL in `(after, hi]` (spec 014).
fn render(conn: &Connection, after: i64, hi: i64) -> Result<HashMap<i64, RenderFacts>> {
    let mut st = conn.prepare_cached(
        "SELECT url_id, dep_text, dep_headings, dep_internal_links, dep_structured,
                raw_has_title, raw_has_description, raw_has_canonical, raw_has_directives
         FROM render_comparisons WHERE url_id > ?1 AND url_id <= ?2",
    )?;
    let mut out = HashMap::new();
    let mut rows = st.query([after, hi])?;
    while let Some(r) = rows.next()? {
        out.insert(
            r.get(0)?,
            RenderFacts {
                dependence: r.get::<_, Option<f64>>(1)?.unwrap_or(0.0),
                dep_headings: r.get(2)?,
                dep_internal_links: r.get(3)?,
                dep_structured: r.get(4)?,
                raw_has_title: r.get::<_, Option<bool>>(5)?.unwrap_or(true),
                raw_has_description: r.get::<_, Option<bool>>(6)?.unwrap_or(true),
                raw_has_canonical: r.get::<_, Option<bool>>(7)?.unwrap_or(true),
                raw_has_directives: r.get::<_, Option<bool>>(8)?.unwrap_or(true),
            },
        );
    }
    Ok(out)
}

/// What the writer stores for one page.
struct PageRow {
    id: i64,
    eval: checks::PageEval,
    score: score::PageScore,
    /// Points the page could earn, in hundredths, for top-fix gains.
    possible: u64,
    blocked: u64,
    js_dependence: Option<f64>,
    counts: Option<Counts>,
    /// `snippet_limits` as stored (JSON), made on the worker.
    snippet_limits: Option<String>,
    /// Failed and unavailable checks as stored, made on the worker so the
    /// writing thread only binds values.
    findings: Vec<FindingRow>,
    entity: Option<Entity>,
}

/// One `aeo_findings` row: rule, outcome, points lost, evidence (JSON).
type FindingRow = (&'static str, &'static str, i64, Option<String>);

struct Counts {
    question_headings: u32,
    direct_answers: u32,
    figures: u32,
    quotations: u32,
    citations: u32,
    lists: u32,
    tables: u32,
    longest_paragraph: u32,
}

#[allow(clippy::too_many_arguments)]
fn page(
    row: UrlRaw,
    url: &Url,
    blocked: u64,
    robots_known: bool,
    bots: &[AiBot],
    masks: PurposeMasks,
    today: Date,
    th: &Thresholds,
    weights: &Weights,
) -> PageRow {
    let p = row.page.expect("only pages are evaluated");
    let extracted = p.h1_count != u32::MAX;
    let limits = ContentLimits {
        long_paragraph_words: th.long_paragraph_words,
    };
    // Facts read during the crawl; otherwise parse the stored HTML (files
    // crawled by an older build, or by a version of the reader since bumped).
    let content: Option<ContentFacts> = match &p.facts {
        Some(data) => content::from_stored(data, limits),
        None => p
            .html
            .as_ref()
            .and_then(|(packed, size)| body::decompress(packed, *size).ok())
            .and_then(|bytes| {
                let doc = extract::parse_body(p.encoding.as_deref(), &bytes, p.partial)?;
                Some(content::read(&doc, url, bytes.len(), p.lang.as_deref(), limits))
            }),
    };
    let blocks: Vec<&str> = p.json_ld.iter().map(String::as_str).collect();
    let (items, mut messages) = structured::read_json_ld(&blocks);
    messages.extend(structured::validate(&items));
    messages.sort_by(|a, b| (a.block, &a.pointer, &a.message).cmp(&(b.block, &b.pointer, &b.message)));
    let facts: StructuredFacts = structured::facts(&items);
    let input = PageInput {
        url,
        robots_known,
        blocked,
        masks,
        bots,
        noindex: p.noindex,
        extracted,
        title: p.title.as_deref(),
        title_pages: p.title_pages,
        h1: p.h1.as_deref(),
        h1_count: if extracted { p.h1_count } else { 0 },
        directives: &p.directives,
        content: content.as_ref(),
        structured: &facts,
        sd_messages: &messages,
        sd_blocks: p.sd_blocks,
        render: p.render.as_ref(),
        today,
        type_override: None,
    };
    let eval = checks::evaluate(&input, th);
    let score = score::page_score(
        &eval.outcomes,
        weights,
        eval.blocked_for_all_search,
        th.blocked_cap,
    );
    let possible = eval
        .outcomes
        .iter()
        .filter(|(r, o)| r.points > 0 && matches!(o, Outcome::Pass | Outcome::Fail))
        .map(|(r, _)| score::scaled_points(r, weights))
        .sum();
    let snippet_limits = snippet_summary(&p.directives, content.as_ref()).map(|v| v.to_string());
    let lost: HashMap<&str, u64> = score.lost.iter().copied().collect();
    let findings = eval
        .outcomes
        .iter()
        .filter_map(|(rule, outcome)| {
            let label = match outcome {
                Outcome::Fail => "fail",
                Outcome::NotAvailable => "not_available",
                _ => return None,
            };
            let evidence = eval
                .evidence
                .iter()
                .find(|(id, _)| *id == rule.rule.id)
                .map(|(_, v)| v.to_string());
            Some((
                rule.rule.id,
                label,
                lost.get(rule.rule.id).copied().unwrap_or(0) as i64,
                evidence,
            ))
        })
        .collect();
    PageRow {
        id: row.id,
        counts: content.as_ref().map(|c| Counts {
            question_headings: c.questions.len() as u32,
            direct_answers: c
                .questions
                .iter()
                .filter(|q| q.answer.is_some() && q.answer_words <= th.direct_answer_words)
                .count() as u32,
            figures: c.figures,
            quotations: c.quotations,
            citations: c.citations,
            lists: c.lists,
            tables: c.tables,
            longest_paragraph: c.longest_paragraph,
        }),
        eval,
        score,
        possible,
        blocked,
        js_dependence: p.render.map(|r| r.dependence * 100.0),
        snippet_limits,
        findings,
        entity: facts.org.clone(),
    }
}

/// FR-013: every directive that limits AI use or quoting, by crawler.
fn snippet_summary(directives: &[Directive], content: Option<&ContentFacts>) -> Option<Value> {
    let mut by: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for d in directives {
        let name = d.directive.to_ascii_lowercase();
        if matches!(
            name.as_str(),
            "nosnippet" | "max-snippet" | "noarchive" | "nocache" | "noai" | "noimageai"
        ) {
            let v = match &d.value {
                Some(v) => format!("{name}:{v}"),
                None => name,
            };
            by.entry(d.crawler.clone()).or_default().push(v);
        }
    }
    let (dn, ds) = content.map_or((0, 0), |c| (c.data_nosnippet, c.data_snippet));
    if by.is_empty() && dn == 0 && ds == 0 {
        return None;
    }
    Some(json!({ "directives": by, "data_nosnippet": dn, "data_snippet": ds }))
}

type PageJob = (UrlRaw, Url, u64, bool);

/// Pages per batch handed to one worker, so the cores stay busy on uneven
/// pages.
const BATCH: usize = 32;

/// Page workers that live for the whole pass. One chunk is in flight at a
/// time: [`Pool::submit`] hands it out in batches and [`Pool::collect`]
/// returns its results in page order.
struct Pool {
    jobs: std::sync::mpsc::Sender<(usize, Vec<PageJob>)>,
    results: std::sync::mpsc::Receiver<(usize, Vec<PageRow>)>,
}

impl Pool {
    fn start<'scope, 'env, F>(
        s: &'scope std::thread::Scope<'scope, 'env>,
        workers: usize,
        check: &'env F,
    ) -> Pool
    where
        F: Fn(PageJob) -> PageRow + Sync,
    {
        let (jobs, job_rx) = std::sync::mpsc::channel::<(usize, Vec<PageJob>)>();
        let (result_tx, results) = std::sync::mpsc::channel();
        let job_rx = std::sync::Arc::new(std::sync::Mutex::new(job_rx));
        for _ in 0..workers {
            let job_rx = job_rx.clone();
            let result_tx = result_tx.clone();
            s.spawn(move || {
                loop {
                    // The guard drops at the end of the statement, so other
                    // workers take batches while this one checks pages.
                    let job = job_rx.lock().expect("job queue").recv();
                    let Ok((n, batch)) = job else { break };
                    let rows: Vec<PageRow> = batch.into_iter().map(check).collect();
                    if result_tx.send((n, rows)).is_err() {
                        break;
                    }
                }
            });
        }
        Pool { jobs, results }
    }

    /// Hands out one chunk; returns how many batches it made.
    fn submit(&self, inputs: Vec<PageJob>) -> usize {
        let mut n = 0;
        let mut it = inputs.into_iter().peekable();
        while it.peek().is_some() {
            let batch: Vec<PageJob> = it.by_ref().take(BATCH).collect();
            self.jobs.send((n, batch)).expect("page workers are running");
            n += 1;
        }
        n
    }

    /// The results of the chunk in flight, in page order.
    fn collect(&self, batches: usize) -> Vec<PageRow> {
        let mut parts: Vec<Option<Vec<PageRow>>> = (0..batches).map(|_| None).collect();
        for _ in 0..batches {
            let (n, rows) = self.results.recv().expect("page worker panicked");
            parts[n] = Some(rows);
        }
        parts.into_iter().flatten().flatten().collect()
    }
}

fn write_pages(conn: &Connection, rows: &[PageRow]) -> Result<()> {
    let mut page = conn.prepare_cached(
        "INSERT INTO aeo_pages (url_id, page_type, page_type_reason, score, access, readable, answer, trust,
             structured, checks_used, checks_total, provisional, blocked_bots, flags, version_used, js_dependence,
             published, modified, dates_agree, author, question_headings, direct_answers, figures, quotations,
             citations, lists, tables, longest_paragraph, snippet_limits)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20,
             ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29)",
    )?;
    let mut finding = conn.prepare_cached(
        "INSERT INTO aeo_findings (url_id, rule_id, outcome, points_lost, evidence) VALUES (?1, ?2, ?3, ?4, ?5)",
    )?;
    for r in rows {
        let part = |p: Part| r.score.parts.iter().find(|x| x.part == p).and_then(|x| x.score);
        let c = r.counts.as_ref();
        page.execute(params![
            r.id,
            r.eval.page_type.as_str(),
            r.eval.page_type_reason,
            r.score.score,
            part(Part::Access),
            part(Part::Readable),
            part(Part::Answer),
            part(Part::Trust),
            part(Part::Structured),
            r.score.checks_used,
            r.score.checks_total,
            r.score.provisional,
            // SQLite integers are signed; the mask uses at most 64 bits.
            r.blocked as i64,
            r.eval.flags as i64,
            r.eval.version_used,
            r.js_dependence,
            r.eval.published,
            r.eval.modified,
            r.eval.dates_agree,
            r.eval.author,
            c.map(|c| c.question_headings),
            c.map(|c| c.direct_answers),
            c.map(|c| c.figures),
            c.map(|c| c.quotations),
            c.map(|c| c.citations),
            c.map(|c| c.lists),
            c.map(|c| c.tables),
            c.map(|c| c.longest_paragraph),
            r.snippet_limits,
        ])?;
        for (rule, label, lost, evidence) in &r.findings {
            finding.execute(params![r.id, rule, label, lost, evidence])?;
        }
    }
    Ok(())
}

/// Running totals over the page pass.
#[derive(Default)]
struct Aggregate {
    scores: Vec<u32>,
    distribution: [u64; 4],
    provisional: u64,
    /// Per rule: lost page-score hundredths summed over pages, and pages.
    losses: BTreeMap<&'static str, (u64, u64)>,
    /// Per part: (sum of part scores, pages where the part counted).
    parts: BTreeMap<Part, (u64, u64)>,
    /// Organisation names seen, with the first full entity for each.
    entities: BTreeMap<String, (u64, Entity)>,
    home_entity: Option<Entity>,
}

impl Aggregate {
    fn add(&mut self, r: &PageRow) {
        let s = r.score.score;
        self.scores.push(s);
        self.distribution[match s {
            0..=19 => 0,
            20..=49 => 1,
            50..=79 => 2,
            _ => 3,
        }] += 1;
        if r.score.provisional {
            self.provisional += 1;
        }
        for (id, pts) in &r.score.lost {
            let e = self.losses.entry(id).or_default();
            e.0 += (pts * 10_000).checked_div(r.possible).unwrap_or(0);
            e.1 += 1;
        }
        for p in &r.score.parts {
            if let Some(v) = p.score {
                let e = self.parts.entry(p.part).or_default();
                e.0 += u64::from(v);
                e.1 += 1;
            }
        }
        if let Some(e) = &r.entity {
            let name = e.name.clone().unwrap_or_default();
            if r.eval.page_type_reason == "home page" && self.home_entity.is_none() {
                self.home_entity = Some(e.clone());
            }
            self.entities.entry(name).or_insert_with(|| (0, e.clone())).0 += 1;
        }
    }

    fn parts(&self, weights: &Weights) -> Vec<PartAverage> {
        Part::PAGE
            .iter()
            .map(|&part| PartAverage {
                part,
                score: self
                    .parts
                    .get(&part)
                    .filter(|(_, n)| *n > 0)
                    .map(|(sum, n)| ((sum + n / 2) / n) as u32),
                max: weights.of(part),
            })
            .collect()
    }

    /// FR-029: the home page's entity, else the one most pages state.
    fn entity(&self) -> Option<Entity> {
        self.home_entity.clone().or_else(|| {
            self.entities
                .iter()
                .max_by(|a, b| a.1.0.cmp(&b.1.0).then(b.0.cmp(a.0)))
                .map(|(_, (_, e))| e.clone())
        })
    }
}

/// FR-030: crawled about and contact pages, by URL pattern.
fn about_contact(conn: &Connection) -> Result<(Option<String>, Option<String>)> {
    let find = |patterns: &[&str]| -> Result<Option<String>> {
        let cond = patterns
            .iter()
            .map(|_| "LOWER(url) LIKE ?")
            .collect::<Vec<_>>()
            .join(" OR ");
        let sql = format!(
            "SELECT url FROM urls WHERE internal = 1 AND state = 'crawled' AND status BETWEEN 200 AND 299
             AND resource_type = 'html' AND ({cond}) ORDER BY LENGTH(url), url LIMIT 1"
        );
        let args: Vec<String> = patterns.iter().map(|p| format!("%/{p}%")).collect();
        Ok(conn
            .query_row(&sql, rusqlite::params_from_iter(args.iter()), |r| r.get(0))
            .optional()?)
    };
    Ok((
        find(&["about", "about-us", "who-we-are", "odot", "אודות"])?,
        find(&["contact", "contact-us", "kontakt", "צור-קשר"])?,
    ))
}

fn site_checks(
    hosts: &BTreeMap<String, HostSummary>,
    main_host: Option<&str>,
    bots: &[AiBot],
    entity: &Option<Entity>,
    about: bool,
    contact: bool,
) -> Vec<(&'static AeoRule, Outcome, Option<String>)> {
    let rule = |id: &str| crate::rules::by_id(id).expect("site rule exists");
    let mut out = Vec::new();
    let main = main_host.and_then(|h| hosts.get(h));

    let unavailable: Vec<&str> = hosts
        .values()
        .filter(|h| h.robots == "unavailable")
        .map(|h| h.host_key.as_str())
        .collect();
    let any_known = hosts.values().any(|h| h.robots != "ignored_not_fetched");
    out.push((
        rule("ai.site.robots_unavailable"),
        if !any_known {
            Outcome::NotAvailable
        } else if unavailable.is_empty() {
            Outcome::Pass
        } else {
            Outcome::Fail
        },
        (!unavailable.is_empty()).then(|| unavailable.join(", ")),
    ));

    let search_blocked: Vec<String> = main
        .map(|h| {
            h.access
                .iter()
                .filter(|a| matches!(a.home_verdict, Verdict::Blocked | Verdict::Unavailable))
                .filter(|a| {
                    bots.iter()
                        .any(|b| b.token == a.bot && b.purpose == BotPurpose::Search)
                })
                .map(|a| a.bot.clone())
                .collect()
        })
        .unwrap_or_default();
    out.push((
        rule("ai.site.search_access"),
        match main {
            None => Outcome::NotAvailable,
            Some(h) if h.robots == "ignored_not_fetched" => Outcome::NotAvailable,
            _ if search_blocked.is_empty() => Outcome::Pass,
            _ => Outcome::Fail,
        },
        (!search_blocked.is_empty()).then(|| search_blocked.join(", ")),
    ));

    let entity_ok = entity.as_ref().is_some_and(|e| e.name.is_some());
    out.push((
        rule("ai.site.entity"),
        if entity_ok { Outcome::Pass } else { Outcome::Fail },
        None,
    ));

    out.push((
        rule("ai.site.about_contact"),
        if about && contact {
            Outcome::Pass
        } else {
            Outcome::Fail
        },
        match (about, contact) {
            (true, true) => None,
            (false, true) => Some("no about page".into()),
            (true, false) => Some("no contact page".into()),
            (false, false) => Some("no about page and no contact page".into()),
        },
    ));

    let llms = main.and_then(|h| h.llms_txt.as_ref());
    out.push((
        rule("ai.site.llms_txt"),
        match llms {
            None => Outcome::NotAvailable,
            Some(f) if f.report.as_ref().is_some_and(LlmsReport::is_valid) => Outcome::Pass,
            Some(_) => Outcome::Fail,
        },
        llms.map(|f| match &f.report {
            Some(r) if !r.is_valid() => format!("{} problems", r.problems.len()),
            Some(_) => "valid".into(),
            None => f.outcome.clone(),
        }),
    ));
    out
}

fn write_site(conn: &Connection, s: &Summary, opts: &AnalyzeOptions, duration_ms: i64) -> Result<()> {
    let mut host =
        conn.prepare("INSERT INTO aeo_hosts (host_key, robots, signals, llms_txt) VALUES (?1, ?2, ?3, ?4)")?;
    let mut access = conn.prepare(
        "INSERT INTO aeo_bot_access (host_key, bot, home_verdict, matched_group, urls_total, urls_blocked, rules)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
    )?;
    for h in &s.hosts {
        host.execute(params![
            h.host_key,
            h.robots,
            to_json(&h.signals),
            h.llms_txt.as_ref().map(to_json)
        ])?;
        for a in &h.access {
            access.execute(params![
                h.host_key,
                a.bot,
                to_json(&a.home_verdict).trim_matches('"'),
                a.matched_group,
                a.urls_total as i64,
                a.urls_blocked as i64,
                to_json(&a.rules),
            ])?;
        }
    }
    let mut finding = conn.prepare(
        "INSERT INTO aeo_findings (url_id, rule_id, outcome, points_lost, evidence) VALUES (NULL, ?1, ?2, ?3, ?4)",
    )?;
    for o in &s.site_outcomes {
        let label = match o.outcome {
            Outcome::Fail => "fail",
            Outcome::NotAvailable => "not_available",
            _ => continue,
        };
        let pts = crate::rules::by_id(&o.rule_id).map_or(0, |r| r.points * 100);
        finding.execute(params![
            o.rule_id,
            label,
            if label == "fail" { pts } else { 0 },
            o.evidence.as_ref().map(|e| json!({ "detail": e }).to_string())
        ])?;
    }
    let part = |p: Part| s.parts.iter().find(|x| x.part == p).and_then(|x| x.score);
    conn.execute(
        "INSERT INTO aeo_site (id, score, access, readable, answer, trust, structured, site_checks, distribution,
             entity, pages_found, top_fixes, site_outcomes, profile, versions, ran_at, pages_analyzed, duration_ms)
         VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
        params![
            s.score,
            part(Part::Access),
            part(Part::Readable),
            part(Part::Answer),
            part(Part::Trust),
            part(Part::Structured),
            s.site_checks,
            to_json(&s.distribution),
            to_json(&s.entity),
            json!({ "about": s.about_url, "contact": s.contact_url }).to_string(),
            to_json(&s.top_fixes),
            to_json(s),
            json!({ "weights": opts.weights, "thresholds": opts.thresholds }).to_string(),
            to_json(&s.versions),
            jiff::Timestamp::now().as_millisecond(),
            s.pages_analyzed as i64,
            duration_ms,
        ],
    )?;
    Ok(())
}

/// JSON with sorted object keys, so equal results give equal bytes (SC-006).
fn to_json<T: Serialize>(v: &T) -> String {
    let value = serde_json::to_value(v).expect("results serialize");
    sorted(value).to_string()
}

fn sorted(v: Value) -> Value {
    match v {
        Value::Object(m) => {
            let b: BTreeMap<String, Value> = m.into_iter().map(|(k, v)| (k, sorted(v))).collect();
            Value::Object(b.into_iter().collect())
        }
        Value::Array(a) => Value::Array(a.into_iter().map(sorted).collect()),
        other => other,
    }
}

/// One check's result on a page, for the explanation (FR-043).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, rename = "AiPageCheck")]
pub struct PageCheck {
    pub rule_id: String,
    /// `fail` or `not_available`; checks that passed are not stored row by row.
    pub outcome: String,
    /// Points lost, in hundredths.
    #[ts(type = "number")]
    pub points_lost: i64,
    pub evidence: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, rename = "AiPageResult")]
pub struct PageResult {
    pub url: String,
    pub page_type: String,
    pub page_type_reason: Option<String>,
    #[ts(type = "number | null")]
    pub score: Option<i64>,
    pub provisional: bool,
    pub version_used: String,
    pub published: Option<String>,
    pub modified: Option<String>,
    pub author: Option<String>,
    /// Built-in crawlers that may not fetch the page, by name.
    pub blocked_crawlers: Vec<String>,
    pub checks: Vec<PageCheck>,
}

/// Which page [`page_result`] reads.
#[derive(Debug, Clone, Copy)]
pub enum PageKey<'a> {
    Id(i64),
    Url(&'a str),
}

/// The stored result for one URL; `None` when the URL has no AI result.
pub fn page_result(conn: &Connection, key: PageKey<'_>) -> Result<Option<PageResult>> {
    if !store::has_table(conn, "aeo_pages")? {
        return Ok(None);
    }
    let (by_id, by_url) = match key {
        PageKey::Id(id) => (Some(id), None),
        PageKey::Url(url) => (None, Some(url)),
    };
    let row = conn
        .query_row(
            "SELECT u.id, u.url, a.page_type, a.page_type_reason, a.score, a.provisional, a.version_used,
                    a.published, a.modified, a.author, a.blocked_bots
             FROM urls u JOIN aeo_pages a ON a.url_id = u.id
             WHERE (?1 IS NOT NULL AND u.id = ?1) OR (?2 IS NOT NULL AND u.url = ?2)",
            params![by_id, by_url],
            |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    PageResult {
                        url: r.get(1)?,
                        page_type: r.get(2)?,
                        page_type_reason: r.get(3)?,
                        score: r.get(4)?,
                        provisional: r.get(5)?,
                        version_used: r.get(6)?,
                        published: r.get(7)?,
                        modified: r.get(8)?,
                        author: r.get(9)?,
                        blocked_crawlers: names_of(r.get::<_, i64>(10)? as u64),
                        checks: vec![],
                    },
                ))
            },
        )
        .optional()?;
    let Some((id, mut page)) = row else {
        return Ok(None);
    };
    let mut st = conn.prepare(
        "SELECT rule_id, outcome, points_lost, evidence FROM aeo_findings WHERE url_id = ?1
         ORDER BY points_lost DESC, rule_id",
    )?;
    page.checks = st
        .query_map([id], |r| {
            Ok(PageCheck {
                rule_id: r.get(0)?,
                outcome: r.get(1)?,
                points_lost: r.get(2)?,
                evidence: r.get(3)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(Some(page))
}

fn names_of(mask: u64) -> Vec<String> {
    ai_bots::all()
        .iter()
        .filter(|b| b.index < 64 && mask & (1 << b.index) != 0)
        .map(|b| b.name.clone())
        .collect()
}

/// A draft llms.txt from the crawl's indexable pages (FR-047). Never uploaded.
pub fn draft_llms_txt(conn: &Connection, max_links: usize) -> Result<String> {
    let start = store::meta(conn, "start_url")?.and_then(|u| Url::parse(&u).ok());
    let host = start.as_ref().map(spiderlens_engine::crawl::host_key);
    let site_name = start
        .as_ref()
        .and_then(|u| u.host_str().map(str::to_string))
        .unwrap_or_else(|| "Site".into());
    let mut st = conn.prepare(
        "SELECT u.url, COALESCE(u.title, ''), p.meta_description,
                (SELECT COUNT(*) FROM links l WHERE l.to_id = u.id)
         FROM urls u LEFT JOIN pages p ON p.url_id = u.id
         WHERE u.internal = 1 AND u.state = 'crawled' AND u.resource_type = 'html'
           AND u.status BETWEEN 200 AND 299 AND COALESCE(u.noindex, 0) = 0
           AND (u.canonical IS NULL OR u.canonical = u.url)
           AND (?1 IS NULL OR u.host_key = ?1)
         ORDER BY u.id",
    )?;
    let pages: Vec<llms::DraftPage> = st
        .query_map([host], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, i64>(3)?,
            ))
        })?
        .filter_map(|row| {
            let (url, title, description, inlinks) = row.ok()?;
            Some(llms::DraftPage {
                url: Url::parse(&url).ok()?,
                title,
                description,
                inlinks: inlinks as u64,
            })
        })
        .collect();
    let summary = pages
        .iter()
        .find(|p| p.url.path() == "/")
        .and_then(|p| p.description.clone());
    Ok(llms::draft(&site_name, summary.as_deref(), &pages, max_links))
}
