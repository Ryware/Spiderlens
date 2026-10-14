//! Reading PDF documents (spec 027 part C, plan D4): properties, page count,
//! text and links.
//!
//! Reading is all-or-partial, never fatal: a damaged or encrypted file keeps
//! what could be read, with a status saying why the rest is missing. The
//! caller runs [`read`] on a blocking thread under a watchdog; it checks its
//! deadline between pages and publishes its progress through [`Progress`],
//! so a watchdog that gives up still has the facts read so far.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use lopdf::{Dictionary, Document, Object};

pub mod pool;
use serde::Serialize;

/// How far reading got.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PdfStatus {
    Read,
    /// Some facts are missing; `detail` says why.
    PartlyRead,
    /// Protected by a password: properties only.
    Encrypted,
    /// Larger than the PDF size limit: not downloaded past it.
    OverSizeLimit,
    /// Not a readable PDF.
    Failed,
}

impl PdfStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::PartlyRead => "partly_read",
            Self::Encrypted => "encrypted",
            Self::OverSizeLimit => "over_size_limit",
            Self::Failed => "failed",
        }
    }
}

/// A link annotation (FR-C03).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PdfLink {
    pub uri: String,
    /// 1-based page number.
    pub page: u32,
}

/// Everything read from one PDF (FR-C02).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PdfFacts {
    pub status: PdfStatus,
    pub detail: Option<String>,
    pub title: Option<String>,
    pub author: Option<String>,
    pub subject: Option<String>,
    pub keywords: Option<String>,
    pub creator: Option<String>,
    pub producer: Option<String>,
    /// ISO 8601 when the PDF date parses, else as written.
    pub created: Option<String>,
    pub modified: Option<String>,
    pub pages: Option<u32>,
    /// The catalog's `/Lang`.
    pub lang: Option<String>,
    /// Words in the text read; None when no page could be read.
    pub words: Option<u32>,
    /// Pages whose text was read.
    pub pages_read: u32,
    pub encrypted: bool,
    /// Tagged for accessibility (`/MarkInfo /Marked true`).
    pub tagged: bool,
    pub links: Vec<PdfLink>,
    /// The text, page after page.
    #[serde(skip)]
    pub text: String,
}

impl PdfFacts {
    fn new() -> Self {
        Self {
            status: PdfStatus::Read,
            detail: None,
            title: None,
            author: None,
            subject: None,
            keywords: None,
            creator: None,
            producer: None,
            created: None,
            modified: None,
            pages: None,
            lang: None,
            words: None,
            pages_read: 0,
            encrypted: false,
            tagged: false,
            links: Vec::new(),
            text: String::new(),
        }
    }

    /// A PDF that was not downloaded past the size limit (C1.5).
    pub fn over_size_limit(limit_bytes: u64) -> Self {
        Self {
            status: PdfStatus::OverSizeLimit,
            detail: Some(format!("Larger than the PDF size limit ({} MB)", limit_bytes / (1024 * 1024))),
            ..Self::new()
        }
    }

    /// Nothing could be read, for `why`.
    pub fn failed(why: impl Into<String>) -> Self {
        Self {
            status: PdfStatus::Failed,
            detail: Some(why.into()),
            ..Self::new()
        }
    }

    /// Text with no letters or digits in it: a scan (C1.2).
    pub fn no_text(&self) -> bool {
        self.pages_read > 0 && self.words == Some(0)
    }

    fn partly(&mut self, why: impl Into<String>) {
        if self.status == PdfStatus::Read {
            self.status = PdfStatus::PartlyRead;
        }
        if self.detail.is_none() {
            self.detail = Some(why.into());
        }
    }
}

/// The facts read so far, shared with the watchdog.
pub type Progress = Arc<Mutex<PdfFacts>>;

pub fn progress() -> Progress {
    Arc::new(Mutex::new(PdfFacts::new()))
}

/// What the watchdog keeps when a read runs past its time limit (C3.2).
pub fn timed_out(progress: &Progress) -> PdfFacts {
    let mut f = progress.lock().map(|f| f.clone()).unwrap_or_else(|e| e.into_inner().clone());
    if f.pages.is_none() && f.title.is_none() && f.pages_read == 0 {
        f.status = PdfStatus::Failed;
        f.detail = Some("Time limit reached before anything could be read".into());
    } else {
        f.status = PdfStatus::PartlyRead;
        f.detail = Some("Time limit reached".into());
    }
    f
}

