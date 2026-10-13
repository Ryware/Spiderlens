# Spec backlog and numbering

One row per spec number. Pick the next free number from this file before
starting a spec, and add your row in the same PR, so two threads never take
the same number. Gaps against other SEO crawlers are tracked in the feature
gap report from 2026-10-13; every gap now has a number below.

Status values:

- **merged**: spec on `main`, code merged or merging.
- **approved**: spec approved by the owner, plan or code in progress.
- **in progress**: spec being written or waiting for approval.
- **planned**: number reserved, spec not started yet.

| # | Spec | Covers | Status |
|---|---|---|---|
| 000 | Constitution | Principles every spec follows (`.specify/memory/constitution.md`) | merged |
| 001 | Project foundation | Repo, CI, installers, CLI skeleton | merged |
| 002 | Crawl configuration | Start URL, Spider and List modes, limits, include/exclude, user agent, headers, profiles | merged |
| 003 | Frontier and scope | Queue, URL normalization, depth, robots.txt, sitemaps from robots.txt as seeds | merged |
| 004 | Fetching, errors and retries | Error classes, retries with backoff, Retry-After, per-host pause, `spiderlens retry` | merged (code merging) |
| 005 | Page extraction | Titles, descriptions, headings, directives, links, images, hreflang, structured data, stored HTML | merged |
| 006 | Crawl storage and files | Crash-safe crawl files, save, open, resume, migrations, recent crawls | merged (code merging) |
| 007 | Results interface | Grid, tabs, filters, inspector, CSV export | merged |
| 008 | Crawl speed and performance | Per-host adaptive limits, compression, batched writes, benchmarks | merged |
| 009 | Issue rules engine | Rule format, priorities, explanations, first rule set (security, hreflang, canonicals, duplicates, pixel width) | in progress |
| 010 | End-of-crawl report and exports | Report screen, health score, HTML/XLSX/CSV, bulk exports | in progress |
| 011 | Sitemaps and orphan pages | Sitemap discovery and audit, orphan pages, XML and image sitemap generation | in progress |
| 012 | Crawl comparison | Diff two crawls, change detection, staging vs production URL mapping | in progress |
| 013 | Custom extraction and search | XPath, CSS path and regex extraction; source code search | in progress |
| 014 | JavaScript rendering | Headless Chromium, raw vs rendered, screenshots, JS errors | approved (plan merging, code started) |
| 015 | AEO and GEO checks | AI crawler access, llms.txt, schema coverage, answer readiness, structured data validation | in progress |
| 016 | AI agents | Local MCP server, LLM scoring, custom AI prompts per page | in progress |
| 017 | Design system and app shell | Three-pane shell, tokens, settings sheet | merged |
| 018 | Appearance and themes | Brightness steps, accent palettes, follow system | merged |
| 019 | Crawl data management and cleanup | Manage crawls, delete to trash, remove stored HTML, auto cleanup, storage reserve | approved |
| 020 | Near duplicate content | Near-match similarity, semantic similarity with embeddings | in progress |
| 021 | Scheduled crawls | Recurring crawls with automatic exports | in progress |
| 022 | Site visualisations | Crawl tree, directory tree, force-directed graphs, word clouds | in progress |
| 023 | Link Score | Internal link score, depth and inlink analysis | in progress |
| 024 | External integrations | Search Console, Google Analytics 4, PageSpeed Insights, link metrics (Ahrefs, Majestic, Moz), Looker Studio | in progress |
| 025 | Site login, cookies and proxy | Forms-based login, Basic/Digest/NTLM auth, cookie storage and cookie audit, proxy | in progress |
| 026 | Accessibility and mobile | Accessibility rules (WCAG) and mobile usability on rendered pages | in progress |
| 027 | Spelling, PDF, AMP, SERP and segmentation | Spelling and grammar, PDF crawling, AMP validation, SERP mode, URL segments | in progress |

Next free number: **028**.
