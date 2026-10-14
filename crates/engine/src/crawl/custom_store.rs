//! Custom rule results in the crawl file (spec 013, data-model.md).
//!
//! One packed row per page whatever the number of rules (plan D7): the
//! crawl's single writer is the bottleneck, so a row per page and rule
//! would cost as much as the page's links.

use std::collections::HashMap;

use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Map, Value};
use spiderlens_config::{CustomRules, Extractor, Search};

use super::{CrawlStore, Result, StoreError};
use crate::custom::{PageRules, RuleOutcome, flags};
use crate::extract::body;

/// Bytes per rule slot in `custom_results.slots`.
const SLOT_BYTES: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleKind {
    Extractor,
    Search,
}

impl RuleKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Extractor => "extractor",
            Self::Search => "search",
        }
    }
}

/// A rule as stored with a crawl.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredRule {
    pub id: i64,
    pub slot: u32,
    pub kind: RuleKind,
    pub name: String,
    pub def_json: String,
    pub pos: i64,
    pub last_run_at: Option<String>,
    pub run_pages: Option<i64>,
    pub run_state: Option<String>,
}

/// Where each configured rule's results go: slots in rule order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Slots {
    pub extractors: Vec<u32>,
    pub searches: Vec<u32>,
}

/// A page's stored HTML as read for a rerun.
#[derive(Debug, Clone)]
pub struct StoredBody {
    pub url_id: i64,
    pub url: String,
    /// The encoding the crawl decoded the page with.
    pub encoding: Option<String>,
    /// The body was cut off when crawled.
    pub partial: bool,
    /// zstd-compressed received body.
    pub packed: Vec<u8>,
}

/// By slot: stored values after the first, and an error message.
pub type SlotValues = HashMap<u32, (Vec<String>, Option<String>)>;

/// One page's results, unpacked.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PageCustom {
    /// By slot: (count, flags, first value).
    pub slots: Vec<(u32, u16, Option<String>)>,
    /// By slot: extractor values after the first (which is in `slots`),
    /// or every search match in context; and an error message.
    pub values: SlotValues,
}

impl PageCustom {
    /// Every stored value of a slot, the first included.
    pub fn all_values(&self, slot: u32) -> Vec<String> {
        let mut out: Vec<String> = self
            .slots
            .get(slot as usize)
            .and_then(|s| s.2.clone())
            .into_iter()
            .collect();
        if let Some((rest, _)) = self.values.get(&slot) {
            out.extend(rest.iter().cloned());
        }
        out
    }

    pub fn error(&self, slot: u32) -> Option<&str> {
        self.values.get(&slot).and_then(|v| v.1.as_deref())
    }
}

impl CrawlStore {
    /// Whether the crawl has stored rules (FR-027: a crawl without rules
    /// does no rule work, though its file has the empty tables).
    pub fn has_custom_rules(&self) -> Result<bool> {
        has_rules(&self.conn)
    }

    /// The tables are created by migration step 6; a file older than that
    /// gets them here when rules first run on it, without a full upgrade.
    pub fn ensure_custom_schema(&self) -> Result<()> {
        if !has_tables(&self.conn)? {
            self.conn.execute_batch(crate::crawl::migrate::CUSTOM_SCHEMA)?;
        }
        let features = self.meta("data_features")?.unwrap_or_default();
        if !features.split(',').any(|f| f == "custom") {
            let joined = if features.is_empty() {
                "custom".to_string()
            } else {
                format!("{features},custom")
            };
            self.set_meta("data_features", &joined)?;
        }
        Ok(())
    }

    /// The rules stored with this crawl, in display order.
    pub fn custom_rules(&self) -> Result<Vec<StoredRule>> {
        read_rules(&self.conn)
    }

