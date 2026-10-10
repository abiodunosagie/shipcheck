//! Checks that need only one page's HTML. Pure functions, so they are tested without a network.

use crate::model::Issue;
use scraper::{Html, Selector};
use url::Url;

/// What kind of resource a page loads, which decides how its check result is reported.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ResourceKind {
    Image,
    Script,
    Stylesheet,
    Icon,
    PreviewImage,
}

/// A URL found on a page that the crawler should verify.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Target {
    pub url: Url,
    pub kind: TargetKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TargetKind {
    Link,
    Resource(ResourceKind),
}

/// The result of reading one page.
#[derive(Debug, Default)]
pub struct PageAnalysis {
    pub issues: Vec<Issue>,
    /// Every link and resource worth checking, already resolved against the page URL.
    pub targets: Vec<Target>,
    /// Whether the page declares a favicon with `<link rel="icon">`.
    pub has_icon: bool,
}

/// Recommended upper bound for a title, beyond which search results truncate it.
pub const TITLE_MAX: usize = 60;
/// Recommended meta description length range.
pub const DESCRIPTION_RANGE: std::ops::RangeInclusive<usize> = 50..=160;

fn sel(s: &str) -> Selector {
    Selector::parse(s).expect("static selector is valid")
}

fn meta_content(doc: &Html, attr: &str, name: &str) -> Option<String> {
    let s = sel(&format!(r#"meta[{attr}="{name}"]"#));
    doc.select(&s)
        .next()
        .and_then(|m| m.value().attr("content"))
        .map(|c| c.trim().to_owned())
}

/// Resolves an href against the page, skipping links that cannot be fetched (mailto, tel, fragments...).
fn resolve(base: &Url, href: &str) -> Option<Url> {
    let href = href.trim();
    if href.is_empty() || href.starts_with('#') {
        return None;
    }
    let lower = href.to_ascii_lowercase();
    if ["mailto:", "tel:", "javascript:", "data:", "sms:"]
        .iter()
        .any(|p| lower.starts_with(p))
    {
        return None;
    }
    let mut url = base.join(href).ok()?;
    if !matches!(url.scheme(), "http" | "https") {
        return None;
    }
    url.set_fragment(None);
    Some(url)
}

/// Runs every single-page check on `html`, served from `page`.
pub fn analyze(page: &Url, html: &str) -> PageAnalysis {
    let doc = Html::parse_document(html);
    let p = page.as_str();
    let mut out = PageAnalysis::default();
    let issues = &mut out.issues;

    // Title
    let title = doc
        .select(&sel("head title"))
        .next()
        .map(|t| t.text().collect::<String>().trim().to_owned());
    match title.as_deref() {
        None | Some("") => issues.push(Issue::error(p, "title", "missing <title>")),
        Some(t) if t.chars().count() > TITLE_MAX => issues.push(Issue::warning(
            p,
            "title",
            format!(
                "title is {} characters; search results cut off after about {TITLE_MAX}",
                t.chars().count()
            ),
        )),
        _ => {}
    }

    // Meta description
    match meta_content(&doc, "name", "description") {
        None => issues.push(Issue::warning(p, "description", "missing meta description")),
        Some(d) => {
            let n = d.chars().count();
            if !DESCRIPTION_RANGE.contains(&n) {
                issues.push(Issue::warning(
                    p,
                    "description",
                    format!(
                        "meta description is {n} characters; aim for {} to {}",
                        DESCRIPTION_RANGE.start(),
                        DESCRIPTION_RANGE.end()
                    ),
                ));
            }
        }
    }

    // Basics every page needs
    let lang = doc
        .select(&sel("html"))
        .next()
        .and_then(|h| h.value().attr("lang"))
        .unwrap_or("")
        .trim()
        .to_owned();
    if lang.is_empty() {
        issues.push(Issue::warning(
            p,
            "lang",
            "missing lang attribute on <html>, screen readers guess the language",
        ));
    }
    if meta_content(&doc, "name", "viewport").is_none() {
        issues.push(Issue::error(
            p,
            "viewport",
            "missing viewport meta tag, the page will not scale on phones",
        ));
    }
    if doc
        .select(&sel(r#"link[rel="canonical"]"#))
        .next()
        .is_none()
    {
        issues.push(Issue::warning(p, "canonical", "missing canonical link"));
    }

    // Link previews (Open Graph and Twitter cards)
    if meta_content(&doc, "property", "og:title").is_none() {
        issues.push(Issue::warning(
            p,
            "og-title",
            "missing og:title, link previews fall back to the page title",
        ));
    }
    if meta_content(&doc, "property", "og:description").is_none() {
        issues.push(Issue::warning(
            p,
            "og-description",
            "missing og:description",
        ));
    }
    match meta_content(&doc, "property", "og:image") {
        None => issues.push(Issue::error(p, "og-image", "missing og:image, shared links show a blank preview")),
        Some(src) => match Url::parse(&src) {
            Ok(url) if matches!(url.scheme(), "http" | "https") => out
                .targets
                .push(Target { url, kind: TargetKind::Resource(ResourceKind::PreviewImage) }),
            _ => issues.push(Issue::error(
                p,
                "og-image",
                format!("og:image must be an absolute URL, got \"{src}\"; most platforms ignore relative ones"),
            )),
        },
    }
    if meta_content(&doc, "name", "twitter:card").is_none() {
        issues.push(Issue::warning(
            p,
            "twitter-card",
            "missing twitter:card, X shows a small preview or none",
        ));
    }

    // Headings
    let h1s = doc.select(&sel("h1")).count();
    if h1s == 0 {
        issues.push(Issue::warning(p, "h1", "no <h1> on the page"));
    } else if h1s > 1 {
        issues.push(Issue::warning(
            p,
            "h1",
            format!("{h1s} <h1> elements; use one per page"),
        ));
    }

    // Images: alt text and resources to verify
    for img in doc.select(&sel("img")) {
        let src = img.value().attr("src").unwrap_or("");
        if img.value().attr("alt").is_none() {
            let shown = if src.is_empty() {
                "(no src)".to_owned()
            } else {
                src.to_owned()
            };
            issues.push(Issue::error(
                p,
                "img-alt",
                format!("image without alt text: {shown} (use alt=\"\" if decorative)"),
            ));
        }
        if let Some(url) = resolve(page, src) {
            out.targets.push(Target {
                url,
                kind: TargetKind::Resource(ResourceKind::Image),
            });
        }
    }

    // Scripts, stylesheets, icons
    for s in doc.select(&sel("script[src]")) {
        if let Some(url) = s.value().attr("src").and_then(|h| resolve(page, h)) {
            out.targets.push(Target {
                url,
                kind: TargetKind::Resource(ResourceKind::Script),
            });
        }
    }
    for l in doc.select(&sel("link[href][rel]")) {
        let rel = l.value().attr("rel").unwrap_or("").to_ascii_lowercase();
        let kind = if rel.split_whitespace().any(|r| r == "stylesheet") {
            ResourceKind::Stylesheet
        } else if rel
            .split_whitespace()
            .any(|r| r == "icon" || r == "apple-touch-icon")
        {
            out.has_icon = true;
            ResourceKind::Icon
        } else {
            continue;
        };
        if let Some(url) = l.value().attr("href").and_then(|h| resolve(page, h)) {
            out.targets.push(Target {
                url,
                kind: TargetKind::Resource(kind),
            });
        }
    }

    // Links
    for a in doc.select(&sel("a[href]")) {
        if let Some(url) = a.value().attr("href").and_then(|h| resolve(page, h)) {
            out.targets.push(Target {
                url,
                kind: TargetKind::Link,
            });
        }
    }

    // Mixed content: an https page loading http resources (links to http pages are fine)
    if page.scheme() == "https" {
        for t in &out.targets {
            if matches!(t.kind, TargetKind::Resource(_)) && t.url.scheme() == "http" {
                out.issues.push(Issue::error(
                    p,
                    "mixed-content",
                    format!(
                        "https page loads an http resource, browsers block it: {}",
                        t.url
                    ),
                ));
            }
        }
    }

    out.targets
        .sort_by(|a, b| a.url.as_str().cmp(b.url.as_str()));
    out.targets.dedup();
    out
}
