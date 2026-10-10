//! End to end: serve fixture sites from 127.0.0.1 and crawl them. No test touches the internet.

use shipcheck::crawl::{run, Config};
use shipcheck::model::{Report, Severity};
use shipcheck::serve;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use url::Url;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn offline() -> Config {
    Config {
        external: false,
        ..Config::default()
    }
}

fn count(report: &Report, check: &str, needle: &str) -> usize {
    report
        .issues
        .iter()
        .filter(|i| i.check == check && i.message.contains(needle))
        .count()
}

#[tokio::test]
async fn a_complete_site_passes_with_no_issues() {
    // The good fixture's og:image must be absolute, so point it at the served pixel.
    let dir = TempDir::copy_of(&fixture("good"));
    let base = serve::start(dir.path().to_owned()).await.unwrap();
    for page in ["index.html", "about.html"] {
        let p = dir.path().join(page);
        let html = std::fs::read_to_string(&p).unwrap();
        std::fs::write(p, html.replace("OG_IMAGE_URL", &format!("{base}pixel.svg"))).unwrap();
    }
    let report = run(Url::parse(&base).unwrap(), &offline()).await.unwrap();
    assert_eq!(report.pages_checked, 2, "home and about");
    assert!(report.issues.is_empty(), "{:#?}", report.issues);
}

#[tokio::test]
async fn a_broken_site_reports_each_problem_once() {
    let base = serve::start(fixture("bad")).await.unwrap();
    let report = run(Url::parse(&base).unwrap(), &offline()).await.unwrap();
    let has = |check: &str, sev: Severity, needle: &str| {
        report
            .issues
            .iter()
            .any(|i| i.check == check && i.severity == sev && i.message.contains(needle))
    };
    assert_eq!(report.pages_checked, 2, "home and blog");
    assert!(has("broken-link", Severity::Error, "/nowhere"));
    assert!(has("broken-link", Severity::Error, "/gone.html"));
    assert!(has("broken-resource", Severity::Error, "/missing.png"));
    assert!(has("img-alt", Severity::Error, "/missing.png"));
    assert!(has("og-image", Severity::Error, "absolute"));
    assert!(has("h1", Severity::Warning, "2 <h1>"));
    assert!(has("viewport", Severity::Error, ""));
    assert!(has("robots", Severity::Warning, ""));
    assert!(has("sitemap", Severity::Warning, ""));
    assert!(has("favicon", Severity::Warning, ""));
    // A broken sub-page is a broken link on the page that links to it, and nothing more.
    assert_eq!(count(&report, "broken-link", "/nowhere"), 1);
    assert_eq!(count(&report, "broken-link", "/gone.html"), 1);
    assert_eq!(count(&report, "fetch", ""), 0, "{:#?}", report.issues);
}

#[tokio::test]
async fn a_start_page_that_fails_is_reported() {
    let base = serve::start(fixture("good")).await.unwrap();
    let start = Url::parse(&base).unwrap().join("/missing-page").unwrap();
    let report = run(start, &offline()).await.unwrap();
    assert_eq!(report.pages_checked, 0);
    assert_eq!(count(&report, "fetch", "404"), 1, "{:#?}", report.issues);
}

#[tokio::test]
async fn a_start_page_that_is_not_html_is_an_error_not_a_silent_pass() {
    let base = serve::start(fixture("good")).await.unwrap();
    let start = Url::parse(&base).unwrap().join("/robots.txt").unwrap();
    let report = run(start, &offline()).await.unwrap();
    assert_eq!(report.pages_checked, 0);
    assert_eq!(
        count(&report, "fetch", "not HTML"),
        1,
        "{:#?}",
        report.issues
    );
}

#[tokio::test]
async fn the_crawl_limit_is_respected_and_reported() {
    let base = serve::start(fixture("bad")).await.unwrap();
    let cfg = Config {
        max_pages: 1,
        ..offline()
    };
    let report = run(Url::parse(&base).unwrap(), &cfg).await.unwrap();
    assert_eq!(report.pages_checked, 1);
    assert_eq!(count(&report, "crawl-limit", ""), 1);
}