/// Reads `bytes`, stopping between pages once `deadline` passes. Never
/// panics; a panic inside the PDF libraries becomes a "partly read" or
/// "failed" status with the facts read before it.
pub fn read(bytes: &[u8], deadline: Instant, progress: &Progress) -> PdfFacts {
    let result = catch_unwind(AssertUnwindSafe(|| read_inner(bytes, deadline, progress)));
    let mut f = progress.lock().map(|f| f.clone()).unwrap_or_else(|e| e.into_inner().clone());
    if result.is_err() {
        if f.pages.is_none() {
            f.status = PdfStatus::Failed;
            f.detail = Some("Not a readable PDF".into());
        } else {
            f.partly(format!("Page {} could not be read", f.pages_read + 1));
        }
    }
    finish_words(&mut f);
    f
}

fn finish_words(f: &mut PdfFacts) {
    if f.pages_read > 0 {
        let words = f.text.split_whitespace().filter(|w| w.chars().any(char::is_alphanumeric)).count();
        f.words = Some(u32::try_from(words).unwrap_or(u32::MAX));
    }
}

fn update(progress: &Progress, f: impl FnOnce(&mut PdfFacts)) {
    if let Ok(mut g) = progress.lock() {
        f(&mut g);
    }
}

fn read_inner(bytes: &[u8], deadline: Instant, progress: &Progress) {
    if !bytes.starts_with(b"%PDF-") && !bytes.get(..1024).is_some_and(|h| h.windows(5).any(|w| w == b"%PDF-")) {
        update(progress, |f| {
            f.status = PdfStatus::Failed;
            f.detail = Some("Not a PDF file".into());
        });
        return;
    }
    let mut doc = match Document::load_mem(bytes) {
        Ok(d) => d,
        Err(e) => {
            update(progress, |f| {
                f.status = PdfStatus::Failed;
                f.detail = Some(format!("Not a readable PDF: {e}"));
            });
            return;
        }
    };
    let encrypted = doc.was_encrypted() || doc.is_encrypted();
    let locked = doc.is_encrypted() && doc.decrypt("").is_err();
    let pages = doc.get_pages();
    let info = info_dict(&doc);
    let catalog = doc.catalog().ok();
    update(progress, |f| {
        f.encrypted = encrypted;
        f.pages = Some(u32::try_from(pages.len()).unwrap_or(u32::MAX));
        if let Some(info) = info {
            f.title = text_of(&doc, info, b"Title");
            f.author = text_of(&doc, info, b"Author");
            f.subject = text_of(&doc, info, b"Subject");
            f.keywords = text_of(&doc, info, b"Keywords");
            f.creator = text_of(&doc, info, b"Creator");
            f.producer = text_of(&doc, info, b"Producer");
            f.created = text_of(&doc, info, b"CreationDate").map(|d| pdf_date(&d));
            f.modified = text_of(&doc, info, b"ModDate").map(|d| pdf_date(&d));
        }
        if let Some(c) = catalog {
            f.lang = text_of(&doc, c, b"Lang");
            f.tagged = c
                .get(b"MarkInfo")
                .ok()
                .and_then(|m| resolve(&doc, m).as_dict().ok())
                .and_then(|m| m.get(b"Marked").ok())
                .and_then(|m| m.as_bool().ok())
                .unwrap_or(false);
        }
    });
    if locked {
        // Objects stay encrypted, but the page tree's /Count is a number.
        let count = catalog
            .and_then(|c| c.get(b"Pages").ok())
            .and_then(|p| resolve(&doc, p).as_dict().ok())
            .and_then(|p| p.get(b"Count").ok())
            .and_then(|c| c.as_i64().ok())
            .and_then(|c| u32::try_from(c).ok());
        update(progress, |f| {
            if f.pages == Some(0) {
                f.pages = count;
            }
            // Encrypted strings read as noise: keep none of them.
            f.title = None;
            f.author = None;
            f.subject = None;
            f.keywords = None;
            f.creator = None;
            f.producer = None;
            f.status = PdfStatus::Encrypted;
            f.detail = Some("Protected by a password: text and links are not readable".into());
        });
        return;
    }
    for (&n, &id) in &pages {
        let links: Vec<PdfLink> = doc
            .get_page_annotations(id)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|a| link_uri(&doc, a))
            .map(|uri| PdfLink { uri, page: n })
            .collect();
        update(progress, |f| f.links.extend(links));
    }
    for &n in pages.keys() {
        if Instant::now() >= deadline {
            update(progress, |f| f.partly("Time limit reached"));
            return;
        }
        let mut out = String::new();
        let page = catch_unwind(AssertUnwindSafe(|| {
            let mut dev = pdf_extract::PlainTextOutput::new(&mut out);
            pdf_extract::output_doc_page(&doc, &mut dev, n)
        }));
        match page {
            Ok(Ok(())) => update(progress, |f| {
                if !f.text.is_empty() {
                    f.text.push('\n');
                }
                f.text.push_str(out.trim());
                f.pages_read += 1;
            }),
            Ok(Err(e)) => update(progress, |f| f.partly(format!("Page {n} could not be read: {e}"))),
            Err(_) => update(progress, |f| f.partly(format!("Page {n} could not be read"))),
        }
    }
}

