# Feature Specification: Site Visualisations

**Feature Branch**: `022-site-visualisations`

**Created**: 2026-10-13

**Status**: Draft (awaiting owner approval)

**Input**: User description: "Feature gap row 11 against Screaming Frog: site visualisations. Crawl tree and directory tree, a force-directed link graph, and inlink anchor text word clouds, rendered locally in the app with a budget for 100,000 nodes (for example sampling or level of detail), and export as an image."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - See the shape of a site as a crawl tree (Priority: P1)

An SEO consultant has crawled a client site and wants to show the client, in one picture, how deep the site is and where the weight sits. They open Visualisations › Crawl tree. The start URL is the root; each page hangs under the page through which the crawl first reached it on its shortest path, so the levels of the picture are the crawl depths. Nodes are coloured by response (success, redirect, client error, server error, blocked) and sized by Link Score (spec 023), so a 404 three clicks deep or a strong page buried at depth 6 stands out. The consultant expands and collapses branches, hovers a node to see its URL, title, status and score, and clicks it to show its details in the detail panel.

**Why this priority**: The crawl tree is the visualisation users ask for first and the one that explains crawl depth and architecture to non-specialists. It uses data every crawl already stores.

**Independent Test**: Crawl a fixture site with a known structure, open the crawl tree, and check that each node's parent, depth, colour and size match the expected values, that expand and collapse work, and that selection reaches the detail panel.

**Acceptance Scenarios**:

1. **Given** a finished crawl, **When** the user opens Crawl tree, **Then** the start URL is the root, every other crawled internal URL appears under the page through which it was first discovered at its shortest depth, and each level of the tree equals one crawl depth.
2. **Given** a node, **When** the user looks at it, **Then** its colour shows its response category and its size shows its Link Score, and a legend lists every colour and size meaning with the number of URLs in each colour.
3. **Given** a node with children, **When** the user clicks its expand or collapse control (or presses the matching key), **Then** its branch opens or closes without the rest of the picture jumping away from the node.
4. **Given** the user hovers a node, **When** the tooltip appears, **Then** it shows the URL, title, status code, crawl depth, Link Score and unique inlinks.
5. **Given** the user clicks a node, **When** the detail panel is open, **Then** it shows that URL's details as if the row were selected in the results table, and "Show in table" opens the Internal tab with that URL selected.
6. **Given** the user switches the layout between Tree and Radial, **When** the picture redraws, **Then** it shows the same nodes, colours and expanded branches in the other layout.

---

### User Story 2 - See the site as folders: directory tree (Priority: P1)

The same consultant wants to see how the URL structure is organised: which folders hold most pages, where errors cluster, and which sections are thin. Visualisations › Directory tree groups URLs by the folders in their paths (`/blog/`, `/blog/2025/`, ...). Each folder node shows how many URLs it contains and how they split by response category; pages are the leaves.

**Why this priority**: Folder structure and link structure are the two views of architecture users compare; both are cheap to build from stored data and together cover the main questions.

**Independent Test**: Crawl a fixture site with known folders and counts, open the directory tree, and compare every folder's children and counts with the expected values.

**Acceptance Scenarios**:

1. **Given** a finished crawl, **When** the user opens Directory tree, **Then** the root is the host, each folder in a URL path is a node holding its subfolders and pages, and each folder node shows its total number of URLs and a breakdown by response category.
2. **Given** a crawl covering several subdomains, **When** the user opens Directory tree, **Then** each host is its own top-level branch under a crawl root.
3. **Given** a folder node, **When** the user chooses "Show in table", **Then** the Internal tab opens filtered to URLs inside that folder, with the count matching the node.
4. **Given** URLs that differ only by query string, **When** they appear in the tree, **Then** they sit under the same page path, grouped as one node with a count, and expand into the individual URLs.

---

### User Story 3 - Explore how pages link to each other: link graph (Priority: P2)

