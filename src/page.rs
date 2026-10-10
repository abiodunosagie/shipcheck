//! Checks that need only one page's HTML. Pure functions, so they are tested without a network.

use crate::model::Issue;
use scraper::{ElementRef, Html, Selector};
use std::collections::HashMap;
use url::Url;

/// What kind of resource a page loads, which decides how its check result is reported.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResourceKind {
    Image,
    Script,
    Stylesheet,
    Icon,
    PreviewImage,
}

/// A URL found on a page that the crawler should verify.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    pub url: Url,
    pub kind: TargetKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

/// Runs every single-page check on `html`, served from `page`.
pub fn analyze(page: &Url, html: &str) -> PageAnalysis {
    let doc = Html::parse_document(html);
    let meta = Meta::read(&doc);
    // Relative URLs resolve against `<base href>` when the page sets one.
    let base = first(&doc, "base[href]")
        .and_then(|b| b.value().attr("href"))
        .and_then(|h| page.join(h.trim()).ok())
        .unwrap_or_else(|| page.clone());

    let mut out = PageAnalysis::default();
    let p = page.as_str();
    head_checks(&doc, &meta, p, &mut out.issues);
    preview_checks(&meta, p, &mut out);
    heading_checks(&doc, p, &mut out.issues);
    collect_targets(&doc, &base, p, &mut out);
    if page.scheme() == "https" {
        mixed_content(p, &mut out);
    }
    out.targets
        .sort_by(|a, b| a.url.as_str().cmp(b.url.as_str()));
    out.targets.dedup();
    out
}

/// Title, description, language, viewport and canonical.
fn head_checks(doc: &Html, meta: &Meta, p: &str, issues: &mut Vec<Issue>) {
    let title = first(doc, "head title").map(|t| t.text().collect::<String>().trim().to_owned());
    match title.as_deref() {
        None | Some("") => issues.push(Issue::error(p, "title", "missing <title>")),
        Some(t) if t.chars().count() > TITLE_MAX => {
            let msg = format!(
                "title is {} characters; search results cut off after about {TITLE_MAX}",
                t.chars().count()
            );
            issues.push(Issue::warning(p, "title", msg));
        }
        _ => {}
    }

    match meta.get("description") {
        None => issues.push(Issue::warning(p, "description", "missing meta description")),
        Some(d) if !DESCRIPTION_RANGE.contains(&d.chars().count()) => {
            let msg = format!(
                "meta description is {} characters; aim for {} to {}",
                d.chars().count(),
                DESCRIPTION_RANGE.start(),
                DESCRIPTION_RANGE.end()
            );
            issues.push(Issue::warning(p, "description", msg));
        }
        Some(_) => {}
    }

    let lang = first(doc, "html")
        .and_then(|h| h.value().attr("lang"))
        .unwrap_or("");
    if lang.trim().is_empty() {
        issues.push(Issue::warning(
            p,
            "lang",
            "missing lang attribute on <html>, screen readers guess the language",
        ));
    }
    if meta.get("viewport").is_none() {
        issues.push(Issue::error(
            p,
            "viewport",
            "missing viewport meta tag, the page will not scale on phones",
        ));
    }
    if !links_with_rel(doc)
        .iter()
        .any(|(rel, _)| rel.split_whitespace().any(|r| r == "canonical"))
    {
        issues.push(Issue::warning(p, "canonical", "missing canonical link"));
    }
}

/// Open Graph and Twitter card tags, which decide how a shared link looks.
fn preview_checks(meta: &Meta, p: &str, out: &mut PageAnalysis) {
    let issues = &mut out.issues;
    if meta.get("og:title").is_none() {
        issues.push(Issue::warning(
            p,
            "og-title",
            "missing og:title, link previews fall back to the page title",
        ));
    }
    if meta.get("og:description").is_none() {
        issues.push(Issue::warning(
            p,
            "og-description",
            "missing og:description",
        ));
    }
    match meta.get("og:image") {
        None => issues.push(Issue::error(
            p,
            "og-image",
            "missing og:image, shared links show a blank preview",
        )),
        Some(src) => match Url::parse(src) {
            Ok(url) if matches!(url.scheme(), "http" | "https") => out.targets.push(Target {
                url,
                kind: TargetKind::Resource(ResourceKind::PreviewImage),
            }),
            _ => {
                let msg = format!(
                    "og:image must be an absolute URL, got \"{src}\"; most platforms ignore relative ones"
                );
                issues.push(Issue::error(p, "og-image", msg));
            }
        },
    }
    if meta.get("twitter:card").is_none() {
        issues.push(Issue::warning(
            p,
            "twitter-card",
            "missing twitter:card, X shows a small preview or none",
        ));
    }
}

