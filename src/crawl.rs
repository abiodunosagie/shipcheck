//! Crawls a site's own pages, then verifies every link and resource they reference.

use crate::model::{Issue, Report, Severity};
use crate::page::{self, ResourceKind, Target, TargetKind};
use futures::stream::{self, StreamExt};
use reqwest::header::{CONTENT_LENGTH, CONTENT_TYPE};
use reqwest::{Client, Response};
use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};
use std::time::Duration;
use url::Url;

/// Default for `--max-pages`.
pub const DEFAULT_MAX_PAGES: usize = 200;
/// Default for `--concurrency`.
pub const DEFAULT_CONCURRENCY: usize = 8;
/// Default for `--timeout`, in seconds.
pub const DEFAULT_TIMEOUT_SECS: u64 = 15;
/// Default for `--max-image-kb`.
pub const DEFAULT_MAX_IMAGE_KB: u64 = 300;

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
            max_pages: DEFAULT_MAX_PAGES,
            concurrency: DEFAULT_CONCURRENCY,
            timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECS),
            external: true,
            max_image_bytes: DEFAULT_MAX_IMAGE_KB * 1024,
        }
    }
}

/// Only this much of a page is read, so a huge or endless response cannot exhaust memory.
const MAX_PAGE_BYTES: usize = 10 * 1024 * 1024;
/// Stop counting an image after this many bytes; anything bigger is reported as at least this.
const MAX_COUNTED_BYTES: u64 = 64 * 1024 * 1024;

const USER_AGENT: &str = concat!(
    "shipcheck/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/abiodunosagie/shipcheck)"
);

/// How a checked URL answered.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Outcome {
    Ok {
        bytes: Option<u64>,
        content_type: Option<String>,
    },
    Status(u16),
    Failed(String),
}

/// A link or resource and every page that references it.
struct Referenced {
    target: Target,
    pages: BTreeSet<String>,
}

/// What the crawl step hands to the later steps.
struct Crawled {
    /// Every unique URL found, keyed by URL.
    targets: BTreeMap<String, Referenced>,
    /// The origins that count as this site: the start URL's, plus the one it redirected to.
    site: Vec<Url>,
    any_icon: bool,
}

/// Audits the site starting at `start`.
pub async fn run(start: Url, cfg: &Config) -> Result<Report, String> {
    let client = Client::builder()
        .user_agent(USER_AGENT)
        .timeout(cfg.timeout)
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .map_err(|e| format!("could not create HTTP client: {e}"))?;
    let mut report = Report {
        site: start.as_str().to_owned(),
        ..Default::default()
    };
    let crawled = crawl_pages(&client, &start, cfg, &mut report).await;
    verify_targets(&client, &crawled, cfg, &mut report).await;
    check_site_files(&client, &crawled, &mut report).await;
    report.sort();
    Ok(report)
}

