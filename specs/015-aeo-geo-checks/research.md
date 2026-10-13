# Research: AEO and GEO Checks (spec 015)

Research done on 2026-10-13. Vendor pages change often; the AI crawler list (FR-006, FR-007) carries its own version and date so it can be updated without a spec change.

## R1. Known AI crawlers (first version of the built-in list)

"Honours robots.txt" is what the vendor documents. "Runs JavaScript" is from vendor documentation where stated, otherwise from the Vercel and MERJ log study (December 2024): none of the major AI crawlers rendered JavaScript; ChatGPT and Claude crawlers download script files but do not execute them; Gemini (through Googlebot) and Applebot render.

| Vendor | Robots token / user agent | Purpose | Honours robots.txt | Runs JavaScript | IP ranges |
|---|---|---|---|---|---|
| OpenAI | GPTBot | Model training | Yes | No | openai.com/gptbot.json |
| OpenAI | OAI-SearchBot | ChatGPT search index (robots.txt changes take about 24 h) | Yes | No | openai.com/searchbot.json |
| OpenAI | ChatGPT-User | User-requested fetch | "robots.txt rules may not apply" | No | openai.com/chatgpt-user.json |
| OpenAI | OAI-AdsBot | Checks ad landing pages | Not stated | Unknown | openai.com/adsbot.json |
| Anthropic | ClaudeBot | Model training (supports Crawl-delay) | Yes | No | claude.com/crawling/bots.json |
| Anthropic | Claude-SearchBot | Search index | Yes | Unknown | same |
| Anthropic | Claude-User | User-requested fetch | Yes (all Anthropic bots) | Unknown | same |
| Perplexity | PerplexityBot | Search index, not training | Yes | No | perplexity.com/perplexitybot.json |
| Perplexity | Perplexity-User | User-requested fetch | "Generally ignores" | No | perplexity.com/perplexity-user.json |
| Google | Googlebot | Search, including AI Overviews and AI Mode | Yes | Yes | Google's ipranges JSON files |
| Google | Google-Extended (token only) | Gemini training and grounding; does not affect Search | n/a | n/a | n/a |
| Google | Google-CloudVertexBot, GoogleOther | Vertex AI agents; research | Yes | Unknown | Google's ipranges JSON files |
| Apple | Applebot | Spotlight, Siri, Safari; may train | Yes; follows the Googlebot group when no Applebot group exists; ignores Crawl-delay | Yes | search.developer.apple.com/applebot.json |
| Apple | Applebot-Extended (token only) | Training opt-out | n/a | n/a | n/a |
| Microsoft | bingbot | Bing index, which grounds Copilot (no separate Copilot token) | Yes | Yes | bing.com/toolbox/bingbot.json |
| Meta | Meta-ExternalAgent | Training and indexing | Yes | No | Meta docs |
| Meta | Meta-WebIndexer | Meta AI search citations | Yes | Unknown | Meta docs |
| Meta | Meta-ExternalFetcher | User-requested fetch | "May bypass" | Unknown | Meta docs |
| Common Crawl | CCBot | Open dataset widely used for training | Yes | No | index.commoncrawl.org/ccbot.json |
| Amazon | Amazonbot | Products and services, may train; treats `noarchive` as no training | Yes; ignores Crawl-delay | Unknown | Amazon docs |
| Amazon | Amzn-SearchBot | Alexa search, no training | Yes | Unknown | Amazon docs |
| Amazon | Amzn-User | User-requested fetch | "May not follow all" | Unknown | Amazon docs |
| ByteDance | Bytespider | Search and LLM training | Claimed; disputed by third-party measurements | No | none published |
| DuckDuckGo | DuckAssistBot | Real-time fetch for DuckAssist answers, no training | Yes (takes up to 72 h) | Unknown | DuckDuckGo docs |
| Mistral | MistralAI-User, MistralAI-Index, MistralAI-Training | User fetch, search index, training | User and Training yes; Index not stated | Unknown | mistral.ai/mistralai-user-ips.json, mistral.ai/mistralai-index-ips.json |

