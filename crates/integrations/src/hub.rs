//! One entry point for the desktop app and the CLI (contracts/cli.md,
//! contracts/desktop-commands.md): connections, settings, runs and the
//! cache, so both front ends behave the same.
//!
//! Every method is blocking. The keychain is only touched outside the async
//! parts, because the Linux keychain client runs its own runtime; network
//! parts run on a current-thread runtime (`run::block_on`). Callers run these
//! on a worker thread.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use jiff::Timestamp;
use rusqlite::{Connection as Db, OpenFlags};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::cache::Cache;
use crate::credentials::{self, CredentialId, CredentialStore, StoreKind};
use crate::gsc::{self, PageRequest, Property, QueryRow};
use crate::http::Http;
use crate::limits::Limits;
use crate::oauth::{self, Client, Endpoints, Session, StoredGoogle};
use crate::run::{self, Control, Ctx};
use crate::settings::{Connection, Folder, IntegrationSettings, ProviderSettings};
use crate::store::{self, OrphanCandidate, RunProgress};
use crate::{Error, Provider, Result, links, sheets};

mod link_runs;
mod sheet_export;

/// What Settings › Integrations shows for one provider.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProviderStatus {
    pub provider: Provider,
    pub label: String,
    /// What leaves this computer when the provider runs (FR-003).
    pub sends: String,
    pub terms_url: String,
    pub uses_google_account: bool,
    /// Implemented in this build; the others are shown as coming later.
    pub available: bool,
    pub settings: ProviderSettings,
    pub per_minute: u32,
    pub concurrency: u32,
    pub published_per_minute: u32,
    pub published_concurrency: u32,
    /// Connections that can serve this provider.
    pub connections: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Status {
    pub store: StoreKind,
    /// A Google client is compiled into this build or the user saved one.
    pub google_client: bool,
    pub own_google_client: bool,
    pub connections: Vec<Connection>,
    pub providers: Vec<ProviderStatus>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CacheInfo {
    #[ts(type = "number")]
    pub bytes: u64,
    #[ts(type = "number")]
    pub entries: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct OrphanPage {
    pub rows: Vec<OrphanCandidate>,
    #[ts(type = "number")]
    pub total: i64,
}

/// Providers this build can run.
pub const AVAILABLE: &[Provider] = &[
    Provider::SearchConsole,
    Provider::Ahrefs,
    Provider::Majestic,
    Provider::Moz,
    Provider::Sheets,
];

const OWN_CLIENT: (&str, &str) = ("google_client", "own");

pub struct Hub {
    folder: Folder,
    store: Arc<dyn CredentialStore>,
    endpoints: Endpoints,
    webmasters: gsc::Api,
    cache_path: Option<PathBuf>,
    built_in: Option<Client>,
    links: links::Apis,
    sheets: sheets::Api,
}

impl Hub {
    /// The app's own folder, keychain and cache.
    pub fn open_default() -> Result<Self> {
        let folder = Folder::default_location()
            .ok_or_else(|| Error::Invalid("no settings folder on this system".into()))?;
        Ok(Self::new(
            folder,
            credentials::default_store(),
            Cache::default_path(),
        ))
    }

    pub fn new(folder: Folder, store: Arc<dyn CredentialStore>, cache_path: Option<PathBuf>) -> Self {
        Self {
            folder,
            store,
            endpoints: Endpoints::default(),
            webmasters: gsc::Api::default(),
            cache_path,
            built_in: Client::built_in(),
            links: links::Apis::default(),
            sheets: sheets::Api::default(),
        }
    }

    /// Points Google calls elsewhere (tests use local fakes).
    pub fn with_google(mut self, endpoints: Endpoints, webmasters: gsc::Api, client: Option<Client>) -> Self {
        self.endpoints = endpoints;
        self.webmasters = webmasters;
        self.built_in = client;
        self
    }

    pub fn store_kind(&self) -> StoreKind {
        self.store.kind()
    }

    fn cache(&self) -> Option<Cache> {
        self.cache_path.as_deref().and_then(|p| Cache::open(p).ok())
    }

    fn limits(&self, p: Provider) -> Limits {
        Limits::defaults(p).with_settings(&self.folder.settings().provider(p))
    }

    fn ctx(&self, p: Provider, refresh: bool) -> Ctx {
        let mut ctx = Ctx::new(p, self.limits(p), self.cache());
        ctx.refresh = refresh;
        ctx
    }

    pub fn status(&self) -> Status {
        let settings = self.folder.settings();
        let connections = self.folder.connections();
        let providers = Provider::ALL
            .into_iter()
            .map(|p| self.provider_status(p, &settings, &connections))
            .collect();
        Status {
            store: self.store.kind(),
            google_client: self.built_in.is_some() || settings.own_google_client,
            own_google_client: settings.own_google_client,
            connections,
            providers,
        }
    }

    fn provider_status(
        &self,
        p: Provider,
        settings: &IntegrationSettings,
        connections: &[Connection],
    ) -> ProviderStatus {
        let s = settings.provider(p);
        let l = Limits::defaults(p).with_settings(&s);
        ProviderStatus {
            provider: p,
            label: p.label().into(),
            sends: p.sends().into(),
            terms_url: p.terms_url().into(),
            uses_google_account: p.uses_google_account(),
            available: AVAILABLE.contains(&p),
            settings: s,
            per_minute: l.per_minute,
            concurrency: l.concurrency,
            published_per_minute: l.published_per_minute,
            published_concurrency: l.published_concurrency,
            connections: connections
                .iter()
                .filter(|c| serves(c, p))
                .map(|c| c.label.clone())
                .collect(),
        }
    }

    /// Turns a provider on or off and sets its limits (FR-001, FR-004).
    pub fn save_settings(&self, p: Provider, s: ProviderSettings) -> Result<ProviderStatus> {
        let mut all = self.folder.settings();
        all.set_provider(p, s);
        self.folder.save_settings(&all)?;
        Ok(self.provider_status(p, &all, &self.folder.connections()))
    }

    /// Saves the user's own Google client in the keychain (FR-016).
    pub fn save_google_client(&self, client: Option<(String, Option<SecretString>)>) -> Result<()> {
        let id = CredentialId::new(OWN_CLIENT.0, OWN_CLIENT.1);
        let mut all = self.folder.settings();
        match client {
            Some((client_id, secret)) => {
                let stored = StoredGoogle {
                    client_id,
                    client_secret: secret.map(|s| s.expose_secret().to_string()),
                    refresh_token: String::new(),
                };
                self.store.set(&id, &stored.to_secret())?;
                all.own_google_client = true;
            }
            None => {
                self.store.delete(&id)?;
                all.own_google_client = false;
            }
        }
        self.folder.save_settings(&all)
    }

    fn google_client(&self) -> Result<Client> {
        if self.folder.settings().own_google_client {
            let s = self
                .store
                .get(&CredentialId::new(OWN_CLIENT.0, OWN_CLIENT.1))?
                .ok_or_else(|| Error::Auth("your own Google client is missing; enter it again".into()))?;
            return Ok(StoredGoogle::from_secret(&s)?.client());
        }
        self.built_in.clone().ok_or_else(|| {
            Error::Auth("this build has no Google client; add your own in Settings › Integrations".into())
        })
    }

    /// Signs in to Google for `features` plus every feature already granted,
    /// since Google gets one consent for all scopes (plan D3). `open` gets the
    /// sign-in link. Returns early when `cancel` is cancelled.
    pub fn connect_google(
        &self,
        features: &[&str],
        timeout: Duration,
        cancel: &Control,
        open: impl FnOnce(&str),
    ) -> Result<Connection> {
        let client = self.google_client()?;
        let mut wanted: Vec<String> = features.iter().map(|s| s.to_string()).collect();
        for c in self.folder.connections().iter().filter(|c| c.kind == "google") {
            for s in &c.scopes {
                if !wanted.contains(s) {
                    wanted.push(s.clone());
                }
            }
        }
        let scopes: Vec<&str> = wanted
            .iter()
            .map(|f| oauth::scope_for(f).ok_or_else(|| Error::Invalid(format!("unknown Google feature {f}"))))
            .collect::<Result<_>>()?;
        let http = Http::new(Duration::from_secs(30));
        let a = run::block_on(async {
            let sign_in = oauth::authorize(&http, &client, &self.endpoints, &scopes, timeout, open);
            let cancelled = async {
                while !cancel.cancelled() {
                    tokio::time::sleep(Duration::from_millis(200)).await;
                }
            };
            tokio::select! {
                r = sign_in => r,
                () = cancelled => Err(Error::Auth("sign-in cancelled".into())),
            }
        })?;
        let granted: Vec<String> = wanted
            .into_iter()
            .filter(|f| oauth::scope_for(f).is_some_and(|s| a.scopes.iter().any(|g| g == s)))
            .collect();
        let stored = StoredGoogle {
            client_id: client.id.clone(),
            client_secret: client.secret.as_ref().map(|s| s.expose_secret().to_string()),
            refresh_token: a.refresh_token.expose_secret().to_string(),
        };
        self.store
            .set(&CredentialId::new("google", &a.email), &stored.to_secret())?;
        self.upsert(Connection {
            kind: "google".into(),
            label: a.email,
            scopes: granted,
            created_at: Timestamp::now().to_string(),
            store: self.store.kind(),
        })
    }

    /// Saves an API key for a key-based provider (FR-013).
    pub fn save_key(&self, p: Provider, label: &str, key: SecretString) -> Result<Connection> {
        if p.uses_google_account() {
            return Err(Error::Invalid(format!(
                "{} uses a Google account, not a key",
                p.label()
            )));
        }
        let label = label.trim();
        if label.is_empty() || key.expose_secret().trim().is_empty() {
            return Err(Error::Invalid("a key needs a name and a value".into()));
        }
        self.store.set(&CredentialId::new(p.id(), label), &key)?;
        self.upsert(Connection {
            kind: p.id().into(),
            label: label.into(),
            scopes: Vec::new(),
            created_at: Timestamp::now().to_string(),
            store: self.store.kind(),
        })
    }

    fn upsert(&self, c: Connection) -> Result<Connection> {
        let mut all = self.folder.connections();
        all.retain(|x| !(x.kind == c.kind && x.label == c.label));
        all.push(c.clone());
        self.folder.save_connections(&all)?;
        Ok(c)
    }

    /// Removes a connection: revokes a Google sign-in at Google, deletes the
    /// keychain entry and the listing (FR-017). Returns whether Google
    /// confirmed the revoke; the local copy is gone either way.
    pub fn forget(&self, kind: &str, label: &str) -> Result<bool> {
        let id = CredentialId::new(kind, label);
        let secret = self.store.get(&id)?;
        self.store.delete(&id)?;
        let mut all = self.folder.connections();
        all.retain(|x| !(x.kind == kind && x.label == label));
        self.folder.save_connections(&all)?;
        let mut revoked = false;
        if kind == "google"
            && let Some(s) = secret
            && let Ok(stored) = StoredGoogle::from_secret(&s)
        {
            let http = Http::new(Duration::from_secs(15));
            let token = SecretString::from(stored.refresh_token);
            revoked = run::block_on(oauth::revoke(&http, &self.endpoints, &token)).is_ok();
        }
        Ok(revoked)
    }

    /// The Google connection to use for `p`: the named one, or the only one
    /// that has the scope.
    fn google_session(&self, p: Provider, account: Option<&str>) -> Result<(Session, String)> {
        let feature = match p {
            Provider::SearchConsole | Provider::UrlInspection => "search_console",
            Provider::Analytics => "analytics",
            Provider::Sheets => "sheets",
            _ => {
                return Err(Error::Invalid(format!(
                    "{} does not use a Google account",
                    p.label()
                )));
            }
        };
        let candidates: Vec<Connection> = self
            .folder
            .connections()
            .into_iter()
            .filter(|c| c.kind == "google" && c.scopes.iter().any(|s| s == feature))
            .filter(|c| account.is_none_or(|a| a == c.label))
            .collect();
        let c = match candidates.as_slice() {
            [] => return Err(Error::NoConnection(p.label().into())),
            [one] => one.clone(),
            _ => {
                return Err(Error::Invalid(format!(
                    "more than one Google account can read {}; choose one",
                    p.label()
                )));
            }
        };
        let secret = self
            .store
            .get(&CredentialId::new("google", &c.label))?
            .ok_or_else(|| Error::NoConnection(format!("{} ({}; connect it again)", p.label(), c.label)))?;
        let stored = StoredGoogle::from_secret(&secret)?;
        Ok((
            Session::new(Http::new(Duration::from_secs(30)), self.endpoints.clone(), stored),
            c.label,
        ))
    }

    /// Search Console properties, the one covering `crawl`'s start URL first.
    pub fn properties(&self, account: Option<&str>, crawl: Option<&Path>) -> Result<Vec<Property>> {
        let start = crawl.map(start_url).transpose()?.flatten();
        let (session, _) = self.google_session(Provider::SearchConsole, account)?;
        let ctx = self.ctx(Provider::SearchConsole, false);
        run::block_on(gsc::properties(
            &ctx,
            &session,
            &self.webmasters,
            start.as_deref(),
        ))
    }

    /// The property to use when none was chosen: the suggested one.
    pub fn suggested_property(&self, account: Option<&str>, crawl: &Path) -> Result<String> {
        self.properties(account, Some(crawl))?
            .into_iter()
            .find(|p| p.suggested)
            .map(|p| p.id)
            .ok_or_else(|| Error::Invalid("no Search Console property covers this site; choose one".into()))
    }

    /// Fetches Search Console page data into `crawl` (US1). Refused while a
    /// crawl holds the file (FR-002).
    pub fn fetch_search_console(
        &self,
        crawl: &Path,
        account: Option<&str>,
        req: &PageRequest,
        control: &Control,
        on_progress: &mut dyn FnMut(&RunProgress),
    ) -> Result<RunProgress> {
        self.require_enabled(Provider::SearchConsole)?;
        let (session, label) = self.google_session(Provider::SearchConsole, account)?;
        let update = spiderlens_engine::crawl::open_for_update(crawl, None)?;
        let ctx = self.ctx(Provider::SearchConsole, req.fresh);
        run::block_on(gsc::run_pages(
            &ctx,
            &session,
            &self.webmasters,
            &update,
            &label,
            req,
            None,
            control,
            on_progress,
        ))
    }

    /// Continues a paused or interrupted run, the latest one by default (FR-005).
    pub fn resume(
        &self,
        crawl: &Path,
        run_id: Option<i64>,
        control: &Control,
        on_progress: &mut dyn FnMut(&RunProgress),
    ) -> Result<RunProgress> {
        let runs = self.runs(crawl)?;
        let r = match run_id {
            Some(id) => runs.into_iter().find(|r| r.id == id),
            None => runs.into_iter().find(|r| r.state.is_resumable()),
        }
        .ok_or_else(|| Error::Invalid("no paused run in this crawl".into()))?;
        if !r.state.is_resumable() {
            return Err(Error::Invalid(format!(
                "run {} is {}; start a new one",
                r.id,
                r.state.as_str()
            )));
        }
        match r.provider {
            Provider::SearchConsole => {
                let req: PageRequest =
                    serde_json::from_value(r.params.clone()).map_err(|e| Error::Invalid(e.to_string()))?;
                let (session, label) =
                    self.google_session(Provider::SearchConsole, r.account_label.as_deref())?;
                let update = spiderlens_engine::crawl::open_for_update(crawl, None)?;
                let ctx = self.ctx(Provider::SearchConsole, req.fresh);
                run::block_on(gsc::run_pages(
                    &ctx,
                    &session,
                    &self.webmasters,
                    &update,
                    &label,
                    &req,
                    Some(r.id),
                    control,
                    on_progress,
                ))
            }
            Provider::Ahrefs | Provider::Majestic | Provider::Moz => {
                self.resume_link_run(crawl, &r, control, on_progress)
            }
            p => Err(Error::Invalid(format!(
                "{} is not available in this build",
                p.label()
            ))),
        }
    }

    fn require_enabled(&self, p: Provider) -> Result<()> {
        if self.folder.settings().provider(p).enabled {
            Ok(())
        } else {
            Err(Error::Invalid(format!(
                "{} is off; turn it on in Settings › Integrations first",
                p.label()
            )))
        }
    }

    /// Runs recorded in `crawl`, newest first.
    pub fn runs(&self, crawl: &Path) -> Result<Vec<RunProgress>> {
        let db = read_only(crawl)?;
        if !store::has_tables(&db)? {
            return Ok(Vec::new());
        }
        store::runs(&db)
    }

    /// A page's top search queries, fetched on demand (US1.6).
    pub fn queries(&self, crawl: &Path, url_id: i64) -> Result<Vec<QueryRow>> {
        let label = self
            .runs(crawl)?
            .into_iter()
            .find(|r| r.provider == Provider::SearchConsole)
            .and_then(|r| r.account_label);
        let (session, label) = self.google_session(Provider::SearchConsole, label.as_deref())?;
        let update = spiderlens_engine::crawl::open_for_update(crawl, None)?;
        let ctx = self.ctx(Provider::SearchConsole, false);
        run::block_on(gsc::top_queries(
            &ctx,
            &session,
            &self.webmasters,
            update.conn(),
            &label,
            url_id,
        ))
    }

    pub fn orphans(&self, crawl: &Path, source: Option<&str>, offset: i64, limit: i64) -> Result<OrphanPage> {
        let db = read_only(crawl)?;
        if !store::has_tables(&db)? {
            return Ok(OrphanPage {
                rows: Vec::new(),
                total: 0,
            });
        }
        let (rows, total) = store::orphans(&db, source, offset, limit)?;
        Ok(OrphanPage { rows, total })
    }

    /// Deletes one provider's data, or all external data, from `crawl` (FR-011).
    pub fn remove(&self, crawl: &Path, provider: Option<Provider>) -> Result<()> {
        let update = spiderlens_engine::crawl::open_for_update(crawl, None)?;
        store::remove(update.conn(), provider)
    }

    pub fn cache_info(&self) -> Result<CacheInfo> {
        let Some(c) = self.cache() else {
            return Ok(CacheInfo { bytes: 0, entries: 0 });
        };
        let (bytes, entries) = c.size()?;
        Ok(CacheInfo { bytes, entries })
    }

    pub fn cache_clear(&self, provider: Option<Provider>) -> Result<usize> {
        match self.cache() {
            Some(c) => c.clear(provider),
            None => Ok(0),
        }
    }
}

fn serves(c: &Connection, p: Provider) -> bool {
    match p {
        Provider::SearchConsole | Provider::UrlInspection => {
            c.kind == "google" && c.scopes.iter().any(|s| s == "search_console")
        }
        Provider::Analytics => c.kind == "google" && c.scopes.iter().any(|s| s == "analytics"),
        Provider::Sheets => c.kind == "google" && c.scopes.iter().any(|s| s == "sheets"),
        other => c.kind == other.id(),
    }
}

fn read_only(path: &Path) -> Result<Db> {
    Ok(Db::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?)
}

fn start_url(path: &Path) -> Result<Option<String>> {
    let db = read_only(path)?;
    match db.query_row("SELECT value FROM meta WHERE key = 'start_url'", [], |r| r.get(0)) {
        Ok(v) => Ok(Some(v)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}
