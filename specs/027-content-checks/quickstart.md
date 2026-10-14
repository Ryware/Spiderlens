# Quickstart: Content Checks (spec 027)

Validation scenarios per part, run against local fixture sites (`crates/test-fixtures`), never live sites.

## A SERP preview
1. `spiderlens serp crates/test-fixtures/data/serp/reference.csv --format csv` → every width within 2% of the reference column, every truncation flag equal (SC-A1).
2. Open a pre-027 crawl file → Page titles tab shows Title px and truncation without recrawl (A1.3).
3. Select a row → Search preview shows desktop and mobile; type a longer title → width and cut point update under 100 ms (A2.2); export proposals → CSV row has current and proposed values (A2.3).

## B Segments
1. Load the segments fixture crawl, add "Blog" (path contains `/blog/`) and "Shop" (path contains `/shop/`) → Internal tab shows the Segment column and the filter offers Blog, Shop, No segment with counts (B1.1).
2. Move Shop above Blog → a URL under `/shop/blog/` changes segment (B1.2).
3. `spiderlens export <crawl> --tab internal --segment Shop` → only Shop URLs (B3.3).

## C PDF
1. `spiderlens crawl http://127.0.0.1:<port>/ --crawl-pdfs` on the PDF fixture → PDFs tab values equal `expected.json` (SC-C1); scanned, encrypted, damaged and oversized fixtures each show their status.
2. Same crawl without `--crawl-pdfs` → PDFs have response facts only (C1.6).
3. Benchmark fixture 10k pages + 1k PDFs → HTML pages per second within 10% (SC-C2).

## D Spelling
1. `spiderlens spell dictionaries add-file he.aff he.dic`, then `spiderlens spell <crawl>` on the spelling fixture → exactly the planted English and Hebrew errors (SC-D1), prefixed Hebrew words accepted, niqqud text accepted.
2. Crawl with `--spelling` → fetch rate within 5% of the same crawl without it (SC-D3).

## E AMP
1. `spiderlens crawl ... --check-amp` on the AMP fixture → each check fails exactly on its fixture page; mismatched pair reported (SC-E1).
