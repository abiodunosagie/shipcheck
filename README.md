# shipcheck

[![CI](https://github.com/abiodunosagie/shipcheck/actions/workflows/ci.yml/badge.svg)](https://github.com/abiodunosagie/shipcheck/actions/workflows/ci.yml)

A command-line tool, written in Rust, that audits a website before launch. Point it at a live URL or at a local build folder. It crawls every page on the site, checks every link and resource once, and reports what would embarrass you after launch: broken links, blank link previews, images without descriptions, oversized images, and missing basics such as the viewport tag, robots.txt and sitemap.xml.

The exit code makes it a CI gate. A pull request that breaks a link or drops the link-preview image fails the build.

## Quick start

```bash
git clone https://github.com/abiodunosagie/shipcheck.git
cd shipcheck
cargo install --path .

shipcheck https://example.com        # a live site
shipcheck ./out                      # a local build folder (Next.js out/, Vite dist/, Hugo public/)
```

A local folder is served by a small built-in server on `127.0.0.1`, so you can audit a static build before deploying it.

## Example

Run against the broken test site in `tests/fixtures/bad`:

```
$ shipcheck tests/fixtures/bad --no-external
shipcheck: checking http://127.0.0.1:8080/

http://127.0.0.1:8080/
  error   broken-link      link returns HTTP 404: http://127.0.0.1:8080/nowhere
  error   broken-resource  image returns HTTP 404: http://127.0.0.1:8080/missing.png
  error   img-alt          image without alt text: /missing.png (use alt="" if decorative)
  error   og-image         og:image must be an absolute URL, got "/og.png"; most platforms ignore relative ones
  error   viewport         missing viewport meta tag, the page will not scale on phones
  warning canonical        missing canonical link
  warning description      missing meta description
  warning favicon          no favicon declared and no /favicon.ico
  warning h1               2 <h1> elements; use one per page
  warning lang             missing lang attribute on <html>, screen readers guess the language
  warning og-description   missing og:description
  warning og-title         missing og:title, link previews fall back to the page title
  warning robots           missing robots.txt
  warning sitemap          missing sitemap.xml, search engines discover pages more slowly
  warning twitter-card     missing twitter:card, X shows a small preview or none

http://127.0.0.1:8080/blog/
  error   broken-link      link returns HTTP 404: http://127.0.0.1:8080/gone.html
  error   og-image         missing og:image, shared links show a blank preview
  warning canonical        missing canonical link
  warning description      missing meta description
  warning og-description   missing og:description
  warning og-title         missing og:title, link previews fall back to the page title
  warning twitter-card     missing twitter:card, X shows a small preview or none

2 pages, 5 links and resources checked: 7 errors, 15 warnings
```

## What it checks

Errors fail the run by default. Warnings are reported but do not fail it unless you pass `--fail-on warning`.

| Check | Severity | What it means |
|---|---|---|
| `broken-link` | error | A link returns 4xx or 5xx, or cannot be reached. |
| `broken-resource` | error | An image, script, stylesheet, icon or og:image fails to load. |
| `og-image` | error | Missing, relative, or not served as an image. Shared links then show a blank preview. |
| `img-alt` | error | An image has no `alt` attribute. `alt=""` is accepted for decorative images. |
| `viewport` | error | No viewport meta tag, so the page does not scale on phones. |
| `mixed-content` | error | An `https` page loads an `http` image, script or stylesheet, which browsers block or flag. |
| `title` | error / warning | Missing title (error), or longer than 60 characters (warning). |
| `image-size` | warning | An image is larger than `--max-image-kb` (300 KB by default). |
| `description` | warning | Missing meta description, or outside 50 to 160 characters. |
| `h1` | warning | No `<h1>`, or more than one. |
| `lang` | warning | No `lang` attribute on `<html>`. |
| `canonical` | warning | No canonical link. |
| `og-title`, `og-description`, `twitter-card` | warning | Missing link-preview tags. |
| `robots`, `sitemap`, `favicon` | warning | Missing site-wide files. |
| `crawl-limit` | warning | The crawl stopped at `--max-pages` before reaching every page. |

Links to other sites are checked too. Many large sites answer automated requests with 401, 403 or 429 even when the page is fine, so for external links those answers, and timeouts, are warnings rather than errors.

## Options

```
shipcheck [OPTIONS] <TARGET>

  <TARGET>                 A URL (https://example.com) or a local folder of built HTML
  --max-pages <N>          Stop crawling after this many pages [default: 200]
  --concurrency <N>        How many links to check at once [default: 8]
  --timeout <SECONDS>      Seconds to wait for each request [default: 15]
  --no-external            Only check links within the site
  --max-image-kb <KB>      Warn about images larger than this [default: 300]
  --json                   Print the report as JSON instead of text
  --fail-on <LEVEL>        error (default), warning, or never
```

Exit codes: `0` when the run passes, `1` when it finds problems at the `--fail-on` level, `2` for a usage error such as a target that is neither a folder nor a URL.

## Use it in CI

Audit a static build before it deploys, for example in GitHub Actions:

```yaml
- name: Build the site
  run: npm ci && npm run build          # writes ./out

- name: Install shipcheck
  run: cargo install --git https://github.com/abiodunosagie/shipcheck

- name: Audit the build
  run: shipcheck ./out --no-external
```

`--json` gives a machine-readable report (site, pages checked, links checked, and every issue with its page, check, severity and message) for dashboards or bots.

## How it works

1. **Crawl.** Starting from the target, it fetches same-site HTML pages breadth first and analyses each one: the head tags, the headings, the images, and every link and resource it references. `/about` and `/about/` count as one page, and the crawl stops at `--max-pages`.
2. **Check.** Every unique link and resource across the whole site is requested exactly once, concurrently (`futures::buffer_unordered`), and a failure is reported on every page that references it. It sends HEAD first and falls back to GET when a server rejects HEAD.
3. **Site-wide files.** robots.txt, sitemap.xml, and a favicon when no page declares one.

Some details that matter on real sites:

- **Image sizes are measured, not guessed.** CDNs often answer HEAD over HTTP/2 without a `Content-Length`. When that happens for an image, shipcheck makes a GET and counts the bytes, so an oversized image is never silently skipped.
- **One report per failure.** A broken page is reported as a broken link on each page that links to it, not a second time as a page.
- **Links that cannot be fetched are skipped,** such as `mailto:`, `tel:`, `javascript:`, `data:` and in-page `#fragments`.
- **The local server stays inside its folder.** Paths are percent-decoded before they are checked, so `..`, `%2e%2e` and encoded backslashes cannot climb out of the build folder. It answers GET and HEAD only, binds to `127.0.0.1` on a free port, and resolves `/about` to `about.html` or `about/index.html` the way static hosts do.
- **TLS through rustls,** with gzip and brotli decoding and HTTP/2.

## Tests

```bash
cargo test
```

CI runs formatting, Clippy with warnings denied, and the full test suite on Linux, macOS and Windows. The tests need no network: each one serves a fixture site from `127.0.0.1`.

- `tests/page_checks.rs` checks the page analysis: a complete page has no issues, an empty page reports every missing basic, relative og:image is an error, `alt=""` is accepted, mixed content applies to resources but not links, and links are resolved, deduplicated and filtered.
- `tests/site.rs` crawls whole sites end to end:
  - A complete site passes with zero issues.
  - A broken site reports its broken links, broken images, bad tags and missing files, each exactly once.
  - A start page that fails is reported.
  - The crawl limit is respected and reported.
  - An image served without a `Content-Length` is still measured.
  - The local server refuses every path that tries to leave its folder.

## Project layout

```
src/
  main.rs     command-line options, local folder or URL, exit codes
  crawl.rs    crawl, concurrent link checking, site-wide files
  page.rs     per-page analysis: head tags, headings, images, links and resources
  serve.rs    local static file server for build folders
  report.rs   text and JSON output
  model.rs    issues, severities and the report
tests/
  page_checks.rs   page analysis
  site.rs          end-to-end crawls
  fixtures/        a complete site and a broken site
```

## License

MIT. See [LICENSE](LICENSE).

## Author

Abiodun Osagie, mobile, front-end and full-stack developer.
[Portfolio](https://smithportfolio.smitho.workers.dev) · [GitHub](https://github.com/abiodunosagie) · [LinkedIn](https://linkedin.com/in/osagieabiodun)
