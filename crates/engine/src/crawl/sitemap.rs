//! XML sitemaps and sitemap index files (FR-034..FR-036; research R9).

use std::io::Read;

use quick_xml::Reader;
use quick_xml::events::Event;
use reqwest::Client;
use url::Url;

use crate::fetch::{BodyPolicy, OnLimit, get_following};

/// The protocol allows 50 MB uncompressed per sitemap.
pub const MAX_SITEMAP_BYTES: u64 = 50 * 1024 * 1024;
/// Index files are followed this many levels deep.
pub const MAX_INDEX_DEPTH: u32 = 3;

/// Entries read from one file; the protocol allows no more.
pub const MAX_ENTRIES: u32 = 50_000;
/// Images per `<url>` the image extension allows.
pub const MAX_IMAGES_PER_URL: usize = 1_000;

/// The root element of a sitemap file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Root {
    /// `<urlset>`: page URLs.
    Urlset,
    /// `<sitemapindex>`: more sitemaps.
    Index,
}

impl Root {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Urlset => "urlset",
            Self::Index => "index",
        }
    }
}

/// An hreflang alternate (`<xhtml:link rel="alternate" hreflang=…>`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alternate {
    pub lang: String,
    pub href: String,
}

/// One `<url>` or `<sitemap>` element, as written (spec 011 FR-006).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Entry {
    /// Position in the file, from 0.
    pub pos: u32,
    /// `<loc>` after entity decoding, trimmed. Empty when the element had none.
    pub loc: String,
    pub lastmod: Option<String>,
    pub changefreq: Option<String>,
    pub priority: Option<String>,
    /// `<image:image><image:loc>` values, at most [`MAX_IMAGES_PER_URL`].
    pub images: Vec<String>,
    pub alternates: Vec<Alternate>,
}

/// What reading a whole file established.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileFacts {
    pub root: Root,
    pub compressed: bool,
    /// Uncompressed size read.
    pub bytes: u64,
    pub entries: u32,
    /// More than [`MAX_ENTRIES`] entries or [`MAX_SITEMAP_BYTES`] bytes; the
    /// entries up to the limit were read.
    pub over_limit: bool,
}

/// Why a file could not be read as a sitemap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileErrorClass {
    InvalidXml,
    NotASitemap,
    Compression,
}

impl FileErrorClass {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InvalidXml => "invalid_xml",
            Self::NotASitemap => "not_a_sitemap",
            Self::Compression => "compression",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileError {
    pub class: FileErrorClass,
    pub message: String,
    /// 1-based line where reading stopped, for XML errors.
    pub line: Option<u32>,
    /// Entries handed out before the error.
    pub entries: u32,
}

/// The old shape: every `<loc>` of a file (spec 003).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Sitemap {
    Urls(Vec<String>),
    Index(Vec<String>),
}

/// Reads every `<loc>` into memory. Kept for callers that only need the
/// locations; the crawl uses [`read`].
pub fn parse(bytes: &[u8]) -> Result<Sitemap, String> {
    let mut locs = Vec::new();
    let facts = read(bytes, |e| locs.push(e.loc)).map_err(|e| e.message)?;
    Ok(match facts.root {
        Root::Urlset => Sitemap::Urls(locs),
        Root::Index => Sitemap::Index(locs),
    })
}

/// Where the reader is inside the document.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Field {
    None,
    Loc,
    Lastmod,
    Changefreq,
    Priority,
    ImageLoc,
}