/// Step 1: fetch same-site HTML pages breadth first and analyse each one.
async fn crawl_pages(client: &Client, start: &Url, cfg: &Config, report: &mut Report) -> Crawled {
    let mut crawled = Crawled {
        targets: BTreeMap::new(),
        site: vec![start.clone()],
        any_icon: false,
    };
    let mut queue = VecDeque::from([start.clone()]);
    let mut seen: HashSet<String> = HashSet::from([page_key(start)]);
    // Pages already analysed, by their final URL after redirects.
    let mut analysed: HashSet<String> = HashSet::new();

    while report.pages_checked < cfg.max_pages {
        let Some(url) = queue.pop_front() else { break };
        let is_start = url == *start;
        // A page that fails to load is reported once: for the start page here, for every other
        // page by step 2 as a broken link on each page that links to it.
        let fail = |report: &mut Report, message: String| {
            if is_start {
                report
                    .issues
                    .push(Issue::error(url.as_str(), "fetch", message));
            }
        };
        let resp = match client.get(url.clone()).send().await {
            Ok(r) => r,
            Err(e) => {
                fail(report, format!("could not load page: {}", describe(&e)));
                continue;
            }
        };
        let status = resp.status();
        let final_url = resp.url().clone();
        let ctype = content_type(&resp);
        if !status.is_success() {
            fail(report, format!("page returns HTTP {}", status.as_u16()));
            continue;
        }
        if !is_html(ctype.as_deref()) {
            fail(
                report,
                format!(
                    "page is not HTML (Content-Type \"{}\"), so nothing could be checked",
                    ctype.as_deref().unwrap_or("none")
                ),
            );
            continue;
        }
        // A redirect from http to https, or to www, keeps the crawl on the same site.
        if is_start && !crawled.site.iter().any(|s| same_origin(s, &final_url)) {
            crawled.site.push(final_url.clone());
        }
        if !analysed.insert(page_key(&final_url)) {
            continue;
        }
        seen.insert(page_key(&final_url));
        let html = match read_capped(resp, MAX_PAGE_BYTES).await {
            Ok(h) => h,
            Err(e) => {
                fail(report, format!("could not read page: {}", describe(&e)));
                continue;
            }
        };
        report.pages_checked += 1;
        let analysis = page::analyze(&final_url, &html);
        crawled.any_icon |= analysis.has_icon;
        report.issues.extend(analysis.issues);
        let page_url = final_url.to_string();
        for t in analysis.targets {
            if t.kind == TargetKind::Link
                && in_site(&crawled.site, &t.url)
                && seen.insert(page_key(&t.url))
            {
                queue.push_back(t.url.clone());
            }
            // Every target is probed once in step 2, same-site links included, so pages past the
            // crawl limit are still checked for being reachable.
            let entry = crawled
                .targets
                .entry(t.url.as_str().to_owned())
                .or_insert_with(|| Referenced {
                    target: t.clone(),
                    pages: BTreeSet::new(),
                });
            // One URL can be a link on one page and an og:image on another; keep the kind with
            // the most checks so none are skipped.
            if strength(t.kind) > strength(entry.target.kind) {
                entry.target.kind = t.kind;
            }
            entry.pages.insert(page_url.clone());
        }
    }
    if !queue.is_empty() {
        report.issues.push(Issue::warning(
            &report.site,
            "crawl-limit",
            format!(
                "stopped after {} pages with more links still unvisited; raise --max-pages to check the rest",
                cfg.max_pages
            ),
        ));
    }
    crawled
}

/// Step 2: request every unique link and resource once, concurrently.
async fn verify_targets(client: &Client, crawled: &Crawled, cfg: &Config, report: &mut Report) {
    let jobs: Vec<(&Referenced, bool)> = crawled
        .targets
        .values()
        .map(|r| (r, !in_site(&crawled.site, &r.target.url)))
        .filter(|(_, external)| cfg.external || !external)
        .collect();
    report.links_checked = jobs.len();
    let results: Vec<Vec<Issue>> = stream::iter(jobs)
        .map(|(r, external)| async move {
            let outcome = probe(client, &r.target.url, is_image(r.target.kind)).await;
            judge(&r.target, &outcome, cfg, external)
                .into_iter()
                .flat_map(|(severity, check, message)| {
                    r.pages
                        .iter()
                        .map(move |page| Issue::new(page, check, severity, message.clone()))
                })
                .collect()
        })
        .buffer_unordered(cfg.concurrency.max(1))
        .collect()
        .await;
    report.issues.extend(results.into_iter().flatten());
}

/// Step 3: robots.txt, sitemap.xml, and a favicon when no page declares one.
async fn check_site_files(client: &Client, crawled: &Crawled, report: &mut Report) {
    // The site's own origin is where the start URL ended up after redirects.
    let Some(origin) = crawled.site.last().and_then(|u| u.join("/").ok()) else {
        return;
    };
    let mut files = vec![
        ("robots.txt", "robots", "missing robots.txt"),
        (
            "sitemap.xml",
            "sitemap",
            "missing sitemap.xml, search engines discover pages more slowly",
        ),
    ];
    if !crawled.any_icon {
        files.push((
            "favicon.ico",
            "favicon",
            "no favicon declared and no /favicon.ico",
        ));
    }
    for (file, check, why) in files {
        let Ok(url) = origin.join(file) else { continue };
        if !matches!(probe(client, &url, false).await, Outcome::Ok { .. }) {
            report
                .issues
                .push(Issue::warning(origin.as_str(), check, why));
        }
    }
}

