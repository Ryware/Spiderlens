# Contract: what spec 007 (results) reads

The tables and columns in [data-model.md](../data-model.md) are the
contract. Stable names spec 007 can rely on:

| Results need | Where |
|---|---|
| Primary title, title count | `urls.title`, `pages.title_count` |
| All titles in order | `page_text WHERE kind='title' ORDER BY pos` |
| Meta description, count | `pages.meta_description` (NULL missing, '' empty), `pages.meta_description_count` |
| H1 / H2 first, second, counts | `pages.h1_1`, `h1_2`, `h1_count`, `h2_1`, `h2_2`, `h2_count` |
| All headings | `page_text WHERE kind='heading' ORDER BY pos` |
| Word count, language | `pages.word_count`, `pages.lang` |
| Duplicate title / description / content | `pages.title_hash`, `pages.meta_description_hash` (indexed), `pages.text_hash`; xxh3-64 of the stored text, case-sensitive |
| Canonical(s) with source | summary `urls.canonical`, `canonical_count`, `canonical_conflict`; all of them `links WHERE kind='canonical'` with `source` (`html`/`header`) and `in_head` |
| Meta robots and X-Robots-Tag | `directives` (`source` = `meta`/`header`, `crawler`); summary `urls.noindex`, `urls.nofollow` |
| Images | `links WHERE kind='image'` (`alt` NULL = missing, '' = empty, `width`, `height`, `srcset`, `in_link`), joined to `urls` by `to_id` for status and size |
| Anchor text, rel flags | `links.text`, `links.rel`, `nofollow`, `sponsored`, `ugc` |
| Response headers | `headers.headers` (JSON) |
| Response time, size | `urls.response_ms`, `urls.size_bytes` (`transfer_bytes` when known) |
| Raw HTML | `bodies` (zstd); read through the engine function `CrawlStore::page_html(url_id)`, not by hand |
| Redirect hops | `urls.redirect_to` chain with `urls.redirect_kind` |
| Extraction present | `meta.extraction_version` |
| Content problems | `pages.partial`, `encoding_errors`, `no_content`, `extract_error` |
