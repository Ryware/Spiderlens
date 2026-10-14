# Provider facts for spec 024 (checked 2026-10-13)

These are the published limits and access rules the spec's defaults rely on. They change; the plan re-checks them, and the values live in configuration.

| Provider | Access | Auth | Published limits | Notes |
|---|---|---|---|---|
| Search Console, Search Analytics | Free, user must have access to the property | OAuth, scope `webmasters.readonly` | 1,200 queries/min per site and per user; 40,000/min and 30M/day per project | Up to 25,000 rows per request, page with `startRow`; `dataState` `final` (default) or `all` for fresh data; `aggregationType=byPage` groups by canonical URL; about 16 months of history |
| Search Console, URL Inspection | Free | OAuth, `webmasters.readonly` | 2,000/day and 600/min per site; 10M/day and 15,000/min per project | Returns verdict, coverage state, Google and user canonical, last crawl time, crawled as, robots.txt state, page fetch state, rich results |
| Analytics Data API (GA4) | Free | OAuth, scope `analytics.readonly` | Standard property, Core category: 200,000 tokens/day, 40,000 tokens/hour, 14,000 tokens per project per property per hour, 10 concurrent requests, 10 server errors per project per property per hour | `runReport` with `landingPagePlusQueryString` or `landingPage` |
| Chrome UX Report API (field data) | Free | API key | 150 queries/min per Cloud project, cannot be raised | 28-day window, about two days behind, updates daily around 04:00 UTC; `url` or `origin`; `formFactor` PHONE/TABLET/DESKTOP; LCP, INP, CLS, FCP, TTFB |
| PageSpeed Insights API v5 (lab) | Free | API key recommended for automated use | 25,000/day, 400 per 100 s per project (default); daily reset midnight Pacific | Each call runs Lighthouse remotely, slow per call; categories performance, accessibility, best-practices, seo; strategy mobile or desktop |
| Google Sheets API | Free | OAuth, `drive.file` (only files the app creates) | 300 requests/min per project, 60/min per user per project; no daily limit | Batch requests count once; keep payloads around 2 MB |
| Looker Studio Linking API | Free | None (URL opened in the user's browser, uses their Google account) | n/a | `reporting/create?ds.connector=googleSheets&ds.spreadsheetId=…&ds.worksheetId=…`; template report optional |
| Ahrefs API v3 | Eligible paid plans; free test queries only otherwise | API key created by workspace owner or admin | 60 requests/min default; units per request (minimum 50); plan allowances e.g. Lite 100k units | Batch Analysis: up to 100 targets per request |
| Majestic API | Platinum or API subscription | API key (internal use) or OpenApp for third-party apps | Analysis and Index Item units per plan | `GetIndexItemInfo` for URLs and domains (Trust Flow, Citation Flow, backlinks, referring domains); OpenApp vs key-based use needs checking with Majestic for a distributed app |
| Moz Links API | Paid plans from about $20/month, metered in rows per month; small free tier | Access ID and secret key | Rows per month by plan; free tier rate limited | `url_metrics` takes several targets per request (Page Authority, Domain Authority, linking domains, spam score) |

## Google sign-in for a desktop app

- Recommended flow: system browser plus loopback redirect `http://127.0.0.1:<random port>` with PKCE (S256). Custom URI schemes are no longer supported. Installed apps cannot keep a client secret; refresh tokens are always returned and must be kept in secure long-term storage. Google limits refresh tokens per client and user, so old ones can stop working.
- `webmasters.readonly` and `analytics.readonly` are sensitive scopes: a public app needs Google's verification (verified domain, public home page, privacy policy on the same domain, demo video, typically 3 to 5 business days). Unverified or testing apps show a warning screen, have a user cap and shorter-lived refresh tokens. Personal use and internal (Workspace) use are exempt.
- `drive.file` is the narrow Drive scope (files the app created or the user opened with it).

## Sources

- Search Console usage limits: https://developers.google.com/webmaster-tools/limits
- Search Analytics query: https://developers.google.com/webmaster-tools/v1/searchanalytics/query
- URL Inspection: https://developers.google.com/webmaster-tools/v1/urlInspection.index/inspect
- GA4 Data API quotas: https://developers.google.com/analytics/devguides/reporting/data/v1/quotas
- CrUX API: https://developer.chrome.com/docs/crux/api
- PageSpeed Insights API: https://developers.google.com/speed/docs/insights/v5/get-started and limits summary https://unlighthouse.dev/learn-lighthouse/pagespeed-insights-api/rate-limits
- Sheets API limits: https://developers.google.com/sheets/api/limits
- Looker Studio Linking API: https://developers.google.com/looker-studio/integrate/linking-api
- OAuth for desktop apps: https://developers.google.com/identity/protocols/oauth2/native-app
- Sensitive scope verification: https://developers.google.com/identity/protocols/oauth2/production-readiness/sensitive-scope-verification
- Ahrefs API: https://docs.ahrefs.com/docs/api/reference/introduction ; plans and units https://serpwatch.io/blog/the-8-best-seo-apis-ranked-by-what-they-actually-cost-to-run-in-2026/
- Majestic API: https://majestic.com/account/api
- Moz API plans: https://lucasfonseque.fr/?p=6263

## Plan-time decisions (2026-10-14)

- **Keychain library**: `keyring` 3 covers macOS Keychain, Windows Credential Manager and the Linux Secret Service behind one API; alternatives (`security-framework`, `windows` crate, `secret-service` directly) would mean three code paths. Chosen per the owner's keychain decision (2026-10-13).
- **No incremental scopes**: Google's installed-app guide says incremental authorization is not supported for installed apps, so adding a feature re-consents with the union of scopes (plan D3).
- **Client secret**: Google desktop clients issue a secret that the token endpoint expects, but treats it as not confidential; the built-in one is compiled in, a user's own one goes to the keychain.
- **Cache format**: SQLite (already a dependency) rather than files per response: one file, indexed expiry, atomic writes.
