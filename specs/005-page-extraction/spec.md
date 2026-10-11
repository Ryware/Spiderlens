# Feature Specification: Page Data Extraction

**Feature Branch**: `005-page-extraction`

**Created**: 2026-10-11

**Status**: Approved (2026-10-11)

**Input**: User description: "Page data extraction: for every crawled HTML page, capture the SEO data listed in the brief (response facts, title, meta description, robots directives from meta and headers, all headings, canonicals from HTML and headers, hreflang, links with anchor text and rel values and internal/external flag, images with alt text, structured data, word count, and the raw HTML kept for later checks). Non-HTML resources record response facts only. Extraction must cope with malformed HTML the way browsers do."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - See the core on-page facts of every page (Priority: P1)

An SEO consultant crawls a client site. For every HTML page they need the basic facts an SEO audit starts from: the final status code, content type, page size, response time, redirect path, page title, meta description, every heading on the page (H1 to H6) and a word count. These facts appear against each URL in the results (the Page titles, Meta description, H1 and H2 tabs and the per-URL detail pane) and stay in the crawl file so they can be reviewed later.

**Why this priority**: Without these facts the crawler only proves that URLs exist. Titles, descriptions, headings and response facts are the minimum a user expects from an SEO crawler and are what the first usable release (M1) is judged on.

**Independent Test**: Crawl a local fixture site whose pages have known titles, descriptions, headings and body text, then compare the stored values for each page with the expected values.

**Acceptance Scenarios**:

1. **Given** a page with `<title>Red Shoes | Shop</title>`, **When** it is crawled, **Then** the stored title is "Red Shoes | Shop", its length is 16 characters and the title count is 1.
2. **Given** a page with two `<title>` elements, **When** it is crawled, **Then** both titles are stored in document order, the first is marked as the primary title and the title count is 2.
3. **Given** a page with no meta description, **When** it is crawled, **Then** the page is recorded as having no meta description (distinct from having an empty one).
4. **Given** a page with one H1, three H2s and one H4, **When** it is crawled, **Then** all five headings are stored with their level, text and document order, and the H1 and H2 counts are 1 and 3.
5. **Given** a URL that redirects twice before returning 200, **When** it is crawled, **Then** each hop's URL and status code is stored in order, along with the final status, content type, size and response time.
6. **Given** a page whose visible body text contains 250 words plus scripts, styles and hidden template content, **When** it is crawled, **Then** the stored word count is 250.

---

### User Story 2 - Capture every link with its context (Priority: P1)

The user needs to know every link on every page: where it points, its anchor text, its rel values (nofollow, sponsored, ugc and others), and whether it is internal or external. This drives spider-mode discovery, the Internal and External tabs, and later link audits (orphan pages, broken links, nofollowed internal links).

**Why this priority**: Spider mode cannot discover new pages without link extraction, and link data is the backbone of most SEO analysis. It is required for M1.

**Independent Test**: Crawl a fixture page containing a known set of links (relative, absolute, with base element, with rel values, image links, non-web schemes) and compare the stored link list against the expected list.

**Acceptance Scenarios**:

1. **Given** a page at `https://example.com/a/b.html` with `<a href="../c.html">Next page</a>`, **When** it is crawled, **Then** a link to `https://example.com/c.html` with anchor text "Next page" is stored and marked internal.
2. **Given** a link with `rel="nofollow sponsored"`, **When** it is crawled, **Then** the link is stored with both rel values and is flagged as nofollow and sponsored.
3. **Given** a page with `<base href="https://cdn.example.com/x/">` and a relative link `p.html`, **When** it is crawled, **Then** the link resolves to `https://cdn.example.com/x/p.html`.
4. **Given** a link wrapping only an image with alt text "Home", **When** it is crawled, **Then** the link is stored with empty anchor text, the image alt text "Home" as its alternative text, and is marked as an image link.
5. **Given** links with `mailto:`, `tel:` and `javascript:` targets, **When** the page is crawled, **Then** these links are stored as non-crawlable links and are not handed to the crawl queue.
6. **Given** a link to another domain, **When** it is crawled, **Then** it is stored as external according to the crawl's scope settings.
7. **Given** a page whose meta robots contains `nofollow`, **When** it is crawled, **Then** its links are still stored and are marked as coming from a page-level nofollow page, and whether they are followed is left to the crawl's nofollow settings.

---

### User Story 3 - See indexability signals from both HTML and HTTP headers (Priority: P2)

