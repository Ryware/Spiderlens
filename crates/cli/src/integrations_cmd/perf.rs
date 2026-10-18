//! `spiderlens integrations fetch --provider field-data|lab-tests` and
//! `connect google-key` (spec 024 US3, contracts/cli.md). A child of
//! `integrations_cmd` so it shares its helpers and exit codes.

use std::io::{IsTerminal, Write};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use clap::Args;
use spiderlens_integrations::crux::{FieldRequest, FormFactor};
use spiderlens_integrations::hub::Hub;
use spiderlens_integrations::perf::{PerfEstimate, UrlSet};
use spiderlens_integrations::psi::{LabRequest, StrategyChoice};
use spiderlens_integrations::store::RunProgress;
use spiderlens_integrations::{Error, Provider};

use super::{FetchArgs, exit, finish, pause_on_ctrl_c};

/// Options of the page experience providers.
#[derive(Args, Default)]
pub struct PerfArgs {
    /// Field data: the devices, comma separated (phone, desktop). Default: both.
    #[arg(long, value_delimiter = ',')]
    devices: Vec<String>,
    /// Lab tests: mobile, desktop or both. Default: mobile.
    #[arg(long)]
    strategy: Option<String>,
    /// Field data and lab tests: which pages. `all`, `indexable`, `prefix:ADDRESS`, or `file:PATH` with one
    /// address per line. Default: all pages for field data, indexable pages for lab tests.
    #[arg(long)]
    pages: Option<String>,
    /// Field data and lab tests: at most this many pages, shallowest first. Default for lab tests: 50.
    #[arg(long)]
    max_pages: Option<u32>,
}

fn devices(list: &[String]) -> Result<Vec<FormFactor>, Error> {
    if list.is_empty() {
        return Ok(FormFactor::BOTH.to_vec());
    }
    list.iter()
        .map(|d| match d.trim() {
            "phone" => Ok(FormFactor::Phone),
            "desktop" => Ok(FormFactor::Desktop),
            other => Err(Error::Invalid(format!("{other} is not a device (phone, desktop)"))),
        })
        .collect()
}

fn strategy(s: Option<&str>) -> Result<StrategyChoice, Error> {
    match s {
        None | Some("mobile") => Ok(StrategyChoice::Mobile),
        Some("desktop") => Ok(StrategyChoice::Desktop),
        Some("both") => Ok(StrategyChoice::Both),
        Some(other) => Err(Error::Invalid(format!("{other} is not a strategy (mobile, desktop, both)"))),
    }
}

fn url_set(spec: Option<&str>, default: UrlSet) -> Result<UrlSet, Error> {
    let Some(spec) = spec else {
        return Ok(default);
    };
    if let Some(prefix) = spec.strip_prefix("prefix:") {
        return Ok(UrlSet::Prefix {
            prefix: prefix.to_string(),
        });
    }
    if let Some(path) = spec.strip_prefix("file:") {
        let text = std::fs::read_to_string(path)?;
        let urls = text
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_string)
            .collect();
        return Ok(UrlSet::List { urls });
    }
    match spec {
        "all" => Ok(UrlSet::All),
        "indexable" => Ok(UrlSet::Indexable),
        other => Err(Error::Invalid(format!(
            "{other} is not a page set (all, indexable, prefix:ADDRESS, file:PATH)"
        ))),
    }
}

fn duration(secs: u32) -> String {
    match secs {
        0..=89 => format!("{secs} s"),
        90..=5399 => format!("{} min", secs.div_ceil(60)),
        _ => format!("{} h {} min", secs / 3600, (secs % 3600).div_ceil(60)),
    }
}