/// HEAD first because it is cheap. Some servers answer HEAD with an error (404, 403, 405, 501)
/// while GET works, so any failed HEAD is retried as GET before the URL is called broken.
///
/// With `need_size`, a success without a `Content-Length` header (common for HEAD over HTTP/2
/// on CDNs) is retried as GET and the body is counted, so image sizes are never silently skipped.
async fn probe(client: &Client, url: &Url, need_size: bool) -> Outcome {
    let mut res = client.head(url.clone()).send().await;
    let retry = match &res {
        Ok(r) => !r.status().is_success() || (need_size && header_len(r).is_none()),
        Err(e) => !e.is_timeout(),
    };
    if retry {
        res = client.get(url.clone()).send().await;
    }
    match res {
        Ok(mut r) if r.status().is_success() => {
            let content_type = content_type(&r);
            let mut bytes = header_len(&r);
            if need_size && bytes.is_none() {
                bytes = count_body(&mut r).await;
            }
            Outcome::Ok {
                bytes,
                content_type,
            }
        }
        Ok(r) => Outcome::Status(r.status().as_u16()),
        Err(e) => Outcome::Failed(describe(&e)),
    }
}

/// Body length, or `None` when the body could not be read to the end (so a partial count is never
/// reported as the size).
async fn count_body(r: &mut Response) -> Option<u64> {
    let mut total = 0u64;
    loop {
        match r.chunk().await {
            Ok(Some(chunk)) => {
                total += chunk.len() as u64;
                if total >= MAX_COUNTED_BYTES {
                    return Some(total);
                }
            }
            Ok(None) => return Some(total),
            Err(_) => return None,
        }
    }
}

/// Reads at most `cap` bytes of a page body and decodes it as UTF-8, replacing invalid bytes.
async fn read_capped(mut r: Response, cap: usize) -> reqwest::Result<String> {
    let mut body = Vec::new();
    while let Some(chunk) = r.chunk().await? {
        let room = cap - body.len();
        body.extend_from_slice(&chunk[..chunk.len().min(room)]);
        if body.len() >= cap {
            break;
        }
    }
    Ok(String::from_utf8_lossy(&body).into_owned())
}

/// Turns one URL's outcome into `(severity, check, message)` findings, which the caller reports
/// on every page that references the URL.
fn judge(
    target: &Target,
    outcome: &Outcome,
    cfg: &Config,
    external: bool,
) -> Vec<(Severity, &'static str, String)> {
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
    let url = &target.url;
    match outcome {
        // Many big sites answer automated requests with 401/403/429 even when the page is fine.
        Outcome::Status(s @ (401 | 403 | 429)) if external => vec![(
            Severity::Warning,
            check,
            format!(
                "could not verify {what} {url} (HTTP {s}, the site may block automated checks)"
            ),
        )],
        Outcome::Status(s) => vec![(
            Severity::Error,
            check,
            format!("{what} returns HTTP {s}: {url}"),
        )],
        Outcome::Failed(e) if external => {
            vec![(
                Severity::Warning,
                check,
                format!("could not reach {what} {url} ({e})"),
            )]
        }
        Outcome::Failed(e) => vec![(
            Severity::Error,
            check,
            format!("{what} failed ({e}): {url}"),
        )],
        Outcome::Ok {
            bytes,
            content_type,
        } => {
            let mut found = Vec::new();
            if is_image(target.kind) {
                if let Some(b) = bytes.filter(|b| *b > cfg.max_image_bytes) {
                    found.push((
                        Severity::Warning,
                        "image-size",
                        format!(
                            "{what} is {} KB (limit {} KB): {url}",
                            b / 1024,
                            cfg.max_image_bytes / 1024
                        ),
                    ));
                }
            }
            if target.kind == TargetKind::Resource(ResourceKind::PreviewImage) {
                let ct = content_type.as_deref().unwrap_or("");
                if !ct.to_ascii_lowercase().starts_with("image/") {
                    found.push((
                        Severity::Error,
                        "og-image",
                        format!(
                            "og:image is not served as an image (Content-Type \"{ct}\"): {url}"
                        ),
                    ));
                }
            }
            found
        }
    }
}

/// Same scheme, host and port.
fn same_origin(a: &Url, b: &Url) -> bool {
    a.scheme() == b.scheme()
        && a.host_str() == b.host_str()
        && a.port_or_known_default() == b.port_or_known_default()
}

