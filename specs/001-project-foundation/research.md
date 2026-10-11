# Research: Project Foundation

Each entry: Decision, Rationale, Alternatives considered.

## R1. Workspace layout: one Cargo workspace, engine as a library

- **Decision**: Cargo workspace with `crates/engine` (library), `crates/cli`
  (binary), `crates/test-fixtures` (dev-only library) and `app/src-tauri`
  (desktop binary). Shared dependency versions live in `[workspace.dependencies]`.
- **Rationale**: Principle III. The CLI and the desktop app link the same engine,
  so the same build has the same engine version (FR-011, FR-022). One `Cargo.lock`
  pins every dependency for both products. A workspace is the Rust equivalent of a
  .NET solution or an Nx/Turborepo monorepo.
- **Alternatives**: one crate with feature flags (simpler, but the Tauri
  dependency would leak into the CLI build and break headless Linux, FR-023);
  separate repositories (version drift between CLI and app).

## R2. Async runtime: tokio

- **Decision**: `tokio` multi-threaded runtime in the binaries; the engine exposes
  `async fn` and does not start a runtime itself.
- **Rationale**: reqwest and axum are built on tokio, and Tauri 2 already runs
  one. The library staying runtime-agnostic at its edges means the Tauri app reuses
  its runtime instead of starting a second one. Comparable to Node's event loop or
  .NET's thread pool with `Task`, but explicit: Rust has no built-in runtime.
- **Alternatives**: `async-std` (smaller ecosystem, effectively retired); blocking
  I/O with threads (simple for one page, wrong for the crawler in 003/008).

## R3. HTTP client: reqwest + rustls, redirects handled manually

- **Decision**: `reqwest` with `rustls-tls` (no OpenSSL), `redirect::Policy::none()`,
  `connect_timeout` 10 s and a per-read timeout 30 s. The engine follows redirects
  itself, recording each hop, detecting loops with a set of visited URLs and
  stopping after 10 hops.
- **Rationale**: FR-012 and FR-013 need every hop's status and target, which
  automatic redirect following hides. rustls gives identical TLS behaviour on all
  three OSes and avoids shipping OpenSSL on Windows and macOS.
- **Proxy**: requests go direct (`no_proxy`) in this feature; proxy settings
  arrive with crawl configuration in spec 002.
- **Version**: reqwest 0.12 (0.13 is out; moving is a separate, small change).
- **Alternatives**: `hyper` directly (more code for pooling and proxies);
  `native-tls` (differs per OS, which makes "TLS invalid" classification
  inconsistent); `ureq` (blocking only).

## R4. Body size cap: stream and stop

- **Decision**: read the body chunk by chunk and stop at 50 MB, returning the
  content-problem error class. Bytes are only kept for HTML responses, which need
  the title.
- **Rationale**: FR-019 says memory must not grow beyond the limit. Reading
  `Content-Length` alone is not enough because servers can lie or omit it.
- **Alternatives**: `response.bytes()` (buffers everything first).

## R5. Charset and title extraction: encoding_rs + scraper

- **Decision**: detect the encoding in this order: byte order mark, HTTP
  `Content-Type` charset, `<meta charset>` or `http-equiv` in the first 1024 bytes,
  otherwise UTF-8 (the WHATWG default for unlabelled pages is windows-1252, but
  UTF-8 matches modern sites; documented and tested). Decode with `encoding_rs`,
  parse with `scraper` (html5ever), take the first `<title>` in `<head>`, collapse
  whitespace, trim. Empty means "no title"; non-HTML content types mean "not
  applicable".
- **Rationale**: html5ever is the browser-grade HTML5 parser from Servo, so
  malformed pages behave like they do in Chrome and character references are
  resolved for us (FR-014). encoding_rs is the Firefox encoding library.
- **Alternatives**: regex on `<title>` (fails on comments, scripts and
  references); `lol_html` (streaming rewriter, a good fit for spec 005 at scale,
  more than needed here). Spec 005 revisits this choice.

## R6. robots.txt: texting_robots

- **Decision**: fetch `/robots.txt` for the target origin with the same client.
  2xx: parse with `texting_robots` and test the path for the `SpiderLens` agent.
  4xx: allow all. 5xx or 429: disallow all (decision 7 in the review).
  Redirects on robots.txt are followed up to 5 hops; more counts as no
  robots.txt. If robots.txt cannot be reached at all (DNS, connection, TLS,
  timeout), the check fails with that error class, because the page cannot be
  reached either; the crawler in spec 003 applies "disallow all" instead.
  At most 500 KiB of robots.txt is read, as Google does.
- **Rationale**: matches the cross-spec decision and Google's documented
  behaviour. `texting_robots` implements RFC 9309 and is fuzz-tested.
