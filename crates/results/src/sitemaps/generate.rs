//! XML sitemap generation from a crawl (spec 011 US5, plan D9).
//!
//! One selection query, filtered by the user's patterns in Rust, fills a
//! temporary table of page ids; one ordered scan of it writes the entries.
//! Files are split before either protocol limit would be passed, written
//! under temporary names and renamed only when everything succeeded, and the
//! output is deterministic: URL order, fixed attribute order and a gzip
//! header without a timestamp (research R7, SC-006).

use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};

use rusqlite::OptionalExtension;
use serde::Serialize;
use spiderlens_config::{
    Changefreq, Decision, ImageEntries, LastmodSource, PrioritySource, SitemapGenerateOptions,
    StoredCrawlConfig, UrlFilter,
};
use ts_rs::TS;
use url::Url;

use super::xml::{NS_IMAGE, NS_SITEMAP, NS_XHTML, escape};
use crate::Reader;

/// The protocol's limits per file.
pub const MAX_URLS: u32 = 50_000;
pub const MAX_BYTES: u64 = 50 * 1024 * 1024;
const MAX_IMAGES: usize = 1_000;

/// Where generated files go.
pub enum Output<'a> {
    /// Write into `dir`; existing files with the same names are replaced
    /// only when `replace` is set.
    Folder { dir: &'a Path, replace: bool },
    /// Count only (the sheet's live preview, `--dry-run`). With a folder,
    /// the report lists the files there that would be replaced.
    DryRun { dir: Option<&'a Path> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct GeneratedFile {
    pub name: String,
    /// `pages`, `images` or `index`.
    pub kind: String,
    #[ts(type = "number")]
    pub urls: u64,
    #[ts(type = "number")]
    pub images: u64,
    /// Uncompressed size.
    #[ts(type = "number")]
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct GenerateReport {
    pub files: Vec<GeneratedFile>,
    #[ts(type = "number")]
    pub pages: u64,
    #[ts(type = "number")]
    pub images: u64,
    /// Pages the default selection kept but a pattern left out.
    #[ts(type = "number")]
    pub skipped_by_pattern: u64,
    /// Files in the chosen folder that these names would replace.
    pub existing: Vec<String>,
    /// The base URL the files and the index use.
    pub base_url: String,
}

#[derive(Debug, thiserror::Error)]
pub enum GenerateError {
    #[error("No page matches these options.")]
    NoPages,
    #[error("These files already exist: {}", .0.join(", "))]
    Exists(Vec<String>),
    #[error("{0}")]
    Invalid(String),
    #[error("could not write the sitemap: {0}")]
    Io(#[from] io::Error),
    #[error(transparent)]
    Read(#[from] crate::Error),
}

impl From<rusqlite::Error> for GenerateError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Read(e.into())
    }
}

/// Internal HTML pages that answered 2xx, then the options' indexability
/// choices (FR-020). Robots-blocked pages were never fetched, so they are
/// not crawled and never selected.
fn selection(o: &SitemapGenerateOptions) -> String {
    let mut s = String::from(
        "SELECT u.id, u.url FROM urls u WHERE u.internal = 1 AND u.state = 'crawled' \
         AND u.error_class IS NULL AND u.status BETWEEN 200 AND 299 \
         AND content_kind(u.content_type) = 'html'",
    );
    if !o.include_noindex {
        s.push_str(" AND COALESCE(u.noindex, 0) = 0");
    }
    if !o.include_canonicalised {
        s.push_str(" AND (u.canonical IS NULL OR u.canonical = u.url)");
    }
    s
}

pub fn generate(
    reader: &Reader,
    opts: &SitemapGenerateOptions,
    out: Output<'_>,
) -> Result<GenerateReport, GenerateError> {
    if !(1..=MAX_URLS).contains(&opts.max_urls_per_file) {
        return Err(GenerateError::Invalid(
            "URLs per file must be between 1 and 50,000.".into(),
        ));
    }
    if !opts
        .file_prefix
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        || opts.file_prefix.is_empty()
    {
        return Err(GenerateError::Invalid(
            "The file name may use letters, digits, '.', '-' and '_'.".into(),
        ));
    }
    let filter = UrlFilter::new(&opts.include, &opts.exclude)
        .map_err(|_| GenerateError::Invalid("A pattern is not valid.".into()))?;
    let fixed_lastmod = match &opts.lastmod {
        LastmodSource::Fixed(d) => Some(
            jiff::civil::Date::strptime("%Y-%m-%d", d.trim())
                .map_err(|_| GenerateError::Invalid("Enter the date as YYYY-MM-DD.".into()))?
                .to_string(),
        ),
        _ => None,
    };
    if let PrioritySource::Fixed(p) = opts.priority
        && !(0.0..=1.0).contains(&p)
    {
        return Err(GenerateError::Invalid(
            "Priority must be between 0.0 and 1.0.".into(),
        ));
    }
    let conn = &reader.conn;
    let base = base_url(reader, opts)?;

    // 1. Selection into a temporary table, so hreflang alternates can be
    //    limited to pages that are themselves written (FR-021).
    conn.execute_batch(
        "DROP TABLE IF EXISTS temp.sitemap_selected;
         CREATE TEMP TABLE sitemap_selected (id INTEGER PRIMARY KEY);",
    )?;
    let mut skipped = 0u64;
    {
        let mut insert = conn.prepare("INSERT INTO temp.sitemap_selected (id) VALUES (?1)")?;
        let mut q = conn.prepare(&selection(opts))?;
        let mut rows = q.query([])?;
        while let Some(r) = rows.next()? {
            let id: i64 = r.get(0)?;
            let url: String = r.get(1)?;
            if matches!(filter.decide(&url), Decision::Crawl { .. }) {
                insert.execute([id])?;
            } else {
                skipped += 1;
            }
        }
    }

    let dry = matches!(out, Output::DryRun { .. });
    let dir = match out {
        Output::Folder { dir, .. } => Some(dir),
        Output::DryRun { dir } => dir,
    };
    let replace = matches!(out, Output::Folder { replace: true, .. });
    let gz = opts.gzip;
    let ns_image = opts.images == ImageEntries::Inline;
    let mut pages_out = Splitter::new(
        "pages",
        opts.max_urls_per_file,
        ns_image,
        opts.hreflang,
        gz,
        dir,
        dry,
    );
    let mut images_out = Splitter::new("images", opts.max_urls_per_file, true, false, gz, dir, dry);

    // 2. One ordered scan.
    let mut q = conn.prepare(
        "SELECT u.id, u.url, u.depth, u.fetched_at FROM temp.sitemap_selected s JOIN urls u ON u.id = s.id ORDER BY u.url",
    )?;
    let mut images_q = conn.prepare(
        "SELECT DISTINCT t.url, t.internal FROM links l JOIN urls t ON t.id = l.to_id
         WHERE l.from_id = ?1 AND l.kind = 'image' AND t.error_class IS NULL AND t.status BETWEEN 200 AND 299
         ORDER BY t.url",
    )?;
    let mut alt_q = conn.prepare(
        "SELECT DISTINCT l.lang, t.url FROM links l JOIN urls t ON t.id = l.to_id
         JOIN temp.sitemap_selected s ON s.id = t.id
         WHERE l.from_id = ?1 AND l.kind = 'hreflang' AND l.lang IS NOT NULL ORDER BY l.lang, t.url",
    )?;
    let mut header_q = conn.prepare("SELECT headers FROM headers WHERE url_id = ?1")?;
    let image_hosts: Vec<String> = opts
        .image_hosts
        .iter()
        .map(|h| h.trim().to_ascii_lowercase())
        .collect();

    let mut pages = 0u64;
    let mut images_total = 0u64;
    let mut entry = String::new();
    let mut rows = q.query([])?;
    while let Some(r) = rows.next()? {
        let id: i64 = r.get(0)?;
        let url: String = r.get(1)?;
        let depth: i64 = r.get(2)?;
        let fetched_at: Option<i64> = r.get(3)?;

        let lastmod = match &opts.lastmod {
            LastmodSource::None => None,
            LastmodSource::Fixed(_) => fixed_lastmod.clone(),
            LastmodSource::CrawlDate => fetched_at.and_then(w3c_from_ms),
            LastmodSource::LastModifiedHeader => header_q
                .query_row([id], |r| r.get::<_, String>(0))
                .optional()?
                .and_then(|h| last_modified(&h)),
        };
        let images: Vec<String> = if opts.images == ImageEntries::Off {
            Vec::new()
        } else {
            let mut v = Vec::new();
            let mut ir = images_q.query([id])?;
            while let Some(r) = ir.next()? {
                let u: String = r.get(0)?;
                let internal: bool = r.get(1)?;
                if internal || host_allowed(&u, &image_hosts) {
                    v.push(u);
                    if v.len() == MAX_IMAGES {
                        break;
                    }
                }
            }
            v
        };
        let alternates: Vec<(String, String)> = if opts.hreflang {
            alt_q
                .query_map([id], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<rusqlite::Result<_>>()?
        } else {
            Vec::new()
        };

        entry.clear();
        entry.push_str("<url><loc>");
        escape(&url, &mut entry);
        entry.push_str("</loc>");
        if let Some(l) = &lastmod {
            entry.push_str("<lastmod>");
            entry.push_str(l);
            entry.push_str("</lastmod>");
        }
        if let Some(c) = opts.changefreq {
            entry.push_str("<changefreq>");
            entry.push_str(Changefreq::as_str(c));
            entry.push_str("</changefreq>");
        }
        match opts.priority {
            PrioritySource::Off => {}
            PrioritySource::Fixed(p) => entry.push_str(&format!("<priority>{p:.1}</priority>")),
            PrioritySource::ByDepth => {
                let p = match depth {
                    0 => "1.0",
                    1 => "0.8",
                    2 => "0.6",
                    3 => "0.4",
                    _ => "0.2",
                };
                entry.push_str(&format!("<priority>{p}</priority>"));
            }
        }
        if opts.images == ImageEntries::Inline {
            push_images(&images, &mut entry);
        }
        for (lang, href) in &alternates {
            entry.push_str("<xhtml:link rel=\"alternate\" hreflang=\"");
            escape(lang, &mut entry);
            entry.push_str("\" href=\"");
            escape(href, &mut entry);
            entry.push_str("\"/>");
        }
        entry.push_str("</url>\n");
        pages_out.write(
            &entry,
            images.len() as u64 * u64::from(opts.images == ImageEntries::Inline),
        )?;
        pages += 1;

        if opts.images == ImageEntries::Separate && !images.is_empty() {
            entry.clear();
            entry.push_str("<url><loc>");
            escape(&url, &mut entry);
            entry.push_str("</loc>");
            push_images(&images, &mut entry);
            entry.push_str("</url>\n");
            images_out.write(&entry, images.len() as u64)?;
        }
        if opts.images != ImageEntries::Off {
            images_total += images.len() as u64;
        }
    }
    drop(rows);
    conn.execute_batch("DROP TABLE IF EXISTS temp.sitemap_selected;")?;

    if pages == 0 {
        pages_out.discard();
        images_out.discard();
        return Err(GenerateError::NoPages);
    }

    // 3. Names: one file alone is `prefix.xml`; otherwise numbered parts and
    //    an index.
    let ext = if gz { ".xml.gz" } else { ".xml" };
    let p = &opts.file_prefix;
    let page_parts = pages_out.finish()?;
    let image_parts = images_out.finish()?;
    let single = page_parts.len() == 1 && image_parts.is_empty();
    let mut named: Vec<(Part, GeneratedFile)> = Vec::new();
    let n_pages = page_parts.len();
    for (i, part) in page_parts.into_iter().enumerate() {
        let name = if single {
            format!("{p}{ext}")
        } else if n_pages == 1 {
            format!("{p}-pages{ext}")
        } else {
            format!("{p}-{}{ext}", i + 1)
        };
        let f = GeneratedFile {
            name,
            kind: "pages".into(),
            urls: part.urls,
            images: part.images,
            bytes: part.bytes,
        };
        named.push((part, f));
    }
    for (i, part) in image_parts.into_iter().enumerate() {
        let name = format!("{p}-images-{}{ext}", i + 1);
        let f = GeneratedFile {
            name,
            kind: "images".into(),
            urls: part.urls,
            images: part.images,
            bytes: part.bytes,
        };
        named.push((part, f));
    }
    let index = (!single).then(|| {
        let mut x = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<sitemapindex xmlns=\"");
        x.push_str(NS_SITEMAP);
        x.push_str("\">\n");
        for (_, f) in &named {
            x.push_str("<sitemap><loc>");
            escape(&format!("{base}{}", f.name), &mut x);
            x.push_str("</loc></sitemap>\n");
        }
        x.push_str("</sitemapindex>\n");
        x
    });

    let mut files: Vec<GeneratedFile> = named.iter().map(|(_, f)| f.clone()).collect();
    if let Some(x) = &index {
        files.push(GeneratedFile {
            name: format!("{p}_index{ext}"),
            kind: "index".into(),
            urls: named.len() as u64,
            images: 0,
            bytes: x.len() as u64,
        });
    }
    let existing: Vec<String> = match dir {
        Some(d) => files
            .iter()
            .filter(|f| d.join(&f.name).exists())
            .map(|f| f.name.clone())
            .collect(),
        None => Vec::new(),
    };
    let report = GenerateReport {
        files,
        pages,
        images: images_total,
        skipped_by_pattern: skipped,
        existing,
        base_url: base,
    };
    if dry {
        return Ok(report);
    }
    let dir = dir.expect("folder output has a dir");
    if !report.existing.is_empty() && !replace {
        for (part, _) in &named {
            part.remove();
        }
        return Err(GenerateError::Exists(report.existing));
    }
    if let Some(x) = &index {
        let tmp = dir.join(format!(".{p}_index.tmp"));
        let mut w = open_writer(&tmp, gz)?;
        w.write_all(x.as_bytes())?;
        w.finish()?;
        std::fs::rename(&tmp, dir.join(format!("{p}_index{ext}")))?;
    }
    for (part, f) in &named {
        if let Some(tmp) = &part.tmp {
            std::fs::rename(tmp, dir.join(&f.name))?;
        }
    }
    Ok(report)
}

fn push_images(images: &[String], entry: &mut String) {
    for img in images {
        entry.push_str("<image:image><image:loc>");
        escape(img, entry);
        entry.push_str("</image:loc></image:image>");
    }
}

fn host_allowed(u: &str, hosts: &[String]) -> bool {
    !hosts.is_empty()
        && Url::parse(u)
            .ok()
            .and_then(|u| u.host_str().map(str::to_ascii_lowercase))
            .is_some_and(|h| hosts.iter().any(|x| *x == h))
}

/// The folder URL the files will live at, ending in `/`. Blank means the
/// root of the crawl's start host.
fn base_url(reader: &Reader, opts: &SitemapGenerateOptions) -> Result<String, GenerateError> {
    let given = opts.base_url.trim();
    let url = if given.is_empty() {
        let stored: Option<String> = reader
            .conn
            .query_row("SELECT value FROM meta WHERE key = 'config'", [], |r| r.get(0))
            .optional()?;
        let start = stored
            .and_then(|s| serde_json::from_str::<StoredCrawlConfig>(&s).ok())
            .and_then(|s| match s.input {
                spiderlens_config::ModeInput::Spider { start_url } => Some(start_url),
                spiderlens_config::ModeInput::List(l) => l.urls.into_iter().next(),
            });
        let first: Option<String> = match start {
            Some(s) => Some(s),
            None => reader
                .conn
                .query_row(
                    "SELECT url FROM urls WHERE internal = 1 ORDER BY id LIMIT 1",
                    [],
                    |r| r.get(0),
                )
                .optional()?,
        };
        let u = first.and_then(|s| Url::parse(&s).ok()).ok_or_else(|| {
            GenerateError::Invalid("Enter the address the files will be published at.".into())
        })?;
        let mut root = u;
        root.set_path("/");
        root.set_query(None);
        root.set_fragment(None);
        root
    } else {
        Url::parse(given)
            .ok()
            .filter(|u| matches!(u.scheme(), "http" | "https"))
            .ok_or_else(|| {
                GenerateError::Invalid("The base URL must start with http:// or https://.".into())
            })?
    };
    let mut s = url.to_string();
    // A base like https://a.test/sitemaps means that folder.
    if !s.ends_with('/') {
        s.push('/');
    }
    Ok(s)
}

/// `Last-Modified` from the stored response headers, as a W3C datetime.
fn last_modified(headers_json: &str) -> Option<String> {
    let pairs: Vec<(String, String)> = serde_json::from_str(headers_json).ok()?;
    let v = pairs
        .iter()
        .rev()
        .find(|(k, _)| k.eq_ignore_ascii_case("last-modified"))?
        .1
        .trim()
        .to_string();
    let ts = jiff::fmt::rfc2822::DateTimeParser::new()
        .parse_timestamp(&v)
        .ok()?;
    Some(ts.strftime("%Y-%m-%dT%H:%M:%S+00:00").to_string())
}

fn w3c_from_ms(ms: i64) -> Option<String> {
    let ts = jiff::Timestamp::from_millisecond(ms).ok()?;
    Some(ts.strftime("%Y-%m-%dT%H:%M:%S+00:00").to_string())
}

/// A file being written: plain or deterministic gzip, counting uncompressed
/// bytes.
enum Sink {
    Plain(BufWriter<File>),
    Gz(flate2::write::GzEncoder<BufWriter<File>>),
    Null,
}

impl Sink {
    fn write_all(&mut self, b: &[u8]) -> io::Result<()> {
        match self {
            Self::Plain(w) => w.write_all(b),
            Self::Gz(w) => w.write_all(b),
            Self::Null => Ok(()),
        }
    }

    fn finish(self) -> io::Result<()> {
        match self {
            Self::Plain(mut w) => w.flush(),
            Self::Gz(w) => w.finish()?.flush(),
            Self::Null => Ok(()),
        }
    }
}

fn open_writer(path: &Path, gz: bool) -> io::Result<Sink> {
    let f = BufWriter::new(File::create(path)?);
    Ok(if gz {
        Sink::Gz(
            flate2::GzBuilder::new()
                .mtime(0)
                .operating_system(255)
                .write(f, flate2::Compression::default()),
        )
    } else {
        Sink::Plain(f)
    })
}

struct Part {
    tmp: Option<PathBuf>,
    urls: u64,
    images: u64,
    bytes: u64,
}

impl Part {
    fn remove(&self) {
        if let Some(t) = &self.tmp {
            let _ = std::fs::remove_file(t);
        }
    }
}

/// Writes entries into numbered parts, starting the next part before the URL
/// count or the 50 MB limit would be passed (FR-022).
struct Splitter<'a> {
    kind: &'static str,
    max_urls: u32,
    max_bytes: u64,
    head: String,
    gz: bool,
    dir: Option<&'a Path>,
    dry: bool,
    current: Option<(Sink, Part)>,
    done: Vec<Part>,
}

const TAIL: &str = "</urlset>\n";

impl<'a> Splitter<'a> {
    fn new(
        kind: &'static str,
        max_urls: u32,
        image_ns: bool,
        xhtml_ns: bool,
        gz: bool,
        dir: Option<&'a Path>,
        dry: bool,
    ) -> Self {
        let mut head = format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<urlset xmlns=\"{NS_SITEMAP}\"");
        if image_ns {
            head.push_str(&format!(" xmlns:image=\"{NS_IMAGE}\""));
        }
        if xhtml_ns {
            head.push_str(&format!(" xmlns:xhtml=\"{NS_XHTML}\""));
        }
        head.push_str(">\n");
        Self {
            kind,
            max_urls,
            max_bytes: MAX_BYTES,
            head,
            gz,
            dir,
            dry,
            current: None,
            done: Vec::new(),
        }
    }

    fn write(&mut self, entry: &str, images: u64) -> io::Result<()> {
        let len = entry.len() as u64;
        let full = self.current.as_ref().is_some_and(|(_, p)| {
            p.urls >= u64::from(self.max_urls) || p.bytes + len + TAIL.len() as u64 > self.max_bytes
        });
        if full {
            self.close()?;
        }
        if self.current.is_none() {
            let n = self.done.len() + 1;
            let (sink, tmp) = if self.dry {
                (Sink::Null, None)
            } else {
                let dir = self.dir.expect("folder output has a dir");
                let tmp = dir.join(format!(".spiderlens-{}-{n}.tmp", self.kind));
                (open_writer(&tmp, self.gz)?, Some(tmp))
            };
            let mut part = Part {
                tmp,
                urls: 0,
                images: 0,
                bytes: 0,
            };
            let mut sink = sink;
            sink.write_all(self.head.as_bytes())?;
            part.bytes = self.head.len() as u64;
            self.current = Some((sink, part));
        }
        let (sink, part) = self.current.as_mut().expect("opened above");
        sink.write_all(entry.as_bytes())?;
        part.urls += 1;
        part.images += images;
        part.bytes += len;
        Ok(())
    }

    fn close(&mut self) -> io::Result<()> {
        if let Some((mut sink, mut part)) = self.current.take() {
            sink.write_all(TAIL.as_bytes())?;
            part.bytes += TAIL.len() as u64;
            sink.finish()?;
            self.done.push(part);
        }
        Ok(())
    }

    fn finish(mut self) -> io::Result<Vec<Part>> {
        self.close()?;
        Ok(std::mem::take(&mut self.done))
    }

    fn discard(mut self) {
        let _ = self.close();
        for p in &self.done {
            p.remove();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_part_never_passes_the_byte_limit() {
        let mut s = Splitter::new("pages", MAX_URLS, false, false, false, None, true);
        let head = s.head.len() as u64;
        let entry = "<url><loc>https://a.test/0123456789</loc></url>\n";
        let n = entry.len() as u64;
        // Room for exactly three entries and the closing tag.
        s.max_bytes = head + 3 * n + TAIL.len() as u64;
        for _ in 0..7 {
            s.write(entry, 0).unwrap();
        }
        let parts = s.finish().unwrap();
        let sizes: Vec<_> = parts.iter().map(|p| (p.urls, p.bytes)).collect();
        let full = head + 3 * n + TAIL.len() as u64;
        assert_eq!(
            sizes,
            vec![(3, full), (3, full), (1, head + n + TAIL.len() as u64)]
        );
    }

    #[test]
    fn last_modified_header_becomes_a_w3c_datetime() {
        let h = r#"[["Content-Type","text/html"],["last-modified","Tue, 13 Oct 2026 08:09:10 GMT"]]"#;
        assert_eq!(last_modified(h).as_deref(), Some("2026-10-13T08:09:10+00:00"));
        assert_eq!(last_modified(r#"[["Last-Modified","yesterday"]]"#), None);
        assert_eq!(last_modified("[]"), None);
    }
}