A technical SEO wants to see the real link structure, not just the discovery tree: which pages are hubs, which clusters link only among themselves, which pages hang off one link. Visualisations › Link graph draws internal pages as points and followed internal links as lines, arranged by a force-directed layout so tightly linked pages pull together. The user can view the whole site or focus on one page and its neighbours.

**Why this priority**: It answers questions the trees cannot (clusters, hubs, cross-linking), but it is harder to read and to keep fast, so it follows the trees.

**Independent Test**: Crawl a fixture site with two link clusters joined by one link, open the link graph, and check that the clusters are drawn apart, that the joining link is present, that focusing on a page shows exactly its neighbours to the chosen distance, and that the layout comes out the same on every run.

**Acceptance Scenarios**:

1. **Given** a finished crawl, **When** the user opens Link graph, **Then** internal HTML pages are drawn as nodes and followed internal links between them as lines, coloured and sized as in Story 1, and the layout settles by itself and then stays still.
2. **Given** the user selects a page and chooses Focus, **When** the graph redraws, **Then** it shows that page, the pages linking to it and the pages it links to, up to the chosen distance (1 or 2 steps), with link direction shown.
3. **Given** a site with more pages than the visible node budget, **When** the user opens the whole-site graph, **Then** it shows the strongest pages up to the budget (by Link Score, or unique inlinks when no score exists), states how many pages are not drawn, and offers Focus to explore the rest.
4. **Given** the same crawl and the same settings, **When** the user opens the link graph twice, **Then** the layout is the same both times.
5. **Given** the user drags a node, **When** they release it, **Then** it stays where they put it until they choose Reset layout.

---

### User Story 4 - Read the words other pages use to link: anchor text clouds (Priority: P2)

The user wants to know how the site describes a page when it links to it: does the category page get "Running shoes" or "Click here"? Selecting a URL and opening Anchor text shows a word cloud of the anchor text of its inlinks, with word size by how often it is used. A site-wide cloud shows the anchor text of all internal links. Clicking a word lists the links that use it.

**Why this priority**: Anchor text is a strong relevance signal and a common audit item; the cloud makes it readable at a glance. It is simpler than the graphs but less central to architecture.

**Independent Test**: Crawl fixture pages with known inlink anchor texts, open the cloud for a target page, and compare the words and their counts with the expected values; click a word and check the listed links.

**Acceptance Scenarios**:

1. **Given** a URL with inlinks, **When** the user opens its anchor text cloud, **Then** it shows up to 100 terms from its inlinks' anchor text (image alt text for image links), sized by number of uses, with common stop words for the page's language left out by default.
2. **Given** the cloud, **When** the user switches between Words and Whole anchors, **Then** it shows either single words or complete anchor texts as terms.
3. **Given** the user clicks a term, **When** the list opens, **Then** it shows every inlink using it with its source page, full anchor text and rel attributes, and the count matches the term's count.
4. **Given** a URL with no inlinks, or only image links without alt text, **When** the user opens its cloud, **Then** it says so instead of showing an empty picture.
5. **Given** the user opens the site-wide cloud, **When** it appears, **Then** it covers the anchor text of all followed and nofollow internal links, with a switch to followed links only.

---

### User Story 5 - Export a picture to share (Priority: P2)

The consultant wants the picture in a client deck. Every visualisation exports its current view, as shown (expanded branches, focus, colours, legend), as a PNG image or an SVG file, plus its nodes and links as CSV so the client's team can rebuild the picture elsewhere.

**Why this priority**: Visualisations are mostly made to be shown to someone else; without export they stay on one screen.

**Independent Test**: Export each visualisation as PNG, SVG and CSV and check that the image matches the on-screen view, includes the legend, opens in common image viewers and browsers, and that the CSV lists exactly the nodes and links shown.

**Acceptance Scenarios**:

1. **Given** any visualisation, **When** the user chooses Export › PNG, **Then** they choose a size (1x, 2x or 4x) and theme (light or dark), and the saved image shows the current view with its legend and the crawl's host and date.
2. **Given** any visualisation, **When** the user chooses Export › SVG, **Then** the file opens in common browsers and vector editors and its text (URLs, labels) is selectable.
3. **Given** any visualisation, **When** the user chooses Export › Nodes and links (CSV), **Then** they get the nodes (URL, depth, response category, Link Score, inlinks, parent or folder) and the links shown, following the CSV rules of spec 007.
4. **Given** the CLI, **When** the user exports a visualisation of a crawl file with the same settings, **Then** the SVG and CSV it writes are the same as the desktop export of that default view.

---

### User Story 6 - Stay fast on big sites (Priority: P2)

The user crawls a 100,000-page site and opens the trees and the graph. The views open quickly, stay smooth to pan and zoom, and never freeze the app or slow a running crawl, because only a readable amount is drawn at a time: deep or wide branches are folded into summary nodes ("+2,340 pages") that open on demand, and the whole-site graph draws up to its budget and explains what it left out.

**Why this priority**: The product's top priority is speed; a picture that hangs on a real site is worse than no picture. It is a quality bar for Stories 1 to 5, listed separately so it is tested on its own.

**Independent Test**: Open each visualisation on the 100,000-page fixture crawl and on a live crawl of it, and measure open time, frame rate while panning and zooming, memory and the crawl's URLs per second.

**Acceptance Scenarios**:

1. **Given** a 100,000-page crawl, **When** the user opens Crawl tree or Directory tree, **Then** the first picture appears within 2 seconds, showing the top levels with larger branches folded into summary nodes with their counts.
2. **Given** a folded summary node, **When** the user expands it, **Then** its contents appear within 1 second, and if they exceed the budget they are shown in pages ("next 500") rather than all at once.
3. **Given** any visualisation with its maximum number of visible nodes, **When** the user pans and zooms, **Then** movement stays smooth (see success criteria), and labels appear only when they are large enough to read.
4. **Given** a crawl is running, **When** the user opens a visualisation, **Then** it shows the data crawled so far with the time it was taken, offers Refresh, does not redraw by itself, and the crawl's speed is not reduced.

### Edge Cases

- **No crawl or no internal pages**: each view explains why it is empty and what to do (start or open a crawl).
- **Crawl tree parent**: a URL found first through a redirect hangs under the redirecting URL; a URL found only through a sitemap, a canonical, an hreflang or as a List mode seed hangs under a "Found outside links" group under the root, by source, so nothing is silently dropped.
- **URLs not crawled**: blocked, over a limit, or failed URLs appear in the trees in their own colour; URLs that were only discovered and never queued (for example out of scope) are not shown.
- **External URLs**: left out of all views; the link graph can show external links as one summary node per domain when the user turns it on.
- **Very wide levels**: a page with 50,000 children (for example a paginated archive) shows the first children up to the budget and one summary node for the rest.
- **Very long URLs and non-Latin paths**: labels show the readable form, shortened with an ellipsis; the full URL is in the tooltip, the detail panel and exports. Right-to-left text displays correctly.
- **Link Score not calculated**: node size falls back to unique inlinks and the legend says so; when inlinks are unknown, nodes have one size.
- **Colour meaning**: response category is shown by colour and also by a shape or outline, so it is readable without colour vision; colours follow the design tokens and the user's appearance settings (spec 018).
- **Reduced motion**: when the system asks for reduced motion, layouts appear in their settled position without animation.
- **Older crawl files and read-only files**: views work on any file that opens; missing data (for example no Link Score) falls back as above.
- **Export of huge views**: an export larger than the size limit for PNG (for example 16,000 pixels on a side) is refused with a suggestion to use SVG or to export a focused view.
- **Anchor text**: anchor text is matched after whitespace collapse and ignoring letter case; terms in languages without spaces are shown as whole anchors.
- **Interrupting**: switching away from a view while its layout is still being worked out stops that work.