Consequences for the spec:

- **Three purposes per vendor.** Most vendors now split training, search and user fetch into separate tokens. The most damaging mistake is blocking a search crawler while meaning to block training; the access table groups by purpose for that reason (FR-011).
- **Tokens without crawlers.** Google-Extended and Applebot-Extended never appear in requests. Blocking Google-Extended does not affect AI Overviews, which use Googlebot; there is no way to leave AI Overviews and stay in Search other than snippet controls (R3).
- **Fallbacks.** Applebot follows the Googlebot group when it has none of its own (FR-009).
- **User-requested fetchers** often say robots.txt may not apply (FR-009 label).
- **Compliance is not enforcement.** Cloudflare documented undeclared Perplexity crawlers in August 2025; the overview only states what robots.txt asks.
- **Ownership.** This list is the single definition used by spec 016 (agent access) and spec 009 (issue rules).

## R2. llms.txt

- Proposal at llmstxt.org: a Markdown file at `/llms.txt`. The H1 site name is the only required element; then an optional blockquote summary; optional body text without headings; H2 sections holding link lists (`- [name](url): notes`); an H2 named "Optional" marks skippable links. Pages may offer Markdown versions (`page.md`, `index.html.md`).
- `llms-full.txt` is not in the proposal; it is a convention from documentation platforms. Checked for status, type, size and links only (FR-045).
- Adoption versus use: an Ahrefs study (June 2026, 137,000 sites) found 28% publish llms.txt, but 97% of those files received no requests in May 2026; no major AI platform has committed to reading it. Google says "You don't need to create new machine readable files, AI text files, or markup" for its AI features. Chrome Lighthouse added an "Agentic browsing" category in May 2026 that reports llms.txt server errors (a 404 counts as not applicable).
- Decision: validate it and generate a draft, but give it at most 2 site points and say plainly it is aimed at agents and coding tools (FR-046).

## R3. Other machine-readable AI signals

| Signal | Where | Status | Honoured by |
|---|---|---|---|
| `nosnippet`, `max-snippet`, `data-nosnippet` | robots meta, `X-Robots-Tag`, HTML attribute | Established | Google: keeps content out of AI Overviews and AI Mode as direct input; Apple: keeps content out of AI answers; Bing: may reduce citation quality |
| `noarchive`, `nocache` | robots meta, header | Established | Bing: `noarchive` keeps content out of Copilot answers, `nocache` limits it to URL, title and snippet; Amazon: `noarchive` means no training |
| `data-snippet` | HTML attribute | New (Bing guidelines, February 2026) | Bing: marks text that may be cited |
| `Content-Signal: search, ai-input, ai-train` | robots.txt | Provider convention (Cloudflare, September 2025, served on millions of domains) | Preference only |
| `Content-Usage` (IETF AI preferences) | robots.txt line and HTTP header | Internet-Draft (draft-ietf-aipref-attach-05, August 2026) | Not yet a standard |
| TDMRep | `/.well-known/tdmrep.json`, `tdm-reservation` and `tdm-policy` headers and meta | W3C Community Group report | EU text and data mining reservations |
| RSL | `License:` in robots.txt, `rel="license"` of type `application/rsl+xml` | RSL 1.0 (December 2025) | Backed by CDNs and publishers; no AI vendor named |
| `noai`, `noimageai` | robots meta | Informal (2022) | No major vendor documents it |
| HTTP 402 with crawl price | Response to AI user agents | Pay-per-crawl services | Visible only through the user agent probe (FR-012) |

## R4. What evidence says about being cited

Evidence-backed (basis "vendor documentation" or "published research"):

