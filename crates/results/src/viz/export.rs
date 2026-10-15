//! Exporting a view (spec 022 US5, FR-024): the scene the user sees, as
//! SVG, PNG or CSV, written next to its final name and moved into place,
//! so a failed export never leaves half a file.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::palette::{Palette, Theme};
use super::png::PngError;
use super::svg::Header;
use super::{ColourBy, Scene, csv, png, svg};
use crate::reader::Reader;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ExportFormat {
    #[default]
    Svg,
    Png,
    /// The nodes, and a second file of the links when there are any.
    Csv,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct ExportOptions {
    pub format: ExportFormat,
    /// PNG: 1, 2 or 4.
    pub scale: u8,
    pub theme: Theme,
    /// Token values the window resolved (`--viz-ok` and so on), laid over
    /// the theme's; empty for the default palette (the CLI).
    pub palette: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ExportDone {
    /// The files written, the one asked for first.
    pub files: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum VizExportError {
    #[error(transparent)]
    Png(#[from] PngError),
    #[error("could not write the file: {0}")]
    Io(#[from] std::io::Error),
}

/// Writes `bytes` to `path` through a temporary file beside it.
fn write_atomic(
    path: &Path,
    write: impl FnOnce(&mut fs::File) -> std::io::Result<()>,
) -> std::io::Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".part");
    let tmp = PathBuf::from(tmp);
    let res = (|| {
        let mut f = fs::File::create(&tmp)?;
        write(&mut f)?;
        f.flush()?;
        f.sync_all()?;
        fs::rename(&tmp, path)
    })();
    if res.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    res
}

/// `out.csv` → `out-links.csv`.
pub fn links_path(path: &Path) -> PathBuf {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ext = path
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    path.with_file_name(format!("{stem}-links{ext}"))
}

/// Writes `scene` to `path` as `opts` asks.
pub fn export(
    scene: &Scene,
    colour: ColourBy,
    header: &Header,
    opts: &ExportOptions,
    path: &Path,
) -> Result<ExportDone, VizExportError> {
    let palette = Palette::with(opts.theme, &opts.palette);
    let mut files = vec![path.display().to_string()];
    match opts.format {
        ExportFormat::Svg => {
            let s = svg::svg(scene, &palette, colour, header);
            write_atomic(path, |f| f.write_all(s.as_bytes()))?;
        }
        ExportFormat::Png => {
            let s = svg::svg(scene, &palette, colour, header);
            let scale = match opts.scale {
                2 => 2.0,
                4 => 4.0,
                _ => 1.0,
            };
            let bytes = png::png(&s, scale)?;
            write_atomic(path, |f| f.write_all(&bytes))?;
        }
        ExportFormat::Csv => {
            write_atomic(path, |f| csv::nodes(scene, f))?;
            if csv::has_links(scene) {
                let lp = links_path(path);
                write_atomic(&lp, |f| csv::links(scene, f))?;
                files.push(lp.display().to_string());
            }
        }
    }
    Ok(ExportDone { files })
}

impl Reader {
    /// The node key (`u:<id>`) of a URL in the crawl, as typed or with a
    /// trailing slash, for `--focus` and `--url`.
    pub fn viz_url_key(&mut self, url: &str) -> crate::Result<Option<String>> {
        let mut stmt = self.conn.prepare_cached("SELECT id FROM urls WHERE url = ?1")?;
        for u in [url.to_string(), format!("{url}/")] {
            match stmt.query_row([&u], |r| r.get::<_, i64>(0)) {
                Ok(id) => return Ok(Some(format!("u:{id}"))),
                Err(rusqlite::Error::QueryReturnedNoRows) => {}
                Err(e) => return Err(e.into()),
            }
        }
        Ok(None)
    }

    /// The crawl's host and start date, for an export's header.
    pub fn viz_header(&mut self) -> crate::Result<Header> {
        let meta = |k: &str| -> crate::Result<Option<String>> {
            match self
                .conn
                .query_row("SELECT value FROM meta WHERE key = ?1", [k], |r| r.get(0))
            {
                Ok(v) => Ok(Some(v)),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(e.into()),
            }
        };
        let start = meta("start_url")?;
        let config: Option<serde_json::Value> = meta("config")?.and_then(|c| serde_json::from_str(&c).ok());
        let host = start
            .as_deref()
            .and_then(|u| url::Url::parse(u).ok())
            .and_then(|u| u.host_str().map(str::to_string))
            .or_else(|| {
                config
                    .as_ref()?
                    .get("resolved_hosts")?
                    .get(0)?
                    .as_str()
                    .map(str::to_string)
            })
            .unwrap_or_default();
        let date = config
            .as_ref()
            .and_then(|c| {
                c.get("started_at")?
                    .as_str()
                    .map(|s| s.chars().take(10).collect())
            })
            .unwrap_or_default();
        Ok(Header { host, date })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_file_sits_beside_the_nodes() {
        assert_eq!(
            links_path(Path::new("/x/tree.csv")),
            PathBuf::from("/x/tree-links.csv")
        );
        assert_eq!(links_path(Path::new("/x/tree")), PathBuf::from("/x/tree-links"));
    }
}
