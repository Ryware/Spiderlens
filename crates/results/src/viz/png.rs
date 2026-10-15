//! A scene's SVG rendered to PNG with `resvg` (spec 022 FR-024, plan D9),
//! with the system's fonts.

use std::sync::{Arc, OnceLock};

/// Largest side of an exported PNG, in pixels (spec edge case).
pub const MAX_SIDE: u32 = 16_000;

#[derive(Debug, thiserror::Error)]
pub enum PngError {
    #[error(
        "the picture would be {0} x {1} pixels, over the {MAX_SIDE} pixel limit: export it as SVG, or export a focused view"
    )]
    TooLarge(u32, u32),
    #[error("could not draw the picture: {0}")]
    Render(String),
}

fn fonts() -> Arc<resvg::usvg::fontdb::Database> {
    static DB: OnceLock<Arc<resvg::usvg::fontdb::Database>> = OnceLock::new();
    DB.get_or_init(|| {
        let mut db = resvg::usvg::fontdb::Database::new();
        db.load_system_fonts();
        Arc::new(db)
    })
    .clone()
}

/// The size `svg` comes out at `scale`.
pub fn size(svg: &str, scale: f32) -> Result<(u32, u32), PngError> {
    let tree = parse(svg)?;
    let s = tree.size();
    Ok((
        (s.width() * scale).ceil() as u32,
        (s.height() * scale).ceil() as u32,
    ))
}

fn parse(svg: &str) -> Result<resvg::usvg::Tree, PngError> {
    let opt = resvg::usvg::Options {
        fontdb: fonts(),
        ..Default::default()
    };
    resvg::usvg::Tree::from_str(svg, &opt).map_err(|e| PngError::Render(e.to_string()))
}

/// Renders `svg` at `scale` (1, 2 or 4) to PNG bytes.
pub fn png(svg: &str, scale: f32) -> Result<Vec<u8>, PngError> {
    let tree = parse(svg)?;
    let s = tree.size();
    let (w, h) = (
        (s.width() * scale).ceil() as u32,
        (s.height() * scale).ceil() as u32,
    );
    if w > MAX_SIDE || h > MAX_SIDE {
        return Err(PngError::TooLarge(w, h));
    }
    let mut pixmap = resvg::tiny_skia::Pixmap::new(w.max(1), h.max(1))
        .ok_or_else(|| PngError::Render("no memory for the picture".into()))?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    pixmap.encode_png().map_err(|e| PngError::Render(e.to_string()))
}
