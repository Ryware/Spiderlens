use super::*;

/// Reference widths measured in Chromium with the same fonts (and its
/// kerning and Arabic shaping, which the table does not model).
const REFERENCE: &str = include_str!("../../../test-fixtures/data/serp/reference.csv");

#[test]
fn widths_match_arial_metrics() {
    // Arial's advance widths, in thousandths of an em.
    assert_eq!(em_width(' '), 278);
    assert_eq!(em_width('a'), 556);
    assert_eq!(em_width('W'), 944);
    assert_eq!(em_width('i'), 222);
    assert_eq!(em_width('M'), 833);
}

#[test]
fn wide_and_narrow_letters_differ() {
    assert!(px_width("WWWW", SerpStyle::Title) > 3 * px_width("iiii", SerpStyle::Title));
}

#[test]
fn whitespace_is_collapsed_and_trimmed() {
    assert_eq!(
        px_width("  Red   shoes\n\t| Shop ", SerpStyle::Title),
        px_width("Red shoes | Shop", SerpStyle::Title)
    );
    assert_eq!(px_width("", SerpStyle::Title), 0);
    assert_eq!(px_width("   ", SerpStyle::Title), 0);
}

#[test]
fn styles_scale_with_font_size() {
    let t = px_width("Running shoes", SerpStyle::Title);
    let d = px_width("Running shoes", SerpStyle::Description);
    assert!((f64::from(d) - f64::from(t) * 14.0 / 20.0).abs() <= 1.0);
}

#[test]
fn fallbacks_by_script() {
    assert_eq!(em_width('中'), 1000);
    assert_eq!(em_width('🔥'), 1250);
    assert_eq!(em_width('\u{200D}'), 0);
    assert_eq!(em_width('\u{FE0F}'), 0);
    // Hebrew vowel points take no width.
    assert_eq!(em_width('\u{05B8}'), 0);
    assert!(em_width('ש') > 0);
}

#[test]
fn niqqud_does_not_change_width() {
    assert_eq!(
        px_width("בָּיִת", SerpStyle::Title),
        px_width("בית", SerpStyle::Title)
    );
}

#[test]
fn reference_widths_within_two_percent() {
    // SC-A1: 95% of widths within 2% of the reference.
    let mut rdr = csv::Reader::from_reader(REFERENCE.as_bytes());
    let (mut total, mut close, mut worst) = (0, 0, (0.0f64, String::new()));
    for rec in rdr.records() {
        let rec = rec.unwrap();
        let style = SerpStyle::parse(&rec[1]).unwrap();
        let reference: f64 = rec[3].parse().unwrap();
        let ours = f64::from(px_width(&rec[2], style));
        let err = (ours - reference).abs() / reference.max(1.0);
        total += 1;
        if err <= 0.02 {
            close += 1;
        }
        if err > worst.0 {
            worst = (err, rec[2].to_string());
        }
    }
    assert_eq!(total, 400);
    let share = f64::from(close) / f64::from(total);
    assert!(
        share >= 0.95,
        "{close}/{total} within 2%; worst {:.1}% on {:?}",
        worst.0 * 100.0,
        worst.1
    );
}

#[test]
fn short_text_is_not_cut() {
    let t = truncate("Red Shoes | Shop", SerpStyle::Title, TITLE_DESKTOP_PX);
    assert!(!t.cut);
    assert_eq!(t.text, "Red Shoes | Shop");
}

#[test]
fn long_text_is_cut_at_a_word_with_an_ellipsis() {
    let title = "The Complete Beginner Guide To Choosing The Right Running Shoes For Every Kind Of Runner";
    let t = truncate(title, SerpStyle::Title, TITLE_DESKTOP_PX);
    assert!(t.cut);
    assert!(t.text.ends_with('…'));
    let kept = t.text.trim_end_matches('…');
    assert!(title.starts_with(kept));
    // Cut at a word boundary.
    assert_eq!(title.as_bytes()[kept.len()], b' ');
    assert!(px_width(&t.text, SerpStyle::Title) <= TITLE_DESKTOP_PX);
}

