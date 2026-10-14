//! Content signatures for near duplicate detection (spec 020, plan D2).
//!
//! The main content is cut into tokens (words by the spec 005 rules,
//! lowercased; single characters for scripts written without spaces), the
//! tokens into overlapping runs of [`SHINGLE`], and the set of runs into a
//! fixed-size one-permutation MinHash: each run is hashed once, the hash
//! picks one of [`BINS`] bins and each bin keeps its smallest value. Empty
//! bins borrow from a non-empty one chosen by a fixed sequence (optimal
//! densification), so short texts still compare correctly. The share of
//! equal bins between two signatures estimates the share of shared runs
//! (Jaccard similarity), research R1 and R2.
//!
//! Every hash here is xxh3 or a fixed integer mix, so signatures are byte
//! for byte the same on every platform and version; changing anything that
//! alters them needs a new analysis version.

use super::text::is_mark;

/// Number of bins in a signature.
pub const BINS: usize = 960;
/// Tokens per shingle.
pub const SHINGLE: usize = 5;
/// Bytes of a stored signature: one per bin.
pub const SIG_BYTES: usize = BINS;

/// A content signature: the low 8 bits of each bin's minimum (b-bit
/// MinHash, research R2).
#[derive(Clone, PartialEq, Eq)]
pub struct Signature(pub [u8; BINS]);

impl std::fmt::Debug for Signature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Signature({:02x} {:02x} {:02x} ..)",
            self.0[0], self.0[1], self.0[2]
        )
    }
}

impl Signature {
    pub fn to_bytes(&self) -> Vec<u8> {
        self.0.to_vec()
    }

    /// Reads a stored signature; `None` when the length is wrong.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        bytes.try_into().ok().map(Self)
    }

    /// Bins with the same value in both signatures.
    pub fn equal_bins(&self, other: &Self) -> u32 {
        self.0
            .iter()
            .zip(other.0.iter())
            .map(|(a, b)| u32::from(a == b))
            .sum()
    }

    /// Estimated similarity in whole percent, 0 to 100.
    pub fn similarity(&self, other: &Self) -> u8 {
        similarity_of(self.equal_bins(other))
    }
}

/// Whole percent for a count of equal bins. Two unrelated bins hold the
/// same 8 bits by chance once in 256, so the share of equal bins `m` is
/// corrected to `(m - 1/256) / (1 - 1/256)` (b-bit MinHash estimator).
pub fn similarity_of(equal_bins: u32) -> u8 {
    let m = f64::from(equal_bins) / BINS as f64;
    let j = ((m - CHANCE) / (1.0 - CHANCE)).clamp(0.0, 1.0);
    (j * 100.0).round() as u8
}

/// Chance that two unrelated bins match.
const CHANCE: f64 = 1.0 / 256.0;

/// Scripts written without spaces between words: every character is a
/// token, so their text still forms meaningful shingles (FR-005).
fn no_space_script(c: char) -> bool {
    matches!(c as u32,
        0x0E00..=0x0E7F            // Thai
        | 0x0E80..=0x0EFF          // Lao
        | 0x1000..=0x109F          // Myanmar
        | 0x1780..=0x17FF          // Khmer
        | 0x3040..=0x30FF          // Hiragana, Katakana
        | 0x3400..=0x4DBF          // CJK extension A
        | 0x4E00..=0x9FFF          // CJK unified ideographs
        | 0xAC00..=0xD7AF          // Hangul syllables
        | 0xF900..=0xFAFF          // CJK compatibility ideographs
        | 0x20000..=0x2FFFF) // CJK extensions B and later
}

/// Hashes of the tokens of `text`, in order.
pub fn token_hashes(text: &str) -> Vec<u64> {
    let mut out = Vec::with_capacity(text.len() / 6);
    let mut word = String::new();
    let flush = |word: &mut String, out: &mut Vec<u64>| {
        if !word.is_empty() {
            out.push(xxhash_rust::xxh3::xxh3_64(word.as_bytes()));
            word.clear();
        }
    };
    for c in text.chars() {
        if is_mark(c) {
            // A mark belongs to the character it sits on; after a
            // no-space-script character (already a token) it adds nothing.
            if !word.is_empty() {
                word.push(c);
            }
        } else if no_space_script(c) {
            flush(&mut word, &mut out);
            word.push(c);
            flush(&mut word, &mut out);
        } else if c.is_alphanumeric() {
            word.extend(c.to_lowercase());
        } else {
            flush(&mut word, &mut out);
        }
    }
    flush(&mut word, &mut out);
    out
}

/// Hashes of every run of [`SHINGLE`] consecutive tokens. Fewer tokens than
/// that make one shingle of all of them; no tokens make none.
pub fn shingle_hashes(tokens: &[u64]) -> impl Iterator<Item = u64> + '_ {
    let width = SHINGLE.min(tokens.len().max(1));
    let windows = if tokens.is_empty() {
        0
    } else {
        tokens.len() - width + 1
    };
    (0..windows).map(move |i| {
        let mut buf = [0u8; SHINGLE * 8];
        for (k, t) in tokens[i..i + width].iter().enumerate() {
            buf[k * 8..k * 8 + 8].copy_from_slice(&t.to_le_bytes());
        }
        xxhash_rust::xxh3::xxh3_64(&buf[..width * 8])
    })
}

/// xxh3 with a seed, for keys derived from signatures (band keys).
pub fn seeded_hash(bytes: &[u8], seed: u64) -> u64 {
    xxhash_rust::xxh3::xxh3_64_with_seed(bytes, seed)
}

