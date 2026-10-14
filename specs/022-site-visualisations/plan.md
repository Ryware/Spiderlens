# Implementation Plan: Site Visualisations

**Branch**: `022-site-visualisations` | **Date**: 2026-10-14 | **Spec**: [spec.md](./spec.md)

## Summary

Every visualisation is a **scene** built in Rust (`spiderlens-results::viz`) and drawn by the UI on a 2D canvas. A scene is a flat list of nodes with positions, sizes, categories and labels, a list of lines, a legend, and what was left out. Rust owns the data, the level of detail and every layout (tree, radial, force-directed, word cloud), so the same scene feeds the canvas, the SVG and PNG export, and the CLI (`spiderlens viz`), which is how desktop and CLI exports come out identical (FR-025, SC-007). The UI owns only drawing, panning, zooming, hit testing and the controls.

Two precomputed tables make the trees fast on 100k pages: `viz_crawl_tree` (each URL's tree parent and subtree size) and `viz_dirs` (folder nodes and counts). They are Crawl Analysis passes under the 009 contract (009 plan D12). As in spec 023, stage 1 computes them into the reader's `temp` schema on demand (no file change) and stage 2 registers them as passes once the 009 runner and the 006 migrations are on `main`.

## Technical Context

- **Language**: Rust 1.97, TypeScript + React 18
- **Crates touched**: `spiderlens-results` (scenes, layouts, export), `spiderlens-cli`, `app/src-tauri`, `ui/`
- **New dependencies**: `resvg` (PNG export: renders our own SVG to pixels, pure Rust, the same on every platform; brings `usvg`, `tiny-skia`, `fontdb`). `unicode-segmentation` is already in the lock file (via another crate) and becomes a direct dependency for word splitting. No UI dependency: drawing uses the Canvas 2D API.
- **Storage**: two derived tables (stage 1 `temp`, stage 2 `main` via a migration step, number at merge).
- **Testing**: `cargo test` with fixture crawls and golden SVGs; Vitest for components and the canvas controller; Playwright in Chromium for interaction and frame-time checks.
- **Performance goals**: SC-002 (trees 2 s, expand 1 s), SC-003 (graph 3 s first picture, 10 s settled), SC-004 (30 fps at default budgets), SC-006 (memory under 300 MB).

## Constitution Check

| Principle | How this plan complies |
|-----------|------------------------|
| I. Spec before code | Spec 022 approved 2026-10-13. |
| II. Local-first | All drawing and layout local; no network; no remote fonts. |
| III. Engine independent of the shell | Scenes, layouts and export live in `spiderlens-results`; `spiderlens viz` writes the same SVG and CSV. |
| V. Correct and complete data | Every view states what it left out and why (FR-020); snapshots carry their time. |
| VI. Scales by design | Views request only the nodes they draw (budgets); trees read precomputed tables by index; no view loads the crawl. |
| VII. Test-first | Golden scene and SVG tests per view; UI tests mandatory (Vitest + Playwright). |
| VIII. Simplicity | One scene type for four views; Canvas 2D instead of a graph library; the one new dependency (`resvg`) replaces writing a rasteriser. |

## Design decisions

### D1. One scene type, built in Rust

```rust
pub struct Scene {
    pub view: ViewKind,                 // CrawlTree | DirTree | LinkGraph | Anchors
    pub nodes: Vec<SceneNode>,          // drawn order
    pub links: Vec<SceneLink>,          // (a, b, kind: Tree | Followed | Nofollow | External)
    pub legend: Legend,                 // colour meaning with counts, size meaning
    pub omitted: Option<Omitted>,       // { count, reason, action }
    pub bounds: Rect,
    pub snapshot_at: Option<i64>,       // live crawls (FR-006)
    pub settled: bool,                  // force layouts
}
pub struct SceneNode {
    pub id: NodeId,                     // Url(i64) | Dir(i64) | Summary{parent, page} | Group(Source) | Domain(i64) | Term(u32)
    pub label: String, pub x: f32, pub y: f32, pub r: f32,
    pub category: Category,             // Ok | Redirect | ClientError | ServerError | Blocked | NotCrawled | Other | Folder
    pub score: Option<f32>, pub inlinks: Option<u32>, pub depth: Option<u32>,
    pub hidden: u32,                    // children folded into this node
}
```

The UI asks for a scene with a `ViewState` (view, layout, colour, size, expanded node ids, focus URL and distance, pinned positions, options) and gets the whole scene back. Positions travel as `f32`; a 20,000-node scene is about 1.5 MB of JSON, so scenes over 5,000 nodes are sent as a binary `tauri::ipc::Response` (a packed array plus a string table) to keep IPC under 50 ms.

### D2. Crawl tree: the parent rule

The crawl stores `urls.source_id` (first discoverer) but does not update it when a shorter path is found later (store.rs requeue keeps it). The tree parent is therefore computed, not read:

- `parent = source_id` when the source's depth is `depth - 1`;
- else the anchor or redirect in-link source with depth `depth - 1` and the smallest `urls.id` (earliest discovered), found with one indexed lookup on `links_to`;
- URLs whose source is `sitemap`, `canonical`, `hreflang`, `list` or `image` with no such in-link go under a synthetic "Found outside links" group, one child group per source (spec edge case).

Pass `viz_crawl_tree` writes `viz_crawl_tree(url_id PK, parent_id, child_count, subtree_count, first_child_rank)` for crawled internal URLs and the not-crawled ones that were queued (blocked, failed, over a limit). Subtree counts are one bottom-up sweep by depth. Children are ordered by Link Score when available, else by `urls.id`, so the strongest branches come first and the order is deterministic. Cost on 100k URLs: one scan of `urls`, one probe per URL, under 1 s.

### D3. Directory tree

Pass `viz_dirs` parses each internal URL (already normalised by spec 003) into host and path segments. A folder is every path prefix ending in `/`; a page is the full path; URLs that differ only by query string share a page node with a count (spec US2 scenario 4). Output:

```sql
viz_dirs(id PK, parent_id, host, path, label, urls, ok, redirect, client_error, server_error, blocked, other)
viz_dir_urls(url_id PK, dir_id, page_key)
```

Built in one pass over `urls` ordered by URL text (so a folder's children are contiguous) with a stack of open folders; counts roll up when a folder closes. Several hosts become top-level branches under a crawl root. "Show in table" for a folder is a 007 view with scope `u.id IN (SELECT url_id FROM viz_dir_urls WHERE dir_id IN <subtree>)`, using the `path` prefix as a range on the URL index.

### D4. Level of detail: budgets, folding and paging

- **Initial expansion**: breadth-first from the root, opening a node only if its children fit in the remaining budget (20,000 for tree and radial, 5,000 for force layouts; Settings › Visualisations, capped at 50,000 / 15,000).
- **Folding**: a node with more children than fit shows its first 500 children (by the D2 order) plus one summary node "+N more" carrying the hidden category breakdown; activating it shows the next 500 (spec US6 scenario 2).
- **Expanding**: the UI sends the new expanded set; Rust returns the new scene; nodes that existed keep their positions in tree layouts (tidy layout is local to the changed subtree, D5), so the view does not jump (US1 scenario 3).
- **Whole-site link graph** (FR-021): the top N internal HTML pages by Link Score (spec 023), or followed inlinks when there is no score, always including the start URL; lines are the distinct followed links among them, read with a temp table of selected ids joined to `links_from`. The legend states "Showing 5,000 of 98,231 pages, by Link Score".
- **Focus**: the selected page plus its in- and out-neighbours at distance 1 or 2, each ring capped by the budget, strongest first.

### D5. Layouts, all deterministic

- **Tree (left to right)**: Buchheim-Jünger-Leipert tidy tree, O(n). Node spacing fixed in layout units; the UI scales.
- **Radial**: the same tidy layout mapped to polar coordinates, angle proportional to leaf position, radius proportional to depth, so the two layouts show the same nodes and expansion (US1 scenario 6).
- **Force-directed** (crawl tree, directory tree, link graph): ForceAtlas2-style forces with Barnes-Hut approximation (theta 0.8), linear-log attraction, degree-weighted repulsion, gravity to the centre, adaptive speed. Initial positions come from a seeded generator (seed = hash of the crawl id and view), and summation runs in a fixed order on one thread, so the same crawl and settings give the same layout on every machine (FR-014). It stops at convergence (mean displacement under a threshold) or 500 iterations. Pinned nodes (user-dragged) are fixed inputs.
- **Streaming**: the force layout runs on its own thread and emits a frame about every 100 ms (`viz_layout_frame`, binary positions), so the picture settles visibly; with reduced motion only the final frame is drawn. Leaving the view cancels it (FR-023).
- **Cost**: 5,000 nodes with Barnes-Hut is roughly 2 ms per iteration, so under 2 s to settle; the link graph's data read is under 1 s on 100k pages.

### D6. Anchor text clouds

- **Data**: `SELECT text, alt FROM links WHERE to_id = ? AND kind = 'anchor'` (per URL, `links_to` index) or over all internal anchor links (site-wide, one scan, counted in Rust with a hash map bounded by distinct terms). With positions available (spec 023), a position filter joins `link_positions`.
- **Terms**: Words mode lowercases, splits with `unicode-segmentation` word boundaries and drops stop words for the language; Whole anchors mode uses the whitespace-collapsed, lowercased anchor text. Languages without spaces always use whole anchors (edge case).
- **Stop words**: built-in lists for English, Hebrew, German, French, Spanish, Italian, Portuguese, Dutch and Russian, from the public-domain Snowball lists, embedded as text files (about 15 KB). The language is the target page's `lang` primary subtag; site-wide uses the most common page language.
- **Layout**: top 100 terms, font size by square root of count (12 to 48 layout px), placed on an Archimedean spiral from the centre with rectangle collision, in order of count then term text: deterministic. Text width is estimated from a fixed per-character advance table (Latin, Hebrew, Cyrillic, CJK full width); the canvas draws with `fillText(..., maxWidth)` and the SVG with `textLength`, so a real font never overflows its slot and screen, SVG and CLI agree.

### D7. Drawing in the UI

- **Canvas 2D**, device-pixel-ratio aware, redrawn only on change (pan, zoom, hover, new frame) with `requestAnimationFrame`.
- **Batching**: nodes are drawn grouped by category and shape in one path per group, lines in one path per kind, so 20,000 nodes is a few dozen draw calls. Expected 5-8 ms per frame for 20,000 nodes on the reference machine, measured in Playwright (SC-004). If it misses, the fallback is skipping lines while the user is panning.
- **Shapes for colour-blind safety (FR-005)**: success circle, redirect diamond, client error triangle, server error square, blocked ring, not crawled dashed ring, folder rounded square.
- **Labels**: drawn only when the node's on-screen radius is at least 6 px and the label fits; at most 300 labels per frame, strongest first (FR-022).
- **Hit testing**: a uniform grid over scene bounds, rebuilt when a scene arrives; hover and click are O(1).
- **Interaction**: drag to pan, wheel and pinch to zoom around the pointer, `+` / `-` / `0` (fit), arrows move selection to the nearest node in that direction, Enter expands or collapses, `/` focuses search, Escape clears. Selection uses the shared selected-URL state, so the inspector follows (FR-002). Context menu: Show in table, Copy URL, Open in browser (FR-003).
- **Theme**: colours are read from CSS custom properties once per theme or palette change.

### D8. Colours and tokens

New tokens in `ui/src/styles/tokens.css` for light and dark: `--viz-ok`, `--viz-redirect`, `--viz-client-error`, `--viz-server-error`, `--viz-blocked`, `--viz-not-crawled`, `--viz-folder`, `--viz-line`, `--viz-line-strong`, and a six-step sequential ramp `--viz-depth-1` to `--viz-depth-6` for colour by depth. The existing contrast test is extended to check every viz token against `--surface` at 3:1 (graphical objects, WCAG 1.4.11). Status hues align with `--ok`, `--info`, `--warn`, `--bad` so tables and pictures agree.

The CLI and PNG export need the same colours without a browser: a build script in `spiderlens-results` parses `tokens.css` (the light and dark `--viz-*` OKLCH values), converts them to sRGB hex and generates `viz/palette.rs`. tokens.css stays the only place colours are written (017 rule). The desktop export passes the active palette resolved by the browser, so exports follow the user's palette (spec 018); the CLI uses the default palette with `--theme light|dark`.

### D9. Export

- **SVG**: written by hand in Rust from the scene (no dependency): a `<g>` per layer, shapes as `<circle>/<path>`, labels as `<text>` with `textLength`, legend and a header with host and crawl date (FR-024). Same `ViewState` in, same bytes out.
- **PNG**: the SVG rendered by `resvg` at 1x, 2x or 4x, using system fonts through `fontdb`; refused above 16,000 px on a side with the SVG suggestion (edge case). Runs on a worker thread with progress; atomic write like the 007 CSV export.
- **CSV**: two files or one, per the 007 CSV rules (`write_csv`): nodes (URL, label, kind, depth, category, Link Score, inlinks, parent or folder, hidden count) and links (from, to, kind).
- **Desktop export** sends the current `ViewState` (including pinned positions and the resolved palette); Rust rebuilds the scene, so the export is exactly what is on screen.

### D10. Live crawls and staleness

Views show the data at the moment the scene was built (`snapshot_at`) and do not redraw on their own; a Refresh button rebuilds (FR-006). In stage 1 the two tree tables are computed into `temp` when a view opens and recomputed on Refresh if the reader's data version moved. In stage 2 they are passes; during a running crawl the reader computes them into `temp` on demand (the 009 runner only runs at crawl end). The view building never touches the writer, and the 008 benchmark job guards crawl speed (SC-005).

### D11. Placement in the shell

- Sidebar group "Visualisations" under Crawl results: Crawl tree (`GitFork`), Directory tree (`FolderTree`), Link graph (`Waypoints`), Anchor text (`Type`). Command palette entries for each (FR-001).
- The main area hosts `VizView`: a compact toolbar (layout segmented control, Colour select, Size select, search, Fit, Refresh, Export menu, Focus distance for the graph), the canvas, a legend card bottom-left, an "omitted" notice bottom-right, and tooltips.
- Anchor text for one URL also opens from the inspector's Inlinks section ("Anchor text cloud").
- View choices (layout, colour, size) persist with the existing UI state store; expansion and focus last for the open crawl (FR-007).

### D12. Command line

```text
spiderlens viz <file> --view crawl-tree|directory-tree|link-graph|anchors
               [--layout tree|radial|force] [--colour status|indexability|depth] [--size score|inlinks|uniform]
               [--focus <URL> [--distance 1|2]] [--url <URL>  (anchors for one URL)]
               [--theme light|dark] [--svg out.svg] [--png out.png --scale 2] [--csv out.csv]
```

Builds the default `ViewState` for the view (or the focus), the same as the app's initial state, and writes the requested files.

## Performance plan

| Step | 100k pages | Budget |
|------|-----------|--------|
| `viz_crawl_tree` build | ~1 s | in SC-002 on first open (stage 1); free afterwards |
| `viz_dirs` build | ~1 s | same |
| Tree scene (20k nodes) + tidy layout | ~150 ms | SC-002 |
| Graph data (5k nodes) | ~600 ms | SC-003 |
| Force layout settle (5k nodes) | ~2 s | SC-003 |
| Canvas frame (20k nodes) | 5-8 ms | SC-004 |
| PNG 2x (5k nodes) | ~1.5 s | SC-007 |

Memory: the largest scene is about 3 MB in Rust and the same in the UI; derived tables are on disk. Measured with `synth` 100k and 1M files (SC-006).

## Testing plan

- **Data (SC-001)**: fixture crawls with known structure: crawl tree parents and depths (including shorter-path-found-later, redirects, sitemap-only, blocked), folders and counts (several hosts, query-string pages), link graph lines (two clusters joined by one link), anchor term counts per language.
- **Golden scenes**: each view's default scene on the fixture crawl serialised and compared; golden SVGs compared byte for byte; the same test runs on all three CI platforms (determinism, FR-014).
- **Layouts**: tidy-tree invariants (no overlap, parents centred), radial equals tree node set, force layout stable for a fixed seed, pinned nodes stay.
- **LOD**: budgets respected, summary counts equal hidden counts, paging, focus rings.
- **Export**: PNG decodes with the right size; refusal above the limit; CSV round trip with the 007 rules; desktop vs CLI SVG identical.
- **UI (mandatory)**: Vitest for the canvas controller (pan/zoom maths, hit grid, keyboard navigation, label budget), toolbar, legend, omitted notice, anchor cloud and term list, with the fake transport; Playwright e2e on fake scenes: open each view, hover, click to inspector, expand a summary, export menu, reduced motion, light and dark screenshots; a frame-time probe on a 20,000-node fake scene (SC-004).
- **Benchmarks**: `synth` 100k / 1M scene and pass timings recorded in `benchmarks.md`; crawl speed with a view open via the 008 benchmark job (SC-005).

## Project structure (changes)

```text
crates/results/src/viz/
  mod.rs            Scene, ViewState, build_scene()
  crawl_tree.rs     pass viz_crawl_tree (D2)
  dirs.rs           pass viz_dirs (D3)
  lod.rs            budgets, folding, paging, graph selection (D4)
  tidy.rs radial.rs force.rs   layouts (D5)
  anchors.rs stopwords/*.txt   terms and cloud layout (D6)
  svg.rs png.rs csv.rs         export (D9)
  palette.rs        generated from ui/src/styles/tokens.css by build.rs (D8)
crates/cli/src/viz_cmd.rs
app/src-tauri/src/viz.rs      viz_scene, viz_layout_frame events, viz_export, viz_cancel
ui/src/viz/                   VizView, VizCanvas (controller + renderer), Legend, Omitted, VizToolbar, AnchorCloud, TermLinks
ui/src/shell/Sidebar.tsx      Visualisations group
ui/src/styles/tokens.css      --viz-* tokens
crates/test-fixtures/src/viz.rs   structure fixture site
```

## Dependencies and coordination

- **009** owns the Crawl Analysis runner; `viz_crawl_tree` and `viz_dirs` register as passes in stage 2.
- **006** owns the schema; one migration step for both tables, number at merge.
- **023** supplies `link_metrics` (node size, graph selection, child order) and `link_positions` (anchor cloud filter); every view falls back without them.
- **018** palettes: viz tokens follow the active palette's light and dark values.

## Complexity Tracking

| Addition | Why | Simpler alternative rejected |
|----------|-----|------------------------------|
| `resvg` dependency | PNG export identical on all platforms without a browser (CLI) | Canvas `toBlob` in the UI: no CLI PNG, and results differ per web view engine |
| Layouts in Rust instead of a JS library | Identical desktop and CLI exports (constitution III); one implementation | d3-force / dagre in the UI: CLI would need a JS runtime or a second implementation |
