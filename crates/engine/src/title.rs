//! Character-set detection and title extraction (FR-014, research R5).

use encoding_rs::{Encoding, UTF_8};

use crate::Title;

/// Media types we treat as HTML.
pub fn is_html(content_type: Option<&str>, body_start: &[u8]) -> bool {
    match content_type {
        Some(ct) => ct == "text/html" || ct == "application/xhtml+xml",
        // No Content-Type: sniff the first bytes like a browser would.
        None => {
            let head = String::from_utf8_lossy(&body_start[..body_start.len().min(512)])
                .trim_start()
                .to_ascii_lowercase();
            head.starts_with("<!doctype html") || head.starts_with("<html")
        }
    }
}

/// The encoding the response declares (byte order mark, HTTP header charset
/// or `<meta>` in the first 1024 bytes), if any. Undeclared pages are read
/// as UTF-8, or windows-1252 when they are not valid UTF-8 (extract).
pub fn declared_encoding(header_charset: Option<&str>, body: &[u8]) -> Option<&'static Encoding> {
    if let Some((enc, _)) = Encoding::for_bom(body) {
        return Some(enc);
    }
    if let Some(enc) = header_charset.and_then(|l| Encoding::for_label(l.trim().as_bytes())) {
        return Some(enc);
    }
    let enc = meta_charset(&body[..body.len().min(1024)])?;
    // A page cannot declare UTF-16 in a meta tag it parsed as ASCII.
    if enc == encoding_rs::UTF_16LE || enc == encoding_rs::UTF_16BE {
        return Some(UTF_8);
    }
    Some(enc)
}

/// Finds `charset=` inside a `<meta ...>` tag. Covers both
/// `<meta charset="x">` and `<meta http-equiv content="text/html; charset=x">`.
fn meta_charset(prefix: &[u8]) -> Option<&'static Encoding> {
    let text = String::from_utf8_lossy(prefix).to_ascii_lowercase();
    let mut rest = text.as_str();
    while let Some(start) = rest.find("<meta") {
        let tag_and_after = &rest[start..];
        let end = tag_and_after.find('>').unwrap_or(tag_and_after.len());
        let tag = &tag_and_after[..end];
        if let Some(pos) = tag.find("charset") {
            let after = tag[pos + "charset".len()..].trim_start();
            if let Some(value) = after.strip_prefix('=') {
                let value = value.trim_start().trim_start_matches(['"', '\'']);
                let label: String = value
                    .chars()
                    .take_while(|c| !matches!(c, '"' | '\'' | ';' | ' ' | '/' | '>'))
                    .collect();
                if let Some(enc) = Encoding::for_label(label.as_bytes()) {
                    return Some(enc);
                }
            }
        }
        rest = &tag_and_after[end..];
    }
    None
}

/// The page title, read by the same extraction the crawl uses (spec 005):
/// the first HTML `<title>`, whitespace collapsed; empty counts as missing.
pub fn extract_title(header_charset: Option<&str>, body: &[u8]) -> Title {
    let url = url::Url::parse("about:blank").expect("valid");
    let data = crate::extract::extract_page(&crate::extract::PageInput {
        url: &url,
        charset: header_charset,
        body,
        truncated: false,
        robots_token: "",
        a11y: false,
        hook: None,
    });
    match data.title() {
        Some(t) if !t.is_empty() => Title::Text(t.to_string()),
        _ => Title::Missing,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn title(html: &str) -> Title {
        extract_title(None, html.as_bytes())
    }

    #[test]
    fn plain_title() {
        assert_eq!(
            title("<html><head><title>Home | Example</title></head></html>"),
            Title::Text("Home | Example".into())
        );
    }

    #[test]
    fn character_references_are_resolved() {
        assert_eq!(
            title("<title>Tom &amp; Jerry &mdash; &#x263A;</title>"),
            Title::Text("Tom & Jerry — ☺".into())
        );
    }

    #[test]
    fn whitespace_is_collapsed_and_trimmed() {
        assert_eq!(
            title("<title>\n  Lots   of\tspace \n</title>"),
            Title::Text("Lots of space".into())
        );
    }

    #[test]
    fn non_breaking_space_is_kept() {
        assert_eq!(title("<title>a&nbsp;b</title>"), Title::Text("a\u{a0}b".into()));
    }

    #[test]
    fn missing_and_empty_titles() {
        assert_eq!(title("<html><head></head><body>x</body></html>"), Title::Missing);
        assert_eq!(title("<title>   </title>"), Title::Missing);
    }

    #[test]
    fn first_title_wins_and_body_svg_title_is_ignored() {
        assert_eq!(
            title("<head><title>One</title><title>Two</title></head>"),
            Title::Text("One".into())
        );
        assert_eq!(
            title("<head></head><body><svg><title>Icon</title></svg></body>"),
            Title::Missing
        );
    }

    #[test]
    fn title_in_comment_is_ignored() {
        assert_eq!(
            title("<head><!-- <title>Old</title> --><title>New</title></head>"),
            Title::Text("New".into())
        );
    }

    #[test]
    fn header_charset_windows_1255() {
        let (bytes, _, _) = encoding_rs::WINDOWS_1255.encode("<title>שלום</title>");
        assert_eq!(
            extract_title(Some("windows-1255"), &bytes),
            Title::Text("שלום".into())
        );
    }

    #[test]
    fn meta_charset_shift_jis() {
        let (bytes, _, _) =
            encoding_rs::SHIFT_JIS.encode("<meta charset=\"Shift_JIS\"><title>こんにちは</title>");
        assert_eq!(extract_title(None, &bytes), Title::Text("こんにちは".into()));
    }

    #[test]
    fn http_equiv_charset() {
        let (bytes, _, _) = encoding_rs::WINDOWS_1252.encode(
            "<meta http-equiv=\"Content-Type\" content=\"text/html; charset=windows-1252\"><title>Café</title>",
        );
        assert_eq!(extract_title(None, &bytes), Title::Text("Café".into()));
    }

    #[test]
    fn bom_beats_header() {
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice("<title>naïve</title>".as_bytes());
        assert_eq!(
            extract_title(Some("windows-1252"), &bytes),
            Title::Text("naïve".into())
        );
    }

    #[test]
    fn html_detection() {
        assert!(is_html(Some("text/html"), b""));
        assert!(is_html(Some("application/xhtml+xml"), b""));
        assert!(!is_html(Some("image/png"), b"<html>"));
        assert!(is_html(None, b"  <!DOCTYPE html><html>"));
        assert!(!is_html(None, b"%PDF-1.7"));
    }
}
