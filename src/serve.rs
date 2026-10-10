//! A minimal static file server for auditing a local build folder (for example Next.js `out/`)
//! without deploying it. GET and HEAD only, bound to 127.0.0.1 on a free port.

use std::path::{Component, Path, PathBuf};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// Starts serving `root` in the background and returns the base URL, for example `http://127.0.0.1:53211/`.
pub async fn start(root: PathBuf) -> std::io::Result<String> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let root = root.clone();
            tokio::spawn(async move {
                let _ = handle(stream, &root).await;
            });
        }
    });
    Ok(format!("http://{addr}/"))
}

async fn handle(mut stream: TcpStream, root: &Path) -> std::io::Result<()> {
    let mut buf = vec![0u8; 8192];
    let n = stream.read(&mut buf).await?;
    let request = String::from_utf8_lossy(&buf[..n]);
    let mut parts = request.lines().next().unwrap_or("").split_whitespace();
    let method = parts.next().unwrap_or("");
    let target = parts.next().unwrap_or("/");
    if method != "GET" && method != "HEAD" {
        return respond(&mut stream, 405, "text/plain", b"method not allowed", false).await;
    }
    let path = target.split(['?', '#']).next().unwrap_or("/");
    match locate(root, path) {
        Some(file) => {
            let body = tokio::fs::read(&file).await?;
            respond(
                &mut stream,
                200,
                content_type(&file),
                &body,
                method == "HEAD",
            )
            .await
        }
        None => {
            let body = tokio::fs::read(root.join("404.html"))
                .await
                .unwrap_or_else(|_| b"not found".to_vec());
            respond(
                &mut stream,
                404,
                "text/html; charset=utf-8",
                &body,
                method == "HEAD",
            )
            .await
        }
    }
}

/// Maps a URL path to a file under `root`, the way static hosts do: `/about` tries `about`,
/// `about.html` and `about/index.html`. Rejects any path that would escape `root`.
pub fn locate(root: &Path, url_path: &str) -> Option<PathBuf> {
    let decoded = percent_decode(url_path);
    let rel = Path::new(decoded.trim_start_matches('/'));
    if rel.components().any(|c| !matches!(c, Component::Normal(_))) {
        return None;
    }
    let base = root.join(rel);
    let mut candidates = vec![base.clone(), base.join("index.html")];
    if let Some(name) = base.file_name().map(|n| n.to_string_lossy().into_owned()) {
        candidates.insert(1, base.with_file_name(format!("{name}.html")));
    }
    candidates.into_iter().find(|c| c.is_file())
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
        404 => "Not Found",
        _ => "Method Not Allowed",
    };
    let header = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(header.as_bytes()).await?;
    if !head_only {
        stream.write_all(body).await?;
    }
    stream.shutdown().await
}