/// Reads a sitemap or sitemap index, handing out each entry as soon as its
/// element closes, so memory holds the file and one entry (plan D1). The
/// file is gunzipped first when it starts with the gzip magic bytes,
/// whatever its name or content type says.
pub fn read(bytes: &[u8], mut on_entry: impl FnMut(Entry)) -> Result<FileFacts, FileError> {
    let compressed = bytes.starts_with(&[0x1f, 0x8b]);
    let owned;
    let mut over_limit = false;
    let xml: &[u8] = if compressed {
        let mut out = Vec::new();
        if let Err(e) = flate2::read::GzDecoder::new(bytes)
            .take(MAX_SITEMAP_BYTES + 1)
            .read_to_end(&mut out)
        {
            return Err(FileError {
                class: FileErrorClass::Compression,
                message: format!("the compressed sitemap could not be read ({e})"),
                line: None,
                entries: 0,
            });
        }
        owned = out;
        &owned
    } else {
        bytes
    };
    let xml = if xml.len() as u64 > MAX_SITEMAP_BYTES {
        over_limit = true;
        &xml[..MAX_SITEMAP_BYTES as usize]
    } else {
        xml
    };

    let mut reader = Reader::from_reader(xml);
    let mut buf = Vec::new();
    let mut root: Option<Root> = None;
    let mut entry: Option<Entry> = None;
    let mut field = Field::None;
    let mut text = String::new();
    let mut in_image = false;
    let mut depth = 0usize;
    let mut count: u32 = 0;

    let line_at = |pos: u64| -> u32 {
        let end = (pos as usize).min(xml.len());
        1 + xml[..end].iter().filter(|&&b| b == b'\n').count() as u32
    };
    macro_rules! fail {
        ($class:expr, $msg:expr) => {
            return Err(FileError {
                class: $class,
                message: $msg,
                line: Some(line_at(reader.buffer_position())),
                entries: count,
            })
        };
    }

    loop {
        buf.clear();
        let event = match reader.read_event_into(&mut buf) {
            Ok(e) => e,
            // A file cut at the size limit ends mid-element: keep what was read.
            Err(_) if over_limit => break,
            Err(e) => fail!(
                FileErrorClass::InvalidXml,
                format!("the sitemap is not valid XML ({e})")
            ),
        };
        match event {
            Event::Start(ref e) | Event::Empty(ref e) => {
                let is_empty = matches!(event, Event::Empty(_));
                depth += 1;
                // The element's own nesting level; an empty element closes at once.
                let level = depth;
                if is_empty {
                    depth -= 1;
                }
                let name = e.local_name().as_ref().to_ascii_lowercase();
                let Some(r) = root else {
                    root = Some(match name.as_str() {
                        "urlset" => Root::Urlset,
                        "sitemapindex" => Root::Index,
                        _ => fail!(
                            FileErrorClass::NotASitemap,
                            "the file is not a sitemap (no <urlset> or <sitemapindex>)".into()
                        ),
                    });
                    continue;
                };
                let entry_tag = match r {
                    Root::Urlset => "url",
                    Root::Index => "sitemap",
                };
                if level == 2 && name == entry_tag {
                    if count >= MAX_ENTRIES {
                        over_limit = true;
                        break;
                    }
                    entry = Some(Entry {
                        pos: count,
                        ..Entry::default()
                    });
                    if is_empty {
                        on_entry(entry.take().expect("just set"));
                        count += 1;
                    }
                    continue;
                }
                let Some(en) = entry.as_mut() else { continue };
                match name.as_str() {
                    "image" if !is_empty => in_image = true,
                    "loc" if in_image => field = Field::ImageLoc,
                    "loc" if level == 3 => field = Field::Loc,
                    "lastmod" if level == 3 => field = Field::Lastmod,
                    "changefreq" if level == 3 => field = Field::Changefreq,
                    "priority" if level == 3 => field = Field::Priority,
                    "link" => {
                        let (mut rel, mut lang, mut href) = (None, None, None);
                        for a in e.attributes().flatten() {
                            let v = a
                                .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                                .map(|v| v.trim().to_string())
                                .unwrap_or_default();
                            match a.key.local_name().as_ref() {
                                "rel" => rel = Some(v),
                                "hreflang" => lang = Some(v),
                                "href" => href = Some(v),
                                _ => {}
                            }
                        }
                        if rel
                            .as_deref()
                            .is_some_and(|r| r.eq_ignore_ascii_case("alternate"))
                            && let (Some(lang), Some(href)) = (lang, href)
                        {
                            en.alternates.push(Alternate { lang, href });
                        }
                    }
                    _ => {}
                }
                if field != Field::None {
                    text.clear();
                    if is_empty {
                        field = Field::None;
                    }
                }
            }
            Event::Text(t) if field != Field::None => text.push_str(&t.xml10_content()),
            Event::CData(t) if field != Field::None => text.push_str(&t.xml10_content()),
            Event::GeneralRef(r) if field != Field::None => {
                let ch = if r.is_char_ref() {
                    match r.resolve_char_ref() {
                        Ok(c) => c,
                        Err(e) => fail!(
                            FileErrorClass::InvalidXml,
                            format!("the sitemap is not valid XML ({e})")
                        ),
                    }
                } else {
                    match &*r {
                        "amp" => Some('&'),
                        "lt" => Some('<'),
                        "gt" => Some('>'),
                        "quot" => Some('"'),
                        "apos" => Some('\''),
                        _ => None,
                    }
                };
                match ch {
                    Some(c) => text.push(c),
                    None => fail!(
                        FileErrorClass::InvalidXml,
                        "the sitemap is not valid XML (unknown entity)".into()
                    ),
                }
            }
            Event::End(e) => {
                depth = depth.saturating_sub(1);
                let name = e.local_name().as_ref().to_ascii_lowercase();
                if let Some(en) = entry.as_mut() {
                    if field != Field::None {
                        let value = text.trim().to_string();
                        match field {
                            Field::Loc => en.loc = value,
                            Field::Lastmod => en.lastmod = Some(value),
                            Field::Changefreq => en.changefreq = Some(value),
                            Field::Priority => en.priority = Some(value),
                            Field::ImageLoc if en.images.len() < MAX_IMAGES_PER_URL => en.images.push(value),
                            Field::ImageLoc | Field::None => {}
                        }
                        field = Field::None;
                    } else if name == "image" {
                        in_image = false;
                    } else if depth == 1 {
                        on_entry(entry.take().expect("checked"));
                        count += 1;
                    }
                }
            }
            Event::Eof if over_limit => break,
            Event::Eof if depth > 0 => fail!(
                FileErrorClass::InvalidXml,
                "the sitemap is not valid XML (the file ends before the XML is complete)".into()
            ),
            Event::Eof => break,
            _ => {}
        }
    }
    let Some(root) = root else {
        return Err(FileError {
            class: FileErrorClass::NotASitemap,
            message: "the file is not a sitemap (no <urlset> or <sitemapindex>)".into(),
            line: None,
            entries: 0,
        });
    };
    Ok(FileFacts {
        root,
        compressed,
        bytes: xml.len() as u64,
        entries: count,
        over_limit,
    })
}

