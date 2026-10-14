//! `spiderlens viz` (spec 022 FR-025, contracts/cli.md): the same files as
//! the desktop export of the view's default state (SC-007).

use assert_cmd::Command;
use spiderlens_results::Reader;
use spiderlens_results::synth::Synth;
use spiderlens_results::viz::export::{ExportFormat, ExportOptions, export};
use spiderlens_results::viz::{Layout, SizeBy, ViewKind, ViewState};

fn crawl_file(dir: &tempfile::TempDir) -> std::path::PathBuf {
    let path = dir.path().join("c.spiderlens");
    Synth {
        urls: 600,
        links_per_page: 3,
        seed: 9,
        ..Default::default()
    }
    .write(&path)
    .unwrap();
    path
}

fn viz(args: &[&str]) -> std::process::Output {
    Command::cargo_bin("spiderlens")
        .unwrap()
        .arg("viz")
        .args(args)
        .output()
        .unwrap()
}

/// The desktop's initial state for a view.
fn desktop(view: ViewKind) -> ViewState {
    ViewState {
        view,
        layout: match view {
            ViewKind::LinkGraph => Layout::Force,
            ViewKind::Anchors => Layout::Cloud,
            _ => Layout::Tree,
        },
        size: SizeBy::Score,
        ..ViewState::default()
    }
}

#[test]
fn cli_svg_and_csv_equal_the_desktop_export() {
    let dir = tempfile::tempdir().unwrap();
    let path = crawl_file(&dir);
    for (arg, view) in [
        ("crawl-tree", ViewKind::CrawlTree),
        ("directory-tree", ViewKind::DirectoryTree),
        ("link-graph", ViewKind::LinkGraph),
        ("anchors", ViewKind::Anchors),
    ] {
        let svg = dir.path().join(format!("{arg}.svg"));
        let csv = dir.path().join(format!("{arg}.csv"));
        let out = viz(&[
            path.to_str().unwrap(),
            "--view",
            arg,
            "--svg",
            svg.to_str().unwrap(),
            "--csv",
            csv.to_str().unwrap(),
        ]);
        assert!(
            out.status.success(),
            "{arg}: {}",
            String::from_utf8_lossy(&out.stderr)
        );

        let mut r = Reader::open(&path).unwrap();
        let state = desktop(view);
        let scene = r.viz_scene(&state).unwrap();
        let header = r.viz_header().unwrap();
        for (format, cli) in [(ExportFormat::Svg, &svg), (ExportFormat::Csv, &csv)] {
            let mine = dir.path().join(format!(
                "desktop-{arg}.{}",
                if format == ExportFormat::Svg { "svg" } else { "csv" }
            ));
            export(
                &scene,
                state.colour,
                &header,
                &ExportOptions {
                    format,
                    ..Default::default()
                },
                &mine,
            )
            .unwrap();
            assert_eq!(
                std::fs::read(cli).unwrap(),
                std::fs::read(&mine).unwrap(),
                "{arg} {format:?}"
            );
        }
    }
}

#[test]
fn png_dark_and_usage_errors() {
    let dir = tempfile::tempdir().unwrap();
    let path = crawl_file(&dir);
    let p = path.to_str().unwrap();
    let png = dir.path().join("g.png");
    let out = viz(&[
        p,
        "--view",
        "link-graph",
        "--theme",
        "dark",
        "--png",
        png.to_str().unwrap(),
        "--scale",
        "1",
    ]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(&std::fs::read(&png).unwrap()[..4], b"\x89PNG");

    // No output asked for, a focus on a tree, a URL not in the crawl: usage.
    assert_eq!(viz(&[p, "--view", "crawl-tree"]).status.code(), Some(2));
    assert_eq!(
        viz(&[
            p,
            "--view",
            "crawl-tree",
            "--focus",
            "https://x.test/",
            "--svg",
            "o.svg"
        ])
        .status
        .code(),
        Some(2)
    );
    let missing = viz(&[
        p,
        "--view",
        "anchors",
        "--url",
        "https://nowhere.test/x",
        "--svg",
        "o.svg",
    ]);
    assert_eq!(missing.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&missing.stderr).contains("is not in this crawl"));
    assert_eq!(
        viz(&[p, "--view", "link-graph", "--scale", "3", "--png", "o.png"])
            .status
            .code(),
        Some(2)
    );
    // A file that is not a crawl.
    let junk = dir.path().join("junk.spiderlens");
    std::fs::write(&junk, b"not a crawl").unwrap();
    assert_eq!(
        viz(&[junk.to_str().unwrap(), "--view", "crawl-tree", "--svg", "o.svg"])
            .status
            .code(),
        Some(3)
    );
}
