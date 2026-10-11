# Feature Specification: Project Foundation

**Feature Branch**: `001-project-foundation`

**Created**: 2026-10-11

**Status**: Approved (2026-10-11)

**Input**: User description: "Project foundation: an installable (unsigned for now) SpiderLens desktop app for Windows, macOS and Linux built automatically on every change, a command-line crawler that shares the same engine and can crawl a single page and print its status and title, and automated tests running on all three operating systems. The owner can download a build of any change and run it."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Download and run the desktop app from any change (Priority: P1)

The project owner opens any proposed or accepted change to SpiderLens, finds the builds produced for it, downloads the package for their operating system, installs or opens it, and sees the SpiderLens desktop window. The window shows the product name, the version and the build identifier that links the build back to the exact change it came from. Nothing in this window crawls yet; its purpose is to prove that a real, installable app reaches every supported platform from every change.

**Why this priority**: The owner approves every change (constitution, Development Workflow). Without a build they can run themselves, they cannot judge any later feature. Every later milestone depends on this delivery path working on all three platforms.

**Independent Test**: Submit a trivial change, wait for the automated build, download the package for each platform onto a clean machine of that platform, install or open it, and confirm the window appears showing a version and build identifier that match the change.

**Acceptance Scenarios**:

1. **Given** a change has been submitted to the repository, **When** the automated build finishes successfully, **Then** downloadable packages exist for Windows, macOS on Intel, macOS on Apple Silicon and Linux, all reachable from that change.
2. **Given** the owner has downloaded the Windows package to a supported Windows machine, **When** they install and launch it following the documented steps, **Then** the SpiderLens window opens and shows the product name, version and build identifier.
3. **Given** the owner has downloaded the macOS package matching their Mac's processor, **When** they open it following the documented steps for unsigned apps, **Then** the SpiderLens window opens and shows the product name, version and build identifier.
4. **Given** the owner has downloaded the Linux package to a mainstream Linux desktop, **When** they run it following the documented steps, **Then** the SpiderLens window opens and shows the product name, version and build identifier.
5. **Given** two different changes each produced builds, **When** the owner runs each build, **Then** each shows a different build identifier and each identifier leads back to its own change.
6. **Given** the operating system warns that the app comes from an unidentified developer, **When** the owner follows the project's documented steps for that platform, **Then** they can run the app without changing system-wide security settings permanently.

---

### User Story 2 - Check a single page from the command line (Priority: P1)

A user runs the SpiderLens command-line tool with one web address. The tool fetches that page using the same crawl engine the desktop app will use, then prints the HTTP status code, the final address after any redirects, and the page title. If the fetch fails, it prints a clear error that names the error class and explains what went wrong, and it ends with an exit code that scripts can rely on.

**Why this priority**: This is the first real slice of the crawl engine and the first thing a user can do with SpiderLens. It proves that fetching, redirect following, robots.txt checking, title extraction and error reporting work outside the desktop shell, which the constitution requires (Principle III).

**Independent Test**: Start a local fixture website, run the command-line tool against a set of fixture pages (normal page, redirect, missing title, error status, unreachable host, robots-disallowed page) and compare the printed results and exit codes with the expected values.

**Acceptance Scenarios**:

1. **Given** a reachable HTML page with the title "Home | Example", **When** the user runs a single-page check on its address, **Then** the tool prints status 200, the page's address and the title "Home | Example", and exits with the success exit code.
2. **Given** an address that redirects twice before reaching a page, **When** the user runs a single-page check, **Then** the tool lists each redirect hop with its status code and target, prints the final status and title, and exits with the success exit code.
3. **Given** a page that returns status 404 with a title "Not found", **When** the user runs a single-page check, **Then** the tool prints status 404 and the title "Not found", and exits with the success exit code, because the page was fetched and an HTTP status is a result, not a tool failure.
4. **Given** an HTML page with no title element or an empty title, **When** the user runs a single-page check, **Then** the tool prints the status and states clearly that the page has no title.
5. **Given** a response that is not HTML (for example an image or a PDF), **When** the user runs a single-page check, **Then** the tool prints the status and content type and states that a title is not applicable.
6. **Given** a host name that does not exist, **When** the user runs a single-page check, **Then** the tool prints an error naming the DNS error class and a plain-language message, and exits with the fetch-failure exit code.
7. **Given** an address that is not a valid web address, **When** the user runs a single-page check, **Then** the tool reports a malformed URL without making any network request and exits with the usage-error exit code.
8. **Given** a site whose robots.txt disallows the page for the SpiderLens user agent, **When** the user runs a single-page check, **Then** the tool does not fetch the page, reports that it is blocked by robots.txt (not as an error), and exits with the blocked exit code.
9. **Given** any successful or failed check, **When** the user asks for machine-readable output, **Then** the tool prints the same result as a single structured record containing every field from the human-readable output.

