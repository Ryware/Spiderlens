//! Text rules shared by every extracted value (FR-009, FR-014, plan D5).

/// Trims and collapses runs of ASCII whitespace to one space. HTML treats
/// only ASCII whitespace as collapsible; a non-breaking space is content.
pub fn collapse(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for part in raw
        .split(|c: char| c.is_ascii_whitespace())
        .filter(|p| !p.is_empty())
    {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(part);
    }
    out
}

/// Words are runs of letters, digits and combining marks; anything else
/// (whitespace, punctuation, symbols) separates them (FR-014, research R8).
/// "don't" is two words, "e-mail" is two words, "3.5" is two words.
pub fn word_count(text: &str) -> u32 {
    let mut count = 0u32;
    let mut in_word = false;
    for c in text.chars() {
        let word_char = c.is_alphanumeric() || is_mark(c);
        if word_char && !in_word {
            count += 1;
        }
        in_word = word_char;
    }
    count
}

/// Combining marks (Hebrew points, Devanagari vowel signs, accents written
/// as separate code points) belong to the word they sit on.
pub(crate) fn is_mark(c: char) -> bool {
    matches!(c as u32,
        0x0300..=0x036F | 0x0483..=0x0489 | 0x0591..=0x05BD | 0x05BF | 0x05C1..=0x05C2
        | 0x05C4..=0x05C5 | 0x05C7 | 0x0610..=0x061A | 0x064B..=0x065F | 0x0670
        | 0x06D6..=0x06DC | 0x06DF..=0x06E4 | 0x0900..=0x0903 | 0x093A..=0x094F
        | 0x0E31 | 0x0E34..=0x0E3A | 0x0E47..=0x0E4E | 0x1AB0..=0x1AFF | 0x1DC0..=0x1DFF
        | 0x20D0..=0x20FF | 0xFE20..=0xFE2F)
}

/// Stable 64-bit hash of a stored text, as SQLite's signed INTEGER. xxh3's
/// output is specified, so values match across platforms and versions
/// (research R3).
pub fn hash(text: &str) -> i64 {
    xxhash_rust::xxh3::xxh3_64(text.as_bytes()) as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collapses_ascii_whitespace_only() {
        assert_eq!(collapse("  a \n\t b  "), "a b");
        assert_eq!(collapse("a\u{a0}b"), "a\u{a0}b");
        assert_eq!(collapse(" \n "), "");
    }

    #[test]
    fn words_split_on_whitespace_and_punctuation() {
        assert_eq!(word_count("Hello, world!"), 2);
        assert_eq!(word_count("don't e-mail 3.5"), 6);
        assert_eq!(word_count("שָׁלוֹם עולם"), 2);
        assert_eq!(word_count("  — … "), 0);
        assert_eq!(word_count("naïve café"), 2);
    }

    #[test]
    fn hash_is_stable() {
        // Fixed value: a change here breaks duplicate detection across files.
        assert_eq!(hash(""), 0x2D06800538D394C2u64 as i64);
        assert_ne!(hash("a"), hash("b"));
    }
}
