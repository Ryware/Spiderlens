//! Hub methods for URL Inspection and Google Analytics 4 (spec 024 slices 2
//! and 4). A child module of `hub`, so it reaches the hub's private
//! connection, settings and run helpers.

use std::path::Path;

use jiff::Timestamp;

use super::{Hub, read_only, start_url};
use crate::ga4::{self, Ga4Api, Ga4Request, Ga4Stream};
use crate::inspect::{self, InspectApi, InspectPlan, InspectRequest};
use crate::limits::{Ledger, provider_day};
use crate::run::{self, Control};
use crate::store::RunProgress;
use crate::{Error, Provider, Result};

/// Where the two Google APIs live (tests point them at local fakes).
#[derive(Debug, Clone, Default)]
pub struct GoogleApis {
    pub inspection: InspectApi,
    pub analytics: Ga4Api,
}

impl Hub {
    /// Points the URL Inspection and Analytics calls elsewhere (tests use
    /// local fakes).
    pub fn with_google_apis(mut self, apis: GoogleApis) -> Self {
        self.apis = apis;
        self
    }

    fn ledger(&self) -> Ledger {
        Ledger::new(self.folder.usage_path())
    }

    /// How many URLs a request selects against today's remaining allowance
    /// for the property (US2.1, US2.4).
    pub fn inspection_plan(
        &self,
        crawl: &Path,
        account: Option<&str>,
        req: &InspectRequest,
    ) -> Result<InspectPlan> {
        let (_session, label) = self.google_session(Provider::UrlInspection, account)?;
        let daily = inspect::daily_of(&self.ctx(Provider::UrlInspection, false));
        let now = Timestamp::now();
        let used = self.ledger().used(
            &Ledger::key(Provider::UrlInspection, &label, &req.property),
            &provider_day(now, daily.tz),
        );
        let db = read_only(crawl)?;
        inspect::plan(&db, req, daily, used, now)
    }

    /// Inspects the chosen crawled URLs with Google (US2). Refused while a
    /// crawl holds the file (FR-002).
    pub fn fetch_inspection(
        &self,
        crawl: &Path,
        account: Option<&str>,
        req: &InspectRequest,
        control: &Control,
        on_progress: &mut dyn FnMut(&RunProgress),
    ) -> Result<RunProgress> {
        self.require_enabled(Provider::UrlInspection)?;
        let (session, label) = self.google_session(Provider::UrlInspection, account)?;
        let update = spiderlens_engine::crawl::open_for_update(crawl, None)?;
        let ctx = self.ctx(Provider::UrlInspection, req.fresh);
        let ledger = self.ledger();
        run::block_on(inspect::run_inspection(
            &ctx,
            &session,
            &self.apis.inspection,
            &update,
            &ledger,
            &label,
            req,
            None,
            control,
            &Timestamp::now,
            on_progress,
        ))
    }

    /// The web data streams the account can read, the one for the crawled
    /// site first (US4.1).
    pub fn analytics_streams(&self, account: Option<&str>, crawl: Option<&Path>) -> Result<Vec<Ga4Stream>> {
        let start = crawl.map(start_url).transpose()?.flatten();
        let (session, _) = self.google_session(Provider::Analytics, account)?;
        let ctx = self.ctx(Provider::Analytics, false);
        run::block_on(ga4::streams(
            &ctx,
            &session,
            &self.apis.analytics,
            start.as_deref(),
        ))
    }

    /// The stream to use when none was chosen: the suggested one.
    pub fn suggested_stream(&self, account: Option<&str>, crawl: &Path) -> Result<Ga4Stream> {
        self.analytics_streams(account, Some(crawl))?
            .into_iter()
            .find(|s| s.suggested)
            .ok_or_else(|| Error::Invalid("no Analytics data stream matches this site; choose one".into()))
    }

    /// Fetches sessions per landing page into `crawl` (US4).
    pub fn fetch_analytics(
        &self,
        crawl: &Path,
        account: Option<&str>,
        req: &Ga4Request,
        control: &Control,
        on_progress: &mut dyn FnMut(&RunProgress),
    ) -> Result<RunProgress> {
        self.require_enabled(Provider::Analytics)?;
        let (session, label) = self.google_session(Provider::Analytics, account)?;
        let update = spiderlens_engine::crawl::open_for_update(crawl, None)?;
        let ctx = self.ctx(Provider::Analytics, req.fresh);
        run::block_on(ga4::run_pages(
            &ctx,
            &session,
            &self.apis.analytics,
            &update,
            &label,
            req,
            None,
            control,
            on_progress,
        ))
    }

    /// Continues a paused URL Inspection or Analytics run; `Hub::resume`
    /// hands these providers here.
    pub(super) fn resume_google(
        &self,
        crawl: &Path,
        r: &RunProgress,
        control: &Control,
        on_progress: &mut dyn FnMut(&RunProgress),
    ) -> Result<RunProgress> {
        let invalid = |e: serde_json::Error| Error::Invalid(e.to_string());
        match r.provider {
            Provider::UrlInspection => {
                let req: InspectRequest = serde_json::from_value(r.params.clone()).map_err(invalid)?;
                let (session, label) = self.google_session(Provider::UrlInspection, r.account_label.as_deref())?;
                let update = spiderlens_engine::crawl::open_for_update(crawl, None)?;
                let ctx = self.ctx(Provider::UrlInspection, req.fresh);
                let ledger = self.ledger();
                run::block_on(inspect::run_inspection(
                    &ctx,
                    &session,
                    &self.apis.inspection,
                    &update,
                    &ledger,
                    &label,
                    &req,
                    Some(r.id),
                    control,
                    &Timestamp::now,
                    on_progress,
                ))
            }
            Provider::Analytics => {
                let req: Ga4Request = serde_json::from_value(r.params.clone()).map_err(invalid)?;
                let (session, label) = self.google_session(Provider::Analytics, r.account_label.as_deref())?;
                let update = spiderlens_engine::crawl::open_for_update(crawl, None)?;
                let ctx = self.ctx(Provider::Analytics, req.fresh);
                run::block_on(ga4::run_pages(
                    &ctx,
                    &session,
                    &self.apis.analytics,
                    &update,
                    &label,
                    &req,
                    Some(r.id),
                    control,
                    on_progress,
                ))
            }
            p => Err(Error::Invalid(format!("{} is not available in this build", p.label()))),
        }
    }
}