    /// Stores the configured rules with the crawl and returns their slots.
    /// A rule is matched by kind and name: an unchanged one keeps its slot
    /// and values; a changed definition keeps its slot and is marked stale
    /// until it runs again; a new one gets a fresh slot. Rules the
    /// configuration no longer has keep their stored results until deleted
    /// ([`CrawlStore::delete_custom_rule`]).
    pub fn sync_custom_rules(&self, rules: &CustomRules) -> Result<Slots> {
        self.ensure_custom_schema()?;
        let existing = self.custom_rules()?;
        let mut next_slot = existing.iter().map(|r| r.slot + 1).max().unwrap_or(0);
        let mut slot_for = |kind: RuleKind, name: &str, def: String, pos: usize| -> Result<u32> {
            let found = existing.iter().find(|r| r.kind == kind && r.name == name);
            Ok(match found {
                Some(r) => {
                    let stale = r.def_json != def;
                    self.conn
                        .prepare_cached(
                            "UPDATE custom_rules SET def_json = ?2, pos = ?3,
                             run_state = CASE WHEN ?4 THEN 'stale' ELSE run_state END WHERE id = ?1",
                        )?
                        .execute(params![r.id, def, pos as i64, stale])?;
                    r.slot
                }
                None => {
                    let slot = next_slot;
                    next_slot += 1;
                    self.conn
                        .prepare_cached(
                            "INSERT INTO custom_rules (slot, kind, name, def_json, pos) VALUES (?1, ?2, ?3, ?4, ?5)",
                        )?
                        .execute(params![slot, kind.as_str(), name, def, pos as i64])?;
                    slot
                }
            })
        };
        let mut slots = Slots::default();
        for (i, e) in rules.extractors.iter().enumerate() {
            slots
                .extractors
                .push(slot_for(RuleKind::Extractor, &e.name, def_json_extractor(e), i)?);
        }
        for (i, s) in rules.searches.iter().enumerate() {
            slots
                .searches
                .push(slot_for(RuleKind::Search, &s.name, def_json_search(s), i)?);
        }
        Ok(slots)
    }

    /// Every rule stored with the crawl as a rule set, with its slots, in
    /// display order. This is the crawl's rule set: rules added by a rerun
    /// also run on pages crawled after a resume.
    pub fn stored_rule_set(&self) -> Result<(CustomRules, Slots)> {
        let mut rules = CustomRules::default();
        let mut slots = Slots::default();
        for r in self.custom_rules()? {
            let bad = |e: serde_json::Error| {
                StoreError::Format(format!("stored rule \"{}\" is damaged: {e}", r.name))
            };
            match r.kind {
                RuleKind::Extractor => {
                    rules
                        .extractors
                        .push(serde_json::from_str(&r.def_json).map_err(bad)?);
                    slots.extractors.push(r.slot);
                }
                RuleKind::Search => {
                    rules
                        .searches
                        .push(serde_json::from_str(&r.def_json).map_err(bad)?);
                    slots.searches.push(r.slot);
                }
            }
        }
        Ok((rules, slots))
    }

    /// Rules that never ran start running with this crawl. Rules changed
    /// while paused stay `stale`: earlier pages still have old values.
    pub fn mark_new_custom_rules_running(&self) -> Result<()> {
        self.conn.execute(
            "UPDATE custom_rules SET run_state = 'crawl', last_run_at = ?1 WHERE run_state IS NULL",
            [jiff::Timestamp::now().to_string()],
        )?;
        Ok(())
    }

    /// Marks rules as run now, with how many pages and in what state
    /// (`crawl`, `complete`, `partial`).
    pub fn mark_custom_run(&self, slots: &[u32], state: &str, pages: Option<i64>) -> Result<()> {
        let now = jiff::Timestamp::now().to_string();
        let mut stmt = self.conn.prepare_cached(
            "UPDATE custom_rules SET last_run_at = ?2, run_state = ?3, run_pages = COALESCE(?4, run_pages)
             WHERE slot = ?1",
        )?;
        for s in slots {
            stmt.execute(params![s, now, state, pages])?;
        }
        Ok(())
    }

    pub fn rename_custom_rule(&self, id: i64, name: &str, def_json: &str) -> Result<()> {
        self.conn
            .execute(
                "UPDATE custom_rules SET name = ?2, def_json = ?3 WHERE id = ?1",
                params![id, name, def_json],
            )
            .map_err(|e| match e {
                rusqlite::Error::SqliteFailure(f, _)
                    if f.code == rusqlite::ErrorCode::ConstraintViolation =>
                {
                    StoreError::Format(format!("another rule is already named \"{name}\""))
                }
                other => other.into(),
            })?;
        Ok(())
    }

