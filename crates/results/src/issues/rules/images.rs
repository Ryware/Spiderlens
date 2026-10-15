//! Images. The 007 Images filters use these same conditions (FR-005).

use crate::issues::{Category::Images as C, Priority::*, Rule, Scope, Unit, rule};
use crate::tabs::Need;

pub const MISSING_ALT: &str =
    "EXISTS (SELECT 1 FROM links l WHERE l.to_id = u.id AND l.kind = 'image' AND l.alt IS NULL)";
pub const ALT_OVER: &str = "EXISTS (SELECT 1 FROM links l WHERE l.to_id = u.id AND l.kind = 'image' AND length(l.alt) > thr('image.alt_over'))";
pub const LARGE: &str = "u.size_bytes > thr('image.large')";

pub const RULES: &[Rule] = &[
    rule("image.missing_alt", "Image: missing alt attribute", C, Low, Scope::Images, MISSING_ALT)
        .needs(Need::Extraction)
        .evidence(
            "'No alt on ' || (SELECT COUNT(DISTINCT l.from_id) FROM links l WHERE l.to_id = u.id AND l.kind = 'image' AND l.alt IS NULL) || ' pages'",
        )
        .standards(&["WCAG 2.2 1.1.1"])
        .explain(
            "Screen readers cannot describe an image without alt text, and search engines lose a clue about what the image shows.",
            "Add alt text that says what the image shows or does. For purely decorative images use an empty alt=\"\".",
        ),
    rule("image.alt_over", "Image: alt text over {n} characters", C, Low, Scope::Images, ALT_OVER)
        .threshold(Unit::Chars, 100.0, 10.0, 1000.0)
        .needs(Need::Extraction)
        .evidence(
            "(SELECT MAX(length(l.alt)) FROM links l WHERE l.to_id = u.id AND l.kind = 'image') || ' characters'",
        )
        .explain(
            "Very long alt text is tiring to listen to and is often stuffed with keywords.",
            "Describe the image in a short sentence; put longer descriptions in the page text or a caption.",
        ),
    rule(
        "image.broken",
        "Broken image",
        C,
        High,
        Scope::Images,
        "u.fetched_at IS NOT NULL AND (u.error_class IS NOT NULL OR u.status >= 400)",
    )
    .evidence(concat!(
        "CASE WHEN u.error_class IS NOT NULL THEN error_label(u.error_class) ELSE u.status || COALESCE(' ' || status_text(u.status), '') END",
        " || ', on ' || ",
        "(SELECT COUNT(DISTINCT l.from_id) FROM links l WHERE l.to_id = u.id AND l.kind = 'image')",
        " || ' pages'"
    ))
    .explain(
        "Pages show a missing-image icon where this image should be.",
        "Restore the image file, or update the pages that use it (see Inlinks).",
    ),
    rule("image.large", "Image: over {n} bytes", C, Medium, Scope::Images, LARGE)
        .threshold(Unit::Bytes, 102_400.0, 1_024.0, 52_428_800.0)
        .needs(Need::Timing)
        .evidence("u.size_bytes || ' bytes'")
        .explain(
            "Large images slow pages down, especially on mobile connections, which hurts visitors and page experience signals.",
            "Compress the image, serve a modern format (WebP or AVIF) and size it for how large it is displayed.",
        ),
    rule(
        "image.missing_size",
        "Image: missing width or height",
        C,
        Low,
        Scope::Images,
        "EXISTS (SELECT 1 FROM links l WHERE l.to_id = u.id AND l.kind = 'image' AND (l.width IS NULL OR l.height IS NULL))",
    )
    .needs(Need::Extraction)
    .evidence(concat!("'Used on ' || ", "(SELECT COUNT(DISTINCT l.from_id) FROM links l WHERE l.to_id = u.id AND l.kind = 'image' AND (l.width IS NULL OR l.height IS NULL))", " || ' pages without width and height'"))
    .explain(
        "Without width and height the browser cannot reserve space, so the page jumps as images load (layout shift).",
        "Add width and height attributes matching the image's aspect ratio; CSS can still make it responsive.",
    ),
];
