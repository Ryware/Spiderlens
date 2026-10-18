# JavaScript rendering

Many sites build their pages in the browser: the HTML the server sends is nearly empty and a script adds the title, the text and the links. A crawler that reads only the server's HTML sees an empty page. With JavaScript rendering on, SpiderLens also loads each page in a headless browser, lets the scripts run, and stores what the page looks like afterwards next to what the server sent.

Rendering is off by default. A crawl that does not use it is as fast as before, starts no browser and stores nothing extra.

## What rendering does

- **Both versions are kept.** The raw HTML and the rendered page are stored side by side. Titles, descriptions, headings, canonical links, robots directives, links, structured data and word counts exist in both versions. Nothing about the raw version is overwritten.
- **The raw crawl never waits.** Fetching pages and reading their raw HTML goes on at its normal speed while pages wait for the browser. If the browser falls far behind (more than 50,000 pages waiting), fetching pauses until the queue shrinks, and the speed indicator says so.
- **Each page is requested once.** The browser is given the page SpiderLens already fetched instead of asking the site again. Scripts, stylesheets and data requests the page makes are fetched politely: they follow robots.txt, count against the per-host speed limit and are reused between pages when the site allows caching.
- **Switch between Raw and Rendered.** In the results, the values switch shows either version in the usual tabs (Page Titles, Headings, Links and so on). Rendered values are only used where you choose them.
- **The JavaScript tab** lists pages by what rendering changed: JavaScript-only content, links that exist only after rendering, links that disappear, titles and canonicals that change, noindex that appears or goes away, blocked resources, console errors, timeouts and failures. The page detail has a "Raw vs rendered" section with the differences and an HTML diff.

## Turning it on

Desktop app: Settings, JavaScript rendering, then choose "JavaScript rendering" as the mode.

Command line:

```sh
spiderlens crawl example.com --render
spiderlens crawl example.com --render --render-window mobile --screenshots
```

A crawl finishes only when every eligible page has reached one final state: rendered, render timed out, render failed (with the reason) or not rendered (outside the rendering rules, crawl stopped, browser unavailable, or raw HTML not stored).

To render a crawl that was done without rendering, use `spiderlens render <file>` or the "Render this crawl" action in the desktop app. Pages whose HTML was not stored cannot be rendered later and are listed as "not rendered: raw HTML not stored". Turn on "Store page HTML" for the first crawl if you plan to do this.

## Settings

| Setting (app) | Command line | Default |
|---|---|---|
| Maximum render time | `--render-timeout SECONDS` | 10 seconds (1 to 60) |
| Minimum wait | `--render-min-wait SECONDS` | 0 |
| Window | `--render-window auto\|mobile\|desktop\|WIDTHxHEIGHT` | auto: a phone window for a mobile user agent, a desktop window otherwise |
| Block images, video and audio, fonts, stylesheets, third-party | `--render-block images,media,fonts,stylesheets,third-party` or `none` | images, media and fonts are not downloaded |
| Only render pages matching / Never render pages matching | `--render-include REGEX`, `--render-exclude REGEX` | all internal HTML pages |
| Render error pages | `--render-error-pages` | off |
| Pages rendered at once | `--render-tabs N\|auto` | auto: memory in GB divided by 4, between 1 and 16 |
| Browser memory budget | `--render-memory MB\|auto` | auto: one eighth of the computer's memory |
| Screenshots | `--screenshots` | off |
| JavaScript content thresholds | `--js-content-threshold PERCENT,WORDS` | 20 percent and 50 words |
| Hidden text threshold | `--js-hidden-threshold PERCENT` | 50 percent |
| Browser | `--browser PATH` or "Change browser" | the downloaded browser |

Rendering follows the user agent, the robots.txt choice, the include and exclude rules and the speed limits of the crawl. Only pages inside the crawl are rendered; external pages, redirects, files that are not HTML, and pages blocked by robots.txt are never rendered.

The rendered HTML is stored under the same "Store page HTML" switch as the raw HTML. Screenshots are stored only when turned on and make the crawl file larger.

## The browser

