//! The Page experience results tab (spec 024 US3, FR-025, FR-026): Core Web
//! Vitals field data from the Chrome UX Report for phones and desktops, and
//! lab test results from PageSpeed Insights.
//!
//! Field values are the page's own 75th percentiles. A page without enough
//! traffic has no page-level numbers: its columns stay empty, the "Field data"
//! column says "No page-level data (origin data)", and the origin's numbers
//! sit in the hidden "Origin" columns, so they are never read as the page's
//! own (US3.2).

use std::borrow::Cow;

use crate::tabs::{ColumnDef, FilterDef, HTML_OK, Need, TabDef, ValueKind, c, col, filter};

/// A column whose id, label and expression are built per device.
fn column(id: String, label: String, expr: String, kind: ValueKind, width: u16) -> ColumnDef {
    let mut c = col("", "", "", kind, width);
    c.id = Cow::Owned(id);
    c.label = Cow::Owned(label);
    c.expr = Cow::Owned(expr);
    c
}

/// One value of a device's `cwv_field` row, at the given level.
fn field(device: &str, level: &str, value: &str) -> String {
    format!(
        "(SELECT {value} FROM cwv_field f WHERE f.url_id = u.id AND f.form_factor = '{device}' \
         AND f.level = '{level}')"
    )
}

fn rated(device: &str, metric: &str) -> String {
    field(
        device,
        "url",
        &format!(
            "CASE f.{metric}_rating WHEN 'good' THEN 'Good' WHEN 'needs_improvement' THEN 'Needs improvement' \
             WHEN 'poor' THEN 'Poor' END"
        ),
    )
}

fn level(device: &str) -> String {
    format!(
        "COALESCE((SELECT CASE f.level WHEN 'url' THEN 'Page data' \
         WHEN 'origin' THEN 'No page-level data (origin data)' ELSE 'No data' END \
         FROM cwv_field f WHERE f.url_id = u.id AND f.form_factor = '{device}'), 'Not fetched')"
    )
}

fn lab(strategy: &str, value: &str) -> String {
    format!("(SELECT {value} FROM psi_lab l WHERE l.url_id = u.id AND l.strategy = '{strategy}')")
}

fn field_columns(device: &str, name: &str) -> Vec<ColumnDef> {
    let id = |s: &str| format!("{device}_{s}");
    let label = |s: &str| format!("{s} · {name} (field)");
    let f = |c: ColumnDef| c.needs(Need::FieldData);
    vec![
        f(column(id("lcp"), label("LCP"), field(device, "url", "f.lcp_ms"), ValueKind::Ms, 120)),
        f(column(id("inp"), label("INP"), field(device, "url", "f.inp_ms"), ValueKind::Ms, 120)),
        f(column(
            id("cls"),
            label("CLS"),
            field(device, "url", &cls_text("f")),
            ValueKind::Text,
            100,
        )),
        f(column(
            id("level"),
            format!("Field data · {name}"),
            level(device),
            ValueKind::Text,
            220,
        )),
        f(column(id("fcp"), label("FCP"), field(device, "url", "f.fcp_ms"), ValueKind::Ms, 120).hidden()),
        f(column(id("ttfb"), label("TTFB"), field(device, "url", "f.ttfb_ms"), ValueKind::Ms, 120).hidden()),
        f(column(id("lcp_rating"), label("LCP rating"), rated(device, "lcp"), ValueKind::Text, 150).hidden()),
        f(column(id("inp_rating"), label("INP rating"), rated(device, "inp"), ValueKind::Text, 150).hidden()),
        f(column(id("cls_rating"), label("CLS rating"), rated(device, "cls"), ValueKind::Text, 150).hidden()),
        f(column(
            id("period"),
            format!("Collection period · {name}"),
            format!(
                "COALESCE((SELECT f.period_start || ' to ' || f.period_end FROM cwv_field f \
                 WHERE f.url_id = u.id AND f.form_factor = '{device}' AND f.level <> 'none'), '')"
            ),
            ValueKind::Text,
            190,
        )
        .hidden()),
        f(column(
            id("origin_lcp"),
            format!("LCP · {name} (origin data)"),
            field(device, "origin", "f.lcp_ms"),
            ValueKind::Ms,
            150,
        )
        .hidden()),
        f(column(
            id("origin_inp"),
            format!("INP · {name} (origin data)"),
            field(device, "origin", "f.inp_ms"),
            ValueKind::Ms,
            150,
        )
        .hidden()),
        f(column(
            id("origin_cls"),
            format!("CLS · {name} (origin data)"),
            field(device, "origin", &cls_text("f")),
            ValueKind::Text,
            150,
        )
        .hidden()),
    ]
}

