//! `shipcheck`: audit a website, or a local build folder, before launch.

use clap::{Parser, ValueEnum};
use shipcheck::crawl::{self, Config};
use shipcheck::model::Severity;
use shipcheck::{report, serve};
use std::io::IsTerminal;
use std::path::PathBuf;
use std::time::Duration;
use url::Url;

#[derive(Clone, Copy, Debug, ValueEnum)]
enum FailOn {
    /// Exit 1 only when there are errors (default).
    Error,
    /// Exit 1 on errors or warnings.
    Warning,
    /// Always exit 0, just report.
    Never,
}

#[derive(Parser, Debug)]
#[command(
    name = "shipcheck",
    version,
    about = "Audit a website before launch. Exit code 1 means problems were found."
)]
struct Args {
    /// A URL (https://example.com) or a local folder of built HTML (./out, ./dist).
    target: String,
    /// Stop crawling after this many pages (at least 1).
    #[arg(long, default_value_t = crawl::DEFAULT_MAX_PAGES)]
    max_pages: usize,
    /// How many links to check at once (1 to 64).
    #[arg(long, default_value_t = crawl::DEFAULT_CONCURRENCY)]
    concurrency: usize,
    /// Seconds to wait for each request (1 to 120).
    #[arg(long, default_value_t = crawl::DEFAULT_TIMEOUT_SECS)]
    timeout: u64,
    /// Only check links within the site.
    #[arg(long)]
    no_external: bool,
    /// Warn about images larger than this many kilobytes.
    #[arg(long, default_value_t = crawl::DEFAULT_MAX_IMAGE_KB)]
    max_image_kb: u64,
    /// Print the report as JSON instead of text.
    #[arg(long)]
    json: bool,
    /// What makes the exit code 1.
    #[arg(long, value_enum, default_value_t = FailOn::Error)]
    fail_on: FailOn,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    let start = match resolve_target(&args.target).await {
        Ok(u) => u,
        Err(e) => {
            eprintln!("shipcheck: {e}");
            std::process::exit(2);
        }
    };
    let cfg = Config {
        max_pages: args.max_pages.max(1),
        concurrency: args.concurrency.clamp(1, 64),
        timeout: Duration::from_secs(args.timeout.clamp(1, 120)),
        external: !args.no_external,
        max_image_bytes: args.max_image_kb.saturating_mul(1024),
    };
    if !args.json {
        eprintln!("shipcheck: checking {start}");
    }
    let report = match crawl::run(start, &cfg).await {
        Ok(r) => r,
        Err(e) => {
            eprintln!("shipcheck: {e}");
            std::process::exit(2);
        }
    };
    if args.json {
        println!("{}", report::json(&report));
    } else {
        print!("{}", report::text(&report, std::io::stdout().is_terminal()));
    }
    let failed = match args.fail_on {
        FailOn::Error => report.count(Severity::Error) > 0,
        FailOn::Warning => !report.issues.is_empty(),
        FailOn::Never => false,
    };
    std::process::exit(i32::from(failed));
}

/// A folder is served locally; anything else must parse as an http(s) URL.
async fn resolve_target(target: &str) -> Result<Url, String> {
    let path = PathBuf::from(target);
    if path.is_dir() {
        let root = path.canonicalize().map_err(|e| format!("{target}: {e}"))?;
        let base = serve::start(root)
            .await
            .map_err(|e| format!("could not start local server: {e}"))?;
        return Url::parse(&base).map_err(|e| e.to_string());
    }
    let url = Url::parse(target)
        .map_err(|_| format!("\"{target}\" is not a folder or a URL like https://example.com"))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("only http and https URLs are supported".into());
    }
    Ok(url)
}
