//! End to end: serve the fixture folders with the built-in server and crawl them.

use shipcheck::crawl::{run, Config};
use shipcheck::model::Severity;
use shipcheck::serve;
use std::path::{Path, PathBuf};
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

#[tokio::test]
async fn a_complete_site_passes_with_no_issues() {
    // The good fixture's og:image must be absolute, so point it at the served pixel.
    let dir = tempdir_with_og(&fixture("good"));
    let base = serve::start(dir.clone()).await.unwrap();
    rewrite_og(&dir, &format!("{base}pixel.svg"));
    let report = run(Url::parse(&base).unwrap(), &offline()).await.unwrap();
    assert_eq!(report.pages_checked, 2, "home and about");
    assert!(report.issues.is_empty(), "{:#?}", report.issues);
}

#[tokio::test]
async fn a_broken_site_reports_links_resources_and_tags() {
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
    assert!(
        !report.issues.iter().any(|i| i.check == "fetch"),
        "a broken sub-page is reported once, as a broken link, not again as a page: {:#?}",
        report.issues
    );
}

#[tokio::test]
async fn a_start_page_that_fails_is_reported() {
    let base = serve::start(fixture("good")).await.unwrap();
    let start = Url::parse(&base).unwrap().join("/missing-page").unwrap();
    let report = run(start, &offline()).await.unwrap();
    assert_eq!(report.pages_checked, 0);
    assert!(report
        .issues
        .iter()
        .any(|i| i.check == "fetch" && i.severity == Severity::Error && i.message.contains("404")));
}

/// CDNs often answer HEAD over HTTP/2 without a Content-Length. The size must still be measured.
#[tokio::test]
async fn image_size_is_measured_when_head_has_no_content_length() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/", listener.local_addr().unwrap());
    tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            tokio::spawn(async move {
                let mut buf = vec![0u8; 4096];
                let n = stream.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).into_owned();
                let head = req.starts_with("HEAD");
                let response: Vec<u8> = if req.contains(" /big.png ") {
                    // No Content-Length on either method: the body ends when the connection closes.
                    let mut r =
                        b"HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nConnection: close\r\n\r\n"
                            .to_vec();
                    if !head {
                        r.extend(std::iter::repeat_n(0u8, 400 * 1024));
                    }
                    r
                } else {
                    let body = r#"<html><body><img src="/big.png" alt="big"></body></html>"#;
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        if head { "" } else { body }
                    )
                    .into_bytes()
                };
                let _ = stream.write_all(&response).await;
                let _ = stream.shutdown().await;
            });
        }
    });
    let report = run(Url::parse(&base).unwrap(), &offline()).await.unwrap();
    assert!(
        report
            .issues
            .iter()
            .any(|i| i.check == "image-size" && i.message.contains("400 KB")),
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
    assert!(report.issues.iter().any(|i| i.check == "crawl-limit"));
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
    assert!(serve::locate(&root, "/../bad/index.html").is_none());
    assert!(serve::locate(&root, "/%2e%2e/bad/index.html").is_none());
    assert!(serve::locate(&root, "/%2E%2E%2Fbad%2Findex.html").is_none());
    assert!(
        serve::locate(&root, "/..%5Cbad%5Cindex.html").is_none(),
        "backslashes cannot climb out on Windows"
    );
    assert!(
        serve::locate(&root, "//etc/passwd").is_none(),
        "a leading // stays inside the folder"
    );
    assert!(serve::locate(&root, "/nope").is_none());
    // A multibyte character right after `%` must not panic the decoder.
    assert!(serve::locate(&root, "/%a\u{e9}").is_none());
}

/// Copies a fixture folder into a fresh temp folder so a test can edit it.
fn tempdir_with_og(src: &Path) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "shipcheck-{}-{}",
        std::process::id(),
        rand_suffix()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    for entry in std::fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        std::fs::copy(entry.path(), dir.join(entry.file_name())).unwrap();
    }
    dir
}

fn rewrite_og(dir: &Path, og: &str) {
    for page in ["index.html", "about.html"] {
        let p = dir.join(page);
        let html = std::fs::read_to_string(&p)
            .unwrap()
            .replace("OG_IMAGE_URL", og);
        std::fs::write(p, html).unwrap();
    }
}

fn rand_suffix() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos()
}
