//! Colours for exported pictures (spec 022 plan D8). tokens.css stays the
//! only place colours are written: the CLI reads the light and dark values
//! from it here, and the desktop sends the values its window resolved, so
//! an export follows the user's palette (spec 018). Both are turned into
//! sRGB hex, which every SVG viewer and the PNG renderer understand.

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

const TOKENS: &str = include_str!("../../../../ui/src/styles/tokens.css");

/// The tokens a picture is drawn with.
pub const NAMES: [&str; 17] = [
    "--viz-ok",
    "--viz-redirect",
    "--viz-client-error",
    "--viz-server-error",
    "--viz-blocked",
    "--viz-not-crawled",
    "--viz-folder",
    "--viz-group",
    "--viz-line",
    "--accent",
    "--accent-text",
    "--fg",
    "--fg-2",
    "--fg-3",
    "--bg",
    "--surface",
    "--line",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Theme {
    #[default]
    Light,
    Dark,
}

/// A colour in OKLab with alpha, so mixes match `color-mix(in oklab)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Colour {
    l: f64,
    a: f64,
    b: f64,
    alpha: f64,
}

impl Colour {
    /// `self` with `t` of `other` mixed in (0: self, 1: other).
    pub fn mix(self, other: Colour, t: f64) -> Colour {
        let m = |x: f64, y: f64| x + (y - x) * t;
        Colour {
            l: m(self.l, other.l),
            a: m(self.a, other.a),
            b: m(self.b, other.b),
            alpha: m(self.alpha, other.alpha),
        }
    }

    /// `#rrggbb`, or `#rrggbbaa` when not opaque.
    pub fn hex(self) -> String {
        let (r, g, b) = oklab_to_srgb(self.l, self.a, self.b);
        let byte = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        if self.alpha >= 0.999 {
            format!("#{:02x}{:02x}{:02x}", byte(r), byte(g), byte(b))
        } else {
            format!(
                "#{:02x}{:02x}{:02x}{:02x}",
                byte(r),
                byte(g),
                byte(b),
                byte(self.alpha)
            )
        }
    }
}

