//! Dictionaries (spec 027 FR-D03, FR-D04): American English is bundled;
//! others are Hunspell `.aff`/`.dic` pairs downloaded on request, checked
//! against the SHA-256 in the manifest, or added from files. Each lives in
//! its own folder under the app's data folder, so removing one is deleting
//! a folder. Copyleft dictionaries (Hebrew is AGPL-3.0) are data the user
//! chooses to fetch, never compiled into the app; the manifest carries each
//! licence so it can be shown before downloading.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use ts_rs::TS;

/// The bundled dictionary (American English, SCOWL size 60).
pub const BUNDLED: &str = "en";
const BUNDLED_AFF: &str = include_str!("../dictionaries/en/en.aff");
const BUNDLED_DIC: &str = include_str!("../dictionaries/en/en.dic");

#[derive(Debug, thiserror::Error)]
pub enum DictError {
    #[error("no dictionary for {0} is installed")]
    NotInstalled(String),
    #[error("{0} is not in the list of dictionaries")]
    Unknown(String),
    #[error("the bundled English dictionary cannot be removed")]
    Bundled,
    #[error("{0} is not a valid language code")]
    BadLang(String),
    #[error("the dictionary files could not be read: {0}")]
    Invalid(String),
    #[error("downloading {file} failed: {why}")]
    Download { file: String, why: String },
    #[error("{0} did not match its published checksum, so it was not installed")]
    Checksum(String),
    #[error("{0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, DictError>;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RemoteFile {
    pub name: String,
    pub url: String,
    #[ts(type = "number")]
    pub size: u64,
    pub sha256: String,
}

/// A dictionary that can be downloaded.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Available {
    pub lang: String,
    pub name: String,
    pub licence: String,
    pub files: Vec<RemoteFile>,
}

impl Available {
    pub fn size(&self) -> u64 {
        self.files.iter().map(|f| f.size).sum()
    }
}

/// A dictionary that can be downloaded, with what to show before download.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct Downloadable {
    pub lang: String,
    pub name: String,
    pub licence: String,
    #[ts(type = "number")]
    pub size: u64,
}

/// Installed dictionaries, and those that can be downloaded.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct Dictionaries {
    pub installed: Vec<Installed>,
    pub available: Vec<Downloadable>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Manifest {
    pub source: String,
    pub dictionaries: Vec<Available>,
}

/// The dictionaries this version can download, pinned to one commit of
/// github.com/wooorm/dictionaries with a checksum per file.
pub fn manifest() -> &'static Manifest {
    static M: OnceLock<Manifest> = OnceLock::new();
    M.get_or_init(|| serde_json::from_str(include_str!("../dictionaries.json")).expect("the manifest parses"))
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Origin {
    Bundled,
    Downloaded,
    File,
}

/// An installed dictionary.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Installed {
    pub lang: String,
    pub name: String,
    pub licence: String,
    #[ts(type = "number")]
    pub size: u64,
    pub origin: Origin,
}

/// `en-gb` → `en-GB`, `IW` → `he`. None when it is not a language code.
pub fn normalize_lang(tag: &str) -> Option<String> {
    let mut parts = tag.trim().split(['-', '_']);
    let primary = parts.next()?.to_ascii_lowercase();
    if !(2..=3).contains(&primary.len()) || !primary.chars().all(|c| c.is_ascii_lowercase()) {
        return None;
    }
    let primary = match primary.as_str() {
        "iw" => "he".to_string(),
        _ => primary,
    };
    match parts.next() {
        Some(r) if r.len() == 2 && r.chars().all(|c| c.is_ascii_alphabetic()) => {
            Some(format!("{primary}-{}", r.to_ascii_uppercase()))
        }
        _ => Some(primary),
    }
}

/// Where the app keeps dictionaries: `dictionaries` in the desktop app's
/// data folder (identifier com.ryware.spiderlens), so the app and the CLI
/// share them. `SPIDERLENS_DICTIONARIES` overrides it.
pub fn default_dir() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("SPIDERLENS_DICTIONARIES") {
        return Some(PathBuf::from(d));
    }
    const ID: &str = "com.ryware.spiderlens";
    let env = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    let base = if cfg!(target_os = "windows") {
        env("APPDATA")
    } else if cfg!(target_os = "macos") {
        env("HOME").map(|h| h.join("Library/Application Support"))
    } else {
        env("XDG_DATA_HOME").or_else(|| env("HOME").map(|h| h.join(".local/share")))
    }?;
    Some(base.join(ID).join("dictionaries"))
}

const INFO: &str = "info.json";
const AFF: &str = "index.aff";
const DIC: &str = "index.dic";

