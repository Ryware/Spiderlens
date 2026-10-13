# Contract: command line

## `spiderlens crawl ... --no-store-html`

Crawl without keeping raw HTML (FR-031). Stored in the crawl's
configuration as `extraction.store_html = false`; `pages.html_stored = 0`.

## `spiderlens page <FILE> <URL> [--json]`

Prints everything extracted for one URL of a crawl file: response facts,
titles, description, headings, directives, canonicals, hreflang, links,
images, structured data and content problems. `--json` prints one JSON
object (used by tests and for scripting). Exit 1 if the URL is not in the
file.

## `spiderlens html <FILE> <URL> [--out PATH]`

Writes the stored raw HTML of one page, byte-identical to the received body
(FR-030, US6.1), to stdout or `PATH`. Exit 1 with a message if the crawl
did not store HTML for it.