#[test]
fn mobile_cuts_no_later_than_desktop() {
    let text =
        "Lightweight running shoes with breathable mesh, cushioned soles and free returns on every order";
    let d = truncate(text, SerpStyle::Description, 600);
    let m = truncate(text, SerpStyle::Description, 400);
    assert!(m.text.chars().count() <= d.text.chars().count());
}

#[test]
fn a_single_long_word_is_cut_inside_the_word() {
    let t = truncate(&"W".repeat(200), SerpStyle::Title, TITLE_DESKTOP_PX);
    assert!(t.cut);
    assert!(t.text.chars().count() > 10);
    assert!(px_width(&t.text, SerpStyle::Title) <= TITLE_DESKTOP_PX);
}

#[test]
fn hebrew_is_cut_at_its_logical_end() {
    let text =
        "נעלי ריצה לנשים ולגברים מדריך מלא איך לבחור מידה משלוח חינם החזרות חנות רשמית מבצע קולקציה חדשה";
    let t = truncate(text, SerpStyle::Title, TITLE_MOBILE_PX);
    assert!(t.cut);
    assert!(text.starts_with(t.text.trim_end_matches('…')));
}

#[test]
fn very_long_text_measures_only_the_first_thousand_characters() {
    let long = "a".repeat(5000);
    assert_eq!(
        px_width(&long, SerpStyle::Title),
        px_width(&"a".repeat(MAX_MEASURED_CHARS), SerpStyle::Title)
    );
    assert!(truncate(&long, SerpStyle::Title, 100_000).cut);
}

#[test]
fn measure_reports_both_devices() {
    // Fits desktop (990) but not mobile (680).
    let desc = "Shop lightweight running shoes for road and trail with free delivery, easy returns within thirty days and a price match on every pair we sell";
    let m = measure(desc, SerpStyle::Description);
    assert!(m.px > DESC_MOBILE_PX && m.px <= DESC_DESKTOP_PX, "{}", m.px);
    assert!(!m.cut_desktop);
    assert!(m.cut_mobile);
    assert_eq!(m.display_desktop, desc);
}

#[test]
fn tables_are_sorted_for_binary_search() {
    assert!(widths::WIDTHS.windows(2).all(|w| w[0].0 < w[1].0));
    assert!(widths::ARABIC.windows(2).all(|w| w[0].0 < w[1].0));
}

#[test]
fn arabic_letters_take_their_joined_width() {
    // Beh alone is wide; inside a word it is narrow.
    let alone = px_width("ب", SerpStyle::Title);
    let joined = px_width("ببب", SerpStyle::Title);
    assert!(joined < 3 * alone);
    // Lam-alef is one ligature, narrower than lam and alef apart.
    assert!(px_width("لا", SerpStyle::Title) < px_width("ل ا", SerpStyle::Title));
}

/// The `spiderlens_px` column records this table's widths, so the UI's copy
/// of the table can be checked against the same numbers (plan D1).
#[test]
fn reference_records_our_widths() {
    let mut rdr = csv::Reader::from_reader(REFERENCE.as_bytes());
    for rec in rdr.records() {
        let rec = rec.unwrap();
        let style = SerpStyle::parse(&rec[1]).unwrap();
        let recorded: u32 = rec[4].parse().unwrap();
        assert_eq!(
            px_width(&rec[2], style),
            recorded,
            "{} (re-run `record_our_widths`)",
            &rec[0]
        );
    }
}

/// Rewrites the `spiderlens_px` column after a width table change:
/// `cargo test -p spiderlens-results record_our_widths -- --ignored`.
#[test]
#[ignore]
fn record_our_widths() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../test-fixtures/data/serp/reference.csv"
    );
    let mut rdr = csv::Reader::from_path(path).unwrap();
    let mut out = csv::Writer::from_writer(vec![]);
    out.write_record(["id", "style", "text", "reference_px", "spiderlens_px"])
        .unwrap();
    for rec in rdr.records() {
        let rec = rec.unwrap();
        let px = px_width(&rec[2], SerpStyle::parse(&rec[1]).unwrap()).to_string();
        out.write_record([&rec[0], &rec[1], &rec[2], &rec[3], px.as_str()])
            .unwrap();
    }
    std::fs::write(path, out.into_inner().unwrap()).unwrap();
}