/// The dictionaries folder.
#[derive(Debug, Clone)]
pub struct Store {
    dir: PathBuf,
}

impl Store {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The bundled dictionary first, then installed ones by language.
    pub fn list(&self) -> Vec<Installed> {
        let mut out = vec![Installed {
            lang: BUNDLED.into(),
            name: "English (United States)".into(),
            licence: "MIT AND BSD".into(),
            size: (BUNDLED_AFF.len() + BUNDLED_DIC.len()) as u64,
            origin: Origin::Bundled,
        }];
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return out;
        };
        let mut found: Vec<Installed> = entries
            .flatten()
            .filter_map(|e| {
                let info = std::fs::read_to_string(e.path().join(INFO)).ok()?;
                let i: Installed = serde_json::from_str(&info).ok()?;
                (e.path().join(AFF).is_file() && e.path().join(DIC).is_file()).then_some(i)
            })
            .collect();
        found.sort_by(|a, b| a.lang.cmp(&b.lang));
        out.extend(found);
        out
    }

    pub fn dictionaries(&self) -> Dictionaries {
        Dictionaries {
            installed: self.list(),
            available: manifest()
                .dictionaries
                .iter()
                .filter(|d| !self.is_installed(&d.lang))
                .map(|d| Downloadable {
                    lang: d.lang.clone(),
                    name: d.name.clone(),
                    licence: d.licence.clone(),
                    size: d.size(),
                })
                .collect(),
        }
    }

    pub fn is_installed(&self, lang: &str) -> bool {
        lang == BUNDLED || self.dir.join(lang).join(INFO).is_file()
    }

    /// Loads one dictionary.
    pub fn load(&self, lang: &str) -> Result<spellbook::Dictionary> {
        if lang == BUNDLED {
            return spellbook::Dictionary::new(BUNDLED_AFF, BUNDLED_DIC)
                .map_err(|e| DictError::Invalid(e.to_string()));
        }
        let dir = self.dir.join(lang);
        if !dir.join(INFO).is_file() {
            return Err(DictError::NotInstalled(lang.into()));
        }
        let aff = std::fs::read_to_string(dir.join(AFF))?;
        let dic = std::fs::read_to_string(dir.join(DIC))?;
        spellbook::Dictionary::new(&aff, &dic).map_err(|e| DictError::Invalid(e.to_string()))
    }

    /// Installs a dictionary from a `.aff` and `.dic` pair the user has. The
    /// files must be UTF-8 and load as a dictionary.
    pub fn add_files(&self, lang: &str, aff: &Path, dic: &Path) -> Result<Installed> {
        let lang = normalize_lang(lang).ok_or_else(|| DictError::BadLang(lang.into()))?;
        if lang == BUNDLED {
            return Err(DictError::Bundled);
        }
        let read = |p: &Path| {
            std::fs::read_to_string(p).map_err(|e| DictError::Invalid(format!("{}: {e}", p.display())))
        };
        let (a, d) = (read(aff)?, read(dic)?);
        spellbook::Dictionary::new(&a, &d).map_err(|e| DictError::Invalid(e.to_string()))?;
        let info = Installed {
            name: manifest()
                .dictionaries
                .iter()
                .find(|x| x.lang == lang)
                .map_or_else(|| lang.clone(), |x| x.name.clone()),
            licence: "As supplied".into(),
            size: (a.len() + d.len()) as u64,
            origin: Origin::File,
            lang,
        };
        self.write(&info, &[(AFF, a.as_bytes()), (DIC, d.as_bytes())])?;
        Ok(info)
    }

    /// Downloads a dictionary from the manifest.
    pub async fn download(&self, lang: &str, client: &reqwest::Client) -> Result<Installed> {
        let entry = manifest()
            .dictionaries
            .iter()
            .find(|d| d.lang == lang)
            .ok_or_else(|| DictError::Unknown(lang.into()))?;
        self.download_entry(entry, client).await
    }

    /// Downloads every file of `entry`, checks each against its size and
    /// SHA-256, and installs them only when all match.
    pub async fn download_entry(&self, entry: &Available, client: &reqwest::Client) -> Result<Installed> {
        let mut files: Vec<(&str, Vec<u8>)> = Vec::new();
        for f in &entry.files {
            let fail = |why: String| DictError::Download {
                file: f.name.clone(),
                why,
            };
            let resp = client
                .get(&f.url)
                .send()
                .await
                .and_then(reqwest::Response::error_for_status)
                .map_err(|e| fail(e.to_string()))?;
            let bytes = resp.bytes().await.map_err(|e| fail(e.to_string()))?;
            let digest = Sha256::digest(&bytes);
            let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
            if bytes.len() as u64 != f.size || hex != f.sha256 {
                return Err(DictError::Checksum(f.name.clone()));
            }
            let name = match f.name.as_str() {
                "index.aff" => AFF,
                "index.dic" => DIC,
                _ => "license",
            };
            files.push((name, bytes.to_vec()));
        }
        let info = Installed {
            lang: entry.lang.clone(),
            name: entry.name.clone(),
            licence: entry.licence.clone(),
            size: entry.size(),
            origin: Origin::Downloaded,
        };
        let refs: Vec<(&str, &[u8])> = files.iter().map(|(n, b)| (*n, b.as_slice())).collect();
        self.write(&info, &refs)?;
        Ok(info)
    }

    pub fn remove(&self, lang: &str) -> Result<()> {
        if lang == BUNDLED {
            return Err(DictError::Bundled);
        }
        let dir = self.dir.join(lang);
        if !dir.join(INFO).is_file() {
            return Err(DictError::NotInstalled(lang.into()));
        }
        std::fs::remove_dir_all(dir)?;
        Ok(())
    }

    /// Writes into a temporary folder, then renames it into place, so a
    /// half-written dictionary is never listed.
    fn write(&self, info: &Installed, files: &[(&str, &[u8])]) -> Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        let tmp = self.dir.join(format!(".{}.partial", info.lang));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp)?;
        for (name, bytes) in files {
            std::fs::write(tmp.join(name), bytes)?;
        }
        std::fs::write(
            tmp.join(INFO),
            serde_json::to_vec_pretty(info).expect("info serializes"),
        )?;
        let dest = self.dir.join(&info.lang);
        let _ = std::fs::remove_dir_all(&dest);
        std::fs::rename(&tmp, &dest)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_codes_are_normalized() {
        assert_eq!(normalize_lang("en-gb").as_deref(), Some("en-GB"));
        assert_eq!(normalize_lang("EN_us").as_deref(), Some("en-US"));
        assert_eq!(normalize_lang("iw").as_deref(), Some("he"));
        assert_eq!(normalize_lang("he-IL").as_deref(), Some("he-IL"));
        assert_eq!(normalize_lang("zh-Hant").as_deref(), Some("zh"));
        assert_eq!(normalize_lang("english"), None);
        assert_eq!(normalize_lang(""), None);
    }

    #[test]
    fn the_manifest_lists_hebrew_with_its_licence() {
        let he = manifest().dictionaries.iter().find(|d| d.lang == "he").unwrap();
        assert_eq!(he.licence, "AGPL-3.0");
        assert!(he.size() > 5_000_000);
        assert!(
            he.files
                .iter()
                .all(|f| f.sha256.len() == 64 && f.url.starts_with("https://"))
        );
    }

    #[test]
    fn english_is_bundled_and_cannot_be_removed() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path());
        assert_eq!(store.list().len(), 1);
        let en = store.load(BUNDLED).unwrap();
        assert!(en.check("receive"));
        assert!(!en.check("recieve"));
        assert!(matches!(store.remove(BUNDLED), Err(DictError::Bundled)));
    }

    #[test]
    fn dictionaries_are_added_from_files_and_removed() {
        let dir = tempfile::tempdir().unwrap();
        let src = tempfile::tempdir().unwrap();
        std::fs::write(src.path().join("x.aff"), "SET UTF-8\n").unwrap();
        std::fs::write(src.path().join("x.dic"), "2\nshalom\ntoda\n").unwrap();
        let store = Store::new(dir.path().join("dicts"));
        let info = store
            .add_files("xx", &src.path().join("x.aff"), &src.path().join("x.dic"))
            .unwrap();
        assert!(matches!(info.origin, Origin::File));
        assert_eq!(
            store.list().iter().map(|d| d.lang.as_str()).collect::<Vec<_>>(),
            ["en", "xx"]
        );
        assert!(store.load("xx").unwrap().check("toda"));
        store.remove("xx").unwrap();
        assert_eq!(store.list().len(), 1);
        assert!(matches!(store.load("xx"), Err(DictError::NotInstalled(_))));
    }

    #[test]
    fn files_that_are_not_a_dictionary_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let src = tempfile::tempdir().unwrap();
        std::fs::write(src.path().join("x.aff"), "FLAG num\n").unwrap();
        std::fs::write(src.path().join("x.dic"), "1\nword/abc\n").unwrap();
        let store = Store::new(dir.path());
        assert!(
            store
                .add_files("xx", &src.path().join("x.aff"), &src.path().join("x.dic"))
                .is_err()
        );
        assert_eq!(store.list().len(), 1);
    }
}
