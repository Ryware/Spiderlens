# Quickstart: Custom Extraction and Custom Search

Uses the local fixture site (`cargo run -p spiderlens-test-fixtures --bin fixture-site`)
pages under `/custom/`.

1. Settings › Custom rules › Extraction › Add: name "Price", CSS `.product-price`, Text.
   Add "SKU", XPath `//meta[@itemprop="sku"]/@content`. Add "Reviews", JSON-LD,
   type `Product`, path `aggregateRating.reviewCount`.
2. Search › Add: "Consent script", `consent-v2.js`, does not contain, HTML.
3. Test › pick `/custom/product-1.html`: Price "$49.00", SKU "AB-123", Reviews "12",
   Consent script 0 (meets).
4. Crawl the fixture site. Custom extraction tab: three columns filled live.
   Custom search tab › "Consent script: does not contain": the 7 expected pages.
5. Change Price to `.price-now`, Run on this crawl: progress, then new values;
   other columns unchanged; no requests in the fixture log.
6. CLI: `spiderlens rules run crawl.sqlite --rules rules.json` gives the same values;
   `spiderlens export crawl.sqlite --tab custom_extraction --all-values " | "`.
7. Crawl with Store page HTML off: Run on this crawl is disabled with the reason.
