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

/// The words [`word_count`] counts, in order (spec 014 research R10).
pub fn words(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| !(c.is_alphanumeric() || is_mark(c)))
        .filter(|w| !w.is_empty())
}

/// Sentences and English syllables in visible text, for Flesch reading ease
/// (spec 009 FR-031, plan D11a). A sentence ends at `.`, `!` or `?`
/// followed by whitespace or the end; text with words has at least one.
/// Syllables are vowel groups per word, less a silent final "e", at least
/// one per word.
pub fn readability_counts(text: &str) -> (u32, u32) {
    let mut sentences = 0u32;
    let mut syllables = 0u32;
    let mut words = 0u32;
    // The current word, lowercased ASCII letters only.
    let mut word = [0u8; 64];
    let mut len = 0usize;
    let mut in_word = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        let word_char = c.is_alphanumeric() || is_mark(c);
        if word_char {
            if !in_word {
                words += 1;
                len = 0;
            }
            if c.is_ascii_alphabetic() && len < word.len() {
                word[len] = c.to_ascii_lowercase() as u8;
                len += 1;
            }
        } else {
            if in_word {
                syllables += english_syllables(&word[..len]);
            }
            if matches!(c, '.' | '!' | '?') && chars.peek().is_none_or(|n| n.is_whitespace()) {
                sentences += 1;
            }
        }
        in_word = word_char;
    }
    if in_word {
        syllables += english_syllables(&word[..len]);
    }
    if words > 0 {
        sentences = sentences.max(1);
    }
    (sentences, syllables)
}

fn english_syllables(w: &[u8]) -> u32 {
    let vowel = |b: u8| matches!(b, b'a' | b'e' | b'i' | b'o' | b'u' | b'y');
    let mut n = 0u32;
    let mut prev = false;
    for &b in w {
        let v = vowel(b);
        if v && !prev {
            n += 1;
        }
        prev = v;
    }
    // "make", "time": the final e is silent; "table", "see" keep it.
    if n > 1 && w.ends_with(b"e") && !w.ends_with(b"le") && !w.ends_with(b"ee") {
        n -= 1;
    }
    n.max(1)
}

/// Combining marks (Hebrew points, Devanagari vowel signs, accents written
/// as separate code points) belong to the word they sit on.
fn is_mark(c: char) -> bool {
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
        for t in [
            "Hello, world!",
            "don't e-mail 3.5",
            "שָׁלוֹם עולם",
            "  — … ",
            "a\u{a0}b",
        ] {
            assert_eq!(words(t).count() as u32, word_count(t), "{t}");
        }
        assert_eq!(
            words("don't e-mail").collect::<Vec<_>>(),
            ["don", "t", "e", "mail"]
        );
    }

    #[test]
    fn readability_counts_sentences_and_syllables() {
        assert_eq!(readability_counts(""), (0, 0));
        assert_eq!(readability_counts("Hello"), (1, 2));
        assert_eq!(readability_counts("The cat sat. It was happy! Was it?"), (3, 9));
        // Decimal points and abbreviations inside a word end no sentence.
        assert_eq!(readability_counts("Version 3.5 is out."), (1, 6));
        assert_eq!(english_syllables(b"make"), 1);
        assert_eq!(english_syllables(b"table"), 2);
        assert_eq!(english_syllables(b"see"), 1);
        assert_eq!(english_syllables(b"beautiful"), 3);
        assert_eq!(english_syllables(b"rhythm"), 1);
        assert_eq!(english_syllables(b""), 1);
    }

    #[test]
    fn known_passage_scores_like_flesch() {
        // A plain children's-book passage scores high, a dense one low.
        let score = |t: &str| {
            let (s, y) = readability_counts(t);
            let w = word_count(t) as f64;
            206.835 - 1.015 * w / s as f64 - 84.6 * y as f64 / w
        };
        let easy = "The cat sat on the mat. The dog ran to the cat. They had fun in the sun.";
        let hard = "Notwithstanding considerable methodological heterogeneity, the investigators \
                    characterised organisational interdependencies comprehensively.";
        assert!(score(easy) > 90.0, "{}", score(easy));
        assert!(score(hard) < 10.0, "{}", score(hard));
    }

    #[test]
    fn hash_is_stable() {
        // Fixed value: a change here breaks duplicate detection across files.
        assert_eq!(hash(""), 0x2D06800538D394C2u64 as i64);
        assert_ne!(hash("a"), hash("b"));
    }
}