## Requirements *(mandatory)*

### Functional Requirements

**Placement and shared behaviour**

- **FR-001**: The sidebar MUST have a Visualisations group with Crawl tree, Directory tree, Link graph and Anchor text, and the command palette MUST open each of them.
- **FR-002**: All views MUST support zoom, pan, fit to window, keyboard navigation between nodes, a search box that finds a URL and centres it, hover tooltips, and selection that drives the detail panel exactly like selecting a row in the results table.
- **FR-003**: Every node MUST offer Show in table, Copy URL and Open in browser; folder nodes MUST offer Show in table filtered to that folder.
- **FR-004**: Colour MUST be selectable from response category (default), indexability and crawl depth; size MUST be selectable from Link Score (default), unique inlinks and uniform. Every view MUST show a legend with counts.
- **FR-005**: Colours MUST come from the design tokens and follow the active theme and palette (specs 017 and 018), with contrast checked, and categories MUST also be distinguishable without colour.
- **FR-006**: Views MUST work identically for a live, paused, finished and reopened crawl; on a live crawl they MUST show a snapshot with its time and a Refresh action and MUST NOT redraw on their own.
- **FR-007**: The user's choices per view (layout, colour, size, expanded branches, focus) MUST be kept while the crawl stays open; layout, colour and size choices MUST be remembered across restarts.

**Crawl tree**

- **FR-008**: The crawl tree MUST place each crawled internal URL under the URL through which it was first discovered at its shortest crawl depth, with the start URL as root, and URLs not discovered through links under a "Found outside links" group grouped by source.
- **FR-009**: The crawl tree MUST offer Tree (left to right) and Radial layouts and a force-directed layout of the same hierarchy.

**Directory tree**

- **FR-010**: The directory tree MUST group internal URLs by host and by each folder in their path, with page URLs as leaves and URLs differing only by query string grouped under their path; folder nodes MUST show their total URL count and counts per response category.
- **FR-011**: The directory tree MUST offer the same layouts as the crawl tree.

**Link graph**

- **FR-012**: The link graph MUST draw internal HTML pages as nodes and distinct followed internal links between them as directed lines, using a force-directed layout that settles and stops.
- **FR-013**: The link graph MUST offer a whole-site mode and a focus mode around a selected URL at a distance of 1 or 2 steps, with inlinks and outlinks distinguishable.
- **FR-014**: The link graph layout MUST be deterministic for the same crawl, mode and settings; nodes moved by the user MUST stay put until Reset layout.
- **FR-015**: The link graph MUST optionally show nofollow internal links (styled differently) and external links as one node per domain; both are off by default.

**Anchor text clouds**

- **FR-016**: For a selected URL, the system MUST show a cloud of up to 100 terms from the anchor text of its internal inlinks (alt text for image links), sized by number of uses, with Words and Whole anchors modes, stop words for the page's language removed by default (switchable), and a filter by link position (spec 023) when positions have been calculated.
- **FR-017**: The system MUST offer a site-wide anchor text cloud over all internal links, with a followed-only switch.
- **FR-018**: Clicking a term MUST list the links using it with source page, full anchor text and rel attributes, with the same list behaviour as the detail panel's Inlinks section (spec 007).

**Level of detail and budgets**

- **FR-019**: Each view MUST draw at most a visible node budget (default 5,000 nodes for the link graph and force-directed layouts, 20,000 for tree and radial layouts, adjustable in Settings within safe limits), folding the rest into summary nodes with counts that expand on demand in pages.
- **FR-020**: When a view leaves nodes out, it MUST say how many and why, and offer the action that reveals them (expand, next page, focus).
- **FR-021**: The whole-site link graph MUST choose which pages to draw by Link Score (spec 023), or by unique inlinks when no score exists, always including the start URL, and draw only links among the drawn pages.
- **FR-022**: Labels MUST appear only at zoom levels where they are readable; at all other zoom levels nodes draw without text.
- **FR-023**: Building and laying out a view MUST NOT block the interface, MUST stop when the user leaves the view, MUST NOT load the whole crawl into memory, and MUST NOT run in or slow down the fetch path.