The user needs to know whether each page can be indexed and which URL it declares as canonical. Robots directives can come from meta tags (generic or crawler-specific) or from the `X-Robots-Tag` header; canonicals and hreflang can come from link elements or from the HTTP `Link` header. The user sees all sources side by side in the Directives and Canonicals tabs and in the detail pane, so conflicts are visible.

**Why this priority**: Indexability problems are among the most costly SEO mistakes, and the Directives and Canonicals tabs are part of M1. It ranks below Stories 1 and 2 because the crawl is still useful without it.

**Independent Test**: Crawl fixture pages that set robots directives, canonicals and hreflang through meta tags, link elements and headers (including conflicting combinations), then check every value and its source.

**Acceptance Scenarios**:

1. **Given** a page with `<meta name="robots" content="noindex, follow">` and a response header `X-Robots-Tag: nosnippet`, **When** it is crawled, **Then** "noindex", "follow" (source: meta, scope: all crawlers) and "nosnippet" (source: header, scope: all crawlers) are all stored.
2. **Given** a header `X-Robots-Tag: googlebot: noindex`, **When** the page is crawled, **Then** "noindex" is stored with source header and scope "googlebot".
3. **Given** a page with `<link rel="canonical" href="/p">` in the head and a header `Link: <https://example.com/q>; rel="canonical"`, **When** it is crawled, **Then** both canonicals are stored as absolute URLs with their sources, and the page is flagged as having more than one canonical target.
4. **Given** a canonical link element that appears in the body instead of the head, **When** the page is crawled, **Then** the canonical is stored and marked as found in the body.
5. **Given** a page with three `<link rel="alternate" hreflang="...">` elements including `x-default`, **When** it is crawled, **Then** all three are stored with language code, target URL and source.

---

### User Story 4 - Get correct data from messy real-world HTML (Priority: P2)

Real sites have unclosed tags, missing head or body elements, stray characters, wrong or missing character set declarations, and pages cut off mid-download. The user expects the crawler to read these pages the way a browser does, not to skip them or produce nonsense, and to be told when a page could only be partly read.

**Why this priority**: Results that disagree with what a browser shows destroy trust in the tool. It ranks P2 because well-formed fixture pages already deliver value through Stories 1 to 3.

**Independent Test**: Crawl a fixture set of deliberately malformed pages whose browser-parsed structure is known, and compare the extracted data with what a standards-following browser produces.

**Acceptance Scenarios**:

1. **Given** a page with no `<head>` or `<body>` tags and an unclosed `<title>`, **When** it is crawled, **Then** the title and other data match what a standards-following browser shows for that page.
2. **Given** a page whose head is closed early by stray body content before a `<meta name="description">`, **When** it is crawled, **Then** the description is stored and marked as found outside the head.
3. **Given** a page served without a character set but with `<meta charset="windows-1252">` and accented characters, **When** it is crawled, **Then** the text is stored correctly decoded.
4. **Given** a page containing byte sequences invalid in its declared encoding, **When** it is crawled, **Then** extraction still completes, invalid characters are replaced, and the page is flagged with an encoding content problem.
5. **Given** a page whose body was cut off by the size limit or a dropped connection, **When** it is crawled, **Then** data is extracted from the part received and the page is flagged as partially extracted.

---

### User Story 5 - Record images and structured data (Priority: P3)

The user wants every image on each page with its source and alt text (and whether alt is missing or deliberately empty) for the Images tab, and the raw structured data blocks (JSON-LD and microdata) on each page so later audits can check them.

**Why this priority**: Image alt text and structured data matter for SEO, but the Images tab can ship after titles, links and directives, and structured data validation is an M2 audit.

**Independent Test**: Crawl fixture pages with known images (alt present, empty, missing, responsive sources) and known JSON-LD and microdata, and compare stored values.

**Acceptance Scenarios**:

1. **Given** `<img src="/a.jpg" alt="">` and `<img src="/b.jpg">` on a page, **When** it is crawled, **Then** the first image is stored with an empty alt and the second with alt missing, each with an absolute source URL.
2. **Given** an image with a `srcset` of three candidates, **When** it is crawled, **Then** all candidate URLs are stored with the image.
3. **Given** a page with two JSON-LD blocks, one of which is invalid JSON, **When** it is crawled, **Then** both blocks are stored as raw text in order and the invalid one is flagged as not parseable.
4. **Given** a page with microdata describing a Product with name and price, **When** it is crawled, **Then** the item type and its properties and values are stored.