fn srgb_to_linear(c: f64) -> f64 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(c: f64) -> f64 {
    if c <= 0.003_130_8 {
        12.92 * c
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

/// OKLab to gamma-encoded sRGB (Björn Ottosson's matrices).
fn oklab_to_srgb(l: f64, a: f64, b: f64) -> (f64, f64, f64) {
    let l_ = l + 0.396_337_777_4 * a + 0.215_803_757_3 * b;
    let m_ = l - 0.105_561_345_8 * a - 0.063_854_172_8 * b;
    let s_ = l - 0.089_484_177_5 * a - 1.291_485_548 * b;
    let (l3, m3, s3) = (l_ * l_ * l_, m_ * m_ * m_, s_ * s_ * s_);
    let r = 4.076_741_662_1 * l3 - 3.307_711_591_3 * m3 + 0.230_969_929_2 * s3;
    let g = -1.268_438_004_6 * l3 + 2.609_757_401_1 * m3 - 0.341_319_396_5 * s3;
    let bl = -0.004_196_086_3 * l3 - 0.703_418_614_7 * m3 + 1.707_614_701 * s3;
    (linear_to_srgb(r), linear_to_srgb(g), linear_to_srgb(bl))
}

fn srgb_to_oklab(r: f64, g: f64, b: f64) -> (f64, f64, f64) {
    let (r, g, b) = (srgb_to_linear(r), srgb_to_linear(g), srgb_to_linear(b));
    let l = (0.412_221_470_8 * r + 0.536_332_536_3 * g + 0.051_445_992_9 * b).cbrt();
    let m = (0.211_903_498_2 * r + 0.680_699_545_1 * g + 0.107_396_956_6 * b).cbrt();
    let s = (0.088_302_461_9 * r + 0.281_718_837_6 * g + 0.629_978_700_5 * b).cbrt();
    (
        0.210_454_255_3 * l + 0.793_617_785 * m - 0.004_072_046_8 * s,
        1.977_998_495_1 * l - 2.428_592_205 * m + 0.450_593_709_9 * s,
        0.025_904_037_1 * l + 0.782_771_766_2 * m - 0.808_675_766 * s,
    )
}

fn number(s: &str) -> Option<f64> {
    let s = s.trim();
    match s.strip_suffix('%') {
        Some(p) => p.trim().parse::<f64>().ok().map(|v| v / 100.0),
        None => s.parse().ok(),
    }
}

/// Parses the colour forms tokens.css and a browser's computed style use:
/// `oklch(L C H [/ a])`, `#rgb[a]`, `#rrggbb[aa]`, `rgb(r g b [/ a])` or
/// `rgb(r, g, b[, a])`.
pub fn parse(css: &str) -> Option<Colour> {
    let css = css.trim();
    if let Some(hex) = css.strip_prefix('#') {
        let v: Vec<u8> = match hex.len() {
            3 | 4 => hex
                .chars()
                .map(|c| c.to_digit(16).map(|d| (d * 17) as u8))
                .collect::<Option<_>>()?,
            6 | 8 => (0..hex.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).ok())
                .collect::<Option<_>>()?,
            _ => return None,
        };
        let (l, a, b) = srgb_to_oklab(v[0] as f64 / 255.0, v[1] as f64 / 255.0, v[2] as f64 / 255.0);
        return Some(Colour {
            l,
            a,
            b,
            alpha: v.get(3).map_or(1.0, |x| *x as f64 / 255.0),
        });
    }
    let (name, rest) = css.split_once('(')?;
    let body = rest.strip_suffix(')')?;
    let (main, alpha) = match body.split_once('/') {
        Some((m, a)) => (m, number(a)?),
        None => (body, 1.0),
    };
    let parts: Vec<&str> = main.split([' ', ',']).filter(|p| !p.is_empty()).collect();
    match name.trim() {
        "oklch" if parts.len() == 3 => {
            let l = number(parts[0])?;
            let c = number(parts[1])?;
            let h = number(parts[2].trim_end_matches("deg"))?.to_radians();
            Some(Colour {
                l,
                a: c * h.cos(),
                b: c * h.sin(),
                alpha,
            })
        }
        "rgb" | "rgba" if parts.len() >= 3 => {
            let ch = |p: &str| {
                if p.ends_with('%') {
                    number(p)
                } else {
                    number(p).map(|v| v / 255.0)
                }
            };
            let (l, a, b) = srgb_to_oklab(ch(parts[0])?, ch(parts[1])?, ch(parts[2])?);
            let alpha = if parts.len() == 4 {
                number(parts[3])?
            } else {
                alpha
            };
            Some(Colour { l, a, b, alpha })
        }
        _ => None,
    }
}

/// The custom properties set in the first rule whose selector starts with
/// `selector`.
fn block(css: &str, selector: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let Some(start) = css.find(selector) else {
        return out;
    };
    let Some(open) = css[start..].find('{') else {
        return out;
    };
    let body = &css[start + open + 1..];
    let body = &body[..body.find('}').unwrap_or(body.len())];
    for decl in body.split(';') {
        let decl = decl.trim();
        // Comments sit before declarations in this file.
        let decl = decl.rsplit("*/").next().unwrap_or(decl).trim();
        if let Some((k, v)) = decl.split_once(':')
            && k.trim().starts_with("--")
        {
            out.insert(k.trim().to_string(), v.trim().to_string());
        }
    }
    out
}

/// Resolves `var(--x)` chains.
fn resolve(vars: &HashMap<String, String>, name: &str, depth: u8) -> Option<String> {
    let v = vars.get(name)?;
    match v.strip_prefix("var(").and_then(|r| r.strip_suffix(')')) {
        Some(inner) if depth < 8 => resolve(vars, inner.split(',').next()?.trim(), depth + 1),
        Some(_) => None,
        None => Some(v.clone()),
    }
}

/// The resolved colours a picture is drawn with, by token name.
#[derive(Debug, Clone)]
pub struct Palette {
    colours: BTreeMap<String, Colour>,
}

impl Palette {
    /// The default palette for a theme, from tokens.css.
    pub fn theme(theme: Theme) -> Palette {
        let mut vars = block(TOKENS, ":root {");
        if theme == Theme::Dark {
            vars.extend(block(TOKENS, ":root[data-tone=\"dark\"]"));
        }
        let colours = NAMES
            .iter()
            .filter_map(|n| Some((n.to_string(), parse(&resolve(&vars, n, 0)?)?)))
            .collect();
        Palette { colours }
    }

    /// The theme's palette with the values a window resolved laid over it
    /// (the user's palette and brightness); values that do not parse are
    /// left at the theme's.
    pub fn with(theme: Theme, resolved: &BTreeMap<String, String>) -> Palette {
        let mut p = Palette::theme(theme);
        for (k, v) in resolved {
            if let Some(c) = parse(v) {
                p.colours.insert(k.clone(), c);
            }
        }
        p
    }

    pub fn get(&self, name: &str) -> Colour {
        self.colours.get(name).copied().unwrap_or(Colour {
            l: 0.5,
            a: 0.0,
            b: 0.0,
            alpha: 1.0,
        })
    }

    pub fn hex(&self, name: &str) -> String {
        self.get(name).hex()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colour_forms_round_trip_to_hex() {
        assert_eq!(parse("#ff8000").unwrap().hex(), "#ff8000");
        assert_eq!(parse("#fff").unwrap().hex(), "#ffffff");
        assert_eq!(parse("rgb(12, 34, 56)").unwrap().hex(), "#0c2238");
        assert_eq!(parse("rgb(12 34 56 / 0.5)").unwrap().hex(), "#0c223880");
        assert_eq!(parse("oklch(1 0 0)").unwrap().hex(), "#ffffff");
        assert_eq!(parse("oklch(0 0 0)").unwrap().hex(), "#000000");
        // A mid teal stays a teal.
        let teal = parse("oklch(0.50 0.095 188)").unwrap().hex();
        let (r, g, b) = (&teal[1..3], &teal[3..5], &teal[5..7]);
        let v = |s: &str| u8::from_str_radix(s, 16).unwrap();
        assert!(v(g) > v(r) && v(b) > v(r), "{teal}");
        assert!(parse("color(display-p3 1 0 0)").is_none());
    }

    #[test]
    fn themes_come_from_tokens_css() {
        let light = Palette::theme(Theme::Light);
        let dark = Palette::theme(Theme::Dark);
        for n in NAMES {
            assert!(light.colours.contains_key(n), "light {n}");
            assert!(dark.colours.contains_key(n), "dark {n}");
        }
        // Light background is near white, dark near black; --viz-ok follows --ok.
        assert!(light.get("--bg").l > 0.9 && dark.get("--bg").l < 0.3);
        assert_eq!(
            light.hex("--viz-ok"),
            parse("oklch(0.50 0.12 155)").unwrap().hex()
        );
        assert_ne!(light.hex("--viz-ok"), dark.hex("--viz-ok"));
    }

    #[test]
    fn resolved_values_override_the_theme() {
        let mut m = BTreeMap::new();
        m.insert("--accent".to_string(), "oklch(0.5 0.16 266)".to_string());
        m.insert("--fg".to_string(), "not a colour".to_string());
        let p = Palette::with(Theme::Light, &m);
        assert_eq!(p.hex("--accent"), parse("oklch(0.5 0.16 266)").unwrap().hex());
        assert_eq!(p.hex("--fg"), Palette::theme(Theme::Light).hex("--fg"));
    }
}