/// Parses a `<lastmod>` value in one of the six W3C datetime forms the
/// sitemap protocol allows (`YYYY`, `YYYY-MM`, `YYYY-MM-DD`, then with
/// `Thh:mmTZD`, `Thh:mm:ssTZD` or `Thh:mm:ss.sTZD`), returning UTC seconds
/// since 1970. Dates without a time are taken at 00:00 UTC (research R3).
pub fn parse_lastmod(value: &str) -> Option<i64> {
    let b = value.trim().as_bytes();
    let num = |s: &[u8]| -> Option<i64> {
        if s.is_empty() || !s.iter().all(u8::is_ascii_digit) {
            return None;
        }
        std::str::from_utf8(s).ok()?.parse().ok()
    };
    if b.len() < 4 {
        return None;
    }
    let year = num(&b[..4])?;
    let (mut month, mut day) = (1, 1);
    let mut rest = &b[4..];
    if !rest.is_empty() {
        if rest.len() < 3 || rest[0] != b'-' {
            return None;
        }
        month = num(&rest[1..3])?;
        rest = &rest[3..];
        if !rest.is_empty() {
            if rest.len() < 3 || rest[0] != b'-' {
                return None;
            }
            day = num(&rest[1..3])?;
            rest = &rest[3..];
        }
    }
    if !(1..=12).contains(&month) || day < 1 || day > days_in_month(year, month) {
        return None;
    }
    let mut seconds = 0;
    if !rest.is_empty() {
        // A time needs a full date.
        if b.len() < 10 || rest[0] != b'T' || rest.len() < 6 || rest[3] != b':' {
            return None;
        }
        let hour = num(&rest[1..3])?;
        let minute = num(&rest[4..6])?;
        let mut sec = 0;
        rest = &rest[6..];
        if rest.first() == Some(&b':') {
            if rest.len() < 3 {
                return None;
            }
            sec = num(&rest[1..3])?;
            rest = &rest[3..];
            if rest.first() == Some(&b'.') {
                let digits = rest[1..].iter().take_while(|c| c.is_ascii_digit()).count();
                if digits == 0 {
                    return None;
                }
                rest = &rest[1 + digits..];
            }
        }
        if hour > 23 || minute > 59 || sec > 59 {
            return None;
        }
        let offset = match rest {
            [b'Z'] => 0,
            [sign @ (b'+' | b'-'), h1, h2, b':', m1, m2] => {
                let h = num(&[*h1, *h2])?;
                let m = num(&[*m1, *m2])?;
                if h > 14 || m > 59 || (h == 14 && m > 0) {
                    return None;
                }
                let o = h * 3600 + m * 60;
                if *sign == b'+' { o } else { -o }
            }
            _ => return None,
        };
        seconds = hour * 3600 + minute * 60 + sec - offset;
    }
    Some(days_from_civil(year, month, day) * 86_400 + seconds)
}

fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

fn days_in_month(y: i64, m: i64) -> i64 {
    match m {
        2 if is_leap(y) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Days since 1970-01-01 (Howard Hinnant's algorithm).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The outcome of requesting one sitemap file.
pub struct Fetched {
    pub status: Option<u16>,
    pub final_url: Url,
    pub redirected: bool,
    pub bytes: Vec<u8>,
}

/// A failed request: the spec 004 class and a plain message.
pub struct FetchError {
    pub class: &'static str,
    pub message: String,
    pub status: Option<u16>,
    pub final_url: Option<Url>,
}

/// Fetches one sitemap file, following redirects. Bodies over the 50 MB
/// limit are cut there and read up to it.
pub async fn fetch(client: &Client, url: &Url) -> Result<Fetched, FetchError> {
    let keep_all = |_: Option<&str>| true;
    let policy = BodyPolicy {
        max_bytes: MAX_SITEMAP_BYTES + 1,
        on_limit: OnLimit::Truncate,
        keep: &keep_all,
    };
    let outcome = get_following(client, url, 5, &policy, &[]).await;
    let redirected = !outcome.hops.is_empty();
    let fetched = outcome.result.map_err(|e| FetchError {
        class: e.class.as_str(),
        message: e.message,
        status: None,
        final_url: None,
    })?;
    let status = fetched.status.as_u16();
    if !fetched.status.is_success() {
        return Err(FetchError {
            class: "http_status",
            message: format!("the server answered {status}"),
            status: Some(status),
            final_url: Some(fetched.final_url),
        });
    }
    Ok(Fetched {
        status: Some(status),
        final_url: fetched.final_url,
        redirected,
        bytes: fetched.body.unwrap_or_default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn urlset_and_index() {
        let urlset = br#"<?xml version="1.0"?><urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
            <url><loc> https://a.test/a </loc></url><url><loc>https://a.test/b?x=1&amp;y=2</loc></url></urlset>"#;
        assert_eq!(
            parse(urlset).unwrap(),
            Sitemap::Urls(vec!["https://a.test/a".into(), "https://a.test/b?x=1&y=2".into()])
        );
        let index = br#"<sitemapindex><sitemap><loc>https://a.test/s1.xml</loc></sitemap></sitemapindex>"#;
        assert_eq!(
            parse(index).unwrap(),
            Sitemap::Index(vec!["https://a.test/s1.xml".into()])
        );
    }

    #[test]
    fn gzip_is_detected_by_content() {
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        gz.write_all(b"<urlset><url><loc>https://a.test/z</loc></url></urlset>")
            .unwrap();
        let bytes = gz.finish().unwrap();
        assert_eq!(
            parse(&bytes).unwrap(),
            Sitemap::Urls(vec!["https://a.test/z".into()])
        );
    }

    fn entries(xml: &[u8]) -> (Result<FileFacts, FileError>, Vec<Entry>) {
        let mut out = Vec::new();
        let r = read(xml, |e| out.push(e));
        (r, out)
    }

    #[test]
    fn reads_every_field_of_an_entry() {
        let xml = br#"<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9"
        xmlns:image="http://www.google.com/schemas/sitemap-image/1.1"
        xmlns:xhtml="http://www.w3.org/1999/xhtml"
        xmlns:video="http://www.google.com/schemas/sitemap-video/1.1">
  <url>
    <loc>https://a.test/en/</loc>
    <lastmod>2026-10-01</lastmod>
    <changefreq>weekly</changefreq>
    <priority>0.8</priority>
    <xhtml:link rel="alternate" hreflang="de" href="https://a.test/de/"/>
    <xhtml:link rel="alternate" hreflang="en" href="https://a.test/en/"/>
    <image:image><image:loc>https://a.test/a.png</image:loc></image:image>
    <image:image><image:loc><![CDATA[https://a.test/b.png]]></image:loc></image:image>
    <video:video><video:content_loc>https://a.test/v.mp4</video:content_loc><video:player_loc>x</video:player_loc></video:video>
  </url>
  <url><loc>https://a.test/plain</loc></url>
</urlset>"#;
        let (r, e) = entries(xml);
        let facts = r.unwrap();
        assert_eq!(facts.root, Root::Urlset);
        assert_eq!(facts.entries, 2);
        assert!(!facts.compressed && !facts.over_limit);
        assert_eq!(e[0].loc, "https://a.test/en/");
        assert_eq!(e[0].lastmod.as_deref(), Some("2026-10-01"));
        assert_eq!(e[0].changefreq.as_deref(), Some("weekly"));
        assert_eq!(e[0].priority.as_deref(), Some("0.8"));
        assert_eq!(e[0].images, vec!["https://a.test/a.png", "https://a.test/b.png"]);
        assert_eq!(
            e[0].alternates,
            vec![
                Alternate {
                    lang: "de".into(),
                    href: "https://a.test/de/".into()
                },
                Alternate {
                    lang: "en".into(),
                    href: "https://a.test/en/".into()
                },
            ]
        );
        assert_eq!(
            e[1],
            Entry {
                pos: 1,
                loc: "https://a.test/plain".into(),
                ..Entry::default()
            }
        );
    }

    #[test]
    fn index_entries_carry_lastmod() {
        let xml = br#"<sitemapindex><sitemap><loc>https://a.test/s1.xml</loc><lastmod>2026-01-02T03:04:05Z</lastmod></sitemap><sitemap/></sitemapindex>"#;
        let (r, e) = entries(xml);
        assert_eq!(r.unwrap().root, Root::Index);
        assert_eq!(e.len(), 2);
        assert_eq!(e[0].lastmod.as_deref(), Some("2026-01-02T03:04:05Z"));
        assert_eq!(e[1].loc, "", "an empty element is an entry with no location");
    }

    #[test]
    fn errors_name_the_class_and_line() {
        let (r, e) =
            entries(b"<urlset>\n<url><loc>https://a.test/a</loc></url>\n<url><loc>x</url>\n</urlset>");
        let err = r.unwrap_err();
        assert_eq!(err.class, FileErrorClass::InvalidXml);
        assert_eq!(err.line, Some(3));
        assert_eq!(err.entries, 1);
        assert_eq!(e.len(), 1);
        let err = read(b"<html><body>404</body></html>", |_| {}).unwrap_err();
        assert_eq!(err.class, FileErrorClass::NotASitemap);
        let err = read(&[0x1f, 0x8b, 1, 2, 3], |_| {}).unwrap_err();
        assert_eq!(err.class, FileErrorClass::Compression);
    }

    #[test]
    fn stops_at_the_entry_limit() {
        let mut xml = String::from("<urlset>");
        for i in 0..=MAX_ENTRIES {
            xml.push_str(&format!("<url><loc>https://a.test/{i}</loc></url>"));
        }
        xml.push_str("</urlset>");
        let (r, e) = entries(xml.as_bytes());
        let facts = r.unwrap();
        assert!(facts.over_limit);
        assert_eq!(facts.entries, MAX_ENTRIES);
        assert_eq!(e.len(), MAX_ENTRIES as usize);
    }

    #[test]
    fn lastmod_forms() {
        assert_eq!(parse_lastmod("1970-01-01"), Some(0));
        assert_eq!(parse_lastmod("1970"), Some(0));
        assert_eq!(parse_lastmod("1970-02"), Some(31 * 86_400));
        assert_eq!(parse_lastmod("1970-01-01T01:00Z"), Some(3600));
        assert_eq!(parse_lastmod("1970-01-01T01:00:30+01:00"), Some(30));
        assert_eq!(parse_lastmod("1970-01-01T00:00:00.123-00:30"), Some(1800));
        assert_eq!(parse_lastmod(" 2024-02-29 "), Some(1_709_164_800));
        for bad in [
            "",
            "26",
            "2026-13-01",
            "2026-02-30",
            "2025-02-29",
            "2026-1-01",
            "2026-10-01T25:00Z",
            "2026-10-01T10:00",
            "2026-10-01T10:00+15:00",
            "2026-10-01 10:00Z",
            "2026/10/01",
            "2026T10:00Z",
            "2026-10-01T10:00:00.Z",
            "yesterday",
        ] {
            assert_eq!(parse_lastmod(bad), None, "{bad}");
        }
    }

    proptest::proptest! {
        #[test]
        fn lastmod_matches_a_reference(secs in 0i64..4_102_444_800, off_min in -840i64..=840) {
            let off_min = off_min - off_min % 15;
            let local = secs + off_min * 60;
            let days = local.div_euclid(86_400);
            let rem = local.rem_euclid(86_400);
            // civil_from_days
            let z = days + 719_468;
            let era = z.div_euclid(146_097);
            let doe = z - era * 146_097;
            let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
            let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
            let mp = (5 * doy + 2) / 153;
            let d = doy - (153 * mp + 2) / 5 + 1;
            let m = if mp < 10 { mp + 3 } else { mp - 9 };
            let y = yoe + era * 400 + i64::from(m <= 2);
            let sign = if off_min < 0 { '-' } else { '+' };
            let s = format!(
                "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}{sign}{:02}:{:02}",
                rem / 3600, rem % 3600 / 60, rem % 60, off_min.abs() / 60, off_min.abs() % 60
            );
            proptest::prop_assert_eq!(parse_lastmod(&s), Some(secs), "{}", s);
        }

        #[test]
        fn any_urlset_round_trips(locs in proptest::collection::vec("[a-z0-9/&<>' ]{1,30}", 0..40)) {
            let mut xml = String::from("<urlset>");
            for l in &locs {
                let esc = l.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('\'', "&apos;");
                xml.push_str(&format!("<url><loc>https://a.test/{esc}</loc></url>"));
            }
            xml.push_str("</urlset>");
            let mut got = Vec::new();
            read(xml.as_bytes(), |e| got.push(e.loc)).unwrap();
            let want: Vec<String> = locs.iter().map(|l| format!("https://a.test/{l}").trim().to_string()).collect();
            proptest::prop_assert_eq!(got, want);
        }
    }

    #[test]
    fn invalid_files_are_errors() {
        assert!(parse(b"<html><body>Not found</body></html>").is_err());
        assert!(parse(b"<urlset><url><loc>x</url>").is_err());
        assert!(parse(b"").is_err());
        assert!(parse(b"<urlset><url>").is_err(), "truncated");
    }
}