/// The estimate goes first, so a long run is never a surprise (FR-004).
fn print_estimate(p: Provider, e: &PerfEstimate) {
    let allowance = e
        .share_of_allowance
        .map_or(String::new(), |s| format!(", {:.0}% of a day's allowance", s * 100.0));
    eprintln!(
        "{}: {} requests for {} pages, about {} at {} a minute and {} at a time{allowance}. {}",
        p.label(),
        e.requests,
        e.urls,
        duration(e.duration_secs),
        e.per_minute,
        e.concurrency,
        p.sends()
    );
    if e.days_needed > 1 {
        eprintln!(
            "This needs {} days of the daily allowance; the run pauses at the limit and `resume` continues it.",
            e.days_needed
        );
    }
}

fn progress() -> impl FnMut(&RunProgress) {
    let mut last = Instant::now();
    move |p: &RunProgress| {
        if last.elapsed() >= Duration::from_millis(500) {
            last = Instant::now();
            eprintln!("{} of {} pages", p.done + p.failed, p.requested);
        }
    }
}

pub(super) fn fetch(h: &Hub, p: Provider, a: &FetchArgs) -> Result<ExitCode, Error> {
    let key = a.account.as_deref();
    let control = pause_on_ctrl_c();
    let run = if p == Provider::FieldData {
        let req = FieldRequest {
            devices: devices(&a.perf.devices)?,
            urls: url_set(a.perf.pages.as_deref(), UrlSet::All)?,
            max: a.perf.max_pages,
            fresh: a.fresh,
        };
        print_estimate(p, &h.estimate_field_data(&a.file, &req)?);
        h.fetch_field_data(&a.file, key, &req, &control, &mut progress())?
    } else {
        let defaults = LabRequest::default();
        let req = LabRequest {
            strategy: strategy(a.perf.strategy.as_deref())?,
            urls: url_set(a.perf.pages.as_deref(), defaults.urls)?,
            max: a.perf.max_pages.or(defaults.max),
            fresh: a.fresh,
        };
        print_estimate(p, &h.estimate_lab_tests(&a.file, &req)?);
        h.fetch_lab_tests(&a.file, key, &req, &control, &mut progress())?
    };
    Ok(finish(&run, a.json))
}

/// `connect google-key --label NAME`: one Google API key for field data and
/// lab tests, read from standard input so it stays out of shell history.
pub(super) fn connect_key(h: &Hub, label: Option<&str>) -> Result<ExitCode, Error> {
    let label = label.ok_or_else(|| Error::Invalid("a key needs --label".into()))?;
    if std::io::stdin().is_terminal() {
        eprint!("Paste the Google API key and press Enter: ");
        std::io::stderr().flush().ok();
    }
    let mut key = String::new();
    std::io::stdin().read_line(&mut key)?;
    h.save_google_api_key(label, key.trim().to_string().into())?;
    println!("Saved the Google API key {label} for field data and lab tests.");
    Ok(ExitCode::from(exit::FETCHED))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn devices_strategies_and_page_sets_are_read_from_their_names() {
        assert_eq!(devices(&[]).unwrap(), FormFactor::BOTH.to_vec());
        assert_eq!(devices(&["phone".into()]).unwrap(), vec![FormFactor::Phone]);
        assert!(devices(&["tablet".into()]).is_err());
        assert_eq!(strategy(None).unwrap(), StrategyChoice::Mobile);
        assert_eq!(strategy(Some("both")).unwrap(), StrategyChoice::Both);
        assert!(strategy(Some("watch")).is_err());
        assert_eq!(url_set(None, UrlSet::All).unwrap(), UrlSet::All);
        assert_eq!(url_set(Some("indexable"), UrlSet::All).unwrap(), UrlSet::Indexable);
        assert_eq!(
            url_set(Some("prefix:https://a.test/blog/"), UrlSet::All).unwrap(),
            UrlSet::Prefix {
                prefix: "https://a.test/blog/".into()
            }
        );
        assert!(url_set(Some("some"), UrlSet::All).is_err());
    }

    #[test]
    fn durations_read_naturally() {
        assert_eq!(duration(45), "45 s");
        assert_eq!(duration(858), "15 min");
        assert_eq!(duration(8334), "2 h 19 min");
    }
}
