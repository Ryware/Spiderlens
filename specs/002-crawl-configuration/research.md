# Research: Crawl Configuration

Each entry: Decision, Rationale, Alternatives considered.

## R1. One typed model, one validator, in Rust

- **Decision**: `CrawlConfig` is a Rust struct tree in `spiderlens-config`.
  `validate(&CrawlConfig, &ModeInput) -> Vec<Issue>` returns every problem
  at once, each with a field path (`scope.include[2]`), a severity (`Error`
  blocks start, `Warning` must be shown) and a plain-language fix. The UI calls
  it through the transport, debounced as the user types.
- **Rationale**: FR-007 and SC-008 need every invalid setting named, so a
  fail-fast `Result` is wrong here; a list of issues is the right shape. One
  validator means the CLI and the desktop can never disagree (FR-036). The
  same issue list carries the "less respectful" warnings (SC-009), so the UI
  cannot forget one: it renders whatever the validator returns.
- **Alternatives**: duplicate validation in TypeScript (two sources of truth);
  JSON Schema validation (cannot express cross-field rules such as "custom user
  agent needs a robots token").

## R2. TypeScript types generated from Rust

- **Decision**: `ts-rs` derives `.ts` files into `ui/src/generated/`. They are
  committed, and CI regenerates them and fails on a diff.
- **Rationale**: like OpenAPI codegen or NSwag in .NET, but without a schema in
  between: the Rust type is the schema.
- **Alternatives**: `specta`/`tauri-specta` (also generates command bindings,
  but its Tauri 2 support is still a release candidate); hand-written types.

## R3. Patterns: the `regex` crate

- **Decision**: include and exclude patterns are compiled with `regex`, matched
  with `is_match` against the full normalized URL (unanchored, as in other
  SEO crawlers). All include patterns form one `RegexSet`, all excludes another.
  The tester uses `RegexSet::matches` to report which pattern decided.
- **Rationale**: `regex` guarantees linear-time matching. A user pattern like
  `(a+)+$` cannot freeze a crawl of a million URLs, which can happen with
  backtracking engines (Java, .NET, PCRE). The cost is that look-around and
  back-references are not supported; the validator explains that when it sees
  them.
- **Alternatives**: `fancy-regex` (supports look-around, but can backtrack
  exponentially); glob patterns (less expressive than users of other SEO
  crawlers expect).

## R4. Registrable domain: `psl`

- **Decision**: `psl` crate, which compiles the Public Suffix List into the
  binary. "This subdomain only" compares exact hosts; "all subdomains"
  compares registrable domains. Bare domain and `www.` are the same site
  (decision 7 in the review) for redirect resolution (FR-003).
- **Rationale**: no network fetch at runtime (Principle II). The list is
  refreshed whenever the dependency is updated.
- **Alternatives**: `publicsuffix` with a list downloaded at runtime (network);
  a naive "last two labels" rule (wrong for `example.co.uk` and `github.io`).

## R5. URL normalization for List mode

- **Decision**: parse with `url` (WHATWG). That lowercases the scheme and host,
  converts IDN hosts to punycode, removes default ports and resolves dot
  segments. On top of it, drop the fragment. Missing scheme becomes `https://`.
  Only `http` and `https` are accepted. The function lives in
  `url_norm.rs`; spec 003 adds its query-parameter rules there.
- **Rationale**: matches the edge case "case of the host, default port,
  fragment are duplicates". The WHATWG parser behaves like the browser.

## R6. List-mode parsing: streaming, off the UI thread

- **Decision**: `parse_list(reader, on_progress) -> ListInput`. Text files are
  read line by line; CSV files go through the `csv` crate and every cell is
  tried as a URL. Encoding: BOM, else UTF-8, else windows-1252 with a notice.
  Dedupe with a `HashSet` of normalized strings. The desktop runs it with
  `spawn_blocking` and emits progress events every 50,000 lines. Invalid lines
  keep their line number and reason; the first 10,000 are kept for display,
  and the count is always exact.
- **Rationale**: SC-002. A million average URLs is about 100 MB in a hash set,
  acceptable for now; spec 006 moves the list into the crawl file.
- **Alternatives**: parse in the webview (JavaScript on the UI thread would
  freeze, and the logic would be duplicated).

## R7. Secret header values: `secrecy` plus a placeholder on disk

- **Decision**: `HeaderValue` is `Plain(String)` or `Secret(SecretString)`.
  `SecretString` prints `[REDACTED]` in `Debug` and wipes its memory on drop.
  Serialization writes `{"secret": true}` with no value, so profiles and crawl
  files never contain it (FR-038). Loading such a header gives
  `Secret(Missing)`, and the validator reports "re-enter the value" as an
  error that blocks starting.
- **Rationale**: the type system enforces the rule. There is no code path that
  serializes the value, so a logging or export bug cannot leak it.
- **Alternatives**: OS keychain (better for convenience later, adds a
  dependency per OS; the spec says re-enter).

## R8. Profile file format and forward compatibility

- **Decision**: JSON:
  `{"format":"spiderlens-profile","format_version":1,"app_version":"0.1.0","name":"…","config":{…},"start_url":null}`.
  Import with `serde_ignored` to collect the paths of unknown fields (newer
  versions), which are shown to the user and dropped. Missing fields take
  defaults (older versions). A wrong `format` or invalid JSON is refused.
- **Rationale**: FR-035, SC-007. JSON is diffable and readable in a code
  review, which is useful for profiles shared across a team.
- **Alternatives**: TOML (nicer by hand, awkward for nested lists of headers
  and patterns); binary formats (not reviewable).

## R9. Profile store: one file per profile

- **Decision**: `ProfileStore::open(dir)`. File name is a slug of the name plus
  `.json`. An `index.json` holds the default profile. Writes go to a temporary
  file and are renamed into place (atomic on all three OSes).
- **Rationale**: a crash mid-save cannot corrupt other profiles. Upgrades keep
  files because the format is versioned (FR-031).

## R10. Presets: derived, not stored

- **Decision**: speed preset name is computed: if the values equal Polite, it
  is Polite; if they equal Fast, Fast; otherwise Custom. The stored config
  includes the derived name for readability, but loading ignores it.
- **Rationale**: FR-021 ("editing any value changes the preset to Custom")
  becomes impossible to get wrong; a stored name and values can never disagree.

## R11. Mid-crawl changes: enforced by type

- **Decision**: `StoredCrawlConfig { locked: CrawlConfig, mode_input,
  resolved_hosts, app_version, started_at, changes: Vec<ConfigChange> }`.
  `ConfigChange` holds a timestamp and either a new `SpeedSettings` or a new
  `RetrySettings`, nothing else. `effective()` applies the changes in order.
- **Rationale**: FR-040 says only speed and retry can change. The change type
  cannot express anything else, so there is no runtime check to forget.

## R12. CLI: same struct, flags as overrides

- **Decision**: `spiderlens config show [--profile FILE] [crawl flags]` prints
  the resolved configuration; `spiderlens config validate` and
  `spiderlens config test-patterns`. Crawl flags (`--max-urls`,
  `--include`, `--speed fast`, `--header`, …) are a clap `Args` struct that is
  flattened into `config` now and into `crawl` in spec 003. Precedence:
  built-in defaults, then profile file, then flags.
- **Rationale**: FR-036 and SC-007. The same merge is used by the desktop when
  a profile is loaded and edited.

## R13. Engine uses the configuration

- **Decision**: `CheckOptions` (spec 001) is replaced by a view built from
  `CrawlConfig` (timeouts, redirect limit, user agent, robots token, headers).
  `spiderlens check` accepts the same shared flags.
- **Rationale**: one place for defaults. Spec 001's defaults are unchanged.