- **Access and indexing come first.** Google: pages must be indexed and snippet-eligible to appear in AI features, with no extra requirements; Bing: clear facts near the top, consistent entity names, one topic per URL.
- **Server-rendered content.** Most AI crawlers do not run JavaScript (R1). This makes the raw versus rendered comparison of spec 014 the strongest single content check, which is why "Readable without JavaScript" carries 25 points.
- **Facts, quotations and sources.** The GEO study (Aggarwal et al., KDD 2024) measured visibility gains in generative answers of about 41% for adding quotations, 31% for statistics, 27% for citing sources and 28% for fluency, and a loss of about 8% for keyword stuffing. It is a lab benchmark; results vary by domain.
- **Freshness.** Seer Interactive (July 2026, 7,683 pages cited by ChatGPT, Gemini and Perplexity): 75% of cited pages had been updated within a year; the update date matters more than the publish date. Correlational.
- **Crawl hygiene.** About 34% of ChatGPT and Claude crawler fetches hit 404 pages and 14% of ChatGPT's fetches hit redirects (Vercel): broken links, redirects and canonicals matter, and are already covered by specs 003 to 005.

Weaker evidence (basis "common practice"): answer-first paragraphs (partly supported by Bing's "key information near the top"), question headings, lists and tables, FAQ sections, author markup, Organization and Person `sameAs`. The spec presents them as good practice and shows the basis on every check (FR-038).

Structured data: an Ahrefs controlled study (August 2025 to March 2026, 1,885 pages against 4,000 controls) found no major citation uplift on any platform. Google requires structured data to match visible content. So this spec scores structured data for validity and agreement with the page (10 points), not as a lever.

Types with required and recommended property rules in the first release (FR-031): Organization, LocalBusiness, Person, WebSite, WebPage, Article, NewsArticle, BlogPosting, BreadcrumbList, FAQPage, QAPage, HowTo, Product, Offer, AggregateRating, Review, Event, Recipe, VideoObject, SoftwareApplication. The rules follow Schema.org and Google's structured data documentation; the bundled vocabulary is a Schema.org release, recorded with its version.

## R5. Other tools (positioning)

- **Screaming Frog** (v21 to v24, November 2024 to May 2026): OpenAI, Gemini and Ollama prompts per page, embeddings and semantic similarity, an MCP server. No dedicated AI crawler, llms.txt or AEO checks in the release notes.
- **Sitebulb** v10 (September 2026): MCP server and skills; no AI readiness hints.
- **Semrush**: an "AI Search Health" score (robots.txt blocking of 8 AI bots, schema, crawlable navigation, llms.txt).
- **Ahrefs Brand Radar**: tracks brand mentions and citations in AI answers; monitoring, not an on-site audit.
- **Lumar Agentic Readiness, squirrelscan**: newer web auditors covering llms.txt, content without JavaScript, AI crawler rules, content signals, TDMRep, RSL, pay-per-crawl.

Gap SpiderLens fills: no desktop crawler runs per-crawler, per-purpose access checks across the whole site, combined with a real raw versus rendered comparison, AI usage signals, llms.txt validation and freshness and entity checks, all offline and explained. LLM-based judging (Screaming Frog's approach) stays in spec 016, opt-in.

## R6. Design notes for planning (not requirements)

- **Where checks run.** A post-crawl analysis step over the crawl file, in parallel across cores, writing its own tables; never in the fetch loop (FR-001). Re-parsing stored HTML for structure checks costs roughly the extraction parse rate of spec 005 (about 3,300 pages per second on one core), so a 100,000-page crawl fits SC-002 on a multi-core laptop.
- **Rules as data.** Each check is a rule in the spec 009 shape, plus part, points and basis; the score is a pure function of rule outcomes and the scoring profile, which makes SC-006 and SC-007 testable.
- **Interface with spec 014.** Read-only use of its stored JavaScript dependence values; when absent, the raw-only heuristic of FR-017.
- **Storage.** One additive migration step (expected Step 7), nullable columns, per the storage feature's rule.

## Decisions for the owner

Each has a default the spec already assumes.

1. **User agent probe (FR-012)**: (a) included, off by default, explicit start *(recommended: it is the only way to see firewall blocks and pay-per-crawl 402s)*; (b) left out, because it sends other vendors' user agent strings.
2. **Score weights**: (a) Access 30, Readable without JavaScript 25, Answer readiness 20, Trust and freshness 15, Structured data 10 *(recommended, follows the evidence in R4)*; (b) equal parts of 20.
3. **llms.txt**: (a) validate, generate a draft, at most 2 site points *(recommended)*; (b) validate only, no generator.
4. **Languages for question and date detection**: (a) English, Hebrew, Spanish, French, German, Italian, Portuguese, Dutch *(recommended)*; (b) English only first.

## Sources

- OpenAI crawlers: https://developers.openai.com/api/docs/bots
- Anthropic crawlers: https://support.claude.com/en/articles/8896518
- Perplexity crawlers: https://docs.perplexity.ai/guides/bots
- Google, AI features and your website: https://developers.google.com/search/docs/appearance/ai-features
- Google common crawlers and Google-Extended: https://developers.google.com/search/docs/crawling-indexing/google-common-crawlers
- Google, verifying Googlebot and IP ranges: https://developers.google.com/search/docs/crawling-indexing/verifying-googlebot
- Applebot: https://support.apple.com/en-us/119829
- Meta crawlers: https://developers.facebook.com/docs/sharing/webmasters/web-crawlers/
- Common Crawl CCBot: https://commoncrawl.org/ccbot
- Amazonbot: https://developer.amazon.com/amazonbot
- DuckAssistBot: https://duckduckgo.com/duckduckgo-help-pages/results/duckassistbot
- Mistral crawlers: https://docs.mistral.ai/robots
- Bingbot: https://ppc.land/bingbot/
- Bytespider: https://ppc.land/bytespider/
- Vercel and MERJ, The rise of the AI crawler: https://vercel.com/blog/the-rise-of-the-ai-crawler
- Cloudflare on undeclared Perplexity crawlers: https://blog.cloudflare.com/perplexity-is-using-stealth-undeclared-crawlers-to-evade-website-no-crawl-directives/
- llms.txt proposal: https://llmstxt.org/
- Ahrefs llms.txt study: https://ahrefs.com/blog/llmstxt-study/
- Lighthouse agentic browsing audit: https://ppc.land/google-adds-llms-txt-to-lighthouse-as-agentic-web-standards-heat-up/
- IETF AI preferences attachment draft: https://datatracker.ietf.org/doc/draft-ietf-aipref-attach/
- Cloudflare Content Signals: https://blog.cloudflare.com/content-signals-policy/
- TDMRep: https://w3c.github.io/cg-reports/tdmrep/CG-FINAL-tdmrep-20240510/
- RSL: https://rslstandard.org/rsl
- noai meta origin: https://techcrunch.com/2022/11/11/deviantart-provides-a-way-for-artists-to-opt-out-of-ai-art-generators
- Google robots meta and AI Mode: https://searchenginejournal.com/google-updates-robots-meta-tag-document-to-include-ai-mode/541371/
- Bing webmaster guidelines: https://www.bing.com/webmasters/help/webmaster-guidelines-30fba23a and https://www.searchenginejournal.com/bing-adds-geo-to-official-guidelines-expands-ai-abuse-definitions/568442
- Pay-per-crawl 402: https://docs.squirrelscan.com/rules/ax/pay-per-crawl
- GEO study (Aggarwal et al.): https://arxiv.org/html/2311.09735v3
- Seer Interactive freshness study: https://www.seerinteractive.com/insights/study-content-recencys-impact-on-ai-visibility-in-2026
- Ahrefs structured data study (summary): https://www.seroundtable.com/study-schema-citations-study-41311.html
- Screaming Frog release history: https://www.screamingfrog.co.uk/seo-spider/release-history/
- Sitebulb release notes: https://sitebulb.com/release-notes
- Semrush AI Search Health: https://www.semrush.com/kb/1601-ai-search-health-audit
- Ahrefs Brand Radar: https://ahrefs.com/blog/new-features-dec-2025
- Lumar Agentic Readiness: https://agentic-readiness.lumar.io/docs/access/ai-pref
