# Quickstart: Crawl Report and Exports

1. Crawl the report fixture site: `cargo test -p spiderlens-results --test report` builds it and checks every figure and the score.
2. In the app, finish any crawl: a toast offers "View report"; the sidebar has Report above the tabs. Click a count, the results table opens on exactly those URLs.
3. Export › Report as HTML: open the file with networking off; every section is there. Export › Report as PDF: the system print dialog opens with Save as PDF.
4. Export › Bulk export: pick tables and Excel; the workbook opens with an Overview sheet.
5. CLI:
   ```
   spiderlens report crawl.sqlite --audit
   spiderlens report crawl.sqlite --audit --json | jq .score
   spiderlens report crawl.sqlite --html report.html --title "Client audit"
   spiderlens export crawl.sqlite --bulk all --format xlsx --out audit.xlsx
   spiderlens export crawl.sqlite --bulk links,issues --format csv --out audit-csv
   ```