**Export**

- **FR-024**: Every view MUST export its current state as PNG (1x, 2x or 4x, light or dark theme) and SVG, including the legend, the crawl's host and the crawl date, and as CSV of the nodes and links shown (CSV rules of spec 007).
- **FR-025**: The command-line interface MUST export each view's default state (and a focus view for a given URL) for a crawl file as SVG and CSV, identical to the desktop export of the same view and settings.

### Key Entities

- **Visualisation View**: One of Crawl tree, Directory tree, Link graph, Anchor text; with its layout, colour and size choices, expanded branches, focus and node budget.
- **Tree Node**: A URL (crawl tree) or a host, folder or page (directory tree), with parent, depth, response category, Link Score, inlink count and child count.
- **Summary Node**: A stand-in for folded children, with the count and category breakdown of what it hides.
- **Graph Link**: A distinct followed (or optionally nofollow) internal link drawn between two nodes, with direction.
- **Anchor Term**: A word or whole anchor text with its number of uses for a target URL or the whole site.

## Success Criteria *(mandatory)*

### Measurable Outcomes

Timings are measured on the reference machine shared by every SpiderLens spec (spec 007 Assumptions).

- **SC-001**: On the structure fixture sites, 100% of crawl tree parents and depths, directory tree folders and counts, link graph links and anchor term counts match the expected values.
- **SC-002**: On the 100,000-page fixture crawl, Crawl tree and Directory tree show their first picture within 2 seconds, and expanding any summary node shows its contents within 1 second.
- **SC-003**: On the 100,000-page fixture crawl, the whole-site link graph at its default budget shows a first picture within 3 seconds and settles within 10 seconds.
- **SC-004**: At the default node budgets, panning and zooming run at 30 frames per second or more, and no interaction freezes the interface for more than 100 ms.
- **SC-005**: With any visualisation open during a crawl of the 100,000-page fixture site, the crawl's URLs per second stay within 5% of the same crawl with the view closed.
- **SC-006**: Memory used by an open visualisation stays under 300 MB on the 100,000-page crawl, and no more than 20% higher on a 1,000,000-URL crawl.
- **SC-007**: PNG export at 2x of a view at its default budget completes within 5 seconds; desktop and CLI SVG and CSV exports of the same view and settings are identical.
- **SC-008**: A first-time user can open the crawl tree, find a 404 page in it, and export the picture as PNG in under 2 minutes without help.

## Assumptions

- **Where the data comes from**: crawl depth, discovery source and links are already stored by specs 003 and 005; Link Score and inlink counts come from spec 023. Without spec 023, views fall back to stored inlink counts and uniform sizes, so this feature can ship on its own.
- **Precomputation**: any whole-crawl precomputation a view needs (for example tree parents and folder counts) runs as a pass of the shared Crawl Analysis step owned by spec 009, never as a step of its own.
- **Rendering locally**: all drawing happens inside the app with no network access; nothing is sent anywhere (constitution II).
- **Budgets**: default node budgets (5,000 and 20,000) reflect what stays readable and smooth on the reference machine; they are a setting, with upper limits so the app cannot be made unusable.
- **Determinism**: force-directed layouts start from a fixed seed derived from the crawl, so the same crawl gives the same picture and desktop and CLI exports match.
- **Stop words**: built-in lists for the languages SpiderLens shows in its interface and the most common web languages; the full list is decided in the plan. Users cannot edit them in this feature.
- **Out of scope**: body text word clouds, 3D views, animated crawl replays, comparing two crawls visually (spec 012 may add it), editing links in a "what if" mode, external backlink graphs (spec 024), custom colour rules, and printing directly from the app.
