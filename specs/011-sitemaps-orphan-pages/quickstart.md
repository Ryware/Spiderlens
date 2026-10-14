# Quickstart: Sitemaps and Orphan Pages

1. Start the sitemap fixture: `cargo run -p spiderlens-test-fixtures --bin sitemap-site` (serves 127.0.0.1:8711 with every seeded problem from the spec).
2. Crawl it from the CLI: `spiderlens crawl http://127.0.0.1:8711/ --out /tmp/sm.sqlite`.
3. `spiderlens sitemaps /tmp/sm.sqlite`: the tree shows the index, the oversized file, the HTML file, the 404, and each with its problem.
4. `spiderlens orphans /tmp/sm.sqlite`: lists `/orphan-b` as orphan, `/nofollow-c` as nofollow only; `/linked-a` and `/redirect-target-d` absent.
5. `spiderlens issues /tmp/sm.sqlite --category Sitemaps`: one hit per seeded problem.
6. Crawl again with `--no-crawl-sitemap-only`: orphans listed with "not crawled: found only in sitemap".
7. Generate: `spiderlens sitemap generate /tmp/sm.sqlite --out /tmp/sm-out --base-url http://127.0.0.1:8711/ --images inline --hreflang`; run it twice and `cmp` the outputs; on Linux `xmllint --noout --schema specs/011-sitemaps-orphan-pages/xsd/sitemap.xsd /tmp/sm-out/sitemap-1.xml`.
8. Desktop: open the file, Sitemaps tab, Sitemap files, inspector Sitemaps section, ⌘K "Generate XML sitemap".
9. Benchmark: `cargo run --release -p spiderlens-test-fixtures --bin sitemap-site -- --entries 1000000 --files 20` and crawl with `--speed fast`; compare with `--no-sitemaps`.