---

### User Story 3 - Automated tests on all three operating systems for every change (Priority: P2)

Every time a change is submitted, the project's automated tests run on Windows, macOS and Linux. The change shows a clear pass or fail result per operating system. A change whose tests fail on any operating system is marked as failing, and the owner can see which tests failed and on which system.

**Why this priority**: The constitution makes passing tests on all three systems the definition of done (Principle VII). This story gives every later feature a place to put its tests. It ranks below P1 because a build that runs is the first thing the owner needs, but no later feature can be accepted without it.

**Independent Test**: Submit one change with all tests passing and one change that adds a deliberately failing test. Confirm the first shows passing on all three systems, and the second shows failing with the failing test named.

**Acceptance Scenarios**:

1. **Given** a change is submitted, **When** the automated checks run, **Then** the test suite runs separately on Windows, macOS and Linux and each system reports its own pass or fail result on the change.
2. **Given** a change that breaks a test on only one operating system, **When** the checks finish, **Then** the change is marked failing overall, and the failing system and test name are visible to the owner.
3. **Given** the test suite, **When** it runs, **Then** every crawl-behaviour test uses a local fixture website and no test contacts a live third-party website.
4. **Given** the built packages for a change, **When** the automated checks run, **Then** each package is started automatically on its own operating system, the check confirms that it starts and reports the expected version, and the result is shown on the change.
5. **Given** each acceptance scenario in this specification, **When** the owner reviews the test suite, **Then** each scenario maps to at least one named automated test.

---

### User Story 4 - The desktop app and command-line tool share one engine (Priority: P2)

The desktop app and the command-line tool are built from the same engine at the same version. Both show the same version and build identifier. The engine has no dependency on the desktop window, so the command-line tool runs on a machine with no graphical desktop.

**Why this priority**: Principle III requires that everything the desktop app can do is also possible from the command line, and that the engine is independent of the shell. Setting this up now is cheap. Undoing a shared, tangled design later would be expensive.

**Independent Test**: From one build, run the command-line tool and the desktop app, compare the version and build identifiers they report, and run the command-line tool on a system with no graphical desktop.

**Acceptance Scenarios**:

1. **Given** the desktop app and command-line tool from the same build, **When** the user asks each for its version, **Then** both report the same product version, engine version and build identifier.
2. **Given** a Linux machine with no graphical desktop, **When** the user runs a single-page check with the command-line tool, **Then** it completes normally.
3. **Given** the downloadable builds for a change, **When** the owner looks for the command-line tool, **Then** a command-line package is available for Windows, macOS (both processor types) and Linux alongside the desktop packages.

---

### User Story 5 - A contributor can build and test locally (Priority: P3)

A contributor gets the SpiderLens repository on Windows, macOS or Linux, follows the documented setup steps, and can build the desktop app and the command-line tool and run the full test suite locally. The repository holds a specs folder where each feature's specification, plan and tasks live.

**Why this priority**: It speeds up development and makes failures found in automated checks reproducible. The owner can still review changes without it, using downloaded builds, so it ranks below the other stories.

**Independent Test**: On a fresh machine of each operating system, follow only the documented steps and confirm that the build and the full test suite succeed.

**Acceptance Scenarios**:

1. **Given** a fresh machine with only the documented prerequisites installed, **When** a contributor follows the documented steps, **Then** they can build both the desktop app and the command-line tool and run the full test suite with one documented command each.
2. **Given** the repository, **When** a contributor looks at its structure, **Then** they find a specs folder containing one folder per feature, starting with this one.

---

### Edge Cases

- **Build fails on one operating system**: the change is marked failing. Packages that did build for the other systems stay downloadable and are labelled as coming from a failing change.
- **Unsigned-app warnings**: Windows and macOS warn about or block unsigned apps. The project documentation gives step-by-step instructions for each platform. Signing is out of scope until milestone M3.
- **Wrong processor type on macOS**: the downloads are labelled clearly enough that the owner can pick Intel or Apple Silicon. If the owner picks the wrong one, the operating system's own message is acceptable.
- **Old builds**: builds stay downloadable for at least 90 days after the change. After that, the change page says the build has expired instead of showing a broken link.
- **Redirect loop or long chain**: the single-page check stops after 10 redirect hops, reports a redirect loop or chain-over-limit error that lists the hops it followed, and exits with the fetch-failure exit code.
- **Timeout**: if the server does not respond within the default time limit, the tool reports the timeout error class (connect or read) and exits with the fetch-failure exit code. It never hangs indefinitely.
- **TLS certificate invalid**: the tool reports the TLS invalid error class and does not show page content.
- **Title edge cases**: titles containing character references (for example `&amp;`), surrounding or repeated whitespace, line breaks, or non-UTF-8 encodings are printed as a reader would see them: decoded, with whitespace collapsed and trimmed. If a page has several title elements, the first one in the document head is used.
- **Very large page or broken compression**: a response over the size limit, or one that fails to decompress or is truncated, is reported with the content-problem error class. The tool does not crash or use memory proportional to the response size.
- **robots.txt unavailable**: if robots.txt returns 4xx, the page is treated as allowed. If it returns 5xx or cannot be reached, the page is treated as disallowed for this check, and the report says why.
- **Address without a scheme** (for example `example.com`): the tool assumes `https://` and says that it did so.
- **No network connection at all**: the tool reports a DNS or connection error with a message suggesting the user check their network, and does not crash.
- **Command-line tool given no address or unknown options**: the tool prints brief usage help and exits with the usage-error exit code.

