//! Human-readable and JSON output.

use crate::model::{Report, Severity};
use std::fmt::Write;

/// Groups issues by page, errors first, with a one-line summary at the end.
pub fn text(report: &Report, color: bool) -> String {
    let (red, yellow, bold, dim, reset) = if color {
        ("\x1b[31m", "\x1b[33m", "\x1b[1m", "\x1b[2m", "\x1b[0m")
    } else {
        ("", "", "", "", "")
    };
    let mut out = String::new();
    let mut current = "";
    for issue in &report.issues {
        if issue.page != current {
            current = &issue.page;
            let _ = writeln!(out, "\n{bold}{current}{reset}");
        }
        let (c, label) = match issue.severity {
            Severity::Error => (red, "error  "),
            Severity::Warning => (yellow, "warning"),
        };
        let _ = writeln!(
            out,
            "  {c}{label}{reset} {dim}{:<16}{reset} {}",
            issue.check, issue.message
        );
    }
    let errors = report.count(Severity::Error);
    let warnings = report.count(Severity::Warning);
    let _ = writeln!(
        out,
        "\n{bold}{} pages, {} links and resources checked: {red}{errors} error{}{reset}{bold}, {yellow}{warnings} warning{}{reset}",
        report.pages_checked,
        report.links_checked,
        if errors == 1 { "" } else { "s" },
        if warnings == 1 { "" } else { "s" },
    );
    out
}

pub fn json(report: &Report) -> String {
    serde_json::to_string_pretty(report).expect("report serializes")
}