fn heading_checks(doc: &Html, p: &str, issues: &mut Vec<Issue>) {
    let h1s = doc.select(&sel("h1")).count();
    if h1s == 0 {
        issues.push(Issue::warning(p, "h1", "no <h1> on the page"));
    } else if h1s > 1 {
        let msg = format!("{h1s} <h1> elements; use one per page");
        issues.push(Issue::warning(p, "h1", msg));
    }
}

/// Images (with their alt text), scripts, stylesheets, icons and links.
fn collect_targets(doc: &Html, base: &Url, p: &str, out: &mut PageAnalysis) {
    let push = |out: &mut PageAnalysis, href: Option<&str>, kind: TargetKind| {
        if let Some(url) = href.and_then(|h| resolve(base, h)) {
            out.targets.push(Target { url, kind });
        }
    };

    for img in doc.select(&sel("img")) {
        let src = img.value().attr("src");
        if img.value().attr("alt").is_none() {
            let shown = src.filter(|s| !s.is_empty()).unwrap_or("(no src)");
            let msg = format!("image without alt text: {shown} (use alt=\"\" if decorative)");
            out.issues.push(Issue::error(p, "img-alt", msg));
        }
        push(out, src, TargetKind::Resource(ResourceKind::Image));
    }
    for s in doc.select(&sel("script[src]")) {
        push(
            out,
            s.value().attr("src"),
            TargetKind::Resource(ResourceKind::Script),
        );
    }
    for (rel, link) in links_with_rel(doc) {
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
        push(out, link.value().attr("href"), TargetKind::Resource(kind));
    }
    for a in doc.select(&sel("a[href]")) {
        push(out, a.value().attr("href"), TargetKind::Link);
    }
}

/// An https page loading http resources. Links to http pages are fine; only loaded resources count.
fn mixed_content(p: &str, out: &mut PageAnalysis) {
    for t in &out.targets {
        if matches!(t.kind, TargetKind::Resource(_)) && t.url.scheme() == "http" {
            let msg = format!(
                "https page loads an http resource, browsers block it: {}",
                t.url
            );
            out.issues.push(Issue::error(p, "mixed-content", msg));
        }
    }
}

/// `<meta>` values by lowercased `name` or `property`. Open Graph tags are often written with
/// `name=` instead of `property=`, and attribute values with any capitalisation, so both count.
struct Meta(HashMap<String, String>);

impl Meta {
    fn read(doc: &Html) -> Self {
        let mut map = HashMap::new();
        for m in doc.select(&sel("meta[content]")) {
            let el = m.value();
            let content = el.attr("content").unwrap_or("").trim();
            for key in [el.attr("name"), el.attr("property")].into_iter().flatten() {
                // The first tag wins, as it does for crawlers.
                map.entry(key.trim().to_ascii_lowercase())
                    .or_insert_with(|| content.to_owned());
            }
        }
        Self(map)
    }

    fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).map(String::as_str)
    }
}

/// Every `<link href rel>` with its rel attribute lowercased.
fn links_with_rel(doc: &Html) -> Vec<(String, ElementRef<'_>)> {
    doc.select(&sel("link[href][rel]"))
        .map(|l| (l.value().attr("rel").unwrap_or("").to_ascii_lowercase(), l))
        .collect()
}

fn sel(s: &str) -> Selector {
    Selector::parse(s).expect("static selector is valid")
}

fn first<'a>(doc: &'a Html, selector: &str) -> Option<ElementRef<'a>> {
    doc.select(&sel(selector)).next()
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
