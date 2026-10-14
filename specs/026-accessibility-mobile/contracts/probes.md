# Contract: page probes used by this feature

The probe hook itself (`RenderJob.probes`, `RenderOutcome.probes`, `ProbeOutcome`, `RenderJob.load_types`, the shared per-page cap) is owned and published by spec 014 in `specs/014-javascript-rendering/contracts/render-crate.md`. This file only defines what this feature passes and expects.

## `a11y`

- **Script**: `axe.min.js` (vendored, unmodified) followed by `crates/engine/assets/a11y_runner.js`, which calls `axe.run(document, options)` with the options of research R3 and returns the reduced JSON below. Runs in an isolated world.
- **Args**: `{ "node_cap": 20, "page_cap": 500, "excerpt": 300 }`.
- **Budget**: the page budget (`budget_secs`) minus the time `mobile_layout` used.
- **Result**:

```json
{
  "engine": "4.14.0",
  "ms": 412,
  "results": [
    {
      "id": "color-contrast", "kind": "violation", "impact": "serious",
      "tags": ["wcag2aa", "wcag143"], "total": 3,
      "nodes": [
        { "selector": "main > p.note", "excerpt": "<p class=\"note\">",
          "data": { "fg": "#999999", "bg": "#ffffff", "ratio": 2.85, "expected": 4.5, "font_px": 14, "bold": false },
          "box": [16, 820, 380, 40] }
      ]
    }
  ]
}
```

## `mobile_layout`

- **Script**: `crates/engine/assets/mobile_layout.js`. Only added when mobile checks are on and the window is mobile.
- **Args**: `{ "small_text_px": 12, "tap_target_px": 24, "examples": 20 }`.
- **Result**:

```json
{ "window_px": 411, "content_px": 1200, "widest": "table#prices",
  "small_text_share": 62.5, "small_targets": 14,
  "examples": [ { "selector": "nav a:nth-child(3)", "w": 16, "h": 16 } ] }
```

## Load types

When the level is Full or mobile checks are on, the job adds `font` and `image` to `RenderJob.load_types` (FR-012).
