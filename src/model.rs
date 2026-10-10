//! Issues and the report that collects them.

use serde::Serialize;
use std::fmt;

/// How bad an issue is. Errors fail the run by default; warnings only with `--fail-on warning`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Warning,
    Error,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Severity::Warning => "warning",
            Severity::Error => "error",
        })
    }
}

/// One finding on one page (or on the site as a whole).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Issue {
    /// The page the issue was found on, or the site origin for site-wide checks.
    pub page: String,
    /// Short machine-friendly check name, for example `broken-link` or `og-image`.
    pub check: &'static str,
    pub severity: Severity,
    pub message: String,
}

impl Issue {
    pub fn new(
        page: &str,
        check: &'static str,
        severity: Severity,
        message: impl Into<String>,
    ) -> Self {
        Self {
            page: page.to_owned(),
            check,
            severity,
            message: message.into(),
        }
    }
    pub fn error(page: &str, check: &'static str, message: impl Into<String>) -> Self {
        Self::new(page, check, Severity::Error, message)
    }
    pub fn warning(page: &str, check: &'static str, message: impl Into<String>) -> Self {
        Self::new(page, check, Severity::Warning, message)
    }
}

/// Everything one run found.
#[derive(Debug, Default, Serialize)]
pub struct Report {
    pub site: String,
    pub pages_checked: usize,
    pub links_checked: usize,
    pub issues: Vec<Issue>,
}

impl Report {
    pub fn count(&self, severity: Severity) -> usize {
        self.issues
            .iter()
            .filter(|i| i.severity == severity)
            .count()
    }

    /// Sorts issues by page, then errors before warnings, then check name, so output is stable.
    pub fn sort(&mut self) {
        self.issues.sort_by(|a, b| {
            a.page
                .cmp(&b.page)
                .then(b.severity.cmp(&a.severity))
                .then(a.check.cmp(b.check))
                .then(a.message.cmp(&b.message))
        });
        self.issues.dedup();
    }
}
