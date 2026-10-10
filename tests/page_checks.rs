//! Single-page checks, run on HTML strings with no network.

use shipcheck::model::Severity;
use shipcheck::page::{analyze, ResourceKind, TargetKind};
use url::Url;

fn page() -> Url {
    Url::parse("https://example.com/blog/post").unwrap()
}

fn checks(html: &str) -> Vec<(&'static str, Severity)> {
    analyze(&page(), html)
        .issues
        .into_iter()
        .map(|i| (i.check, i.severity))
        .collect()
}

const COMPLETE: &str = r#"<!doctype html><html lang="en"><head>
<meta name="viewport" content="width=device-width">
<title>A fine page</title>
<meta name="description" content="A description that is comfortably between fifty and one hundred sixty characters.">
<link rel="canonical" href="https://example.com/blog/post">
<meta property="og:title" content="A fine page">
<meta property="og:description" content="Preview text">
<meta property="og:image" content="https://example.com/og.png">
<meta name="twitter:card" content="summary_large_image">
</head><body><h1>Heading</h1><img src="a.png" alt="An a"></body></html>"#;

#[test]
fn a_complete_page_has_no_issues() {
    assert_eq!(checks(COMPLETE), vec![]);
}

#[test]
fn an_empty_page_reports_every_missing_basic() {
    let found = checks("<html><head></head><body></body></html>");
    for (check, severity) in [
        ("title", Severity::Error),
        ("description", Severity::Warning),
        ("lang", Severity::Warning),
        ("viewport", Severity::Error),
        ("canonical", Severity::Warning),
        ("og-image", Severity::Error),
        ("twitter-card", Severity::Warning),
        ("h1", Severity::Warning),
    ] {
        assert!(
            found.contains(&(check, severity)),
            "expected {check} {severity}, got {found:?}"
        );
    }
}

#[test]
fn a_relative_og_image_is_an_error_because_platforms_ignore_it() {
    let html = COMPLETE.replace("https://example.com/og.png", "/og.png");
    assert!(checks(&html).contains(&("og-image", Severity::Error)));
}

#[test]
fn images_need_alt_but_empty_alt_is_fine_for_decoration() {
    let html = COMPLETE.replace(
        r#"<img src="a.png" alt="An a">"#,
        r#"<img src="a.png"><img src="b.png" alt="">"#,
    );
    let alts: Vec<_> = checks(&html)
        .into_iter()
        .filter(|(c, _)| *c == "img-alt")
        .collect();
    assert_eq!(alts, vec![("img-alt", Severity::Error)]);
}

#[test]
fn several_h1_and_long_titles_are_warnings() {
    let html = COMPLETE
        .replace("<h1>Heading</h1>", "<h1>One</h1><h1>Two</h1>")
        .replace(
            "A fine page</title>",
            "A title that keeps going far past the sixty characters search shows</title>",
        );
    let found = checks(&html);
    assert!(found.contains(&("h1", Severity::Warning)));
    assert!(found.contains(&("title", Severity::Warning)));
}

#[test]
fn http_resources_on_an_https_page_are_mixed_content_but_http_links_are_not() {
    let html = COMPLETE.replace(
        "</body>",
        r#"<script src="http://cdn.example.com/x.js"></script><a href="http://old.example.com">old</a></body>"#,
    );
    let mixed: Vec<_> = analyze(&page(), &html)
        .issues
        .into_iter()
        .filter(|i| i.check == "mixed-content")
        .collect();
    assert_eq!(mixed.len(), 1);
    assert!(mixed[0].message.contains("x.js"));
}

#[test]
fn targets_are_resolved_deduplicated_and_skip_unfetchable_links() {
    let html = COMPLETE.replace(
        "</body>",
        r##"<a href="../about">About</a><a href="/about#team">Team</a><a href="mailto:a@b.c">Mail</a>
        <a href="tel:+1">Call</a><a href="#top">Top</a><a href="javascript:void(0)">JS</a>
        <link rel="stylesheet" href="/site.css"><link rel="icon" href="/favicon.svg"></body>"##,
    );
    let a = analyze(&page(), &html);
    let urls: Vec<_> = a
        .targets
        .iter()
        .map(|t| (t.url.as_str().to_owned(), t.kind))
        .collect();
    assert!(urls.contains(&("https://example.com/about".into(), TargetKind::Link)));
    assert_eq!(
        urls.iter()
            .filter(|(u, _)| u == "https://example.com/about")
            .count(),
        1,
        "fragment variants merge"
    );
    assert!(urls.contains(&(
        "https://example.com/blog/a.png".into(),
        TargetKind::Resource(ResourceKind::Image)
    )));
    assert!(urls.contains(&(
        "https://example.com/og.png".into(),
        TargetKind::Resource(ResourceKind::PreviewImage)
    )));
    assert!(urls.contains(&(
        "https://example.com/site.css".into(),
        TargetKind::Resource(ResourceKind::Stylesheet)
    )));
    assert!(urls.iter().all(|(u, _)| u.starts_with("https://")));
    assert!(a.has_icon);
}

#[test]
fn relative_urls_follow_the_base_tag() {
    let html = COMPLETE.replace("</head>", r#"<base href="/docs/"></head>"#);
    let urls: Vec<String> = analyze(&page(), &html)
        .targets
        .iter()
        .map(|t| t.url.to_string())
        .collect();
    assert!(
        urls.contains(&"https://example.com/docs/a.png".to_owned()),
        "{urls:?}"
    );
}

#[test]
fn tags_match_in_any_case_and_open_graph_also_counts_under_name() {
    let html = COMPLETE
        .replace(r#"name="description""#, r#"name="Description""#)
        .replace(r#"rel="canonical""#, r#"rel="Canonical""#)
        .replace(r#"property="og:image""#, r#"name="og:image""#)
        .replace(r#"property="og:title""#, r#"property="OG:Title""#);
    assert_eq!(checks(&html), vec![]);
}
