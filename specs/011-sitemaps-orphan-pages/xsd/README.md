# Sitemap schemas for tests

Used by `crates/results/tests/sitemap_generate.rs` to check generated files
with `xmllint` (research R6). Nothing here is downloaded at test time.

| File | Source |
| --- | --- |
| `sitemap.xsd` | The official sitemaps.org schema for `<urlset>` (last modified 2008-03-26), as shipped in the MIT-licensed `sitemap` npm package 8.0.0 (`schema/sitemap.xsd`). Unchanged. |
| `siteindex.xsd` | Written from the sitemaps.org protocol for index files; same structure as the official `siteindex.xsd`. |
| `sitemap-image.xsd` | Written from Google's image sitemap documentation. |
| `xhtml-link.xsd` | Declares only `<xhtml:link rel="alternate" hreflang href>`, the element sitemaps use for hreflang. |
| `urlset-check.xsd` | Imports the three above, for checking `<urlset>` files. |
