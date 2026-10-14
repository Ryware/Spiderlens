# Quickstart: JavaScript Rendering

How to check the feature end to end on the local fixture. Commands and flags are defined in [contracts/cli.md](./contracts/cli.md); stored data in [data-model.md](./data-model.md).

## Prerequisites

- A Chromium-family browser: installed Chrome or Edge, or `cargo run -p spiderlens -- browser install --yes`.
- Check it: `cargo run -p spiderlens -- browser status` (exit 0 means usable).
- For the golden file only: Node 22 with Playwright's Chromium (as for spec 005's `browser-golden.mjs`).

## 1. Automated tests

```sh
# Pure parts, no browser needed: comparison, dependence, settle rule, cache, states
cargo test -p spiderlens-engine compare
cargo test -p spiderlens-render settle

# Render integration tests against the JavaScript fixture site (needs a browser;
# skipped with a message locally if none, failing in CI)
SPIDERLENS_REQUIRE_BROWSER=1 cargo test -p spiderlens-engine --test rendering
SPIDERLENS_REQUIRE_BROWSER=1 cargo test -p spiderlens --test render_cli

# Results: JavaScript tab, filters, rendered values, diff
cargo test -p spiderlens-results --test rendering

# UI
pnpm test
```

Expected: every test passes. The fixture site serves one page per acceptance scenario (tasks T005, T020, T039, T046, T056 and T068); each test names the scenario it covers.

## 2. Render the fixture site from the CLI

```sh
cargo run --release -p spiderlens-test-fixtures --bin js-site &        # http://127.0.0.1:8710
cargo run --release -p spiderlens -- crawl http://127.0.0.1:8710/ --render --out /tmp/js.spiderlens
```

Expected:
- The progress shows fetched and rendered figures separately, and "fetching done; rendering continues" near the end.
- The summary's Rendering block adds up to the number of eligible pages, with one "Render timed out" (`/never-settles`), one "Render failed" (`/crash`, after 2 attempts) and the `/admin/` pages "Not rendered: outside rendering rules" only when run with `--render-exclude /admin/`.

Then look at single pages:

```sh
spiderlens page /tmp/js.spiderlens http://127.0.0.1:8710/title-changed --json   # raw "Loading", rendered "Red Shoes | Shop"
spiderlens html /tmp/js.spiderlens http://127.0.0.1:8710/title-changed --diff
spiderlens export /tmp/js.spiderlens --tab javascript --filter js_links --out /tmp/js-links.csv
curl -s http://127.0.0.1:8710/__stats       # each page path requested once (SC-006)
```

## 3. Raw crawl is unchanged (speed first)

```sh
cargo run --release -p spiderlens-test-fixtures --bin bench-site -- --pages 100000 &
cargo run --release -p spiderlens -- crawl http://127.0.0.1:8700/p/0 --speed fast --out /tmp/raw.spiderlens --quiet
```

Expected: no browser process appears at any point (`pgrep -f headless` stays empty), and throughput and peak memory match the baseline in [benchmarks.md](./benchmarks.md) within the gate (SC-001). The full before and after method is task T080.

## 4. Render speed and memory

```sh
cargo run --release -p spiderlens-test-fixtures --bin bench-site -- --pages 10000 --spa &
cargo run --release -p spiderlens -- crawl http://127.0.0.1:8700/p/0 --speed fast --render --out /tmp/spa.spiderlens
```

Expected: at least 10 pages per second rendered on the reference machine (SC-002), memory of SpiderLens plus the browser below 3 GB (SC-003).

## 5. Desktop app

1. Settings, "JavaScript rendering": switch the mode on. If no browser is found, the notice offers the download with its size.
2. Crawl `http://127.0.0.1:8710/`. The status area shows a rendering row.
3. Open the JavaScript tab: filter "Contains JavaScript links" lists `/js-nav`; select it, open "Raw vs rendered" in the detail pane and step through the differences.
4. Switch the results to raw values in the toolbar: the Page titles tab shows "Loading" for `/title-changed`.
5. Quit the app during a rendered crawl; within 10 s no browser process from it is left (SC-009).

## 6. Render an existing crawl

```sh
spiderlens crawl http://127.0.0.1:8710/ --out /tmp/later.spiderlens
spiderlens render /tmp/later.spiderlens
curl -s http://127.0.0.1:8710/__stats       # page paths still requested once in total
```