/// SplitMix64's finaliser: a fixed, well-mixed integer permutation.
const fn mix(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// The signature of `text`, or `None` when it has no tokens.
pub fn signature(text: &str) -> Option<Signature> {
    let tokens = token_hashes(text);
    signature_of_shingles(shingle_hashes(&tokens))
}

/// The signature of a set of shingle hashes (duplicates are harmless).
pub fn signature_of_shingles(shingles: impl Iterator<Item = u64>) -> Option<Signature> {
    let mut mins = [u64::MAX; BINS];
    let mut filled = [false; BINS];
    let mut any = false;
    for h in shingles {
        // The bin comes from the high bits of the shingle hash, the value
        // compared from an independent mix of it.
        let bin = ((u128::from(h) * BINS as u128) >> 64) as usize;
        let v = mix(h ^ 0x9E37_79B9_7F4A_7C15);
        if v < mins[bin] {
            mins[bin] = v;
        }
        filled[bin] = true;
        any = true;
    }
    if !any {
        return None;
    }
    let mut out = [0u8; BINS];
    for i in 0..BINS {
        let src = if filled[i] {
            i
        } else {
            // Optimal densification: probe bins in a sequence fixed by the
            // empty bin's index until a filled one is found. Two signatures
            // with the same filled bins borrow the same way.
            let mut attempt = 1u64;
            loop {
                let j = ((u128::from(mix(((i as u64) << 32) | attempt)) * BINS as u128) >> 64) as usize;
                if filled[j] {
                    break j;
                }
                attempt += 1;
            }
        };
        out[i] = mins[src] as u8;
    }
    Some(Signature(out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn words(n: usize, seed: u64) -> Vec<String> {
        (0..n)
            .map(|i| format!("w{}", mix(seed.wrapping_mul(1_000_003) + i as u64) % 1_000_000))
            .collect()
    }

    #[test]
    fn tokens_follow_word_rules_and_case() {
        assert_eq!(token_hashes("Hello, WORLD"), token_hashes("hello world"));
        assert_eq!(token_hashes("don't").len(), 2);
        assert_eq!(token_hashes("  — … ").len(), 0);
        assert_eq!(token_hashes("שָׁלוֹם עולם").len(), 2);
    }

    #[test]
    fn no_space_scripts_split_per_character() {
        assert_eq!(token_hashes("東京都に行く").len(), 6);
        assert_eq!(token_hashes("abc東京").len(), 3);
        assert_eq!(
            token_hashes("สวัสดีครับ").len(),
            "สวัสดีครับ".chars().filter(|c| !is_mark(*c)).count()
        );
    }

    #[test]
    fn shingles_slide_and_short_texts_make_one() {
        let t = token_hashes("a b c d e f g");
        assert_eq!(shingle_hashes(&t).count(), 3);
        let t = token_hashes("a b");
        assert_eq!(shingle_hashes(&t).count(), 1);
        assert_eq!(shingle_hashes(&[]).count(), 0);
    }

    #[test]
    fn identical_and_disjoint() {
        let a = words(400, 1).join(" ");
        let b = words(400, 2).join(" ");
        let sa = signature(&a).unwrap();
        assert_eq!(sa.similarity(&signature(&a).unwrap()), 100);
        assert!(sa.similarity(&signature(&b).unwrap()) <= 2);
        assert!(signature("  ... ").is_none());
    }

    #[test]
    fn bytes_round_trip() {
        let s = signature("one two three four five six").unwrap();
        let bytes = s.to_bytes();
        assert_eq!(bytes.len(), SIG_BYTES);
        assert_eq!(Signature::from_bytes(&bytes), Some(s));
        assert_eq!(Signature::from_bytes(&bytes[1..]), None);
    }

    #[test]
    fn golden_signature_is_stable() {
        // Fixed bytes: a change here changes every stored signature and
        // needs a new analysis version (plan D2).
        let s = signature("The quick brown fox jumps over the lazy dog near the river bank").unwrap();
        let head: [u8; 6] = s.0[..6].try_into().unwrap();
        let sum: u64 = s.0.iter().map(|&v| u64::from(v)).sum();
        assert_eq!((head, sum), GOLDEN);
    }

    const GOLDEN: ([u8; 6], u64) = ([18, 188, 18, 18, 47, 188], 135_781);

    fn shingle_set(text: &str) -> HashSet<u64> {
        shingle_hashes(&token_hashes(text)).collect()
    }

    fn jaccard(a: &HashSet<u64>, b: &HashSet<u64>) -> f64 {
        a.intersection(b).count() as f64 / a.union(b).count() as f64
    }

    /// SC-003: estimates within 3 points of the exact value for at least 95%
    /// of pairs, from 100 words up.
    #[test]
    fn accuracy_against_exact_jaccard() {
        let mut errors = Vec::new();
        for case in 0..400u64 {
            let n = [100, 300, 1000, 2000][(case % 4) as usize];
            let base = words(n, 10 + case);
            // Replace a run of words in the middle to reach various levels.
            let change = ((case * 7) % 40) as usize * n / 400; // 0 to ~10% of words
            let mut other = base.clone();
            let fresh = words(change, 9_000 + case);
            for (k, w) in fresh.into_iter().enumerate() {
                other[(n / 3 + k * 3) % n] = w;
            }
            let (a, b) = (base.join(" "), other.join(" "));
            let exact = jaccard(&shingle_set(&a), &shingle_set(&b)) * 100.0;
            let est = f64::from(signature(&a).unwrap().similarity(&signature(&b).unwrap()));
            errors.push((est - exact).abs());
        }
        errors.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let p95 = errors[errors.len() * 95 / 100 - 1];
        assert!(p95 <= 3.0, "95th percentile error {p95:.2} points");
    }
}
