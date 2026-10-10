//! Crawls a site's own pages, then verifies every link and resource they reference.

use crate::model::{Issue, Report};
use crate::page::{self, ResourceKind, Target, TargetKind};
use futures::stream::{self, StreamExt};
use reqwest::header::{CONTENT_LENGTH, CONTENT_TYPE};
use reqwest::{Client, StatusCode};
use std::collections::{BTreeMap, HashSet, VecDeque};
use std::time::Duration;
use url::Url;

/// Settings for one run.
#[derive(Clone, Debug)]
pub struct Config {
    pub max_pages: usize,
    pub concurrency: usize,
    pub timeout: Duration,
    /// Check links to other sites too (slower, and some sites block automated requests).
    pub external: bool,
    /// Images larger than this many bytes get a warning.
    pub max_image_bytes: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_pages: 200,
            concurrency: 8,
            timeout: Duration::from_secs(15),
            external: true,
            max_image_bytes: 300 * 1024,
        }
    }
}

/// How a checked URL answered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Ok {
        status: u16,
        bytes: Option<u64>,
        content_type: Option<String>,
    },
    Status(u16),
    Failed(String),
}

pub const USER_AGENT: &str = concat!(
    "shipcheck/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/abiodunosagie/shipcheck)"
);

pub fn client(timeout: Duration) -> reqwest::Result<Client> {
    Client::builder()
        .user_agent(USER_AGENT)
        .timeout(timeout)
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
}

/// Same scheme, host and port: a page we should crawl rather than just check.
fn same_site(a: &Url, b: &Url) -> bool {
    a.scheme() == b.scheme()
        && a.host_str() == b.host_str()
        && a.port_or_known_default() == b.port_or_known_default()
}

/// Treats `/about` and `/about/` as one page so it is not crawled twice.
fn page_key(u: &Url) -> String {
    let mut k = u.clone();
    k.set_query(None);
    let s = k.as_str().to_owned();
    s.strip_suffix('/').map(str::to_owned).unwrap_or(s)
}

fn is_html(content_type: Option<&str>) -> bool {
    content_type.is_some_and(|c| c.to_ascii_lowercase().starts_with("text/html"))
}

/// Stop counting a body after this many bytes; anything this big is reported as at least this.
const MAX_COUNTED_BYTES: u64 = 64 * 1024 * 1024;

/// HEAD first (cheap); some servers reject HEAD, so fall back to GET on 405 or 501.
///
/// With `need_size`, a success without a `Content-Length` header (common for HEAD over HTTP/2
/// on CDNs) is retried as a GET and the body is counted, so image sizes are never silently skipped.
async fn probe(client: &Client, url: &Url, need_size: bool) -> Outcome {
    let mut res = client.head(url.clone()).send().await;
    if let Ok(r) = &res {
        let rejected = matches!(
            r.status(),
            StatusCode::METHOD_NOT_ALLOWED | StatusCode::NOT_IMPLEMENTED
        );
        let sizeless = need_size && r.status().is_success() && header_len(r).is_none();
        if rejected || sizeless {
            res = client.get(url.clone()).send().await;
        }
    }
    match res {
        Ok(mut r) if r.status().is_success() => {
            let content_type = r
                .headers()
                .get(CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned);
            let mut bytes = header_len(&r);
            if need_size && bytes.is_none() {
                let mut total = 0u64;
                while let Ok(Some(chunk)) = r.chunk().await {
                    total += chunk.len() as u64;
                    if total >= MAX_COUNTED_BYTES {
                        break;
                    }
                }
                bytes = Some(total);
            }
            Outcome::Ok {
                status: r.status().as_u16(),
                bytes,
                content_type,
            }
        }
        Ok(r) => Outcome::Status(r.status().as_u16()),
        Err(e) if e.is_timeout() => Outcome::Failed("timed out".into()),
        Err(e) if e.is_connect() => Outcome::Failed("could not connect".into()),
        Err(e) => Outcome::Failed(e.to_string()),
    }
}

fn header_len(r: &reqwest::Response) -> Option<u64> {
    r.headers()
        .get(CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok())
}

fn is_image(kind: TargetKind) -> bool {
    matches!(
        kind,
        TargetKind::Resource(ResourceKind::Image | ResourceKind::PreviewImage)
    )
}

/// Turns one URL's outcome into issues for every page that references it.
pub fn judge(
    target: &Target,
    outcome: &Outcome,
    pages: &[String],
    cfg: &Config,
    external: bool,
) -> Vec<Issue> {
    let mut out = Vec::new();
    let what = match target.kind {
        TargetKind::Link => "link",
        TargetKind::Resource(ResourceKind::Image) => "image",
        TargetKind::Resource(ResourceKind::PreviewImage) => "og:image",
        TargetKind::Resource(ResourceKind::Script) => "script",
        TargetKind::Resource(ResourceKind::Stylesheet) => "stylesheet",
        TargetKind::Resource(ResourceKind::Icon) => "icon",
    };
    let check = if target.kind == TargetKind::Link {
        "broken-link"
    } else {
        "broken-resource"
    };
    for page in pages {
        match outcome {
            // Many big sites answer automated requests with 401/403/429 even when the page is fine.
            Outcome::Status(s @ (401 | 403 | 429)) if external => out.push(Issue::warning(
                page,
                check,
                format!(
                    "could not verify {what} {} (HTTP {s}, the site may block automated checks)",
                    target.url
                ),
            )),
            Outcome::Status(s) => out.push(Issue::error(
                page,
                check,
                format!("{what} returns HTTP {s}: {}", target.url),
            )),
            Outcome::Failed(e) if external => out.push(Issue::warning(
                page,
                check,
                format!("could not reach {what} {} ({e})", target.url),
            )),
            Outcome::Failed(e) => out.push(Issue::error(
                page,
                check,
                format!("{what} failed ({e}): {}", target.url),
            )),
            Outcome::Ok {
                bytes,
                content_type,
                ..
            } => {
                if is_image(target.kind) {
                    if let Some(b) = bytes.filter(|b| *b > cfg.max_image_bytes) {
                        out.push(Issue::warning(
                            page,
                            "image-size",
                            format!(
                                "{what} is {} KB (limit {} KB): {}",
                                b / 1024,
                                cfg.max_image_bytes / 1024,
                                target.url
                            ),
                        ));
                    }
                }
                if target.kind == TargetKind::Resource(ResourceKind::PreviewImage) {
                    let ct = content_type.as_deref().unwrap_or("");
                    if !ct.to_ascii_lowercase().starts_with("image/") {
                        out.push(Issue::error(
                            page,
                            "og-image",
                            format!(
                                "og:image is not served as an image (Content-Type \"{ct}\"): {}",
                                target.url
                            ),
                        ));
                    }
                }
            }
        }
    }
    out
}

