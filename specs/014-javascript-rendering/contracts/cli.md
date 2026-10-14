# Contract: command line

Every rendering setting is a flag of `spiderlens crawl` (and of `spiderlens config show/validate`, which share `CrawlArgs`) with the same default as the desktop app (FR-047). Flags live under a new help heading, "Rendering". `--resume` keeps refusing other options, as today: a resumed crawl uses the rendering settings stored in its file.

## `spiderlens crawl` flags

| Flag | Value | Default | Maps to |
|---|---|---|---|
| `--render` | | off (raw HTML) | `rendering.mode = javascript` |
| `--render-timeout` | `SECONDS` (1 to 60) | `10` | `max_render_secs` |
| `--render-min-wait` | `SECONDS` | `0` | `min_wait_secs` |
| `--render-window` | `auto`, `mobile`, `desktop` or `WIDTHxHEIGHT` | `auto` | `window` |
| `--render-block` | comma list of `images`, `media`, `fonts`, `stylesheets`, `third-party`, or `none` | `images,media,fonts` | `block.*`; `none` = load all resources |
| `--render-include` | `REGEX`, repeatable | none | `include` |
| `--render-exclude` | `REGEX`, repeatable | none | `exclude` |
| `--render-error-pages` | | off | `render_error_pages` |
| `--render-tabs` | `N` (1 to 16) or `auto` | `auto` | `concurrency` |
| `--render-memory` | `MB` or `auto` | `auto` | `memory_budget_mb` |
| `--screenshots` | | off | `screenshots` |
| `--js-content-threshold` | `PERCENT,WORDS` | `20,50` | `js_content_min_percent`, `js_content_min_words` |
| `--js-hidden-threshold` | `PERCENT` | `50` | `hidden_content_percent` |
| `--browser` | `PATH` | detected | not stored in the file (machine preference) |
| `--allow-browser-download` | | off | downloads the pinned browser if none is usable, without asking |

Rendering flags other than `--render` given without `--render` are an error (exit 2), so a typo cannot silently produce a raw crawl.

## Browser check (FR-045)

With `--render`, before the crawl file is created:

- A usable browser is found: `notice: rendering with Google Chrome 155.0.8059.39 (installed)` on stderr.
- None found, without `--allow-browser-download`: `error: JavaScript rendering needs a Chromium-based browser and none was found. Run "spiderlens browser install" (downloads about 100 MB) or add --allow-browser-download, or crawl without --render.` Exit **7** (6 was taken by spec 006's disk-problem pause). No crawl file is created. When this build has no pinned download for the platform, the message names `--browser PATH` and installing Chrome, Chromium or Edge instead of `spiderlens browser install`.
- None found, with `--allow-browser-download`: the download runs with a progress line, then the crawl starts.

## Progress (stderr, FR-040)

The existing line, plus while rendering is on:

```text
rendered 1,204 · waiting 3,880 · rendering 4 · 11.2 pages/s · limited by: memory guard (4 → 3 tabs)
```

When the fetch stage has finished: `fetching done; rendering continues`.

## Summary (FR-041)

Text summary gains a "Rendering" block: rendered, timed out, failed by reason, not rendered by reason (these add up to the eligible pages), average and slowest render time, JavaScript errors, failed and blocked resources, and the renderer name and version. `--json` adds a `render` object (`RenderSummary` in [data-model.md](../data-model.md)); it is absent for raw crawls.

## New commands

### `spiderlens browser status [--json]`

Prints the browser rendering would use (name, version, source, path) or why none is usable. Exit 0 when usable, 7 when not. CI runs this on every runner (task T002).

### `spiderlens browser install [--yes] [--json]`

Downloads the pinned `chrome-headless-shell` for this platform into the SpiderLens data folder, after printing its size and asking for confirmation unless `--yes`. Verifies the SHA-256 (exit 4 on mismatch, nothing kept). Exit 0 when installed or already present.

### `spiderlens browser remove`

Deletes the downloaded browser. Never touches an installed Chrome or Edge.

### `spiderlens render <FILE> [rendering flags] [--allow-browser-download] [--json] [--quiet]`

Renders the pages of an existing, finished crawl from their stored raw HTML (FR-006), with the given rendering settings (defaults as above). The main documents are not requested again; page resources are, politely. Refused with exit 2 if the crawl stored no HTML, is still running, or was already rendered (re-rendering is out of scope). Pages without stored HTML end as `Not rendered: raw HTML not stored`. Ctrl-C pauses; `spiderlens render <FILE>` again resumes.

### `spiderlens html <FILE> <URL> [--rendered | --diff] [--out PATH]`

- Default: the raw HTML, as today.
- `--rendered`: the stored rendered HTML, exactly as serialised (FR-032). Exit 1 if not stored.
- `--diff`: the normalised raw versus rendered diff in unified format, the same hunks the desktop pane shows (FR-028).

### `spiderlens screenshot <FILE> <URL> --out PATH`

Writes the stored screenshot (WebP). Exit 1 if none.

### `spiderlens page <FILE> <URL> [--json]`

Unchanged for raw crawls. For rendered pages adds `render` (state, reason, attempts, timing, window, JavaScript redirect), `rendered` (the same fields as the raw page data, each list item with `origin`), `comparison` (FR-024), `dependence` (FR-029), `resources`, `console`.

### `spiderlens export <FILE> --tab javascript [--filter ID] [--values raw|rendered]`

The JavaScript tab exports like any other tab (spec 010 builds on this). `--values` applies to every tab of a rendered crawl and defaults to `rendered` (FR-021).

## Exit codes

Existing codes are unchanged. New: **7** = no usable browser for rendering (6 is spec 006's "paused by a disk or write problem").
