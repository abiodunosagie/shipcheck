//! The binary itself: exit codes and output formats, run against the fixture sites.

use std::process::{Command, Output};

fn shipcheck(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_shipcheck"))
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("the binary runs")
}

#[test]
fn errors_exit_1_and_the_text_report_groups_them_by_page() {
    let out = shipcheck(&["tests/fixtures/bad", "--no-external"]);
    assert_eq!(out.status.code(), Some(1));
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("broken-link"), "{text}");
    assert!(
        text.contains("2 pages, 5 links and resources checked"),
        "{text}"
    );
}

#[test]
fn fail_on_decides_whether_warnings_fail_the_run() {
    // A copy of the complete site with one warning: no sitemap.xml. Its og:image points off-site
    // so `--no-external` leaves it unchecked.
    let dir = std::env::temp_dir().join(format!("shipcheck-cli-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/good");
    for entry in std::fs::read_dir(&src).unwrap() {
        let entry = entry.unwrap();
        if entry.file_name() == "sitemap.xml" {
            continue;
        }
        let text = std::fs::read_to_string(entry.path()).unwrap();
        let text = text.replace("OG_IMAGE_URL", "https://example.com/og.png");
        std::fs::write(dir.join(entry.file_name()), text).unwrap();
    }
    let folder = dir.to_str().unwrap();
    let run = |fail_on: &str| {
        shipcheck(&[folder, "--no-external", "--fail-on", fail_on])
            .status
            .code()
    };
    assert_eq!(run("error"), Some(0), "warnings alone pass by default");
    assert_eq!(run("warning"), Some(1));
    assert_eq!(run("never"), Some(0));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn json_output_parses_and_carries_every_issue() {
    let out = shipcheck(&["tests/fixtures/bad", "--no-external", "--json"]);
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).expect("valid JSON");
    assert_eq!(report["pages_checked"], 2);
    let issues = report["issues"].as_array().unwrap();
    assert!(issues
        .iter()
        .any(|i| i["check"] == "broken-link" && i["severity"] == "error"));
}

#[test]
fn a_target_that_is_neither_folder_nor_url_exits_2() {
    for target in ["./no-such-folder", "ftp://example.com"] {
        let out = shipcheck(&[target]);
        assert_eq!(out.status.code(), Some(2), "{target}");
        assert!(!out.stderr.is_empty());
    }
}
