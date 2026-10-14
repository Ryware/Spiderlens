//! Search result snippet measuring (spec 027 part A, plan D1).
//!
//! Widths come from a generated table of advance widths (`widths.rs`, made
//! by `tools/serp-widths` from fonts metric-compatible with Arial), so the
//! same title measures the same on every operating system, in the app, the
//! CLI and the issue rules. No kerning: the table is per character.
//!
//! The same table is exported to `ui/src/generated/serpWidths.json`, so the
//! preview editor measures as the user types; a test on each side checks
//! they agree on the fixture strings.

#[rustfmt::skip]
mod widths;

use serde::Serialize;
use ts_rs::TS;

/// Which line of a search result is measured.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum SerpStyle {
    Title,
    Description,
}

impl SerpStyle {
    /// Font size search results draw this line at, in CSS pixels.
    pub const fn font_px(self) -> u32 {
        match self {
            Self::Title => 20,
            Self::Description => 14,
        }
    }

    /// Default width limits in pixels: (desktop, mobile) (FR-A03).
    pub const fn limits(self) -> (u32, u32) {
        match self {
            Self::Title => (TITLE_DESKTOP_PX, TITLE_MOBILE_PX),
            Self::Description => (DESC_DESKTOP_PX, DESC_MOBILE_PX),
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "title" => Some(Self::Title),
            "desc" | "description" => Some(Self::Description),
            _ => None,
        }
    }
}

pub const TITLE_DESKTOP_PX: u32 = 600;
pub const TITLE_MOBILE_PX: u32 = 580;
pub const TITLE_SHORT_PX: u32 = 200;
pub const DESC_DESKTOP_PX: u32 = 990;
pub const DESC_MOBILE_PX: u32 = 680;

/// Only this many characters are measured; anything longer is far over
/// every limit already (spec edge case).
pub const MAX_MEASURED_CHARS: usize = 1000;

const ELLIPSIS: char = '…';

/// Width of one character in thousandths of an em.
pub fn em_width(ch: char) -> u32 {
    let cp = ch as u32;
    if let Ok(i) = widths::WIDTHS.binary_search_by_key(&cp, |&(c, _)| c) {
        return u32::from(widths::WIDTHS[i].1);
    }
    fallback(cp)
}

/// Widths for characters the fonts do not cover, by script.
fn fallback(cp: u32) -> u32 {
    match cp {
        // Zero-width and joining controls, variation selectors, combining marks.
        0x200B..=0x200F | 0x2060..=0x2064 | 0xFE00..=0xFE0F | 0xE0100..=0xE01EF => 0,
        0x0300..=0x036F | 0x0591..=0x05C7 | 0x064B..=0x065F | 0x1AB0..=0x1AFF | 0x20D0..=0x20FF => 0,
        // Han, kana, Hangul and full-width forms: one em.
        0x1100..=0x11FF
        | 0x2E80..=0x9FFF
        | 0xA960..=0xA97F
        | 0xAC00..=0xD7FF
        | 0xF900..=0xFAFF
        | 0xFE30..=0xFE4F
        | 0xFF00..=0xFF60
        | 0xFFE0..=0xFFE6
        | 0x20000..=0x3FFFF => 1000,
        // Emoji and pictographs.
        0x2600..=0x27BF | 0x1F000..=0x1FAFF => 1250,
        // Anything else: the width of an average lowercase letter.
        _ => 556,
    }
}

/// Collapses runs of whitespace and trims, as search results display text.
fn displayed(text: &str) -> impl Iterator<Item = char> + '_ {
    text.split_whitespace()
        .enumerate()
        .flat_map(|(i, word)| (i > 0).then_some(' ').into_iter().chain(word.chars()))
}

const TATWEEL: char = '\u{0640}';
const LAM: char = '\u{0644}';

fn arabic(ch: char) -> Option<&'static (u32, u16, u16, u16, u16, bool)> {
    let cp = ch as u32;
    widths::ARABIC
        .binary_search_by_key(&cp, |f| f.0)
        .ok()
        .map(|i| &widths::ARABIC[i])
}

/// Whether the next letter can connect to this one.
fn joins_next(ch: char) -> bool {
    ch == TATWEEL || arabic(ch).is_some_and(|f| f.5)
}

fn is_joining(ch: char) -> bool {
    ch == TATWEEL || arabic(ch).is_some()
}

