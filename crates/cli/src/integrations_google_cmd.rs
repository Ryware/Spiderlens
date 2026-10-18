//! `spiderlens integrations fetch --provider url-inspection|analytics`
//! (spec 024 tasks T031 and T039). A child module of `integrations_cmd`, so
//! it shares that file's arguments, progress output and exit codes.

use std::process::ExitCode;

use spiderlens_integrations::ga4::{Ga4Request, Ga4Stream};
use spiderlens_integrations::hub::Hub;
use spiderlens_integrations::inspect::InspectRequest;
use spiderlens_integrations::{Error, Provider};

use super::{FetchArgs, finish, pause_on_ctrl_c, progress};

/// Inspects crawled URLs with Google's URL Inspection (US2). Prints the
/// count against today's allowance first.
pub(super) fn fetch_inspection(h: &Hub, a: FetchArgs) -> Result<ExitCode, Error> {
    let property = match a.property {
        Some(p) => p,
        None => h.suggested_property(a.account.as_deref(), &a.file)?,
    };
    let req = InspectRequest {
        property,
        urls: a.urls,
        max: a.max,
        fresh: a.fresh,
    };
    let plan = h.inspection_plan(&a.file, a.account.as_deref(), &req)?;
    eprintln!(
        "URL Inspection: {} URLs of {} ({}). Today's allowance: {} left of {} (resets {}). {}",
        plan.selected,
        req.property,
        req.urls,
        plan.remaining_today,
        plan.allowance,
        plan.resets_at,
        Provider::UrlInspection.sends()
    );
    if plan.days_needed > 1 {
        eprintln!(
            "This takes {} days at the daily allowance. It pauses when the allowance is used up; continue each day with `spiderlens integrations resume`.",
            plan.days_needed
        );
    }
    let control = pause_on_ctrl_c();
    let r = h.fetch_inspection(&a.file, a.account.as_deref(), &req, &control, &mut progress())?;
    Ok(finish(&r, a.json))
}

const NO_STREAM: &str = "no Analytics data stream matches; list them with `spiderlens integrations properties --provider analytics`";

/// `28d` or `28`: the number of days.
fn days(text: &str) -> Result<i64, Error> {
    text.trim()
        .trim_end_matches('d')
        .parse::<i64>()
        .ok()
        .filter(|n| (1..=1000).contains(n))
        .ok_or_else(|| Error::Invalid(format!("{text} is not a number of days such as 28d")))
}

/// The stream to fetch: the one named, the only web stream of the named
/// property, or the one matching the crawled site.
fn pick_stream(h: &Hub, a: &FetchArgs) -> Result<Ga4Stream, Error> {
    match (&a.property, &a.stream) {
        (None, None) => h.suggested_stream(a.account.as_deref(), &a.file),
        (property, stream) => {
            let all = h.analytics_streams(a.account.as_deref(), Some(&a.file))?;
            let wanted_property = property
                .as_ref()
                .map(|p| p.strip_prefix("properties/").unwrap_or(p).to_string());
            let mut matching = all.into_iter().filter(|s| {
                wanted_property
                    .as_ref()
                    .is_none_or(|p| s.property.strip_prefix("properties/") == Some(p.as_str()))
                    && stream.as_ref().is_none_or(|id| &s.stream == id)
            });
            let first = matching.next().ok_or_else(|| Error::Invalid(NO_STREAM.into()))?;
            if matching.next().is_some() {
                return Err(Error::Invalid(
                    "more than one data stream matches; add --property and --stream".into(),
                ));
            }
            Ok(first)
        }
    }
}

/// Fetches sessions per landing page from Google Analytics 4 (US4).
pub(super) fn fetch_analytics(h: &Hub, a: FetchArgs) -> Result<ExitCode, Error> {
    let stream = pick_stream(h, &a)?;
    let mut req = Ga4Request::last_28_days(stream.property.clone(), stream.stream.clone());
    if let Some(last) = &a.last {
        let (start, end) = ga4_range(days(last)?);
        req.start_date = start;
        req.end_date = end;
    } else if let (Some(from), Some(to)) = (&a.from, &a.to) {
        for d in [from, to] {
            d.parse::<jiff::civil::Date>()
                .map_err(|_| Error::Invalid(format!("{d} is not a date (YYYY-MM-DD)")))?;
        }
        req.start_date = from.clone();
        req.end_date = to.clone();
    }
    req.keep_query = a.keep_query;
    req.fresh = a.fresh;
    eprintln!(
        "Analytics: {} / {} ({}) from {} to {}{}. {}",
        stream.property_name,
        stream.stream_name,
        stream.default_uri,
        req.start_date,
        req.end_date,
        if req.keep_query {
            ", query strings kept apart"
        } else {
            ", query strings combined"
        },
        Provider::Analytics.sends()
    );
    let control = pause_on_ctrl_c();
    let r = h.fetch_analytics(&a.file, a.account.as_deref(), &req, &control, &mut progress())?;
    Ok(finish(&r, a.json))
}

/// The last `n` days up to yesterday.
fn ga4_range(n: i64) -> (String, String) {
    let today = jiff::Timestamp::now().to_zoned(jiff::tz::TimeZone::UTC).date();
    let end = today.checked_sub(jiff::Span::new().days(1)).unwrap_or(today);
    let start = end.checked_sub(jiff::Span::new().days(n - 1)).unwrap_or(end);
    (start.to_string(), end.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn days_accepts_a_count_with_or_without_the_unit() {
        assert_eq!(days("28d").unwrap(), 28);
        assert_eq!(days(" 7 ").unwrap(), 7);
        assert!(days("0d").is_err());
        assert!(days("weeks").is_err());
    }

    #[test]
    fn the_range_ends_yesterday_and_spans_n_days() {
        let (s, e) = ga4_range(28);
        let (s, e): (jiff::civil::Date, jiff::civil::Date) = (s.parse().unwrap(), e.parse().unwrap());
        assert_eq!(e.since(s).unwrap().get_days(), 27);
    }
}