- **Alternatives**: `robotstxt` (port of Google's C++ parser; heavier API);
  writing our own (Principle VIII).

## R7. Error model: enum of classes, thiserror in the library, anyhow in the binary

- **Decision**: `enum ErrorClass { Dns, DnsTemporary, Connection, Timeout,
  TlsInvalid, RedirectLoop, ContentProblem, MalformedUrl }`. A
  `CheckError { class, message, attempts }` is part of the result, not a Rust
  panic or a thrown exception. The CLI maps outcomes to exit codes.
- **Rationale**: FR-017 requires exactly one class per failure. A Rust `enum`
  plus exhaustive `match` means adding a class later is a compile error
  everywhere it is not handled. This is like a TypeScript discriminated union
  checked with `never`, or a sealed class in Kotlin/C#. Library code returns
  `Result<T, E>` (like `Either`, or Java checked exceptions without the syntax);
  `thiserror` derives the error type. The binary uses `anyhow` for "any error
  with context" at the top level.
- **Alternatives**: string error codes (no exhaustiveness check); exceptions
  do not exist in Rust; panics are for bugs only.

## R8. CLI: clap derive, JSON via serde

- **Decision**: `spiderlens check <url> [--json] [--connect-timeout S]
  [--read-timeout S] [--max-size MB]` and `spiderlens version [--json]`. Missing
  scheme defaults to `https://`. Exit codes: 0 fetched (any HTTP status),
  2 usage error or malformed URL, 3 blocked by robots.txt, 4 fetch failed.
  1 is reserved for unexpected internal errors.
- **Rationale**: clap derive turns a struct into a parser with help text, like
  .NET System.CommandLine or Python's Typer. Exit code 2 is the clap and POSIX
  convention for usage errors. `serde` derives JSON from the same struct the human
  output uses, so the two can never disagree (FR-021).
- **Alternatives**: `argh` (smaller, fewer features); hand-written JSON.

## R9. Version and build identifier

- **Decision**: product version is the workspace version (`0.1.0`). Engine
  version is the engine crate version. Build id is `<short git sha>` from
  `GITHUB_SHA` in CI, or `git rev-parse` locally, or `dev` when neither exists,
  injected at compile time by the engine's `build.rs` through
  `cargo:rustc-env`. CI builds add the run number: `<sha>.<run>`.
- **Rationale**: the build id lives in the engine, so the CLI and the desktop app
  read the same compiled constant (FR-022, SC-008). The sha links to the change
  (FR-005).
- **Alternatives**: `vergen` crate (more features than needed); reading git at
  runtime (fails for end users).

## R10. Fixture website: axum on 127.0.0.1, random port

- **Decision**: a dev-only crate starts an axum server on port 0 in the test
  process and returns its base URL. Routes cover: normal title, character
  references, whitespace, windows-1255 and Shift_JIS pages, missing and empty
  title, image and PDF, 404 with title, 2-hop redirect, redirect loop, 11-hop
  chain, robots disallow, slow response (timeout), oversize body, and a closed
  port for connection refused. DNS failure uses the reserved `.invalid` TLD
  (RFC 2606), which never resolves and does not reach the internet.
  TLS-invalid uses a self-signed certificate served by the fixture with
  `rcgen` + `axum-server` rustls.
- **Rationale**: Principle VII and SC-007; random ports let tests run in
  parallel. The fixture pins rustls to the `ring` provider so it matches
  reqwest (two providers in one process make rustls refuse to start). The fixture serves `robots.txt` with a disallow rule for one path.
- **Alternatives**: `wiremock` (great for request matching, but slow responses and
  oversize streams are easier with plain axum); recorded fixtures (not real HTTP).

## R11. Desktop shell: Tauri 2 with one command

- **Decision**: Tauri 2 app with a single `build_info` command returning the
  engine's `BuildInfo`. The UI calls it through `ui/src/transport.ts`. No updater,
  no network plugins, CSP set to self only.
- **Rationale**: FR-008 to FR-010. Tauri uses the OS webview (WebView2, WKWebView,
  WebKitGTK), so the app is small and starts fast (SC-003). The window is closed
  normally and the process exits (FR-010).
- **Alternatives**: Electron (ruled out in the stack comparison: size and memory).

## R12. CI: GitHub Actions matrix, tauri-action bundles, 90-day artifacts

- **Decision**: one workflow `ci.yml` on `push` to main and `pull_request`.
  Job `test` runs `cargo fmt --check`, `cargo clippy -D warnings`,
  `cargo test --workspace` and the UI tests on `ubuntu-latest`, `windows-latest`,
  `macos-14` (Apple Silicon). Job `build` (needs `test`) builds the desktop
  bundles and the CLI for 4 targets: `x86_64-pc-windows-msvc` (NSIS installer),
  `x86_64-apple-darwin` (cross-compiled on macos-14, dmg), `aarch64-apple-darwin`
  (dmg), `x86_64-unknown-linux-gnu` (AppImage). Artifacts uploaded with
  `retention-days: 90`. A launch smoke check runs each CLI with `version --json`
  and the desktop binary with `--print-build-info` (headless flag that prints and
  exits), on its own OS, using `xvfb-run` on Linux.
- **Rationale**: FR-001 to FR-007, FR-024 to FR-027. Cross-compiling the Intel
  Mac build on Apple Silicon avoids depending on the shrinking Intel runner pool;
  the launch check for the Intel build runs under Rosetta.
- **Alternatives**: separate workflows per OS (harder to see the overall result);
  releases instead of artifacts (out of scope; they are for M3).

## R13. Engine independence check

- **Decision**: a CI step runs `cargo tree -p spiderlens-engine -e normal` and
  fails if `tauri` or `wry` appears.
- **Rationale**: makes Principle III a gate instead of a convention.
