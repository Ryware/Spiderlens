//! Field data and lab tests through the hub (spec 024 US3): the Google API
//! key, estimates, runs and resuming. A child of `hub` so it shares the hub's
//! private state.
//!
//! One Google Cloud API key can serve both providers when the Chrome UX
//! Report API and the PageSpeed Insights API are both enabled for its
//! project, so [`Hub::save_google_api_key`] stores it under both. The key
//! stays in the keychain (FR-013) and is read before any network work starts,
//! outside the async parts.

use std::path::Path;

use secrecy::SecretString;

use crate::credentials::CredentialId;
use crate::crux::{self, FieldRequest};
use crate::limits::Ledger;
use crate::perf::{self, Allowance, PerfEstimate};
use crate::psi::{self, LabRequest};
use crate::run::{self, Control};
use crate::settings::Connection;
use crate::store::RunProgress;
use crate::{Error, Provider, Result};

use super::{Hub, read_only, serves};

impl Hub {
    /// Points field data and lab tests elsewhere (tests use local fakes).
    pub fn with_perf(mut self, crux: crux::Api, psi: psi::Api) -> Self {
        self.crux_api = crux;
        self.psi_api = psi;
        self
    }

    /// Saves one Google API key for both field data and lab tests.
    pub fn save_google_api_key(&self, label: &str, key: SecretString) -> Result<Vec<Connection>> {
        let field = self.save_key(Provider::FieldData, label, key.clone())?;
        let lab = self.save_key(Provider::LabTests, label, key)?;
        Ok(vec![field, lab])
    }

    /// The key to use for `p`: the named one, or the only one saved.
    fn api_key(&self, p: Provider, label: Option<&str>) -> Result<(SecretString, String)> {
        let found: Vec<Connection> = self
            .folder
            .connections()
            .into_iter()
            .filter(|c| serves(c, p))
            .filter(|c| label.is_none_or(|l| l == c.label))
            .collect();
        let c = match found.as_slice() {
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
            .ok_or_else(|| Error::NoConnection(format!("{} ({}; save it again)", p.label(), c.label)))?;
        Ok((secret, c.label))
    }

    fn allowance(&self, label: &str) -> Option<Allowance> {
        let daily = self.limits(Provider::LabTests).daily?;
        Some(Allowance::new(
            Ledger::new(self.folder.usage_path()),
            Ledger::key(Provider::LabTests, label, "-"),
            daily,
        ))
    }

    /// What a field data run costs, shown before it starts (FR-004).
    pub fn estimate_field_data(&self, crawl: &Path, req: &FieldRequest) -> Result<PerfEstimate> {
        let db = read_only(crawl)?;
        let pages = perf::select_urls(&db, &req.urls, req.max)?.ids.len();
        Ok(crux::estimate(
            pages,
            req.devices.len(),
            &self.limits(Provider::FieldData),
        ))
    }

    /// What a lab test run costs and how long it takes.
    pub fn estimate_lab_tests(&self, crawl: &Path, req: &LabRequest) -> Result<PerfEstimate> {
        let db = read_only(crawl)?;
        let pages = perf::select_urls(&db, &req.urls, req.max)?.ids.len();
        let devices = req.strategy.strategies().len();
        Ok(psi::estimate(pages, devices, &self.limits(Provider::LabTests)))
    }

    /// Fetches field data into `crawl` (US3.1, US3.2). Refused while a crawl
    /// holds the file (FR-002).
    pub fn fetch_field_data(
        &self,
        crawl: &Path,
        key_label: Option<&str>,
        req: &FieldRequest,
        control: &Control,
        on_progress: &mut dyn FnMut(&RunProgress),
    ) -> Result<RunProgress> {
        self.require_enabled(Provider::FieldData)?;
        let (key, label) = self.api_key(Provider::FieldData, key_label)?;
        let update = spiderlens_engine::crawl::open_for_update(crawl, None)?;
        let ctx = self.ctx(Provider::FieldData, req.fresh);
        run::block_on(crux::run_field_data(
            &ctx,
            &key,
            &self.crux_api,
            &update,
            &label,
            req,
            None,
            control,
            on_progress,
        ))
    }

    /// Runs lab tests into `crawl` (US3.3).
    pub fn fetch_lab_tests(
        &self,
        crawl: &Path,
        key_label: Option<&str>,
        req: &LabRequest,
        control: &Control,
        on_progress: &mut dyn FnMut(&RunProgress),
    ) -> Result<RunProgress> {
        self.require_enabled(Provider::LabTests)?;
        let (key, label) = self.api_key(Provider::LabTests, key_label)?;
        let update = spiderlens_engine::crawl::open_for_update(crawl, None)?;
        let ctx = self.ctx(Provider::LabTests, req.fresh);
        let allowance = self.allowance(&label);
        run::block_on(psi::run_lab_tests(
            &ctx,
            &key,
            &self.psi_api,
            allowance.as_ref(),
            &update,
            &label,
            req,
            None,
            control,
            on_progress,
        ))
    }

    /// Continues a paused or interrupted field data or lab test run from
    /// the pages still waiting in its queue (FR-005).
    pub(super) fn resume_perf(
        &self,
        crawl: &Path,
        r: &RunProgress,
        control: &Control,
        on_progress: &mut dyn FnMut(&RunProgress),
    ) -> Result<RunProgress> {
        let bad = |e: serde_json::Error| Error::Invalid(e.to_string());
        match r.provider {
            Provider::FieldData => {
                let req: FieldRequest = serde_json::from_value(r.params.clone()).map_err(bad)?;
                let (key, label) = self.api_key(Provider::FieldData, r.account_label.as_deref())?;
                let update = spiderlens_engine::crawl::open_for_update(crawl, None)?;
                let ctx = self.ctx(Provider::FieldData, req.fresh);
                run::block_on(crux::run_field_data(
                    &ctx,
                    &key,
                    &self.crux_api,
                    &update,
                    &label,
                    &req,
                    Some(r.id),
                    control,
                    on_progress,
                ))
            }
            Provider::LabTests => {
                let req: LabRequest = serde_json::from_value(r.params.clone()).map_err(bad)?;
                let (key, label) = self.api_key(Provider::LabTests, r.account_label.as_deref())?;
                let update = spiderlens_engine::crawl::open_for_update(crawl, None)?;
                let ctx = self.ctx(Provider::LabTests, req.fresh);
                let allowance = self.allowance(&label);
                run::block_on(psi::run_lab_tests(
                    &ctx,
                    &key,
                    &self.psi_api,
                    allowance.as_ref(),
                    &update,
                    &label,
                    &req,
                    Some(r.id),
                    control,
                    on_progress,
                ))
            }
            p => Err(Error::Invalid(format!(
                "{} is not a page experience run",
                p.label()
            ))),
        }
    }
}
