//! `spiderlens serp` (spec 027 US A3): widths from a CSV, no network.

use assert_cmd::Command;

const CSV: &str = "url,title,description\n\
https://example.com/a,Red Shoes | Shop,Short.\n\
https://example.com/b,The Complete Beginner Guide To Choosing The Right Running Shoes For Every Kind Of Runner,\n";

fn serp(args: &[&str]) -> std::process::Output {
    Command::cargo_bin("spiderlens")
        .unwrap()
        .arg("serp")
        .arg("-")
        .args(args)
        .write_stdin(CSV)
        .output()
        .unwrap()
}

#[test]
fn csv_output_has_widths_and_cuts() {
    let out = serp(&["--format", "csv"]);
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    let mut rdr = csv::Reader::from_reader(text.as_bytes());
    let rows: Vec<csv::StringRecord> = rdr.records().map(Result::unwrap).collect();
    assert_eq!(rows.len(), 2);
    let px =
        spiderlens_results::serp::px_width("Red Shoes | Shop", spiderlens_results::serp::SerpStyle::Title);
    assert_eq!(&rows[0][3], px.to_string());
    assert_eq!(&rows[0][4], "false");
    assert_eq!(&rows[1][4], "true");
    assert_eq!(&rows[1][5], "true");
}

#[test]
fn json_and_table_formats() {
    let out = serp(&["--format", "json"]);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v.as_array().unwrap().len(), 2);
    assert_eq!(v[1]["title_measure"]["cut_desktop"], true);
    let out = serp(&[]);
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("https://example.com/b"));
    assert!(text.lines().nth(2).unwrap().contains("cut"));
}

#[test]
fn missing_file_is_a_usage_error() {
    let out = Command::cargo_bin("spiderlens")
        .unwrap()
        .args(["serp", "/no/such/file.csv"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
}