## Requirements *(mandatory)*

### Functional Requirements

**Build and delivery**

- **FR-001**: Every change submitted to the SpiderLens repository MUST automatically trigger a build of the desktop app and the command-line tool for Windows (64-bit), macOS on Intel, macOS on Apple Silicon and Linux (64-bit).
- **FR-002**: The packages each build produces MUST be downloadable by the project owner from the change they were built from. No local tools or manual build steps are needed to obtain them.
- **FR-003**: Each desktop package MUST be in a form a non-developer recognises as installable or runnable on that platform: an installer for Windows, a disk image containing the app for macOS, and a single self-contained runnable package for Linux.
- **FR-004**: Builds in this feature MAY be unsigned. The project MUST provide written, per-platform steps that let the owner open an unsigned build safely.
- **FR-005**: Every build MUST carry a product version and a unique build identifier that links to the exact change it was built from.
- **FR-006**: Downloadable builds MUST remain available for at least 90 days after they were produced.
- **FR-007**: The delivery path MUST use only static file hosting and the project's automated build service. No SpiderLens-operated server may be required (Principle II).

**Desktop app**

- **FR-008**: When launched, the desktop app MUST open a window that shows the product name SpiderLens, the product version, the engine version and the build identifier.
- **FR-009**: The desktop app MUST start without network access and MUST NOT send any telemetry, crash report or other data anywhere.
- **FR-010**: The desktop app MUST be closable through the platform's normal window controls and MUST leave no background process running after it closes.

**Command-line tool and shared engine**

- **FR-011**: The project MUST provide a command-line tool that uses the same crawl engine as the desktop app. The engine MUST NOT depend on the desktop window or user interface (Principle III).
- **FR-012**: The command-line tool MUST provide a single-page check that takes one web address, fetches it, and reports: the requested address, each redirect hop (status code and target), the final address, the HTTP status code, the content type, the response time and the page title.
- **FR-013**: The single-page check MUST follow redirects up to a limit of 10 hops. Going beyond the limit, or revisiting an address already in the chain, MUST be reported as a redirect loop or chain-over-limit error.
- **FR-014**: The title MUST be extracted from HTML responses only, decoded from the page's declared or detected character encoding, with character references resolved and whitespace collapsed and trimmed. A missing or empty title MUST be reported explicitly as "no title". A non-HTML response MUST report the title as not applicable.
- **FR-015**: Before fetching a page, the single-page check MUST read the site's robots.txt and MUST NOT fetch a page that robots.txt disallows for the SpiderLens user agent. It MUST report such a page as blocked by robots.txt, which is a final state and not an error.
- **FR-016**: Every request MUST identify itself with an honest user agent that names SpiderLens and its version (Principle IV).
- **FR-017**: When a fetch fails, the tool MUST report exactly one error class from the project's named set (DNS, DNS temporary, connection, timeout, TLS invalid, redirect loop or chain over limit, content problem, malformed URL), along with a plain-language message and the number of attempts made. Failures MUST never be silent (Principle V).
- **FR-018**: The single-page check MUST apply a default time limit to each request so it never waits indefinitely. Users MUST be able to change the limit with a command-line option.
- **FR-019**: The tool MUST stop reading a response body that exceeds a default size limit and report it with the content-problem error class. Memory use MUST NOT grow with the size of the response beyond that limit.
- **FR-020**: The command-line tool MUST use distinct, documented exit codes for: page fetched (any HTTP status), blocked by robots.txt, fetch failed, and usage error (including malformed URL).
- **FR-021**: The command-line tool MUST offer a machine-readable output option that prints the full result as one structured record, with the same fields as the human-readable output.
- **FR-022**: The command-line tool MUST report its product version, engine version and build identifier on request. These MUST match the values the desktop app from the same build shows.
- **FR-023**: The command-line tool MUST run on all supported operating systems, including machines with no graphical desktop.

**Automated tests and repository**