---

### User Story 6 - Keep the raw page and handle non-HTML resources (Priority: P3)

The user wants the raw HTML of each page kept inside the crawl file so that later checks (M2 audits, M5 AEO/GEO checks) can be run on an existing crawl without recrawling. For resources that are not HTML (images, PDFs, CSS, scripts and so on) the user only needs the response facts.

**Why this priority**: It protects the value of every crawl for later milestones but gives no visible benefit in M1.

**Independent Test**: Crawl a fixture site with HTML pages and non-HTML files, then retrieve the stored HTML for a page and compare it byte for byte with the served content, and check that non-HTML resources have response facts only.

**Acceptance Scenarios**:

1. **Given** a crawled HTML page, **When** the user (or a later check) requests its stored HTML, **Then** the content returned is identical to the decoded response body that was received.
2. **Given** a PDF returned with `X-Robots-Tag: noindex` and a `Link` canonical header, **When** it is crawled, **Then** its response facts, header robots directive and header canonical are stored, and no page content extraction is attempted.
3. **Given** the user has turned off raw HTML retention in the crawl settings, **When** pages are crawled, **Then** all other extracted data is stored and no raw HTML is kept.

---

### Edge Cases

- A response labelled as HTML whose body is actually binary or empty: response facts are stored, extraction yields empty results, and the page is flagged as having no extractable content rather than failing.
- A response with no content type: the content is classified by inspecting its first bytes; if it looks like HTML it is extracted.
- An XHTML page served as `application/xhtml+xml`: treated as HTML for extraction.
- Very large pages (for example 100,000 links or a 50 MB document within the size limit): all data is extracted without the app's memory growing in proportion to the crawl; per-page memory is bounded by the configured size limit.
- Extremely long values (a 50,000-character title or link): stored in full; display truncation is the UI's concern.
- Text containing character references (`&amp;`, `&#233;`) and runs of whitespace or line breaks: decoded and whitespace-collapsed in stored text, while raw HTML keeps the original.
- A link whose href is empty, only a fragment (`#top`), or cannot be parsed as a URL: stored with a reason (self link, fragment-only, malformed) and not handed to the crawl queue as a new URL; malformed ones are reported as malformed URLs per the error rules.
- Several `<base>` elements: the first one with an href is used, as browsers do.
- A meta refresh (`<meta http-equiv="refresh" content="0;url=/new">`): the delay and target are stored and the target is handed to discovery like a link.
- Robots directives written in mixed case, with extra spaces, or with unknown values: known directives are recognised case-insensitively; unknown ones are kept as raw values.
- Several `X-Robots-Tag` headers, or several directives in one header: all are stored.
- Content inside `<noscript>`, `<template>`, comments or HTML-escaped text: comments and template content are ignored; noscript content is treated as a browser with scripting disabled would treat it (its links and images are extracted and marked as coming from noscript).
- Pages that rely on JavaScript to add content: only the content in the HTML as served is extracted; rendered content is out of scope until M4.
- A page that is both redirected and has a canonical: the redirect hops and the final page's canonical are both stored.
- Duplicate links (same target and anchor text) on one page: every occurrence is stored in document order with its position, so counts are correct.

## Requirements *(mandatory)*

### Functional Requirements

**Response facts (all resources)**

- **FR-001**: The system MUST record for every fetched resource: the requested URL, the final URL, the final status code, the content type as declared, the detected resource type (HTML or other), body size as received and after decoding, response time, and the date and time of the fetch.
- **FR-002**: The system MUST record each redirect hop in order with its URL, status code and redirect target, including meta refresh redirects.
- **FR-003**: The system MUST record, for every resource including non-HTML ones, the robots directives from all `X-Robots-Tag` headers and any canonical and hreflang entries from HTTP `Link` headers.
- **FR-004**: For non-HTML resources, the system MUST record only response facts (FR-001 to FR-003) and MUST NOT attempt page content extraction.
- **FR-005**: The system MUST classify a response as HTML when its declared content type is an HTML or XHTML type, or, when no content type is declared, when its first bytes identify it as HTML.

**Parsing and text handling**