/// Width of each character in thousandths of an em, in context: Arabic
/// letters take the width of the form they are drawn in (isolated, initial,
/// medial or final) and lam followed by alef is one ligature, as a browser
/// shapes them.
fn char_widths(chars: &[char]) -> Vec<u32> {
    let mut out = Vec::with_capacity(chars.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let prev_joins = i > 0 && joins_next(chars[i - 1]);
        if c == LAM
            && let Some(&next) = chars.get(i + 1)
            && let Some(&(_, iso, fin)) = widths::LAM_ALEF.iter().find(|l| l.0 == next as u32)
        {
            out.push(u32::from(if prev_joins { fin } else { iso }));
            out.push(0);
            i += 2;
            continue;
        }
        let w = match arabic(c) {
            Some(&(_, iso, fin, init, med, dual)) => {
                let next_joins = dual && chars.get(i + 1).is_some_and(|&n| is_joining(n));
                match (prev_joins, next_joins) {
                    (false, false) => iso,
                    (true, false) => fin,
                    (false, true) => init,
                    (true, true) => med,
                }
                .into()
            }
            None => em_width(c),
        };
        out.push(w);
        i += 1;
    }
    out
}

fn measured_chars(text: &str) -> Vec<char> {
    displayed(text).take(MAX_MEASURED_CHARS).collect()
}

fn units_to_px(units: u64, style: SerpStyle) -> f64 {
    units as f64 * f64::from(style.font_px()) / 1000.0
}

/// The width in pixels of `text` drawn as a search result line, rounded to
/// the nearest pixel.
pub fn px_width(text: &str, style: SerpStyle) -> u32 {
    let units: u64 = char_widths(&measured_chars(text))
        .iter()
        .map(|&w| u64::from(w))
        .sum();
    units_to_px(units, style).round() as u32
}

/// The text as a search result would show it within `limit_px`: unchanged
/// when it fits, otherwise cut at the last word boundary that leaves room
/// for an ellipsis, then the ellipsis. Right-to-left text is cut at its
/// logical end, which is where it ends on screen too.
pub fn truncate(text: &str, style: SerpStyle, limit_px: u32) -> Truncated {
    let all: Vec<char> = displayed(text).collect();
    let chars = &all[..all.len().min(MAX_MEASURED_CHARS)];
    let widths = char_widths(chars);
    let limit_units = u64::from(limit_px) * 1000 / u64::from(style.font_px());
    let total: u64 = widths.iter().map(|&w| u64::from(w)).sum();
    if total <= limit_units && all.len() <= MAX_MEASURED_CHARS {
        return Truncated {
            text: all.into_iter().collect(),
            cut: false,
        };
    }
    let room = limit_units.saturating_sub(u64::from(em_width(ELLIPSIS)));
    let mut used = 0u64;
    let mut end = 0;
    for (i, &w) in widths.iter().enumerate() {
        let w = u64::from(w);
        if used + w > room {
            break;
        }
        used += w;
        end = i + 1;
    }
    // Back off to a word boundary when the cut falls inside a word.
    let mid_word =
        end > 0 && end < chars.len() && !chars[end].is_whitespace() && !chars[end - 1].is_whitespace();
    if mid_word && let Some(space) = chars[..end].iter().rposition(|c| c.is_whitespace()) {
        end = space;
    }
    let mut out: String = chars[..end].iter().collect();
    let trimmed = out.trim_end().len();
    out.truncate(trimmed);
    out.push(ELLIPSIS);
    Truncated { text: out, cut: true }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Truncated {
    pub text: String,
    pub cut: bool,
}

/// Everything the preview and the CLI show for one line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct SnippetMeasure {
    pub chars: u32,
    pub px: u32,
    pub cut_desktop: bool,
    pub cut_mobile: bool,
    pub display_desktop: String,
    pub display_mobile: String,
}

pub fn measure(text: &str, style: SerpStyle) -> SnippetMeasure {
    let (desktop, mobile) = style.limits();
    let d = truncate(text, style, desktop);
    let m = truncate(text, style, mobile);
    SnippetMeasure {
        chars: displayed(text).count() as u32,
        px: px_width(text, style),
        cut_desktop: d.cut,
        cut_mobile: m.cut,
        display_desktop: d.text,
        display_mobile: m.text,
    }
}

#[cfg(test)]
mod tests;