- **FR-024**: Every change MUST automatically run the full test suite on Windows, macOS and Linux and report a pass or fail result for each system on the change.
- **FR-025**: A change MUST be marked as failing if any test fails, or any build fails, on any of the three systems. The failing system and test or build step MUST be identifiable from the change.
- **FR-026**: Tests of crawl behaviour MUST use local fixture websites started by the test suite. No test may contact a live third-party website (Principle VII).
- **FR-027**: The automated checks MUST include a launch check that starts each built desktop package and the command-line tool on its own operating system and confirms that each reports the expected version and build identifier.
- **FR-028**: Every acceptance scenario in this specification MUST be covered by at least one automated test, and the mapping from scenario to test MUST be traceable.
- **FR-029**: The repository MUST contain a specs folder with one folder per feature, plus contributor documentation that explains how to build both products and run the tests locally on each supported operating system.

### Key Entities

- **Change**: a proposed or accepted modification to the repository. It has an identifier, and it has builds and test results attached to it.
- **Build**: the output of building one change. It has a product version, an engine version, a build identifier linking it to its change, a creation date, an overall pass or fail result, and a set of packages.
- **Package**: one downloadable file within a build, for one platform (Windows, macOS Intel, macOS Apple Silicon or Linux) and one product (desktop app or command-line tool).
- **Test Run**: the result of running the test suite for one change on one operating system. It records pass or fail, plus the names of any failing tests.
- **Page Check Result**: the outcome of one single-page check. It records the requested address, the redirect hops, the final address, the HTTP status, the content type, the response time, the title (or "no title" or "not applicable"), and the final state (fetched, blocked by robots.txt, or failed). For a failure it also records the error class, message and attempt count.
- **Fixture Website**: a local test website with known pages, redirects, error responses and robots.txt rules, used by the automated tests in place of live sites.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: For 100% of changes whose builds succeed, desktop and command-line packages for all four platform targets are downloadable from the change within 45 minutes of the change being submitted.
- **SC-002**: The owner can go from opening a change to seeing the running SpiderLens window on a clean Windows, macOS or Linux machine in under 10 minutes, using only the project's written instructions.
- **SC-003**: On a typical machine from the last five years, the desktop window appears within 5 seconds of launch on each of the three operating systems.
- **SC-004**: A single-page check of a page on the local fixture website completes and prints its result in under 2 seconds. A check against an unresponsive server ends with a timeout error no later than 5 seconds after the configured time limit.
- **SC-005**: Across the fixture page set (normal titles, character references, extra whitespace, non-UTF-8 encodings, missing titles, non-HTML responses, redirects, error statuses, robots-blocked pages, and each listed error class), the single-page check reports the expected status, title, final state and exit code for 100% of cases on all three operating systems.
- **SC-006**: 100% of changes have test results from all three operating systems, and those results are available within 45 minutes of the change being submitted.
- **SC-007**: Zero automated tests contact a live third-party website. This can be verified by running the full suite with only local network access.
- **SC-008**: For 100% of builds, the desktop app and command-line tool from the same build report the same version and build identifier, and that identifier resolves to the correct change.

## Assumptions

- The only user of this feature is the project owner, acting as reviewer and tester. Public releases, download pages and auto-update are out of scope.
- Code signing and notarisation of builds are out of scope until milestone M3. Until then, unsigned builds and documented workarounds for operating-system warnings are acceptable.
- "Every change" means every proposed change (before acceptance) and every accepted change to the main line of the repository.
- Supported platforms follow the constitution: Windows 10 or later (64-bit), macOS on both Intel and Apple Silicon (the three most recent major versions), and mainstream 64-bit Linux desktops. Each macOS processor type gets its own download in this feature.
- For Linux, one self-contained runnable package is enough at this stage. Distribution-specific packages can be added later.
- Builds are kept for 90 days. This matches common automated build service defaults and is enough for the owner's review cycle.
- The single-page check makes a single attempt per request. The full retry, backoff and circuit-breaker policy, and the HTTP 429 and 5xx handling that goes with it, is defined in the fetch errors and retries feature (spec 004). This feature only needs to record the attempt count.
- Default time limits: 10 seconds to connect and 30 seconds to read, matching spec 004. Default response size limit: 50 MB. Both are reasonable defaults that later configuration features can make adjustable in the desktop app.
- Overriding robots.txt from the command line is out of scope for this feature. Tests use fixture sites whose robots.txt allows the pages under test.
- The desktop window in this feature only shows identity and version information. Crawling from the desktop app, the results table, crawl files, Spider and List modes, and all audits are out of scope here. They belong to later specs in milestones M1 and M2. JavaScript rendering (M4), AEO/GEO checks (M5) and AI agents (M6) are also out of scope.
- Saving crawl results to a crawl file is out of scope. The single-page check prints its result and stores nothing.
- The project's automated build service has hosted Windows, macOS (both processor types, or cross-building for one) and Linux machines available.
