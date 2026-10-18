//! The hub's link-metrics methods (spec 024 US5): Ahrefs, Majestic and Moz
//! keys, the cost estimate and the runs. Same rules as the other methods:
//! blocking, keychain outside the async part.

use std::path::Path;

use secrecy::SecretString;

use super::{Hub, read_only};
use crate::credentials::CredentialId;
use crate::links::{self, Apis, LinkEstimate, LinkRequest, MetricInfo};
use crate::run::{self, Control};
use crate::settings::Connection;
use crate::store::RunProgress;
use crate::{Error, Provider, Result};

fn is_link_provider(p: Provider) -> bool {
    matches!(p, Provider::Ahrefs | Provider::Majestic | Provider::Moz)
}

impl Hub {
    /// Points the link-data providers elsewhere (tests use local fakes).
    pub fn with_links(mut self, apis: Apis) -> Self {
        self.links = apis;
        self
    }

    /// The metrics a provider offers, for the run panel.
    pub fn link_metrics_catalogue(&self, p: Provider) -> Vec<MetricInfo> {
        links::metric_infos(p)
    }

    /// The saved key to use for `p`: the named one, or the only one.
    fn link_secret(&self, p: Provider, label: Option<&str>) -> Result<(SecretString, String)> {
        if !is_link_provider(p) {
            return Err(Error::Invalid(format!("{} does not use an API key", p.label())));
        }
        let candidates: Vec<Connection> = self
            .folder
            .connections()
            .into_iter()
            .filter(|c| c.kind == p.id())
            .filter(|c| label.is_none_or(|l| l == c.label))
            .collect();
        let c = match candidates.as_slice() {
            [] => return Err(Error::NoConnection(p.label().into())),
            [one] => one.clone(),
            _ => {
                return Err(Error::Invalid(format!(
                    "more than one {} key is saved; choose one",
                    p.label()
                )));
            }
        };
        let secret = self
            .store
            .get(&CredentialId::new(p.id(), &c.label))?
            .ok_or_else(|| Error::NoConnection(format!("{} ({}; save the key again)", p.label(), c.label)))?;
        Ok((secret, c.label))
    }

    /// What a run would cost, in the provider's own units, before anything is
    /// sent (FR-029). Cached pages cost nothing.
    pub fn estimate_links(
        &self,
        crawl: &Path,
        label: Option<&str>,
        req: &LinkRequest,
    ) -> Result<LinkEstimate> {
        let (_, label) = self.link_secret(req.provider, label)?;
        let db = read_only(crawl)?;
        let ctx = self.ctx(req.provider, req.refresh);
        links::estimate(&db, &ctx, &label, req)
    }

    /// Fetches link metrics into `crawl`, stopping at the request's cap (or
    /// the estimate plus 10%). Refused while a crawl holds the file (FR-002).
    pub fn fetch_links(
        &self,
        crawl: &Path,
        label: Option<&str>,
        req: &LinkRequest,
        control: &Control,
        on_progress: &mut dyn FnMut(&RunProgress),
    ) -> Result<RunProgress> {
        self.require_enabled(req.provider)?;
        let (secret, label) = self.link_secret(req.provider, label)?;
        let update = spiderlens_engine::crawl::open_for_update(crawl, None)?;
        let ctx = self.ctx(req.provider, req.refresh);
        run::block_on(links::run_links(
            &ctx,
            &secret,
            &self.links,
            &update,
            &label,
            req,
            None,
            control,
            on_progress,
        ))
    }

    /// Continues a paused or interrupted link-metrics run.
    pub(super) fn resume_link_run(
        &self,
        crawl: &Path,
        r: &RunProgress,
        control: &Control,
        on_progress: &mut dyn FnMut(&RunProgress),
    ) -> Result<RunProgress> {
        let req: LinkRequest =
            serde_json::from_value(r.params.clone()).map_err(|e| Error::Invalid(e.to_string()))?;
        self.require_enabled(r.provider)?;
        let (secret, label) = self.link_secret(r.provider, r.account_label.as_deref())?;
        let update = spiderlens_engine::crawl::open_for_update(crawl, None)?;
        let ctx = self.ctx(r.provider, req.refresh);
        run::block_on(links::run_links(
            &ctx,
            &secret,
            &self.links,
            &update,
            &label,
            &req,
            Some(r.id),
            control,
            on_progress,
        ))
    }
}
