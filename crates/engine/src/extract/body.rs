//! Raw HTML retention: zstd level 3 (plan D6, research R2, FR-030).

use std::cell::RefCell;

pub const CODEC: &str = "zstd";
const LEVEL: i32 = 3;

thread_local! {
    // Creating a compression context costs more than compressing a small
    // page, so each extraction thread keeps one.
    static COMPRESSOR: RefCell<Option<zstd::bulk::Compressor<'static>>> = const { RefCell::new(None) };
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
    zstd::bulk::decompress(stored, size)
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
}
