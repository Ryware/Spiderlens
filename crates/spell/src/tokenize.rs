//! Words to check, from page text (spec 027 FR-D05, edge cases).
//!
//! A word is a run of letters, with apostrophes inside it (`don't`) and, in
//! Hebrew, geresh and gershayim (`צה"ל`). Hyphens and the Hebrew maqaf
//! separate words, so each part of `well-known` is checked. Hebrew niqqud
//! and cantillation marks are removed before lookup, since dictionaries
//! list unpointed words. Skipped: anything glued to a digit, URLs, email
//! addresses, single letters and, when asked, words in capitals only.

/// The writing system of a word, to pick its dictionary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Script {
    Latin,
    Hebrew,
    Cyrillic,
    Other,
}

pub fn script_of(c: char) -> Script {
    match c {
        'A'..='Z' | 'a'..='z' | '\u{00C0}'..='\u{024F}' | '\u{1E00}'..='\u{1EFF}' => Script::Latin,
        '\u{0590}'..='\u{05FF}' | '\u{FB1D}'..='\u{FB4F}' => Script::Hebrew,
        '\u{0400}'..='\u{052F}' => Script::Cyrillic,
        _ => Script::Other,
    }
}

/// Hebrew points and cantillation: dropped before lookup.
fn is_hebrew_mark(c: char) -> bool {
    matches!(c, '\u{0591}'..='\u{05BD}' | '\u{05BF}' | '\u{05C1}' | '\u{05C2}' | '\u{05C4}' | '\u{05C5}' | '\u{05C7}')
}

/// Combining marks that belong to the letter before them.
fn is_mark(c: char) -> bool {
    is_hebrew_mark(c) || matches!(c, '\u{0300}'..='\u{036F}')
}

fn is_apostrophe(c: char) -> bool {
    matches!(c, '\'' | '’' | 'ʼ')
}

/// Geresh and gershayim, and the ASCII marks typed for them.
fn is_hebrew_quote(c: char) -> bool {
    matches!(c, '׳' | '״' | '"' | '\'')
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Word {
    /// As looked up: marks removed, typographic apostrophes made plain.
    pub text: String,
    /// Character offset of the word in the text.
    pub pos: usize,
    /// Length in characters, as written.
    pub len: usize,
    pub script: Script,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Options {
    pub skip_all_caps: bool,
}

/// The words of `text` to check, in order.
pub fn words(text: &str, opts: Options) -> Vec<Word> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        // Whole chunks between spaces that are addresses are skipped.
        if chars[i].is_whitespace() {
            i += 1;
            continue;
        }
        let end = chars[i..].iter().position(|c| c.is_whitespace()).map_or(chars.len(), |n| i + n);
        let chunk: String = chars[i..end].iter().collect();
        if is_address(&chunk) {
            i = end;
            continue;
        }
        split_chunk(&chars[i..end], i, opts, &mut out);
        i = end;
    }
    out
}

fn is_address(chunk: &str) -> bool {
    let lower = chunk.to_ascii_lowercase();
    lower.contains("://")
        || lower.starts_with("www.")
        || (chunk.contains('@') && chunk.contains('.'))
        || lower.trim_end_matches(|c: char| !c.is_alphanumeric()).ends_with(".com")
}

/// Splits one chunk into words.
fn split_chunk(chunk: &[char], offset: usize, opts: Options, out: &mut Vec<Word>) {
    let mut i = 0;
    while i < chunk.len() {
        let c = chunk[i];
        if !(c.is_alphanumeric()) {
            i += 1;
            continue;
        }
        // A run of letters, digits, marks and inner quotes.
        let start = i;
        let mut j = i;
        while j < chunk.len() {
            let c = chunk[j];
            let inner_quote = (is_apostrophe(c) || is_hebrew_quote(c))
                && j > start
                && chunk.get(j + 1).is_some_and(|n| n.is_alphabetic());
            if c.is_alphanumeric() || is_mark(c) || inner_quote {
                j += 1;
            } else {
                break;
            }
        }
        let run = &chunk[start..j];
        i = j;
        if run.iter().any(|c| c.is_numeric()) {
            continue;
        }
        // A change of script inside a run makes separate words.
        let mut k = 0;
        while k < run.len() {
            let script = script_of(run[k]);
            let mut m = k + 1;
            while m < run.len() && (script_of(run[m]) == script || !run[m].is_alphabetic()) {
                m += 1;
            }
            push_word(&run[k..m], offset + start + k, script, opts, out);
            k = m;
        }
    }
}

fn push_word(w: &[char], pos: usize, script: Script, opts: Options, out: &mut Vec<Word>) {
    let text: String = w
        .iter()
        .filter(|c| !is_hebrew_mark(**c))
        .map(|&c| match c {
            '’' | 'ʼ' => '\'',
            '״' => '"',
            '׳' => '\'',
            c => c,
        })
        .collect();
    let letters = text.chars().filter(|c| c.is_alphabetic()).count();
    if letters < 2 || script == Script::Other {
        return;
    }
    if opts.skip_all_caps && text.chars().any(char::is_uppercase) && !text.chars().any(char::is_lowercase) {
        return;
    }
    out.push(Word {
        text,
        pos,
        len: w.len(),
        script,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(s: &str) -> Vec<String> {
        words(s, Options { skip_all_caps: true }).into_iter().map(|w| w.text).collect()
    }

    #[test]
    fn splits_on_spaces_punctuation_and_hyphens() {
        assert_eq!(texts("Hello, world! A well-known fact."), ["Hello", "world", "well", "known", "fact"]);
    }

    #[test]
    fn keeps_inner_apostrophes() {
        assert_eq!(texts("don’t 'quoted' it's"), ["don't", "quoted", "it's"]);
    }

    #[test]
    fn skips_digits_addresses_and_capitals() {
        assert_eq!(
            texts("mp3 2nd see https://x.test/abc mail me@x.test or www.site.org NASA rocks"),
            ["see", "mail", "or", "rocks"]
        );
        let all = words("NASA", Options { skip_all_caps: false });
        assert_eq!(all.len(), 1);
    }

    #[test]
    fn hebrew_niqqud_is_removed_and_maqaf_separates() {
        assert_eq!(texts("בָּיִת גדול"), ["בית", "גדול"]);
        assert_eq!(texts("בית־ספר"), ["בית", "ספר"]);
    }

    #[test]
    fn hebrew_acronyms_keep_their_quotes() {
        assert_eq!(texts("צה״ל ו-צה\"ל"), ["צה\"ל", "צה\"ל"]);
    }

    #[test]
    fn mixed_scripts_become_separate_words() {
        let w = words("Reactבעברית", Options::default());
        assert_eq!(w.iter().map(|w| w.script).collect::<Vec<_>>(), [Script::Latin, Script::Hebrew]);
        assert_eq!(w[1].pos, 5);
    }

    #[test]
    fn positions_are_character_offsets() {
        let w = words("שלום world", Options::default());
        assert_eq!((w[1].pos, w[1].len), (5, 5));
    }
}
