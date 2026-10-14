# Quickstart: Site Visualisations

1. Serve the structure fixture site (`fixture-site viz`) and crawl it to `viz.spiderlens`.
2. In the app, open Visualisations › Crawl tree: the start URL is the root; the 404 page shows as a red triangle at depth 3; hovering shows URL, status, depth, Link Score.
3. Switch Layout to Radial: same nodes. Expand the "+120 more" summary under the archive page: the next 500 children appear.
4. Directory tree: `/blog/` shows its URL count and a split by status; "Show in table" opens Internal filtered to that folder with the same count.
5. Link graph: two clusters drawn apart; select a page, Focus, distance 1: only its neighbours.
6. Select a category page, Inspector › Inlinks › Anchor text cloud: "running shoes" is the largest term.
7. Export › PNG 2x dark; Export › SVG. Then `spiderlens viz viz.spiderlens --view crawl-tree --svg cli.svg` and compare with the default-state SVG from the app: identical.
8. `cargo run -p spiderlens-results --release --example bench_viz -- 100000` prints scene build and layout times (SC-002, SC-003).