fn lab_columns(strategy: &str, name: &str) -> Vec<ColumnDef> {
    let id = |s: &str| format!("lab_{strategy}_{s}");
    let label = |s: &str| format!("{s} · {name} (lab)");
    let f = |c: ColumnDef| c.needs(Need::LabTests);
    vec![
        f(column(
            id("performance"),
            label("Performance"),
            lab(strategy, "l.performance"),
            ValueKind::Int,
            130,
        )),
        f(column(
            id("accessibility"),
            label("Accessibility"),
            lab(strategy, "l.accessibility"),
            ValueKind::Int,
            130,
        )
        .hidden()),
        f(column(
            id("best_practices"),
            label("Best practices"),
            lab(strategy, "l.best_practices"),
            ValueKind::Int,
            130,
        )
        .hidden()),
        f(column(id("seo"), label("SEO"), lab(strategy, "l.seo"), ValueKind::Int, 110).hidden()),
        f(column(id("lcp"), label("LCP"), lab(strategy, "l.lcp_ms"), ValueKind::Ms, 120).hidden()),
        f(column(id("cls"), label("CLS"), lab(strategy, &cls_text("l")), ValueKind::Text, 100).hidden()),
        f(column(
            id("tbt"),
            label("Total blocking time"),
            lab(strategy, "l.tbt_ms"),
            ValueKind::Ms,
            150,
        )
        .hidden()),
        f(column(
            id("speed_index"),
            label("Speed index"),
            lab(strategy, "l.speed_index_ms"),
            ValueKind::Ms,
            130,
        )
        .hidden()),
        f(column(
            id("top_opportunity"),
            label("Top opportunity"),
            lab(strategy, "json_extract(l.opportunities, '$[0].title')"),
            ValueKind::Text,
            260,
        )
        .hidden()),
    ]
}

/// A filter whose condition is built per metric.
fn owned_filter(id: &'static str, label: &'static str, cond: String) -> FilterDef {
    let mut f = filter(id, label, "");
    f.cond = Cow::Owned(cond);
    f
}

/// Pages whose own (page-level) 75th percentile of `metric` rates poor. The
/// origin's rating is never credited to a page.
fn poor(metric: &str) -> String {
    format!(
        "EXISTS (SELECT 1 FROM cwv_field f WHERE f.url_id = u.id AND f.level = 'url' \
         AND f.{metric}_rating = 'poor')"
    )
}

/// A CLS value as text with three decimals (the table's decimal columns
/// show one), or nothing when the metric is missing.
fn cls_text(alias: &str) -> String {
    format!("CASE WHEN {alias}.cls IS NULL THEN NULL ELSE printf('%.3f', {alias}.cls) END")
}

const ORIGIN_ONLY: &str = "EXISTS (SELECT 1 FROM cwv_field f WHERE f.url_id = u.id AND f.level = 'origin') \
     AND NOT EXISTS (SELECT 1 FROM cwv_field f WHERE f.url_id = u.id AND f.level = 'url')";
const NO_FIELD_DATA: &str = "EXISTS (SELECT 1 FROM cwv_field f WHERE f.url_id = u.id) \
     AND NOT EXISTS (SELECT 1 FROM cwv_field f WHERE f.url_id = u.id AND f.level IN ('url', 'origin'))";
const LAB_POOR: &str = "EXISTS (SELECT 1 FROM psi_lab l WHERE l.url_id = u.id AND l.performance < 50)";

pub(crate) fn tab() -> TabDef {
    let mut columns = vec![c::ADDRESS, c::TITLE];
    columns.extend(field_columns("phone", "Phone"));
    columns.extend(field_columns("desktop", "Desktop"));
    columns.extend(lab_columns("mobile", "Mobile"));
    columns.extend(lab_columns("desktop", "Desktop"));
    TabDef {
        id: Cow::Borrowed("page-experience"),
        title: Cow::Borrowed("Page experience"),
        eligible: Cow::Borrowed(HTML_OK),
        columns,
        filters: vec![
            filter("all", "All", "1"),
            owned_filter("poor_lcp", "Poor LCP", poor("lcp")).needs(Need::FieldData),
            owned_filter("poor_inp", "Poor INP", poor("inp")).needs(Need::FieldData),
            owned_filter("poor_cls", "Poor CLS", poor("cls")).needs(Need::FieldData),
            filter("origin_only", "Origin data only", ORIGIN_ONLY).needs(Need::FieldData),
            filter("no_field_data", "No field data", NO_FIELD_DATA).needs(Need::FieldData),
            filter("lab_poor", "Lab score under 50", LAB_POOR).needs(Need::LabTests),
        ],
        search_column: Some(Cow::Borrowed("title")),
        scoped: false,
        rows: None,
    }
}
