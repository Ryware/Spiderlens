# Contract: desktop commands and events

Tauri commands are thin wrappers over `spiderlens-render`, `spiderlens-engine` and `spiderlens-results` (constitution principle III). New commands live in `app/src-tauri/src/render.rs`; results commands stay in `results.rs`. Types are generated with ts-rs into `ui/src/generated`. Errors are plain messages. The UI reaches all of them only through `ui/src/transport.ts`.

## Browser

| Command | Arguments | Returns |
|---|---|---|
| `render_browser_status` | none | `BrowserStatus { usable: BrowserInfo \| null, problem: string \| null, download: { version, bytes } \| null }` |
| `render_browser_install` | none | `job: number`. Starts the pinned download (consent is given in the UI before calling) |
| `render_browser_cancel` | `job` | none |
| `render_browser_set_path` | `path \| null` | `BrowserStatus`. Saves the machine preference; `null` returns to detection |

Events:

| Event | Payload |
|---|---|
| `render-browser-progress` | `job`, `bytes_done`, `bytes_total`, `stage` (`download`, `verify`, `unpack`) |
| `render-browser-finished` | `job`, `status: BrowserStatus`, `error: string \| null` |

## Crawling

| Command | Change |
|---|---|
| `crawl_start` | Unchanged signature. With `rendering.mode = javascript` it checks the browser first and returns the error `browser_unavailable: <message>` without creating a file when none is usable; the UI then shows the consent dialog (FR-045). |
| `crawl_resume` | Same check when the stored crawl renders. |
| `crawl_render` | New. `path`, `rendering: RenderSettings` → `Started { id, path }`. Renders an existing crawl (FR-006); refused with a message when it stored no HTML. Uses the same `crawl-progress` and `crawl-finished` events. |

Events, extended:

| Event | Payload change |
|---|---|
| `crawl-progress` | gains `render: RenderProgress \| null` (null for raw crawls, so existing listeners are unaffected) |
| `crawl-finished` | `report.summary` gains `render: RenderSummary \| null`; `report` gains `render_reasons` with labels, as `reasons` does today |

## Results

| Command | Change |
|---|---|
| `results_tabs` | The JavaScript tab appears in the list (`id: "javascript"`), always; the UI shows its "not rendered" notice when `features.available` lacks `rendering`. |
| `view_open` | `ViewQuery` gains `values: "raw" \| "rendered"` (default `rendered` when the file has rendering). |
| `view_counts` | Takes the same `values`. |
| `url_detail` | `UrlDetail` gains `render: RenderDetail \| null` (state, reason, attempts, render time, window, JavaScript redirect, counts, comparison, dependence, `rendered_html_stored`, `screenshot_stored`). |
| `url_list` | `ListKind` gains `resources` and `console`; `headings`, `outlinks` and `images` rows gain `origin: "raw" \| "rendered" \| "both" \| null`, and take `values`. |
| `url_html` | Gains `version: "raw" \| "rendered"`. This command is in spec 007's contract but not yet registered in `app/src-tauri/src/main.rs`; this feature adds it if spec 007 has not by then. |
| `url_html_diff` | New. `path`, `id`, `from_hunk`, `max_hunks` → `HtmlDiff { hunks: [{ raw_start, raw_lines[], rendered_start, rendered_lines[], kind }], total_hunks, raw_bytes, rendered_bytes }` |
| `url_screenshot` | New. `path`, `id` → raw WebP bytes (`tauri::ipc::Response`) or error `unavailable` |

## Settings

No new command. `config_defaults`, `config_validate`, profiles and `crawl_start` carry the `rendering` section because it is part of `CrawlConfig`.