    /// Removes a rule and its values from every page.
    pub fn delete_custom_rule(&self, id: i64) -> Result<()> {
        let slot: Option<u32> = self
            .conn
            .query_row("SELECT slot FROM custom_rules WHERE id = ?1", [id], |r| r.get(0))
            .optional()?;
        let Some(slot) = slot else { return Ok(()) };
        let ids: Vec<i64> = {
            let mut stmt = self.conn.prepare("SELECT url_id FROM custom_results")?;
            stmt.query_map([], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?
        };
        for url_id in ids {
            self.clear_custom_slot(url_id, slot)?;
        }
        self.conn
            .execute("DELETE FROM custom_rules WHERE id = ?1", [id])?;
        Ok(())
    }

    fn clear_custom_slot(&self, url_id: i64, slot: u32) -> Result<()> {
        if let Some(mut page) = self.page_custom(url_id)? {
            if let Some(s) = page.slots.get_mut(slot as usize) {
                *s = (0, 0, None);
            }
            page.values.remove(&slot);
            self.write_encoded(url_id, &encode_page(&page)?)?;
        }
        Ok(())
    }

    /// Writes one page's results, encoded off the writer thread by
    /// [`encode_custom`].
    pub fn record_custom(&self, url_id: i64, encoded: &EncodedCustom) -> Result<()> {
        self.write_encoded(url_id, encoded)
    }

    /// Replaces the given slots of one page and keeps the others (reruns).
    pub fn update_custom(&self, url_id: i64, slots: &Slots, outcomes: &PageRules) -> Result<()> {
        let mut page = self.page_custom(url_id)?.unwrap_or_default();
        for s in slots.extractors.iter().chain(&slots.searches) {
            page.values.remove(s);
        }
        set_outcomes(&mut page, slots, outcomes);
        self.write_encoded(url_id, &encode_page(&page)?)
    }

    fn write_encoded(&self, url_id: i64, e: &EncodedCustom) -> Result<()> {
        self.conn
            .prepare_cached(
                "INSERT OR REPLACE INTO custom_results (url_id, slots, firsts) VALUES (?1, ?2, ?3)",
            )?
            .execute(params![url_id, e.slots, e.firsts])?;
        match &e.values {
            None => {
                self.conn
                    .prepare_cached("DELETE FROM custom_values WHERE url_id = ?1")?
                    .execute([url_id])?;
            }
            Some(packed) => {
                self.conn
                    .prepare_cached("INSERT OR REPLACE INTO custom_values (url_id, data) VALUES (?1, ?2)")?
                    .execute(params![url_id, packed])?;
            }
        }
        Ok(())
    }

    /// One page's stored rule results. None when the page has none.
    pub fn page_custom(&self, url_id: i64) -> Result<Option<PageCustom>> {
        read_page_custom(&self.conn, url_id)
    }

    /// Pages that have stored HTML, in id order, after `after`.
    pub fn bodies_after(&self, after: i64, limit: usize) -> Result<Vec<StoredBody>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT b.url_id, u.url, p.encoding, COALESCE(p.partial, 0), b.body FROM bodies b
             JOIN urls u ON u.id = b.url_id LEFT JOIN pages p ON p.url_id = b.url_id
             WHERE b.url_id > ?1 AND b.codec = 'zstd' AND u.internal = 1 ORDER BY b.url_id LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![after, limit as i64], |r| {
            Ok(StoredBody {
                url_id: r.get(0)?,
                url: r.get(1)?,
                encoding: r.get(2)?,
                partial: r.get(3)?,
                packed: r.get(4)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// The encoding a page was decoded with and whether it was cut off.
    pub fn page_encoding(&self, url_id: i64) -> Result<(Option<String>, bool)> {
        Ok(self
            .conn
            .prepare_cached("SELECT encoding, partial FROM pages WHERE url_id = ?1")?
            .query_row([url_id], |r| {
                Ok((r.get(0)?, r.get::<_, Option<bool>>(1)?.unwrap_or(false)))
            })
            .optional()?
            .unwrap_or((None, false)))
    }

    /// Internal HTML pages that were extracted but have no stored HTML.
    pub fn extracted_without_body(&self) -> Result<Vec<i64>> {
        let mut stmt = self.conn.prepare(
            "SELECT p.url_id FROM pages p JOIN urls u ON u.id = p.url_id
             WHERE u.internal = 1 AND p.extract_error IS NULL
             AND NOT EXISTS (SELECT 1 FROM bodies b WHERE b.url_id = p.url_id) ORDER BY p.url_id",
        )?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Pages the crawl extracted (the rules' eligible set) and whether any
    /// has stored HTML.
    pub fn extracted_pages(&self) -> Result<(i64, i64)> {
        Ok(self.conn.query_row(
            "SELECT (SELECT COUNT(*) FROM pages p JOIN urls u ON u.id = p.url_id
                     WHERE u.internal = 1 AND p.extract_error IS NULL),
                    (SELECT COUNT(*) FROM bodies b JOIN urls u ON u.id = b.url_id WHERE u.internal = 1)",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?)
    }
}

/// Whether the file has stored rules.
pub fn has_rules(conn: &Connection) -> Result<bool> {
    Ok(
        has_tables(conn)?
            && conn.query_row("SELECT EXISTS (SELECT 1 FROM custom_rules)", [], |r| r.get(0))?,
    )
}

/// Whether the file has the rule tables (format 6 and later, or an older
/// file rules ran on).
pub fn has_tables(conn: &Connection) -> Result<bool> {
    Ok(conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'custom_rules')",
        [],
        |r| r.get(0),
    )?)
}

/// The values row of one page: by slot, extractor values after the first
/// or search matches in context, and an error message.
pub fn unpack_values(data: &[u8]) -> Result<SlotValues> {
    let json = zstd::stream::decode_all(data)
        .map_err(|e| StoreError::Format(format!("stored rule values are damaged: {e}")))?;
    let map: Map<String, Value> = serde_json::from_slice(&json).unwrap_or_default();
    let mut values = SlotValues::new();
    for (k, v) in map {
        if let Some(slot) = k.strip_prefix('e').and_then(|s| s.parse::<u32>().ok()) {
            values.entry(slot).or_default().1 = v.as_str().map(str::to_string);
        } else if let Ok(slot) = k.parse::<u32>() {
            values.entry(slot).or_default().0 = v
                .as_array()
                .map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect())
                .unwrap_or_default();
        }
    }
    Ok(values)
}

/// Count and flags of one slot in a packed `custom_results.slots` value.
pub fn unpack_slot(packed: &[u8], slot: usize) -> Option<(u32, u16)> {
    let chunk = packed.get(slot * SLOT_BYTES..(slot + 1) * SLOT_BYTES)?;
    Some((
        u32::from_le_bytes(chunk[0..4].try_into().expect("4 bytes")),
        u16::from_le_bytes(chunk[4..6].try_into().expect("2 bytes")),
    ))
}

/// The rules stored with a crawl, in display order, through any
/// connection (the results reader has its own).
pub fn read_rules(conn: &Connection) -> Result<Vec<StoredRule>> {
    if !has_tables(conn)? {
        return Ok(Vec::new());
    }
    let mut stmt = conn.prepare(
        "SELECT id, slot, kind, name, def_json, pos, last_run_at, run_pages, run_state
             FROM custom_rules ORDER BY kind = 'search', pos, id",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(StoredRule {
            id: r.get(0)?,
            slot: r.get(1)?,
            kind: if r.get::<_, String>(2)? == "search" {
                RuleKind::Search
            } else {
                RuleKind::Extractor
            },
            name: r.get(3)?,
            def_json: r.get(4)?,
            pos: r.get(5)?,
            last_run_at: r.get(6)?,
            run_pages: r.get(7)?,
            run_state: r.get(8)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// One page's stored rule results through any connection. None when the
/// page has none.
pub fn read_page_custom(conn: &Connection, url_id: i64) -> Result<Option<PageCustom>> {
    if !has_tables(conn)? {
        return Ok(None);
    }
    let row: Option<(Vec<u8>, String)> = conn
        .prepare_cached("SELECT slots, firsts FROM custom_results WHERE url_id = ?1")?
        .query_row([url_id], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()?;
    let Some((packed, firsts)) = row else {
        return Ok(None);
    };
    let firsts: Vec<Value> = serde_json::from_str(&firsts).unwrap_or_default();
    let mut page = PageCustom::default();
    for (i, chunk) in packed.chunks_exact(SLOT_BYTES).enumerate() {
        let count = u32::from_le_bytes(chunk[0..4].try_into().expect("4 bytes"));
        let f = u16::from_le_bytes(chunk[4..6].try_into().expect("2 bytes"));
        let first = firsts.get(i).and_then(Value::as_str).map(str::to_string);
        page.slots.push((count, f, first));
    }
    let data: Option<Vec<u8>> = conn
        .prepare_cached("SELECT data FROM custom_values WHERE url_id = ?1")?
        .query_row([url_id], |r| r.get(0))
        .optional()?;
    if let Some(data) = data {
        page.values = unpack_values(&data)?;
    }
    Ok(Some(page))
}

/// One page's results as stored: the packed slots, the first values and
/// the compressed other values. Built on the extraction thread, so the
/// single writer only inserts (plan D7, T032).
#[derive(Debug, Clone)]
pub struct EncodedCustom {
    slots: Vec<u8>,
    firsts: String,
    values: Option<Vec<u8>>,
}

/// Encodes a crawled page's rule outcomes for [`CrawlStore::record_custom`].
pub fn encode_custom(slots: &Slots, outcomes: &PageRules) -> Result<EncodedCustom> {
    let mut page = PageCustom::default();
    set_outcomes(&mut page, slots, outcomes);
    encode_page(&page)
}

fn encode_page(page: &PageCustom) -> Result<EncodedCustom> {
    let mut packed = Vec::with_capacity(page.slots.len() * SLOT_BYTES);
    let mut firsts = Vec::with_capacity(page.slots.len());
    for (count, f, first) in &page.slots {
        packed.extend_from_slice(&count.to_le_bytes());
        packed.extend_from_slice(&f.to_le_bytes());
        packed.extend_from_slice(&[0, 0]);
        firsts.push(first.as_ref().map_or(Value::Null, |s| Value::String(s.clone())));
    }
    let mut map = Map::new();
    for (slot, (values, error)) in &page.values {
        if !values.is_empty() {
            map.insert(slot.to_string(), Value::from(values.clone()));
        }
        if let Some(e) = error {
            map.insert(format!("e{slot}"), Value::String(e.clone()));
        }
    }
    let values = if map.is_empty() {
        None
    } else {
        let json = Value::Object(map).to_string();
        Some(body::compress(json.as_bytes()).map_err(|e| StoreError::Format(e.to_string()))?)
    };
    Ok(EncodedCustom {
        slots: packed,
        firsts: Value::Array(firsts).to_string(),
        values,
    })
}

fn set_outcomes(page: &mut PageCustom, slots: &Slots, outcomes: &PageRules) {
    let pairs = slots
        .extractors
        .iter()
        .zip(&outcomes.extractors)
        .map(|(s, o)| (*s, o, true))
        .chain(
            slots
                .searches
                .iter()
                .zip(&outcomes.searches)
                .map(|(s, o)| (*s, o, false)),
        );
    for (slot, o, extractor) in pairs {
        let i = slot as usize;
        if page.slots.len() <= i {
            page.slots.resize(i + 1, (0, 0, None));
        }
        let first = if extractor {
            o.values.first().cloned()
        } else {
            None
        };
        page.slots[i] = (o.count, o.flags, first);
        let rest = rest_values(o, extractor);
        if !rest.is_empty() || o.error.is_some() {
            page.values.insert(slot, (rest, o.error.clone()));
        }
    }
}

/// What goes in `custom_values`: extractor values after the first (the
/// first is in `firsts`), every search context.
fn rest_values(o: &RuleOutcome, extractor: bool) -> Vec<String> {
    if extractor {
        o.values.iter().skip(1).cloned().collect()
    } else {
        o.values.clone()
    }
}

/// The JSON a rule is stored with; a change marks the rule stale.
pub fn def_json_extractor(e: &Extractor) -> String {
    serde_json::to_string(e).expect("rules serialize")
}

pub fn def_json_search(s: &Search) -> String {
    serde_json::to_string(s).expect("rules serialize")
}

/// Whether an outcome counts as having run (not skipped in a partial run).
pub fn was_run(f: u16) -> bool {
    f & flags::RUN != 0
}