/// `http://example.com` redirecting to `https://www.example.com` is the most common start.
/// The crawl must continue on the new origin and treat its links as internal.
#[tokio::test]
async fn a_start_url_that_redirects_to_another_origin_is_still_crawled() {
    let target = spawn(|path| match path {
        "/" => html(r#"<a href="/about">About</a><a href="/missing">Missing</a>"#),
        "/about" => html("<p>About</p>"),
        _ => not_found(),
    })
    .await;
    let location = target.clone();
    let start = spawn(move |_| {
        format!("HTTP/1.1 301 Moved Permanently\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            .into_bytes()
    })
    .await;
    let report = run(Url::parse(&start).unwrap(), &offline()).await.unwrap();
    assert_eq!(report.pages_checked, 2, "home and about on the new origin");
    assert_eq!(
        report.links_checked, 2,
        "internal links are checked with --no-external"
    );
    let missing = report
        .issues
        .iter()
        .find(|i| i.check == "broken-link" && i.message.contains("/missing"))
        .expect("the broken internal link is found");
    assert_eq!(missing.severity, Severity::Error);
}

/// CDNs often answer HEAD over HTTP/2 without a Content-Length. The size must still be measured.
#[tokio::test]
async fn image_size_is_measured_when_head_has_no_content_length() {
    let base = spawn(|path| match path {
        "/big.png" => big_png_without_length(),
        _ => html(r#"<img src="/big.png" alt="big">"#),
    })
    .await;
    let report = run(Url::parse(&base).unwrap(), &offline()).await.unwrap();
    assert_eq!(
        count(&report, "image-size", "400 KB"),
        1,
        "{:#?}",
        report.issues
    );
}

/// One URL used as a link on one page and as an image on another gets the image checks.
#[tokio::test]
async fn a_url_used_as_link_and_image_gets_the_image_checks() {
    let base = spawn(|path| match path {
        "/big.png" => big_png_without_length(),
        "/" => html(r#"<a href="/big.png">Full size</a><a href="/gallery">Gallery</a>"#),
        "/gallery" => html(r#"<img src="/big.png" alt="big">"#),
        _ => not_found(),
    })
    .await;
    let report = run(Url::parse(&base).unwrap(), &offline()).await.unwrap();
    assert_eq!(
        count(&report, "image-size", "/big.png"),
        2,
        "{:#?}",
        report.issues
    );
}

#[tokio::test]
async fn the_static_server_only_answers_its_own_host() {
    let base = serve::start(fixture("good")).await.unwrap();
    let addr = Url::parse(&base).unwrap();
    let addr = format!("{}:{}", addr.host_str().unwrap(), addr.port().unwrap());
    for (host, expected) in [(addr.as_str(), "200"), ("evil.example", "400")] {
        let mut s = tokio::net::TcpStream::connect(&addr).await.unwrap();
        // Sent in two writes: the server must read the whole header, not one packet.
        s.write_all(b"GET / HTTP/1.1\r\n").await.unwrap();
        s.write_all(format!("Host: {host}\r\n\r\n").as_bytes())
            .await
            .unwrap();
        let mut reply = String::new();
        s.read_to_string(&mut reply).await.unwrap();
        assert!(
            reply.starts_with(&format!("HTTP/1.1 {expected}")),
            "{host}: {reply}"
        );
    }
}

#[test]
fn the_static_server_never_serves_outside_its_folder() {
    let root = fixture("good");
    assert!(
        serve::locate(&root, "/about").is_some(),
        "extensionless page resolves to about.html"
    );
    assert!(
        serve::locate(&root, "/").is_some(),
        "folder resolves to index.html"
    );
    for path in [
        "/../bad/index.html",
        "/%2e%2e/bad/index.html",
        "/%2E%2E%2Fbad%2Findex.html",
        "/..%5Cbad%5Cindex.html",
        "//etc/passwd",
        "/nope",
        // A multibyte character right after `%` must not panic the decoder.
        "/%a\u{e9}",
    ] {
        assert!(serve::locate(&root, path).is_none(), "{path}");
    }
}

/// Serving `out/` must never answer `/` with `out.html` from the parent folder.
#[test]
fn the_root_path_never_resolves_to_a_sibling_file() {
    let dir = TempDir::new();
    let root = dir.path().join("out");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("index.html"), "inside").unwrap();
    std::fs::write(dir.path().join("out.html"), "outside").unwrap();
    let found = serve::locate(&root, "/").unwrap();
    assert_eq!(std::fs::read_to_string(found).unwrap(), "inside");
}

#[test]
fn hidden_files_are_not_served_but_well_known_is() {
    let dir = TempDir::new();
    std::fs::write(dir.path().join(".env"), "SECRET=1").unwrap();
    std::fs::create_dir(dir.path().join(".well-known")).unwrap();
    std::fs::write(dir.path().join(".well-known/security.txt"), "x").unwrap();
    assert!(serve::locate(dir.path(), "/.env").is_none());
    assert!(serve::locate(dir.path(), "/.well-known/security.txt").is_some());
}

#[cfg(unix)]
#[test]
fn a_symlink_pointing_outside_the_folder_is_not_followed() {
    let dir = TempDir::new();
    let root = dir.path().join("site");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(dir.path().join("private.txt"), "outside").unwrap();
    std::os::unix::fs::symlink(dir.path().join("private.txt"), root.join("leak.txt")).unwrap();
    assert!(serve::locate(&root, "/leak.txt").is_none());
}

// ---- helpers ----

/// A temp folder deleted when the test ends.
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        // Tests run in parallel threads of one process, so a counter keeps the names apart.
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("shipcheck-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn copy_of(src: &Path) -> Self {
        let dir = Self::new();
        for entry in std::fs::read_dir(src).unwrap() {
            let entry = entry.unwrap();
            std::fs::copy(entry.path(), dir.0.join(entry.file_name())).unwrap();
        }
        dir
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A tiny HTTP server for cases the static server cannot produce (redirects, missing headers).
/// `respond` gets the request path and returns the raw response bytes.
async fn spawn(respond: impl Fn(&str) -> Vec<u8> + Send + Sync + 'static) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/", listener.local_addr().unwrap());
    let respond = Arc::new(respond);
    tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            let respond = respond.clone();
            tokio::spawn(async move {
                let mut buf = vec![0u8; 4096];
                let n = stream.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).into_owned();
                let mut parts = req.split_whitespace();
                let method = parts.next().unwrap_or("");
                let path = parts.next().unwrap_or("/");
                let mut response = respond(path);
                if method == "HEAD" {
                    if let Some(end) = response.windows(4).position(|w| w == b"\r\n\r\n") {
                        response.truncate(end + 4);
                    }
                }
                let _ = stream.write_all(&response).await;
                let _ = stream.shutdown().await;
            });
        }
    });
    base
}

fn html(body: &str) -> Vec<u8> {
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .into_bytes()
}

fn not_found() -> Vec<u8> {
    b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec()
}

/// A 400 KB image with no Content-Length: the body ends when the connection closes.
fn big_png_without_length() -> Vec<u8> {
    let mut r = b"HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nConnection: close\r\n\r\n".to_vec();
    r.extend(std::iter::repeat_n(0u8, 400 * 1024));
    r
}
