# Desktop commands: Site Visualisations

| Command | Arguments | Returns |
|---------|-----------|---------|
| `viz_scene` | `state: ViewState` | `Scene` (binary response above 5,000 nodes) |
| `viz_layout_frame` (event) | — | `{ seq, positions: Float32Array, settled }` for force layouts |
| `viz_cancel` | — | stops the running layout or build |
| `viz_term_links` | `target?: url_id, term, mode, offset, limit` | `ListPage<LinkRow>` |
| `viz_export` | `state: ViewState, format: "png" \| "svg" \| "csv", scale?: 1\|2\|4, theme: "light" \| "dark", palette: Palette, path` | `()`; progress as `export_progress` (spec 007) |
| `viz_show_in_table` | `node: NodeId` | opens the Internal tab view scoped to the node (folder subtree or URL) |