/// A URL on this site: one we crawl rather than just check.
fn in_site(site: &[Url], url: &Url) -> bool {
    site.iter().any(|s| same_origin(s, url))
}

/// Treats `/about`, `/about/` and `/about?ref=x` as one page so it is not crawled twice.
fn page_key(u: &Url) -> String {
    let mut k = u.clone();
    k.set_query(None);
    let s = k.as_str().to_owned();
    s.strip_suffix('/').map(str::to_owned).unwrap_or(s)
}

fn is_html(content_type: Option<&str>) -> bool {
    content_type.is_some_and(|c| {
        let c = c.to_ascii_lowercase();
        c.starts_with("text/html") || c.starts_with("application/xhtml+xml")
    })
}

fn is_image(kind: TargetKind) -> bool {
    matches!(
        kind,
        TargetKind::Resource(ResourceKind::Image | ResourceKind::PreviewImage)
    )
}

/// How many checks a kind gets; a link is only checked for reachability.
fn strength(kind: TargetKind) -> u8 {
    match kind {
        TargetKind::Link => 0,
        TargetKind::Resource(ResourceKind::PreviewImage) => 3,
        TargetKind::Resource(ResourceKind::Image) => 2,
        TargetKind::Resource(_) => 1,
    }
}

fn content_type(r: &Response) -> Option<String> {
    r.headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned)
}

fn header_len(r: &Response) -> Option<u64> {
    r.headers()
        .get(CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok())
}

/// A short, readable reason for a failed request.
fn describe(e: &reqwest::Error) -> String {
    if e.is_timeout() {
        "timed out".into()
    } else if e.is_connect() {
        "could not connect".into()
    } else if e.is_redirect() {
        "too many redirects".into()
    } else {
        e.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(kind: TargetKind) -> Target {
        Target {
            url: Url::parse("https://example.com/x").unwrap(),
            kind,
        }
    }

    #[test]
    fn blocked_external_links_are_warnings_but_internal_ones_are_errors() {
        let t = target(TargetKind::Link);
        let cfg = Config::default();
        for status in [401, 403, 429] {
            let external = judge(&t, &Outcome::Status(status), &cfg, true);
            assert_eq!(external[0].0, Severity::Warning, "external {status}");
            let internal = judge(&t, &Outcome::Status(status), &cfg, false);
            assert_eq!(internal[0].0, Severity::Error, "internal {status}");
        }
        assert_eq!(
            judge(&t, &Outcome::Status(404), &cfg, true)[0].0,
            Severity::Error
        );
        let timeout = Outcome::Failed("timed out".into());
        assert_eq!(judge(&t, &timeout, &cfg, true)[0].0, Severity::Warning);
        assert_eq!(judge(&t, &timeout, &cfg, false)[0].0, Severity::Error);
    }

    #[test]
    fn preview_images_are_checked_for_size_and_type() {
        let t = target(TargetKind::Resource(ResourceKind::PreviewImage));
        let cfg = Config::default();
        let ok = Outcome::Ok {
            bytes: Some(1000),
            content_type: Some("image/png".into()),
        };
        assert!(judge(&t, &ok, &cfg, false).is_empty());
        let html = Outcome::Ok {
            bytes: Some(10 * 1024 * 1024),
            content_type: Some("text/html".into()),
        };
        let found: Vec<&str> = judge(&t, &html, &cfg, false).iter().map(|f| f.1).collect();
        assert_eq!(found, ["image-size", "og-image"]);
    }

    #[test]
    fn stronger_kinds_win() {
        let preview = TargetKind::Resource(ResourceKind::PreviewImage);
        let image = TargetKind::Resource(ResourceKind::Image);
        let script = TargetKind::Resource(ResourceKind::Script);
        assert!(strength(preview) > strength(image));
        assert!(strength(image) > strength(script));
        assert!(strength(script) > strength(TargetKind::Link));
    }

    #[test]
    fn page_keys_ignore_trailing_slash_and_query() {
        let k = |s: &str| page_key(&Url::parse(s).unwrap());
        assert_eq!(k("https://a.com/about"), k("https://a.com/about/"));
        assert_eq!(k("https://a.com/about"), k("https://a.com/about?ref=x"));
        assert_ne!(k("https://a.com/about"), k("https://a.com/blog"));
    }
}