/// Audits the site starting at `start`.
pub async fn run(start: Url, cfg: &Config) -> Result<Report, String> {
    let client = client(cfg.timeout).map_err(|e| format!("could not create HTTP client: {e}"))?;
    let mut report = Report {
        site: start.as_str().to_owned(),
        ..Default::default()
    };

    // 1. Crawl same-site HTML pages breadth first.
    let mut queue = VecDeque::from([start.clone()]);
    let mut seen: HashSet<String> = HashSet::from([page_key(&start)]);
    // Every target, with the pages that reference it.
    let mut targets: BTreeMap<String, (Target, Vec<String>)> = BTreeMap::new();
    let mut any_icon = false;
    while let Some(url) = queue.pop_front() {
        if report.pages_checked >= cfg.max_pages {
            break;
        }
        // A page that fails to load is reported once: for the start page here, for every other
        // page by step 2 as a broken link on each page that links to it.
        let resp = match client.get(url.clone()).send().await {
            Ok(r) => r,
            Err(e) => {
                if url == start {
                    report.issues.push(Issue::error(
                        url.as_str(),
                        "fetch",
                        format!("could not load page: {e}"),
                    ));
                }
                continue;
            }
        };
        let status = resp.status();
        let final_url = resp.url().clone();
        let ctype = resp
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        if !status.is_success() {
            if url == start {
                report.issues.push(Issue::error(
                    url.as_str(),
                    "fetch",
                    format!("page returns HTTP {}", status.as_u16()),
                ));
            }
            continue;
        }
        if !is_html(ctype.as_deref()) {
            continue;
        }
        let html = match resp.text().await {
            Ok(t) => t,
            Err(e) => {
                report.issues.push(Issue::error(
                    url.as_str(),
                    "fetch",
                    format!("could not read page: {e}"),
                ));
                continue;
            }
        };
        report.pages_checked += 1;
        let analysis = page::analyze(&final_url, &html);
        any_icon |= analysis.has_icon;
        report.issues.extend(analysis.issues);
        for t in analysis.targets {
            if t.kind == TargetKind::Link
                && same_site(&t.url, &start)
                && seen.insert(page_key(&t.url))
            {
                queue.push_back(t.url.clone());
            }
            // Every target is probed once in step 2, same-site links included, so pages past the
            // crawl limit are still checked for being reachable.
            let entry = targets
                .entry(t.url.as_str().to_owned())
                .or_insert_with(|| (t.clone(), Vec::new()));
            if !entry.1.contains(&final_url.as_str().to_owned()) {
                entry.1.push(final_url.as_str().to_owned());
            }
        }
    }
    if !queue.is_empty() {
        report.issues.push(Issue::warning(
            &report.site,
            "crawl-limit",
            format!(
                "stopped after {} pages; raise --max-pages to check the rest",
                cfg.max_pages
            ),
        ));
    }

    // 2. Verify every link and resource once, concurrently.
    let jobs: Vec<(Target, Vec<String>, bool)> = targets
        .into_values()
        .filter_map(|(t, pages)| {
            let external = !same_site(&t.url, &start);
            (cfg.external || !external).then_some((t, pages, external))
        })
        .collect();
    report.links_checked = jobs.len();
    let results: Vec<Vec<Issue>> = stream::iter(jobs)
        .map(|(t, pages, external)| {
            let client = client.clone();
            async move {
                let outcome = probe(&client, &t.url, is_image(t.kind)).await;
                judge(&t, &outcome, &pages, cfg, external)
            }
        })
        .buffer_unordered(cfg.concurrency.max(1))
        .collect()
        .await;
    report.issues.extend(results.into_iter().flatten());

    // 3. Site-wide files.
    let origin = start.join("/").map_err(|e| e.to_string())?;
    for (file, check, why) in [
        ("robots.txt", "robots", "missing robots.txt"),
        (
            "sitemap.xml",
            "sitemap",
            "missing sitemap.xml, search engines discover pages more slowly",
        ),
    ] {
        if let Ok(u) = origin.join(file) {
            if !matches!(probe(&client, &u, false).await, Outcome::Ok { .. }) {
                report
                    .issues
                    .push(Issue::warning(origin.as_str(), check, why));
            }
        }
    }
    if !any_icon {
        if let Ok(u) = origin.join("favicon.ico") {
            if !matches!(probe(&client, &u, false).await, Outcome::Ok { .. }) {
                report.issues.push(Issue::warning(
                    origin.as_str(),
                    "favicon",
                    "no favicon declared and no /favicon.ico",
                ));
            }
        }
    }

    report.sort();
    Ok(report)
}
