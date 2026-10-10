//! A minimal static file server for auditing a local build folder (for example Next.js `out/`)
//! without deploying it. GET and HEAD only, bound to 127.0.0.1 on a free port.

use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Semaphore;

/// Longest request header accepted; more than enough for a GET with a long URL.
const MAX_HEADER_BYTES: usize = 16 * 1024;
/// A client that has not finished sending its request header by then is dropped.
const READ_TIMEOUT: Duration = Duration::from_secs(10);
/// Connections handled at once; the crawler uses far fewer.
const MAX_CONNECTIONS: usize = 64;

/// The folder being served and the only `Host` header accepted, which stops a web page in the
/// browser from reaching this server through DNS rebinding.
struct Site {
    root: PathBuf,
    host: String,
}

/// Starts serving `root` in the background and returns the base URL, for example `http://127.0.0.1:53211/`.
pub async fn start(root: PathBuf) -> std::io::Result<String> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let site = Arc::new(Site {
        root,
        host: addr.to_string(),
    });
    let slots = Arc::new(Semaphore::new(MAX_CONNECTIONS));
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let Ok(permit) = slots.clone().acquire_owned().await else {
                break;
            };
            let site = site.clone();
            tokio::spawn(async move {
                let _ = handle(stream, &site).await;
                drop(permit);
            });
        }
    });
    Ok(format!("http://{addr}/"))
}

async fn handle(mut stream: TcpStream, site: &Site) -> std::io::Result<()> {
    let head = match tokio::time::timeout(READ_TIMEOUT, read_head(&mut stream)).await {
        Ok(result) => result?,
        Err(_) => return Ok(()),
    };
    let request = String::from_utf8_lossy(&head);
    let mut lines = request.lines();
    let mut parts = lines.next().unwrap_or("").split_whitespace();
    let method = parts.next().unwrap_or("");
    let target = parts.next().unwrap_or("/");
    let host_ok = lines
        .filter_map(|l| l.split_once(':'))
        .any(|(name, value)| name.eq_ignore_ascii_case("host") && value.trim() == site.host);
    if !host_ok {
        return respond(&mut stream, 400, "text/plain", b"bad host", false).await;
    }
    if method != "GET" && method != "HEAD" {
        return respond(&mut stream, 405, "text/plain", b"method not allowed", false).await;
    }
    let head_only = method == "HEAD";
    let path = target.split(['?', '#']).next().unwrap_or("/");
    if let Some(file) = locate(&site.root, path) {
        let body = tokio::fs::read(&file).await?;
        return respond(&mut stream, 200, content_type(&file), &body, head_only).await;
    }
    let body = match locate(&site.root, "/404.html") {
        Some(page) => tokio::fs::read(page).await?,
        None => b"not found".to_vec(),
    };
    respond(
        &mut stream,
        404,
        "text/html; charset=utf-8",
        &body,
        head_only,
    )
    .await
}

/// Maps a URL path to a file under `root`, the way static hosts do: `/about` tries `about`,
/// `about.html` and `about/index.html`. Rejects any path that would escape `root`, including
/// through a symlink, and hidden files.
pub fn locate(root: &Path, url_path: &str) -> Option<PathBuf> {
    let decoded = percent_decode(url_path);
    let rel = Path::new(decoded.trim_start_matches('/'));
    // Only plain names: no `..`, roots or drive prefixes, and no hidden files such as `.env`
    // (`.well-known` is the one dot folder a site legitimately serves).
    let plain = |c: Component| match c {
        Component::Normal(name) => {
            let name = name.to_string_lossy();
            !name.starts_with('.') || name == ".well-known"
        }
        _ => false,
    };
    if !rel.components().all(plain) {
        return None;
    }
    let base = root.join(rel);
    let mut candidates = vec![base.clone()];
    // `about.html` for `/about`. Only for a named path: for `/` the "name" would be the root
    // folder's own, and `<root>.html` sits outside the root.
    if let Some(name) = rel.file_name() {
        let mut html = name.to_os_string();
        html.push(".html");
        candidates.push(base.with_file_name(html));
    }
    candidates.push(base.join("index.html"));
    // A symlink inside the folder can still point outside it, so compare the real paths.
    let real_root = root.canonicalize().ok()?;
    candidates
        .into_iter()
        .filter(|c| c.is_file())
        .find_map(|c| c.canonicalize().ok().filter(|r| r.starts_with(&real_root)))
}

/// Reads up to the blank line that ends an HTTP request header, however many reads it takes.
async fn read_head(stream: &mut TcpStream) -> std::io::Result<Vec<u8>> {
    let mut head = Vec::new();
    let mut buf = [0u8; 2048];
    while !head.windows(4).any(|w| w == b"\r\n\r\n") && head.len() < MAX_HEADER_BYTES {
        let n = stream.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        head.extend_from_slice(&buf[..n]);
    }
    Ok(head)
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        // Work on bytes, not `&str` slices: a multibyte character right after `%` would
        // otherwise split a char boundary and panic.
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(hi), Some(lo)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                out.push(hi << 4 | lo);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex(b: u8) -> Option<u8> {
    (b as char).to_digit(16).map(|d| d as u8)
}

fn content_type(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css",
        "js" | "mjs" => "text/javascript",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "ico" => "image/x-icon",
        "avif" => "image/avif",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "txt" => "text/plain; charset=utf-8",
        "xml" => "application/xml",
        "pdf" => "application/pdf",
        "wasm" => "application/wasm",
        _ => "application/octet-stream",
    }
}

async fn respond(
    stream: &mut TcpStream,
    status: u16,
    ctype: &str,
    body: &[u8],
    head_only: bool,
) -> std::io::Result<()> {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "Unknown",
    };
    let header = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(header.as_bytes()).await?;
    if !head_only {
        stream.write_all(body).await?;
    }
    stream.shutdown().await
}
