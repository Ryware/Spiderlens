//! `spiderlens integrations`: connections, runs and the cache for external
//! data (spec 024, specs/024-external-integrations/contracts/cli.md). Uses
//! the same keychain entries, settings and cache as the desktop app.

use std::io::{IsTerminal, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use clap::{Args, Subcommand, ValueEnum};
use spiderlens_integrations::gsc::PageRequest;
use spiderlens_integrations::hub::Hub;
use spiderlens_integrations::run::Control;
use spiderlens_integrations::store::{RunProgress, RunState};
use spiderlens_integrations::{Error, Provider};

use crate::exit;

// `--provider url-inspection` and `--provider analytics` (spec 024 slices 2
// and 4) live in their own file.
#[path = "integrations_google_cmd.rs"]
mod google;
mod perf;

/// A run stopped at the provider's daily allowance; `resume` continues it
/// after the reset time (contracts/cli.md).
pub(crate) const QUOTA: u8 = 10;

#[derive(Subcommand)]
pub enum IntegrationsCommand {
    /// List providers, their settings and connections.
    List {
        #[arg(long)]
        json: bool,
    },
    /// Turn a provider on or off.
    Enable {
        #[arg(value_enum)]
        provider: ProviderArg,
        /// Turn it off instead.
        #[arg(long)]
        off: bool,
    },
    /// Connect a Google account (opens the browser) or save a provider key (read from standard input).
    Connect(ConnectArgs),
    /// Remove a connection: revoke it and delete it from the keychain.
    Forget {
        /// `google` or a key provider id.
        kind: String,
        /// The account email or key name.
        #[arg(long)]
        label: String,
    },
    /// List the Search Console properties, or the Analytics data streams, a Google account can read.
    Properties {
        /// `search-console` (default) or `analytics`.
        #[arg(long, value_enum)]
        provider: Option<ProviderArg>,
        /// The Google account to use when several are connected.
        #[arg(long)]
        account: Option<String>,
        /// Suggest the property covering this crawl file's site.
        #[arg(long, value_name = "FILE")]
        crawl: Option<PathBuf>,
        #[arg(long)]
        json: bool,
    },
    /// Fetch external data for a crawl file.
    Fetch(FetchArgs),
    /// Continue a paused or interrupted run.
    Resume {
        file: PathBuf,
        #[arg(long)]
        run: Option<i64>,
        #[arg(long)]
        json: bool,
    },
    /// List the runs recorded in a crawl file.
    Runs {
        file: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// A page's top search queries (Search Console), fetched on demand.
    Queries {
        file: PathBuf,
        /// The page's address as crawled.
        url: String,
        #[arg(long)]
        json: bool,
    },
    /// Delete one provider's data, or all external data, from a crawl file.
    Remove {
        file: PathBuf,
        #[arg(long, value_enum, required_unless_present = "all")]
        provider: Option<ProviderArg>,
        #[arg(long, conflicts_with = "provider")]
        all: bool,
    },
    /// The response cache outside crawl files.
    #[command(subcommand)]
    Cache(CacheCommand),
    /// Export a results view to a new Google Sheet (and a Looker Studio link).
    ExportSheets(crate::integrations_links_cmd::SheetsArgs),
}

#[derive(Subcommand)]
pub enum CacheCommand {
    /// Size of the cache.
    Info {
        #[arg(long)]
        json: bool,
    },
    /// Empty the cache, or one provider's part of it.
    Clear {
        #[arg(long, value_enum)]
        provider: Option<ProviderArg>,
    },
}

#[derive(Clone, Copy, ValueEnum)]
pub enum ProviderArg {
    SearchConsole,
    UrlInspection,
    Analytics,
    FieldData,
    LabTests,
    Ahrefs,
    Majestic,
    Moz,
    Sheets,
}

impl From<ProviderArg> for Provider {
    fn from(p: ProviderArg) -> Self {
        match p {
            ProviderArg::SearchConsole => Provider::SearchConsole,
            ProviderArg::UrlInspection => Provider::UrlInspection,
            ProviderArg::Analytics => Provider::Analytics,
            ProviderArg::FieldData => Provider::FieldData,
            ProviderArg::LabTests => Provider::LabTests,
            ProviderArg::Ahrefs => Provider::Ahrefs,
            ProviderArg::Majestic => Provider::Majestic,
            ProviderArg::Moz => Provider::Moz,
            ProviderArg::Sheets => Provider::Sheets,
        }
    }
}

#[derive(Clone, Copy, ValueEnum)]
pub enum GoogleFeature {
    SearchConsole,
    Analytics,
    Sheets,
}

#[derive(Args)]
pub struct ConnectArgs {
    /// `google`, `google-key` (one Google API key for field-data and lab-tests), or a key provider:
    /// field-data, lab-tests, ahrefs, majestic, moz.
    target: String,
    /// Google: what to allow. Repeatable; default search-console.
    #[arg(long, value_enum)]
    scope: Vec<GoogleFeature>,
    /// Key providers: a name for the key.
    #[arg(long)]
    label: Option<String>,
    /// Google: print the sign-in link instead of opening the browser.
    #[arg(long)]
    no_browser: bool,
}

#[derive(Args)]
pub struct FetchArgs {
    /// The crawl file.
    file: PathBuf,
    #[arg(long, value_enum)]
    provider: ProviderArg,
    /// Search Console property (default: the one covering the crawled site);
    /// for analytics, the GA4 property id.
    #[arg(long)]
    property: Option<String>,
    /// URL Inspection: `indexable` (default), `all`, or text the address contains.
    #[arg(long, default_value = "indexable")]
    urls: String,
    /// URL Inspection: inspect at most this many URLs.
    #[arg(long)]
    max: Option<u32>,
    /// Analytics: the web data stream id (default: the one matching the crawled site).
    #[arg(long)]
    stream: Option<String>,
    /// Analytics: report landing pages that differ by query string apart.
    #[arg(long)]
    keep_query: bool,
    /// Analytics: the last N days up to yesterday, such as 28d.
    #[arg(long, conflicts_with_all = ["from", "to"])]
    last: Option<String>,
    /// The Google account to use when several are connected.
    #[arg(long)]
    account: Option<String>,
    /// First day, YYYY-MM-DD (default: the last 28 days of final data).
    #[arg(long, requires = "to")]
    from: Option<String>,
    /// Last day, YYYY-MM-DD.
    #[arg(long, requires = "from")]
    to: Option<String>,
    /// Search type: web, image, video, news, discover, googleNews.
    #[arg(long, default_value = "web")]
    r#type: String,
    /// Include fresh data that may still change, and skip the cache.
    #[arg(long)]
    fresh: bool,
    /// Print the final run as JSON.
    #[arg(long)]
    json: bool,
    /// Field data and lab tests: devices, pages and strategy.
    #[command(flatten)]
    perf: perf::PerfArgs,
    #[command(flatten)]
    links: crate::integrations_links_cmd::LinkArgs,
}

fn hub() -> Result<Hub, Error> {
    #[cfg(debug_assertions)]
    if let Some(home) = std::env::var_os("SPIDERLENS_TEST_HOME") {
        return Ok(test_hooks::hub(PathBuf::from(home)));
    }
    Hub::open_default()
}

fn code(e: &Error) -> u8 {
    match e {
        Error::Invalid(_) | Error::NoConnection(_) => exit::USAGE,
        Error::Update(_) | Error::Io(_) | Error::Sql(_) => exit::LOCAL,
        _ => exit::FAILED,
    }
}

fn fail(e: &Error) -> ExitCode {
    eprintln!("error: {e}");
    ExitCode::from(code(e))
}

pub(crate) fn print_json<T: serde::Serialize>(v: &T) {
    println!("{}", serde_json::to_string_pretty(v).expect("serializes"));
}

pub fn run(cmd: IntegrationsCommand) -> ExitCode {
    let h = match hub() {
        Ok(h) => h,
        Err(e) => return fail(&e),
    };
    match go(&h, cmd) {
        Ok(c) => c,
        Err(e) => fail(&e),
    }
}

fn go(h: &Hub, cmd: IntegrationsCommand) -> Result<ExitCode, Error> {
    match cmd {
        IntegrationsCommand::List { json } => {
            let s = h.status();
            if json {
                print_json(&s);
            } else {
                if s.store == spiderlens_integrations::credentials::StoreKind::MemoryOnly {
                    println!("No keychain is available: connections last until this command ends.");
                }
                for p in &s.providers {
                    let state = match (p.available, p.settings.enabled) {
                        (false, _) => "coming later",
                        (true, true) => "on",
                        (true, false) => "off",
                    };
                    let conns = if p.connections.is_empty() {
                        "not connected".to_string()
                    } else {
                        p.connections.join(", ")
                    };
                    println!("{:<32} {:<13} {}", p.label, state, conns);
                }
            }
            Ok(ExitCode::from(exit::FETCHED))
        }
        IntegrationsCommand::Enable { provider, off } => {
            let p = Provider::from(provider);
            let mut s = h
                .status()
                .providers
                .into_iter()
                .find(|s| s.provider == p)
                .expect("listed")
                .settings;
            s.enabled = !off;
            h.save_settings(p, s)?;
            println!("{} is {}.", p.label(), if off { "off" } else { "on" });
            Ok(ExitCode::from(exit::FETCHED))
        }
        IntegrationsCommand::Connect(a) => connect(h, a),
        IntegrationsCommand::Forget { kind, label } => {
            let revoked = h.forget(&kind, &label)?;
            if kind == "google" && !revoked {
                println!(
                    "Removed {label} here. Google did not confirm the revoke; you can also remove access at https://myaccount.google.com/permissions"
                );
            } else {
                println!("Removed {label}.");
            }
            Ok(ExitCode::from(exit::FETCHED))
        }
        IntegrationsCommand::Properties {
            provider,
            account,
            crawl,
            json,
        } => {
            if matches!(provider, Some(ProviderArg::Analytics)) {
                let streams = h.analytics_streams(account.as_deref(), crawl.as_deref())?;
                if json {
                    print_json(&streams);
                } else {
                    for s in streams {
                        println!(
                            "{} --property {} --stream {}{}",
                            s.display,
                            s.property,
                            s.stream,
                            if s.suggested { "  (suggested)" } else { "" }
                        );
                    }
                }
                return Ok(ExitCode::from(exit::FETCHED));
            }
            let props = h.properties(account.as_deref(), crawl.as_deref())?;
            if json {
                print_json(&props);
            } else {
                for p in props {
                    println!("{}{}", p.display, if p.suggested { "  (suggested)" } else { "" });
                }
            }
            Ok(ExitCode::from(exit::FETCHED))
        }
        IntegrationsCommand::Fetch(a) => fetch(h, a),
        IntegrationsCommand::ExportSheets(a) => crate::integrations_links_cmd::export_sheets(h, a),
        IntegrationsCommand::Resume { file, run, json } => {
            let control = pause_on_ctrl_c();
            let p = h.resume(&file, run, &control, &mut progress())?;
            Ok(finish(&p, json))
        }
        IntegrationsCommand::Runs { file, json } => {
            let runs = h.runs(&file)?;
            if json {
                print_json(&runs);
            } else {
                for r in runs {
                    println!(
                        "{:>4}  {:<16} {:<13} {} rows  {}",
                        r.id,
                        r.provider.id(),
                        r.state.as_str(),
                        r.done,
                        r.message.unwrap_or_default()
                    );
                }
            }
            Ok(ExitCode::from(exit::FETCHED))
        }
        IntegrationsCommand::Queries { file, url, json } => {
            let id = url_id(&file, &url)?;
            let rows = h.queries(&file, id)?;
            if json {
                print_json(&rows);
            } else {
                for q in rows {
                    println!(
                        "{:>8} clicks {:>10} impressions  {:>5.1}  {}",
                        q.clicks, q.impressions, q.position, q.query
                    );
                }
            }
            Ok(ExitCode::from(exit::FETCHED))
        }
        IntegrationsCommand::Remove { file, provider, all } => {
            let p = if all { None } else { provider.map(Provider::from) };
            h.remove(&file, p)?;
            println!("Removed {}.", p.map_or("all external data", Provider::label));
            Ok(ExitCode::from(exit::FETCHED))
        }
        IntegrationsCommand::Cache(CacheCommand::Info { json }) => {
            let i = h.cache_info()?;
            if json {
                print_json(&i);
            } else {
                println!("{} entries, {} bytes", i.entries, i.bytes);
            }
            Ok(ExitCode::from(exit::FETCHED))
        }
        IntegrationsCommand::Cache(CacheCommand::Clear { provider }) => {
            let n = h.cache_clear(provider.map(Provider::from))?;
            println!("Removed {n} cached answers.");
            Ok(ExitCode::from(exit::FETCHED))
        }
    }
}

fn connect(h: &Hub, a: ConnectArgs) -> Result<ExitCode, Error> {
    if a.target == "google" {
        let mut features: Vec<&str> = a
            .scope
            .iter()
            .map(|f| match f {
                GoogleFeature::SearchConsole => "search_console",
                GoogleFeature::Analytics => "analytics",
                GoogleFeature::Sheets => "sheets",
            })
            .collect();
        if features.is_empty() {
            features.push("search_console");
        }
        let control = cancel_on_ctrl_c();
        let no_browser = a.no_browser;
        let c = h.connect_google(&features, Duration::from_secs(300), &control, |url| {
            eprintln!("Sign in to Google in your browser. If it does not open, visit:\n{url}");
            #[cfg(debug_assertions)]
            if std::env::var_os("SPIDERLENS_TEST_BROWSER").is_some() {
                test_hooks::follow(url);
                return;
            }
            if !no_browser {
                open_browser(url);
            }
        })?;
        println!("Connected {} ({}).", c.label, c.scopes.join(", "));
        return Ok(ExitCode::from(exit::FETCHED));
    }
    if a.target == "google-key" {
        return perf::connect_key(h, a.label.as_deref());
    }
    let p = Provider::from_id(&a.target.replace('-', "_"))
        .filter(|p| !p.uses_google_account())
        .ok_or_else(|| Error::Invalid(format!("unknown provider {}", a.target)))?;
    let label = a
        .label
        .ok_or_else(|| Error::Invalid("a key needs --label".into()))?;
    if std::io::stdin().is_terminal() {
        eprint!("Paste the {} key and press Enter: ", p.label());
        std::io::stderr().flush().ok();
    }
    // Never from an argument, so it stays out of shell history (contracts/cli.md).
    let mut key = String::new();
    std::io::stdin().read_line(&mut key)?;
    let c = h.save_key(p, &label, key.trim().to_string().into())?;
    println!("Saved {} key {}.", p.label(), c.label);
    Ok(ExitCode::from(exit::FETCHED))
}

fn fetch(h: &Hub, a: FetchArgs) -> Result<ExitCode, Error> {
    let p = Provider::from(a.provider);
    match p {
        Provider::SearchConsole => {}
        Provider::UrlInspection => return google::fetch_inspection(h, a),
        Provider::Analytics => return google::fetch_analytics(h, a),
        Provider::FieldData | Provider::LabTests => return perf::fetch(h, p, &a),
        Provider::Ahrefs | Provider::Majestic | Provider::Moz => {
            let account = a.account.as_deref();
            return crate::integrations_links_cmd::fetch_links(
                h,
                p,
                &a.file,
                account,
                &a.urls,
                &a.links,
                a.fresh,
                a.json,
            );
        }
        _ => {
            return Err(Error::Invalid(format!(
                "{} is not available in this build yet",
                p.label()
            )));
        }
    }
    let property = match a.property {
        Some(p) => p,
        None => h.suggested_property(a.account.as_deref(), &a.file)?,
    };
    let mut req = PageRequest::last_28_days(property);
    if let (Some(from), Some(to)) = (a.from, a.to) {
        for d in [&from, &to] {
            d.parse::<jiff::civil::Date>()
                .map_err(|_| Error::Invalid(format!("{d} is not a date (YYYY-MM-DD)")))?;
        }
        req.start_date = from;
        req.end_date = to;
    }
    req.search_type = a.r#type;
    req.fresh = a.fresh;
    eprintln!(
        "Search Console: {} from {} to {} ({}). {}",
        req.property,
        req.start_date,
        req.end_date,
        req.search_type,
        p.sends()
    );
    let control = pause_on_ctrl_c();
    let r = h.fetch_search_console(&a.file, a.account.as_deref(), &req, &control, &mut progress())?;
    Ok(finish(&r, a.json))
}

pub(crate) fn progress() -> impl FnMut(&RunProgress) {
    let mut last = Instant::now();
    move |p: &RunProgress| {
        if last.elapsed() >= Duration::from_millis(500) {
            last = Instant::now();
            if p.provider == Provider::UrlInspection {
                eprintln!("{} of {} URLs inspected", p.done, p.requested);
            } else {
                eprintln!("{} rows read", p.done);
            }
        }
    }
}

pub(crate) fn finish(p: &RunProgress, json: bool) -> ExitCode {
    if json {
        print_json(p);
    } else {
        let matched = p
            .matched
            .map_or(String::new(), |m| format!(", {m} matched crawled pages"));
        let unmatched = p
            .unmatched
            .filter(|n| *n > 0)
            .map_or(String::new(), |n| format!(", {n} not in the crawl"));
        println!(
            "Run {}: {} ({} rows{matched}{unmatched}).",
            p.id,
            p.state.as_str(),
            p.done
        );
        if let Some(m) = &p.message {
            println!("{m}");
        }
    }
    match p.state {
        RunState::Finished => ExitCode::from(exit::FETCHED),
        RunState::PausedQuota => {
            if let Some(at) = &p.resume_at {
                eprintln!("Allowance used up; resume after {at} with `spiderlens integrations resume`.");
            }
            ExitCode::from(QUOTA)
        }
        RunState::PausedUser | RunState::Cancelled => ExitCode::from(exit::PAUSED),
        _ => ExitCode::from(exit::FAILED),
    }
}

fn url_id(file: &std::path::Path, url: &str) -> Result<i64, Error> {
    let db = rusqlite::Connection::open_with_flags(file, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    db.query_row("SELECT id FROM urls WHERE url = ?1", [url], |r| r.get(0))
        .map_err(|_| Error::Invalid(format!("{url} is not in this crawl")))
}

/// Ctrl-C pauses the run; it can be resumed later.
pub(crate) fn pause_on_ctrl_c() -> Control {
    let control = Control::default();
    let c = control.clone();
    on_ctrl_c(move || c.pause());
    control
}

pub(crate) fn cancel_on_ctrl_c() -> Control {
    let control = Control::default();
    let c = control.clone();
    on_ctrl_c(move || c.cancel());
    control
}

fn on_ctrl_c(f: impl FnOnce() + Send + 'static) {
    std::thread::spawn(move || {
        let Ok(rt) = tokio::runtime::Builder::new_current_thread().enable_all().build() else {
            return;
        };
        if rt.block_on(tokio::signal::ctrl_c()).is_ok() {
            eprintln!("Stopping…");
            f();
        }
    });
}

pub(crate) fn open_browser(url: &str) {
    if !spiderlens_integrations::browser::open(url) {
        eprintln!("Could not open a browser; open the link above yourself.");
    }
}

/// Test builds only: a private folder, a file-backed credential store and
/// local fake Google services, so CLI tests never touch the real keychain
/// or the network.
#[cfg(debug_assertions)]
mod test_hooks {
    use std::collections::BTreeMap;
    use std::path::PathBuf;
    use std::sync::Arc;

    use secrecy::{ExposeSecret, SecretString};
    use spiderlens_integrations::credentials::{CredentialId, CredentialStore, StoreKind};
    use spiderlens_integrations::gsc::Api;
    use spiderlens_integrations::ga4::Ga4Api;
    use spiderlens_integrations::hub::{GoogleApis, Hub};
    use spiderlens_integrations::inspect::InspectApi;
    use spiderlens_integrations::oauth::{Client, Endpoints};
    use spiderlens_integrations::settings::Folder;
    use spiderlens_integrations::{crux, psi};

    struct FileStore(PathBuf);

    impl FileStore {
        fn all(&self) -> BTreeMap<String, String> {
            std::fs::read(&self.0)
                .ok()
                .and_then(|b| serde_json::from_slice(&b).ok())
                .unwrap_or_default()
        }
        fn save(&self, m: &BTreeMap<String, String>) -> spiderlens_integrations::Result<()> {
            std::fs::write(&self.0, serde_json::to_vec(m).expect("serializes"))?;
            Ok(())
        }
    }

    impl CredentialStore for FileStore {
        fn get(&self, id: &CredentialId) -> spiderlens_integrations::Result<Option<SecretString>> {
            Ok(self.all().remove(&id.account()).map(SecretString::from))
        }
        fn set(&self, id: &CredentialId, secret: &SecretString) -> spiderlens_integrations::Result<()> {
            let mut m = self.all();
            m.insert(id.account(), secret.expose_secret().to_string());
            self.save(&m)
        }
        fn delete(&self, id: &CredentialId) -> spiderlens_integrations::Result<()> {
            let mut m = self.all();
            m.remove(&id.account());
            self.save(&m)
        }
        fn kind(&self) -> StoreKind {
            StoreKind::Keychain
        }
    }

    /// Stands in for the browser: follows the consent redirect.
    pub fn follow(url: &str) {
        let url = url.to_string();
        tokio::spawn(async move {
            let _ = reqwest::get(url).await;
        });
    }

    pub fn hub(home: PathBuf) -> Hub {
        let _ = std::fs::create_dir_all(&home);
        let store = Arc::new(FileStore(home.join("test-keychain.json")));
        let h = Hub::new(Folder::new(&home), store, Some(home.join("cache.sqlite")));
        // Fake URL Inspection and Analytics services (spec 024 slices 2 and 4).
        let mut apis = GoogleApis::default();
        if let Ok(base) = std::env::var("SPIDERLENS_TEST_INSPECTION") {
            apis.inspection = InspectApi { base };
        }
        if let Ok(base) = std::env::var("SPIDERLENS_TEST_ANALYTICS") {
            apis.analytics = Ga4Api {
                admin_base: base.clone(),
                data_base: base,
            };
        }
        let h = h.with_google_apis(apis);
        // Field data and lab tests have their own fake (SPIDERLENS_TEST_PERF).
        let h = match std::env::var("SPIDERLENS_TEST_PERF") {
            Ok(base) => h.with_perf(
                crux::Api {
                    base: format!("{base}/crux/v1"),
                },
                psi::Api {
                    base: format!("{base}/psi/v5"),
                },
            ),
            Err(_) => h,
        };
        let h = match std::env::var("SPIDERLENS_TEST_LINKS") {
            Ok(base) => h.with_links(spiderlens_integrations::links::Apis {
                ahrefs: format!("{base}/ahrefs/v3"),
                majestic: format!("{base}/majestic/api/json"),
                moz: format!("{base}/moz/v2"),
            }),
            Err(_) => h,
        };
        let h = match std::env::var("SPIDERLENS_TEST_SHEETS") {
            Ok(base) => h.with_sheets(spiderlens_integrations::sheets::Api {
                base,
                cell_capacity: std::env::var("SPIDERLENS_TEST_SHEETS_CAPACITY")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(spiderlens_integrations::sheets::CELL_CAPACITY),
            }),
            Err(_) => h,
        };
        match std::env::var("SPIDERLENS_TEST_GOOGLE") {
            Ok(base) => h.with_google(
                Endpoints {
                    auth: format!("{base}/o/oauth2/v2/auth"),
                    token: format!("{base}/token"),
                    revoke: format!("{base}/revoke"),
                },
                Api {
                    base: format!("{base}/webmasters/v3"),
                },
                Some(Client {
                    id: "test-client".into(),
                    secret: None,
                }),
            ),
            Err(_) => h,
        }
    }
}
