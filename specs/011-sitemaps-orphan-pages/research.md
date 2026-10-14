# Research: Sitemaps and Orphan Pages

## R1. Streaming versus whole-file parsing

**Decision**: keep reading one sitemap's bytes into memory (bounded at 50 MB by the protocol and by `MAX_SITEMAP_BYTES`), but parse with a callback per entry instead of building a `Vec`.
**Why**: the fetch layer already buffers bodies; a true network-streaming parser would need an async XML reader and buys at most 50 MB per file, one file at a time. The callback removes the second copy (the `Vec<String>` of every `<loc>`, which for 50,000 long URLs plus entry data is the larger cost once entries carry `lastmod`, images and alternates).
**Rejected**: async streaming XML over the response body (complexity, a new dependency); a DOM parser (2 to 5 times the file size in memory).

## R2. Where entries are matched to URLs

**Decision**: at ingestion, through the same `discover()` call that queues sitemap URLs today (plan D2).
**Why**: the runner already normalizes and looks up every `<loc>`; storing the resulting `url_id` costs one integer per entry and turns every later question into an indexed join. Matching later by URL text would scan or index a million strings twice.

## R3. W3C datetime parsing

**Decision**: a small parser for the six W3C datetime forms the sitemap protocol allows (`YYYY`, `YYYY-MM`, `YYYY-MM-DD`, then with `Thh:mmTZD`, `Thh:mm:ssTZD`, `Thh:mm:ss.sTZD`), validating ranges (month 1 to 12, day by month and leap year, hour 0 to 23, offset within ±14:00), returning UTC seconds. Dates without a time are taken at 00:00 UTC.
**Why**: `chrono` is not in the tree and `time`'s RFC 3339 parser rejects the date-only forms Google accepts. About 40 lines plus property tests against `time` for the full form.

## R4. The location rule

**Decision**: an entry is "outside the sitemap's location" when its scheme and host differ from the sitemap file's final URL, or its path is not under the file's directory, unless the entry's host robots.txt (fetched in this crawl) lists that sitemap file in a `Sitemap:` line (the protocol's cross-submission rule). For local files the base URL the user gives stands in for the file's URL. `www.` and bare host count as different here, unlike scope (spec 003), because search engines treat them as different hosts for sitemaps.
**Why**: this is the sitemaps.org rule that Google and Bing still apply; robots.txt cross-listing is the only exception they document.

## R5. What counts as a link for orphans

**Decision**: followed `anchor` links (including `<area>`, spec 005) from crawled internal HTML pages to a different URL. Redirect and meta refresh targets of linked URLs are reachable (fixpoint in SQL, bounded by the redirect limit). Canonical, hreflang, pagination `<link>` and image references do not count.
**Why**: matches Screaming Frog's orphan definition and what "can a user or crawler navigate here" means; canonicals and hreflang are hints, not navigation. `rel=next/prev` links are `<link>` elements, not navigation either.
**Alternative**: counting nofollow links as links; rejected, the spec lists them as their own class.

## R6. Validating generated files

**Decision**: two layers. A Rust structural validator (root element and namespace, allowed children and their order, required `<loc>`, limits, escaping, extension namespaces) runs in unit tests on all three OSes. The Linux test job also runs `xmllint --noout --schema` against the official XSDs (sitemap.xsd, siteindex.xsd, sitemap-image.xsd, and the XHTML link schema), bundled under `specs/011-sitemaps-orphan-pages/xsd/` so the test never downloads anything.
**Why**: no XSD validator crate is mature enough to add as a dependency; `xmllint` is on the Ubuntu runner (`libxml2-utils`, installed in the job if missing) and is the reference most SEO tools use.

## R7. Deterministic gzip

**Decision**: `flate2::GzBuilder` with mtime 0, no file name, OS byte fixed, default compression level.
**Why**: SC-006 requires byte-identical output, including `.gz` files; the gzip header otherwise embeds the time.

## R8. Rules with no URL row

**Problem**: spec 009 rules are conditions over `urls u`. Invalid `<loc>` entries and file-level problems have no URL row.
**Decision**: those rules are site-wide (009 `kind: SiteWide`) with a count from `sitemap_problems`, and an optional `detail` view id in the rule (`"sitemap-files"`) that the Issues area opens instead of the URL grid. Requested from the 009 thread as a one-field addition to the rule shape; until it lands, the Issues area shows the count and the Sitemap files view lists the items.

## R9. Default sitemap locations

**Decision**: `/sitemap.xml`, then `/sitemap_index.xml`, on the start host only, only when robots.txt lists no sitemap, stopping at the first that returns a sitemap.
**Why**: these two cover the defaults of the common CMSs (WordPress core and Yoast, Shopify, Wix, Squarespace use one of them). More guesses cost requests on every crawl for a small gain; users can add others by hand.

## R10. Old setting key

**Decision**: accept `use_sitemaps` on read and map it to both new flags; always write the new shape. A crawl file stores the config it ran with, so resuming an old crawl keeps its behaviour.
**Why**: FR-001; same compatibility approach as spec 002 profile versions.