- **FR-006**: The system MUST parse HTML following the standard HTML parsing rules that browsers implement, so that malformed markup (unclosed or misnested tags, missing head or body, stray content) produces the same document structure a standards-following browser produces.
- **FR-007**: The system MUST determine the character encoding in the order browsers use (byte order mark, then HTTP header, then in-document declaration, then a default) and decode text accordingly.
- **FR-008**: When content cannot be fully decoded or was truncated, the system MUST still extract what it can, replace invalid characters, and flag the page with the matching content problem (encoding or truncated) as defined by the crawl error rules.
- **FR-009**: The system MUST store extracted text with character references decoded, leading and trailing whitespace removed and internal whitespace collapsed to single spaces.
- **FR-010**: For title, meta description, meta robots, canonical and hreflang elements, the system MUST record whether each was found in the document head or outside it.

**On-page content**

- **FR-011**: The system MUST store every title element's text in document order, the primary (first) title, its length in characters and the title count.
- **FR-012**: The system MUST store every meta description's content in document order, the primary one, its length in characters and the count, and MUST distinguish "missing" from "present but empty".
- **FR-013**: The system MUST store every heading (H1 to H6) with its level, text and document order, plus a count per level.
- **FR-014**: The system MUST compute a word count from the text a user would see in the page body, excluding script, style, template and comment content, and counting words as runs of characters separated by whitespace or punctuation.
- **FR-015**: The system MUST store the page's declared language (the root element's lang attribute) and a fingerprint of the page's visible text, so later audits can find exact duplicate content.

**Indexability signals**

- **FR-016**: The system MUST store robots directives from meta tags named `robots` and from crawler-specific meta tags (for example `googlebot`, `bingbot`), recording for each directive its value, source (meta or header) and the crawler it applies to (all crawlers or a named one).
- **FR-017**: The system MUST recognise standard robots directives case-insensitively (including index, noindex, follow, nofollow, none, all, noarchive, nosnippet, noimageindex, notranslate, max-snippet, max-image-preview, max-video-preview, unavailable_after) and MUST keep unrecognised directives as raw values.
- **FR-018**: The system MUST store every canonical declaration from link elements and from HTTP `Link` headers as an absolute URL with its source, and MUST flag pages whose canonical declarations point to more than one distinct URL.
- **FR-019**: The system MUST store every hreflang declaration from link elements and from HTTP `Link` headers with its language code (including `x-default`), absolute target URL and source.

**Links**

- **FR-020**: The system MUST extract every link from anchor and image-map area elements, recording its resolved absolute URL, the href exactly as written, anchor text, the alt text of any image inside the link, all rel values, its position in document order, and whether it sits inside noscript content.
- **FR-021**: The system MUST resolve relative URLs against the document's base URL, taking the first `<base href>` into account, as browsers do.
- **FR-022**: The system MUST flag each link as nofollow, sponsored and/or ugc based on its rel values, and MUST record when the source page carries a page-level nofollow directive.
- **FR-023**: The system MUST mark each link as internal or external using the crawl's scope rules (subdomain handling as defined by the frontier and scope feature).
- **FR-024**: The system MUST mark links with non-web schemes (such as mailto, tel, javascript, data), empty or fragment-only hrefs, and unparseable hrefs as not crawlable with a reason, and MUST NOT hand them to the crawl queue as new URLs.
- **FR-025**: The system MUST hand every crawlable link target, canonical target, hreflang target, meta refresh target, image source and other discovered resource URL to URL discovery, which decides under the crawl's scope and follow rules whether to queue it.
- **FR-026**: The system MUST also record link elements in the head with rel values next and prev, along with their targets.

**Images and structured data**

- **FR-027**: The system MUST record every image element with its resolved source URL, all `srcset` candidate URLs, its alt text, whether alt is missing, empty or present, any declared width and height, and whether the image is inside a link.
- **FR-028**: The system MUST store every JSON-LD block as raw text in document order and flag whether each one is parseable as JSON.
- **FR-029**: The system MUST store microdata items with their item type, item id when present, and their properties and values, including nested items, as written in the page.

**Raw HTML retention**

- **FR-030**: By default the system MUST keep each HTML page's received body in compressed form inside the crawl file, retrievable byte for byte identical to what was received after transfer decoding.
- **FR-031**: Users MUST be able to turn off raw HTML retention in the crawl settings; the setting MUST be stored with the crawl, and pages crawled with it off MUST be marked as having no stored HTML.

**Behaviour and quality**

