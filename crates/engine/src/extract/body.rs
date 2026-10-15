//! Raw HTML retention: zstd level 3 (plan D6, research R2, FR-030).

use std::cell::RefCell;

pub const CODEC: &str = "zstd";
const LEVEL: i32 = 3;

thread_local! {
    // Creating a compression context costs more than compressing a small
    // page, so each extraction thread keeps one.
    static COMPRESSOR: RefCell<Option<zstd::bulk::Compressor<'static>>> = const { RefCell::new(None) };
    // The same holds for reading bodies back (the AI checks read every page).
    static DECOMPRESSOR: RefCell<Option<zstd::bulk::Decompressor<'static>>> = const { RefCell::new(None) };
}

/// Compresses a received body. The bytes are stored as received (after
/// transfer decoding), never re-encoded, so a round trip is byte-identical.
pub fn compress(body: &[u8]) -> std::io::Result<Vec<u8>> {
    COMPRESSOR.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.is_none() {
            *slot = Some(zstd::bulk::Compressor::new(LEVEL)?);
        }
        slot.as_mut().expect("set above").compress(body)
    })
}

pub fn decompress(stored: &[u8], size: usize) -> std::io::Result<Vec<u8>> {
    DECOMPRESSOR.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.is_none() {
            *slot = Some(zstd::bulk::Decompressor::new()?);
        }
        slot.as_mut().expect("set above").decompress(stored, size)
    })
}

/// Reads back one `bodies` row (codec, size, stored bytes). The one decoder
/// for stored HTML: the crawl store, `spiderlens html` and the results
/// viewer all go through it.
pub fn unpack(codec: &str, size: i64, stored: &[u8]) -> Result<Vec<u8>, String> {
    if codec != CODEC {
        return Err(format!("unknown body codec {codec}"));
    }
    let size = usize::try_from(size).map_err(|_| format!("stored HTML has a bad size {size}"))?;
    decompress(stored, size).map_err(|e| format!("stored HTML is damaged: {e}"))
}

/// Stored HTML as text, decoded with the encoding extraction read the page
/// with (`pages.encoding`), or UTF-8 when that is unknown. Returns the text,
/// the encoding used and whether any bytes were invalid in it.
pub fn to_text(body: &[u8], encoding: Option<&str>) -> (String, &'static str, bool) {
    let enc = encoding
        .and_then(|l| encoding_rs::Encoding::for_label(l.as_bytes()))
        .unwrap_or(encoding_rs::UTF_8);
    let (text, used, errors) = enc.decode(body);
    (text.into_owned(), used.name(), errors)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_is_byte_identical() {
        let mut body = b"<!doctype html><title>\xE9t\xE9</title>".to_vec();
        body.extend(std::iter::repeat_n(b"<p>Lorem ipsum dolor sit amet</p>".as_slice(), 200).flatten());
        let packed = compress(&body).unwrap();
        assert!(packed.len() * 10 < body.len());
        assert_eq!(decompress(&packed, body.len()).unwrap(), body);
        assert_eq!(decompress(&compress(b"").unwrap(), 0).unwrap(), b"");
    }

    #[test]
    fn unpack_checks_the_codec_and_the_bytes() {
        let packed = compress(b"<p>hi</p>").unwrap();
        assert_eq!(unpack(CODEC, 9, &packed).unwrap(), b"<p>hi</p>");
        assert!(
            unpack("brotli", 9, &packed)
                .unwrap_err()
                .contains("unknown body codec")
        );
        assert!(unpack(CODEC, 9, b"not zstd").unwrap_err().contains("damaged"));
        assert!(unpack(CODEC, -1, &packed).is_err());
    }

    #[test]
    fn to_text_uses_the_page_encoding() {
        let (latin, _, _) = encoding_rs::WINDOWS_1252.encode("<p>Café</p>");
        assert_eq!(
            to_text(&latin, Some("windows-1252")),
            ("<p>Café</p>".into(), "windows-1252", false)
        );
        assert_eq!(
            to_text("<p>Café</p>".as_bytes(), None),
            ("<p>Café</p>".into(), "UTF-8", false)
        );
        let (text, _, errors) = to_text(b"<p>\xFF</p>", Some("UTF-8"));
        assert_eq!(text, "<p>\u{FFFD}</p>");
        assert!(errors);
    }
}
