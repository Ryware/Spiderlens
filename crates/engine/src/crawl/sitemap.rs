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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Sitemap {
    /// `<urlset>`: page URLs.
    Urls(Vec<String>),
    /// `<sitemapindex>`: more sitemaps.
    Index(Vec<String>),
}

/// Parses a sitemap, gunzipping it first when it starts with the gzip magic
/// bytes (whatever the file name or content type says).
pub fn parse(bytes: &[u8]) -> Result<Sitemap, String> {
    let owned;
    let xml: &[u8] = if bytes.starts_with(&[0x1f, 0x8b]) {
        let mut out = Vec::new();
        flate2::read::GzDecoder::new(bytes)
            .take(MAX_SITEMAP_BYTES + 1)
            .read_to_end(&mut out)
            .map_err(|e| format!("the compressed sitemap could not be read ({e})"))?;
        if out.len() as u64 > MAX_SITEMAP_BYTES {
            return Err("the sitemap is larger than 50 MB uncompressed".into());
        }
        owned = out;
        &owned
    } else {
        bytes
    };

    let mut reader = Reader::from_reader(xml);
    let mut buf = Vec::new();
    let mut root: Option<String> = None;
    // Text of the `<loc>` being read; entity references arrive as separate
    // events and are appended here.
    let mut loc: Option<String> = None;
    let mut locs = Vec::new();
    let mut open_elements = 0usize;
    let invalid = |e: &dyn std::fmt::Display| format!("the sitemap is not valid XML ({e})");
    loop {
        match reader.read_event_into(&mut buf).map_err(|e| invalid(&e))? {
            Event::Start(e) => {
                open_elements += 1;
                let name = e.local_name().as_ref().to_ascii_lowercase();
                if name == "loc" {
                    loc = Some(String::new());
                }
                root.get_or_insert(name);
            }
            Event::Text(t) => {
                if let Some(l) = loc.as_mut() {
                    l.push_str(&t.xml10_content());
                }
            }
            Event::CData(t) => {
                if let Some(l) = loc.as_mut() {
                    l.push_str(&t.xml10_content());
                }
            }
            Event::GeneralRef(r) => {
                if let Some(l) = loc.as_mut() {
                    let ch = if r.is_char_ref() {
                        r.resolve_char_ref().map_err(|e| invalid(&e))?
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
                    let ch = ch.ok_or_else(|| invalid(&"unknown entity"))?;
                    l.push(ch);
                }
            }
            Event::End(_) => {
                open_elements = open_elements.saturating_sub(1);
                if let Some(l) = loc.take() {
                    locs.push(l.trim().to_string());
                }
            }
            Event::Eof if open_elements > 0 => {
                return Err(invalid(&"the file ends before the XML is complete"));
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    match root.as_deref() {
        Some("urlset") => Ok(Sitemap::Urls(locs)),
        Some("sitemapindex") => Ok(Sitemap::Index(locs)),
        _ => Err("the file is not a sitemap (no <urlset> or <sitemapindex>)".into()),
    }
}

/// Fetches one sitemap file. Errors are plain-language strings stored with
/// the sitemap (FR-036).
pub async fn fetch(client: &Client, url: &Url) -> Result<Vec<u8>, String> {
    let keep_all = |_: Option<&str>| true;
    let policy = BodyPolicy {
        max_bytes: MAX_SITEMAP_BYTES,
        on_limit: OnLimit::Fail,
        keep: &keep_all,
    };
    let outcome = get_following(client, url, 5, &policy, &[]).await;
    let fetched = outcome
        .result
        .map_err(|e| format!("{}: {}", e.class, e.message))?;
    if !fetched.status.is_success() {
        return Err(format!("the server answered {}", fetched.status.as_u16()));
    }
    Ok(fetched.body.unwrap_or_default())
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

    #[test]
    fn invalid_files_are_errors() {
        assert!(parse(b"<html><body>Not found</body></html>").is_err());
        assert!(parse(b"<urlset><url><loc>x</url>").is_err());
        assert!(parse(b"").is_err());
        assert!(parse(b"<urlset><url>").is_err(), "truncated");
    }
}
