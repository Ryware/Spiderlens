//! Checking text against the right dictionary (spec 027 FR-D02, FR-D05,
//! FR-D06).
//!
//! A page is checked in its declared language, else the default one. Words
//! in another script go to an installed dictionary for that script (Hebrew
//! words on an English page to Hebrew, English words on a Hebrew page to
//! English), or are left unchecked when there is none.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use crate::dict::{self, Store};
use crate::tokenize::{self, Options, Script};

/// The script a dictionary's words are written in.
pub fn script_of_lang(lang: &str) -> Script {
    match lang.split('-').next().unwrap_or(lang) {
        "he" | "yi" => Script::Hebrew,
        "ru" | "uk" | "bg" | "be" | "sr" | "mk" => Script::Cyrillic,
        _ => Script::Latin,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Misspelling {
    pub word: String,
    pub pos: usize,
    pub len: usize,
    /// The dictionary that did not know it.
    pub lang: String,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Counts {
    /// Words looked up.
    pub checked: u32,
    /// Words no installed dictionary covers.
    pub unchecked: u32,
    /// Words in the page's own script, and in all scripts, for the
    /// language-mismatch check.
    pub own_script: u32,
    pub all: u32,
}

/// Loaded dictionaries plus the ignore list.
pub struct Checker {
    dicts: HashMap<String, spellbook::Dictionary>,
    /// Lowercase.
    ignore: HashSet<String>,
    opts: Options,
    suggestions: Mutex<HashMap<(String, String), Vec<String>>>,
}

impl Checker {
    /// Loads every installed dictionary. Loading Hebrew takes about 150 ms,
    /// so a checker is built once per pass.
    pub fn new(store: &Store, ignore: &[String], skip_all_caps: bool) -> Self {
        let dicts = store
            .list()
            .into_iter()
            .filter_map(|d| store.load(&d.lang).ok().map(|x| (d.lang, x)))
            .collect();
        Self::with(dicts, ignore, skip_all_caps)
    }

    pub fn with(
        dicts: HashMap<String, spellbook::Dictionary>,
        ignore: &[String],
        skip_all_caps: bool,
    ) -> Self {
        Self {
            dicts,
            ignore: ignore
                .iter()
                .map(|w| w.trim().to_lowercase())
                .filter(|w| !w.is_empty())
                .collect(),
            opts: Options { skip_all_caps },
            suggestions: Mutex::new(HashMap::new()),
        }
    }

    pub fn has(&self, lang: &str) -> bool {
        self.dicts.contains_key(lang)
    }

    /// The installed dictionary for a page: its declared language exactly,
    /// then the language without its region, then for English the bundled
    /// American dictionary. None when nothing fits.
    pub fn pick(&self, declared: Option<&str>, default: &str) -> Option<String> {
        let tag = declared
            .and_then(dict::normalize_lang)
            .or_else(|| dict::normalize_lang(default))?;
        if self.dicts.contains_key(&tag) {
            return Some(tag);
        }
        let primary = tag.split('-').next().unwrap_or(&tag).to_string();
        if self.dicts.contains_key(&primary) {
            return Some(primary);
        }
        // en-GB pages without a British dictionary: another English one.
        self.dicts
            .keys()
            .filter(|k| k.starts_with(&format!("{primary}-")))
            .min()
            .cloned()
    }

    /// The dictionary for words of `script` on a page checked in `lang`.
    fn for_script<'a>(&'a self, lang: &'a str, script: Script) -> Option<&'a str> {
        if script_of_lang(lang) == script {
            return Some(lang);
        }
        let fallback = match script {
            Script::Hebrew => "he",
            Script::Cyrillic => "ru",
            Script::Latin => dict::BUNDLED,
            Script::Other => return None,
        };
        self.dicts.contains_key(fallback).then_some(fallback)
    }

    /// The misspelt words of `text` on a page checked in `lang`.
    pub fn check(&self, lang: &str, text: &str, counts: &mut Counts) -> Vec<Misspelling> {
        let own = script_of_lang(lang);
        let mut out = Vec::new();
        for w in tokenize::words(text, self.opts) {
            counts.all += 1;
            if w.script == own {
                counts.own_script += 1;
            }
            let Some(d) = self.for_script(lang, w.script) else {
                counts.unchecked += 1;
                continue;
            };
            counts.checked += 1;
            if self.ignore.contains(&w.text.to_lowercase()) {
                continue;
            }
            let dict = &self.dicts[d];
            if dict.check(&w.text) {
                continue;
            }
            out.push(Misspelling {
                word: w.text,
                pos: w.pos,
                len: w.len,
                lang: d.to_string(),
            });
        }
        out
    }

    /// Up to five suggestions, computed once per word and dictionary.
    pub fn suggest(&self, lang: &str, word: &str) -> Vec<String> {
        let key = (lang.to_string(), word.to_string());
        if let Some(s) = self.suggestions.lock().expect("not poisoned").get(&key) {
            return s.clone();
        }
        let mut s = Vec::new();
        if let Some(d) = self.dicts.get(lang) {
            d.suggest(word, &mut s);
        }
        s.truncate(5);
        self.suggestions
            .lock()
            .expect("not poisoned")
            .insert(key, s.clone());
        s
    }
}

/// Up to 120 characters of `text` around the word at `pos`, on character
/// boundaries.
pub fn context(text: &str, pos: usize, len: usize) -> String {
    const AROUND: usize = 50;
    let chars: Vec<char> = text.chars().collect();
    let start = pos.saturating_sub(AROUND);
    let end = (pos + len + AROUND).min(chars.len());
    let mut s: String = chars[start..end].iter().collect();
    if start > 0 {
        s.insert(0, '…');
    }
    if end < chars.len() {
        s.push('…');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checker(extra: &[(&str, &str, &str)]) -> Checker {
        let mut dicts = HashMap::new();
        dicts.insert("en".to_string(), Store::new("/nonexistent").load("en").unwrap());
        for (lang, aff, dic) in extra {
            dicts.insert(lang.to_string(), spellbook::Dictionary::new(aff, dic).unwrap());
        }
        Checker::with(dicts, &["SpiderLens".into()], true)
    }

    const HE_AFF: &str = "SET UTF-8\nPFX A Y 1\nPFX A 0 ו .\n";
    const HE_DIC: &str = "3\nשלום/A\nבית/A\nספר\n";

    #[test]
    fn english_misspellings_are_found_and_suggested() {
        let c = checker(&[]);
        let mut n = Counts::default();
        let found = c.check("en", "We recieve SpiderLens news and acommodate NASA.", &mut n);
        assert_eq!(
            found.iter().map(|m| m.word.as_str()).collect::<Vec<_>>(),
            ["recieve", "acommodate"]
        );
        assert_eq!(
            c.suggest("en", "recieve").first().map(String::as_str),
            Some("receive")
        );
        assert_eq!(n.checked, 6);
    }

    #[test]
    fn hebrew_prefixes_and_niqqud_follow_the_dictionary() {
        let c = checker(&[("he", HE_AFF, HE_DIC)]);
        let mut n = Counts::default();
        let found = c.check("he", "ושלום בָּיִת שלוםם", &mut n);
        assert_eq!(
            found.iter().map(|m| m.word.as_str()).collect::<Vec<_>>(),
            ["שלוםם"]
        );
    }

    #[test]
    fn other_scripts_go_to_their_own_dictionary_or_are_skipped() {
        let with_he = checker(&[("he", HE_AFF, HE_DIC)]);
        let mut n = Counts::default();
        let found = with_he.check("en", "The word שלוםם and recieve", &mut n);
        assert_eq!(
            found
                .iter()
                .map(|m| (m.word.as_str(), m.lang.as_str()))
                .collect::<Vec<_>>(),
            [("שלוםם", "he"), ("recieve", "en")]
        );

        let en_only = checker(&[]);
        let mut n = Counts::default();
        assert!(en_only.check("en", "שלוםם", &mut n).is_empty());
        assert_eq!((n.checked, n.unchecked), (0, 1));
        // English words on a Hebrew page use English.
        let mut n = Counts::default();
        let found = with_he.check("he", "שלום recieve", &mut n);
        assert_eq!(found[0].lang, "en");
        assert_eq!((n.own_script, n.all), (1, 2));
    }

    #[test]
    fn the_page_language_picks_the_dictionary() {
        let c = checker(&[("en-GB", "SET UTF-8\n", "1\ncolour\n"), ("he", HE_AFF, HE_DIC)]);
        assert_eq!(c.pick(Some("en-GB"), "en").as_deref(), Some("en-GB"));
        assert_eq!(c.pick(Some("en-AU"), "en").as_deref(), Some("en"));
        assert_eq!(c.pick(Some("he-IL"), "en").as_deref(), Some("he"));
        assert_eq!(c.pick(Some("iw"), "en").as_deref(), Some("he"));
        assert_eq!(c.pick(None, "he").as_deref(), Some("he"));
        assert_eq!(c.pick(Some("fr"), "en"), None);
        assert_eq!(c.pick(Some("not a tag"), "en").as_deref(), Some("en"));
    }

    #[test]
    fn context_is_cut_on_characters() {
        let text = format!("{}שגיאה{}", "א".repeat(80), "ב".repeat(80));
        let ctx = context(&text, 80, 5);
        assert!(ctx.starts_with('…') && ctx.ends_with('…'));
        assert_eq!(ctx.chars().count(), 107);
        assert!(ctx.contains("שגיאה"));
    }
}
