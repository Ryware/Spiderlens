# Quickstart: Link Score

1. `cargo run -p spiderlens-test-fixtures --bin fixture-site -- links` (serves the Link Score fixture site), then crawl it: `spiderlens crawl http://127.0.0.1:<port>/ --out links.spiderlens`.
2. The crawl ends with "Analysing: Link Score 100%". `spiderlens export links.spiderlens --tab links` lists the hub page with score 100 and the deep page last.
3. `spiderlens export links.spiderlens --tab links --filter no-followed` lists the nofollow-only page and the sitemap-only page, not the start URL.
4. In the app, open the file: Internal shows a Link Score column; Links appears in the sidebar; selecting the hub page shows "rank 1 of N" in the inspector.
5. Settings › Link analysis: add `.mega-menu` → Navigation, test it on the home page, turn on weighting, run Crawl Analysis; the footer-only page drops in rank.
6. `cargo run -p spiderlens-results --release --example bench_links -- 100000` prints analysis time and peak memory for a synthetic 100k-page file (SC-002, SC-004).
