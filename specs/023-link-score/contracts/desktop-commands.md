# Desktop commands: Link Score

| Command | Arguments | Returns |
|---------|-----------|---------|
| `analysis_state` | — | `AnalysisState` |
| `run_analysis` | — | `()`; progress as `analysis_progress` events, completion as `analysis_done { state }` |
| `cancel_analysis` | — | `()` (stops within 1 s; earlier results kept) |
| `test_positions` | `url: string, rules: PositionRule[]` | `{ links: [{ pos, position, rule, target, text }] }` |

`url_detail` (spec 007) gains `link_metrics`; `list_page` rows for inlinks and outlinks gain `position` and accept a `position` filter. The `links` tab is served by the existing `open_view` / `rows` commands.