SpiderLens does not ship a browser, so the installer stays small for people who never render. The first time you turn rendering on, SpiderLens asks before downloading one:

- **What it downloads:** Chrome for Testing headless shell, version 155, from `storage.googleapis.com`. It is about 98 MB on a Mac with Apple Silicon and about 119 MB on Windows; the dialog shows the exact size for your computer.
- **Verified:** the download is checked against a SHA-256 fingerprint built into SpiderLens. A file that does not match is deleted and never run.
- **Where it goes:** the SpiderLens data folder, not the system. It is used only by SpiderLens. Remove it from the app, or with `spiderlens browser remove`.
- **No internet:** the dialog says "No internet connection: the browser can't be downloaded now". "Crawl raw HTML instead" stays available. SpiderLens checks the connection every few seconds while the dialog is open and enables "Download and crawl" when it returns; it never downloads without your click. If the connection drops during the download, the download waits and continues from the bytes already saved. Cancel removes the partial files.
- **Use Chrome or Edge instead:** choose an installed Google Chrome, Chromium or Microsoft Edge (version 151 or newer) with "Change browser" in Settings, or `--browser PATH` on the command line. An installed browser is never used unless you choose it, and SpiderLens never touches your profile, cookies or sign-ins.

Command line:

```sh
spiderlens browser status           # which browser would be used, or why none is usable
spiderlens browser install          # download it (asks first; --yes skips the question)
spiderlens browser remove           # delete the downloaded browser
spiderlens crawl example.com --render --allow-browser-download
```

Without a usable browser, `--render` stops before the crawl starts and exits with code 9. When the cause is no internet connection, the message suggests `--browser PATH`.

## Differences from Google's renderer

SpiderLens renders with a current Chromium engine, as Google does, but it is not Google. Known differences:

- **Fixed locale and time zone.** Pages see English (United States) and UTC. A site that changes content by visitor language or time zone shows the same content to every page SpiderLens renders.
- **Cookies live for one render.** Each page starts with a clean browser: no cookies, local storage, service workers or caches from other pages, and never your own profile. Cookies that you set up for the crawl (spec 025) are added. A cookie set by the page itself is gone when that page's render ends.
- **No interaction.** SpiderLens does not click, scroll or type. Content that appears only after a click, or only when scrolling, is not seen. Dialogs, pop-ups, new windows, print dialogs, downloads and permission requests (location, camera, notifications) are dismissed or refused automatically.
- **The settle rule.** A page counts as settled when no network request is in flight and the document has not changed for 500 ms after the load event, or when the maximum render time passes. Content added by a timer later than that is missed; raise "Minimum wait" for slow pages. Pages that never settle (constant analytics) are cut at the maximum time and marked timed out.
- **Window height.** The window is extended in height to fit the page so lazily loaded content below the fold is more likely to appear, up to a limit.
- **Fragment routes.** A route such as `#!/products` is the same URL as the page without the fragment, as in a raw crawl. SpiderLens does not crawl fragments as separate pages.
- **Resources.** By default images, media and fonts are not downloaded, which does not change the text of a page. Turn on "Load all resources" for the most faithful result.

## JavaScript dependence and AI search

Some AI search and answer engines read only the server's HTML and do not run scripts. Text, links and structured data that exist only after rendering are invisible to them. For each page SpiderLens stores the share of text that exists only after rendering and flags pages that depend on JavaScript for their content (the thresholds are in the table above). The JavaScript tab filters "JavaScript content" and "Content hidden from non-rendering crawlers" list them. For content you want AI assistants to quote, put it in the server's HTML.

## Troubleshooting

- **"None found" or exit code 9:** run `spiderlens browser status`, then `spiderlens browser install`, or give a browser with `--browser PATH`.
- **Many "render timed out":** raise the maximum render time, or lower "Pages rendered at once" on a computer short of memory.
- **Rendering slows down by itself:** the memory guard waits when the browser uses more than its budget. Raise the budget in Settings or with `--render-memory`.
- **The crawl file is large:** turn off screenshots, or turn off "Store page HTML" if you do not need the rendered HTML.
