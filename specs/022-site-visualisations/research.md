# Research: Site Visualisations

## R1. Where layouts run

**Decision**: in Rust, in `spiderlens-results`, with the UI only drawing.
**Why**: the CLI must export the same pictures (constitution III, FR-025); one deterministic implementation serves canvas, SVG, PNG and CLI; layout runs off the UI thread for free.
**Alternatives**: d3-force / d3-hierarchy / dagre in the UI (fast to build, but the CLI would need a JS runtime or a second implementation, and floating-point results differ between engines); Graphviz (an external binary, not available on users' machines).

## R2. How the UI draws

**Decision**: Canvas 2D with batched paths per category.
**Why**: 20,000 nodes in a few dozen draw calls runs well within a frame; no dependency; works in every Tauri web view (WebKit on macOS and Linux, WebView2 on Windows).
**Alternatives**: SVG DOM (slows down past a few thousand elements); WebGL via sigma.js or regl (faster at 100k+ but adds a dependency, shader code and context-loss handling, unnecessary within the budgets).

## R3. Force layout algorithm

**Decision**: ForceAtlas2-style forces with Barnes-Hut, seeded and sequential.
**Why**: designed for scale-free web graphs (hubs and long tails), separates clusters clearly, O(n log n) per iteration.
**Alternatives**: Fruchterman-Reingold (clumps hubs, slower to settle on web graphs); stress majorization (O(n²) memory).

## R4. Tree layout

**Decision**: Buchheim-Jünger-Leipert tidy tree, radial as a polar mapping of the same layout.
**Why**: linear time, compact, aesthetically standard; radial and tree show the same structure.

## R5. PNG rendering

**Decision**: `resvg` on our own SVG.
**Why**: pure Rust, renders the exact SVG we export, works in the CLI.
**Alternatives**: canvas `toBlob` (UI only); `cairo` (system library, packaging pain on Windows).

## R6. Stop words

**Decision**: embedded Snowball lists (public domain) for nine languages.
**Why**: small, well known, cover the interface languages and the most common web languages.
**Alternatives**: the `stop-words` crate (pulls large lists for 60 languages; most are never used).