fn info_dict(doc: &Document) -> Option<&Dictionary> {
    doc.trailer.get(b"Info").ok().and_then(|o| resolve(doc, o).as_dict().ok())
}

fn resolve<'a>(doc: &'a Document, o: &'a Object) -> &'a Object {
    match o {
        Object::Reference(id) => doc.get_object(*id).unwrap_or(o),
        _ => o,
    }
}

/// A text string (PDFDocEncoding or UTF-16BE with a BOM), trimmed; None
/// when absent or empty.
fn text_of(doc: &Document, dict: &Dictionary, key: &[u8]) -> Option<String> {
    let o = resolve(doc, dict.get(key).ok()?);
    let bytes = match o {
        Object::String(b, _) => b.as_slice(),
        Object::Name(b) => b.as_slice(),
        _ => return None,
    };
    let s = decode_text(bytes);
    let s = s.trim().trim_matches('\0').trim();
    (!s.is_empty()).then(|| s.to_string())
}

fn decode_text(b: &[u8]) -> String {
    if let Some(rest) = b.strip_prefix(&[0xFE, 0xFF]) {
        let units: Vec<u16> = rest.chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
        return String::from_utf16_lossy(&units);
    }
    if let Some(rest) = b.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(rest).into_owned();
    }
    // PDFDocEncoding matches Latin-1 for the characters titles use.
    b.iter().map(|&c| c as char).collect()
}

/// `D:20250131120000+02'00'` as `2025-01-31T12:00:00+02:00`; anything else
/// as written.
fn pdf_date(raw: &str) -> String {
    let s = raw.strip_prefix("D:").unwrap_or(raw);
    let digits: String = s.chars().take_while(char::is_ascii_digit).collect();
    if digits.len() < 4 {
        return raw.to_string();
    }
    let part = |a: usize, b: usize, default: &str| digits.get(a..b).unwrap_or(default).to_string();
    let date = format!("{}-{}-{}", part(0, 4, "0000"), part(4, 6, "01"), part(6, 8, "01"));
    if digits.len() < 10 {
        return date;
    }
    let time = format!("{}:{}:{}", part(8, 10, "00"), part(10, 12, "00"), part(12, 14, "00"));
    let zone = &s[digits.len()..];
    let tz = match zone.chars().next() {
        Some('Z') => "Z".to_string(),
        Some(sign @ ('+' | '-')) => {
            let z: String = zone[1..].chars().filter(char::is_ascii_digit).collect();
            format!("{sign}{}:{}", z.get(0..2).unwrap_or("00"), z.get(2..4).unwrap_or("00"))
        }
        _ => String::new(),
    };
    format!("{date}T{time}{tz}")
}

/// The target of a `/Link` annotation with a URI action.
fn link_uri(doc: &Document, annot: &Dictionary) -> Option<String> {
    if annot.get(b"Subtype").ok()?.as_name().ok()? != b"Link" {
        return None;
    }
    let action = resolve(doc, annot.get(b"A").ok()?).as_dict().ok()?;
    if action.get(b"S").ok()?.as_name().ok()? != b"URI" {
        return None;
    }
    let uri = match resolve(doc, action.get(b"URI").ok()?) {
        Object::String(b, _) => decode_text(b),
        _ => return None,
    };
    let uri = uri.trim();
    (!uri.is_empty()).then(|| uri.to_string())
}

#[cfg(test)]
mod tests;