- **FR-032**: Extraction MUST be deterministic: the same response always produces the same stored data.
- **FR-033**: A failure while extracting one page MUST NOT stop the crawl; the page MUST keep its response facts and be flagged with a content problem describing the failure.
- **FR-034**: Extraction MUST work identically whether the crawl is run from the desktop app or the command line.
- **FR-035**: Extraction MUST use only the content as served; it MUST NOT run page scripts or fetch additional resources to build the page.

### Key Entities

- **Resource Response**: One fetched URL's response facts: requested and final URL, status, content type, detected resource type, sizes, response time, fetch time, content problem flags, whether raw HTML is stored.
- **Redirect Hop**: One step in a redirect path: from URL, status code, to URL, kind (HTTP or meta refresh), order.
- **Page Content**: Extracted on-page data for an HTML page: titles, meta descriptions, language, word count, text fingerprint, partial-extraction flag, element-location flags.
- **Heading**: Level (1-6), text, document order, belongs to one page.
- **Robots Directive**: Directive value, source (meta or header), crawler scope, belongs to one resource.
- **Canonical Declaration**: Target URL, source (link element or header), location (head or body), belongs to one resource.
- **Hreflang Declaration**: Language code, target URL, source, belongs to one resource.
- **Link**: Source page, target URL as written and resolved, anchor text, image alt text, rel values and nofollow/sponsored/ugc flags, internal or external, crawlable or reason not, element kind, position, noscript flag.
- **Image**: Source page, resolved source URL, srcset candidates, alt text and alt state (missing, empty, present), declared dimensions, linked or not.
- **Structured Data Block**: Kind (JSON-LD or microdata), raw content or item tree, parseable flag, order, belongs to one page.
- **Stored Page Body**: The compressed received HTML for one page.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On the extraction fixture suite (well-formed pages), 100% of expected values for titles, meta descriptions, headings, directives, canonicals, hreflang, links, images and structured data match the stored values.
- **SC-002**: On a fixture set of at least 50 malformed pages, the extracted title, meta description, headings, canonical and link list match what a standards-following browser produces for at least 98% of pages, and no page causes the crawl to stop.
- **SC-003**: On the local 100,000-page fixture site, the crawl still sustains at least 500 URLs per second with extraction and raw HTML retention turned on, and memory use stays flat as the crawl grows.
- **SC-004**: A single 50 MB HTML page with 100,000 links is fully extracted in under 10 seconds on the reference machine defined in spec 008, and its links are all stored.
- **SC-005**: Stored raw HTML for 100% of sampled pages is byte-for-byte identical to the received body, and the stored form takes on average no more than 30% of the original size on the fixture site.
- **SC-006**: Word counts on a fixture set of pages with known counts are within 1% of the expected value for space-separated languages.
- **SC-007**: Every crawled resource has response facts recorded; 0 HTML pages end the crawl without either extracted content or a recorded content problem.

## Assumptions

- This feature covers extraction during M1 crawls (Spider and List modes). Fetching, retries and error classes come from the fetch and error-handling feature; scope, normalization and follow rules come from the frontier and scope feature; the crawl file comes from the storage feature; display in tabs and the detail pane comes from the results UI feature. This spec only defines what data is extracted and stored.
- Header-derived robots directives and canonicals (`X-Robots-Tag`, `Link`) are treated as response facts, so they are recorded for non-HTML resources such as PDFs, as search engines honour them there.
- Raw HTML retention is on by default as decided for later AEO/GEO checks; an opt-out is provided because it is the largest contributor to crawl file size on very large crawls.
- Word counting splits on whitespace and punctuation. For languages written without spaces (for example Chinese or Japanese) the count is an approximation; accurate segmentation for those languages is out of scope for M1.
- The page title and meta description lengths are in characters. Pixel-width measurement, as used in search result previews, is out of scope (later milestone).
- Microdata is stored as written; RDFa extraction is out of scope for M1. Validating structured data against vocabularies is an M2 audit.
- Images are taken from image elements only; images from CSS backgrounds, picture source elements' media rules and lazy-loading attributes set by scripts are out of scope for M1, except that `srcset` candidates are recorded.
- Scripts, stylesheets, iframes and other embedded resources are not extracted as links in M1 beyond what FR-025 hands to discovery; resource-level audits come later.
- Sitemap-declared hreflang and canonical data are out of scope here.
- JavaScript rendering (M4), audits and issue detection (M2), custom extraction rules (later), AEO/GEO analysis (M5) and AI agents (M6) are out of scope.
- Link position is recorded as document order only; classifying links by page region (navigation, footer, content) is out of scope for M1.
