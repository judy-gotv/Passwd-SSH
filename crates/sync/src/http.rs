//! Minimal HTTP/1.1 client (plain + TLS) with zero extra HTTP dependencies.
//!
//! Used for sync-server communication and the auto-update version check.
//! Supports `Content-Length` and `chunked` responses.
//!
//! Author: Passwd Word

use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::error::SyncError;

/// An HTTP response.
#[derive(Debug)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl HttpResponse {
    /// Body as UTF-8 string (lossy).
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    /// Header value (case-insensitive name), if present.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

#[derive(Debug, Clone)]
struct ParsedUrl {
    https: bool,
    host: String,
    port: u16,
    path: String,
}

fn parse_url(url: &str) -> Result<ParsedUrl, SyncError> {
    let (scheme, rest) = url
        .split_once("://")
        .ok_or_else(|| SyncError::Remote(format!("bad url (missing scheme): {url}")))?;
    let https = match scheme {
        "https" => true,
        "http" => false,
        _ => return Err(SyncError::Remote(format!("unsupported scheme: {scheme}"))),
    };
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], rest[i..].to_string()),
        None => (rest, "/".to_string()),
    };
    if authority.is_empty() {
        return Err(SyncError::Remote(format!("bad url (empty host): {url}")));
    }
    let (host, port) = match authority.rsplit_once(':') {
        Some((h, p)) => (
            h.to_string(),
            p.parse::<u16>()
                .map_err(|_| SyncError::Remote(format!("bad port in url: {url}")))?,
        ),
        None => (authority.to_string(), if https { 443 } else { 80 }),
    };
    Ok(ParsedUrl {
        https,
        host,
        port,
        path,
    })
}

fn tls_connector() -> Result<tokio_rustls::TlsConnector, SyncError> {
    let mut roots = rustls::RootCertStore::empty();
    // Prefer the OS trust store; fall back to the bundled Mozilla roots.
    let native = rustls_native_certs::load_native_certs().unwrap_or_default();
    let mut loaded = false;
    for cert in native {
        if roots.add(cert).is_ok() {
            loaded = true;
        }
    }
    if !loaded {
        roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    }
    let config = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    Ok(tokio_rustls::TlsConnector::from(Arc::new(config)))
}

/// Perform one HTTP request.
///
/// `method` is e.g. `"GET"` / `"POST"`. `body` sets `Content-Length`.
/// Redirects are NOT followed (the caller decides).
pub async fn http_request(
    method: &str,
    url: &str,
    body: Option<&[u8]>,
    extra_headers: &[(&str, &str)],
) -> Result<HttpResponse, SyncError> {
    let u = parse_url(url)?;
    let tcp = tokio::time::timeout(
        Duration::from_secs(15),
        tokio::net::TcpStream::connect((u.host.as_str(), u.port)),
    )
    .await
    .map_err(|_| SyncError::Remote(format!("connect timed out: {}:{}", u.host, u.port)))?
    .map_err(|e| SyncError::Remote(format!("connect {}:{}: {e}", u.host, u.port)))?;
    tcp.set_nodelay(true).ok();

    let mut stream: Box<dyn AsyncRead + AsyncWrite + Unpin + Send> = if u.https {
        let name = rustls::pki_types::ServerName::try_from(u.host.as_str())
            .map_err(|_| SyncError::Remote(format!("invalid dns name: {}", u.host)))?
            .to_owned();
        let tls = tls_connector()?
            .connect(name, tcp)
            .await
            .map_err(|e| SyncError::Remote(format!("tls handshake failed: {e}")))?;
        Box::new(tls)
    } else {
        Box::new(tcp)
    };

    let body_len = body.map(|b| b.len()).unwrap_or(0);
    let mut req = format!(
        "{method} {} HTTP/1.1\r\nHost: {}\r\nUser-Agent: OpsDeck/0.1\r\nConnection: close\r\nAccept-Encoding: identity\r\n",
        u.path, u.host
    );
    for (k, v) in extra_headers {
        req.push_str(&format!("{k}: {v}\r\n"));
    }
    if body_len > 0 {
        req.push_str(&format!(
            "Content-Length: {body_len}\r\nContent-Type: application/json\r\n"
        ));
    }
    req.push_str("\r\n");
    stream
        .write_all(req.as_bytes())
        .await
        .map_err(|e| SyncError::Remote(format!("write request: {e}")))?;
    if let Some(b) = body {
        stream
            .write_all(b)
            .await
            .map_err(|e| SyncError::Remote(format!("write body: {e}")))?;
    }
    stream
        .flush()
        .await
        .map_err(|e| SyncError::Remote(format!("flush: {e}")))?;

    read_response(&mut stream).await
}

async fn read_response(
    stream: &mut (dyn AsyncRead + AsyncWrite + Unpin + Send),
) -> Result<HttpResponse, SyncError> {
    let mut buf = Vec::with_capacity(8192);
    let mut tmp = [0u8; 8192];
    let header_end = loop {
        let n = tokio::time::timeout(Duration::from_secs(20), stream.read(&mut tmp))
            .await
            .map_err(|_| SyncError::Remote("read response timed out".into()))?
            .map_err(|e| SyncError::Remote(format!("read response: {e}")))?;
        if n == 0 {
            return Err(SyncError::Remote("connection closed before headers".into()));
        }
        buf.extend_from_slice(&tmp[..n]);
        if let Some(pos) = find_crlf2(&buf) {
            break pos;
        }
        if buf.len() > 128 * 1024 {
            return Err(SyncError::Remote("response headers too large".into()));
        }
    };

    let header_text = String::from_utf8_lossy(&buf[..header_end]).into_owned();
    let mut lines = header_text.lines();
    let status_line = lines.next().unwrap_or("");
    let status: u16 = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| SyncError::Remote(format!("bad status line: {status_line}")))?;
    let mut headers = Vec::new();
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            headers.push((k.trim().to_string(), v.trim().to_string()));
        }
    }

    let is_chunked = headers.iter().any(|(k, v)| {
        k.eq_ignore_ascii_case("transfer-encoding") && v.eq_ignore_ascii_case("chunked")
    });
    let content_length: Option<usize> = headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, v)| v.parse().ok());

    let mut body = buf[header_end..].to_vec();
    if is_chunked {
        body = read_chunked(stream, body).await?;
    } else if let Some(len) = content_length {
        while body.len() < len {
            let n = stream
                .read(&mut tmp)
                .await
                .map_err(|e| SyncError::Remote(format!("read body: {e}")))?;
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&tmp[..n]);
            body.extend_from_slice(&tmp[..n]);
        }
        body.truncate(len);
    } else {
        // No length framing: read until close.
        loop {
            let n = stream
                .read(&mut tmp)
                .await
                .map_err(|e| SyncError::Remote(format!("read body: {e}")))?;
            if n == 0 {
                break;
            }
            body.extend_from_slice(&tmp[..n]);
            if body.len() > 64 * 1024 * 1024 {
                return Err(SyncError::Remote("response body too large".into()));
            }
        }
    }

    Ok(HttpResponse {
        status,
        headers,
        body,
    })
}

async fn read_chunked(
    stream: &mut (dyn AsyncRead + AsyncWrite + Unpin + Send),
    mut buf: Vec<u8>,
) -> Result<Vec<u8>, SyncError> {
    let mut out = Vec::new();
    let mut tmp = [0u8; 8192];
    loop {
        // Read one chunk-size line.
        let line_end = loop {
            if let Some(pos) = find_crlf(&buf) {
                break pos;
            }
            let n = stream
                .read(&mut tmp)
                .await
                .map_err(|e| SyncError::Remote(format!("read chunk: {e}")))?;
            if n == 0 {
                return Err(SyncError::Remote("eof in chunked encoding".into()));
            }
            buf.extend_from_slice(&tmp[..n]);
        };
        let size_line = String::from_utf8_lossy(&buf[..line_end]).into_owned();
        let size = usize::from_str_radix(size_line.split(';').next().unwrap_or("").trim(), 16)
            .map_err(|_| SyncError::Remote("bad chunk size".into()))?;
        buf.drain(..line_end + 2);
        if size == 0 {
            break;
        }
        while buf.len() < size + 2 {
            let n = stream
                .read(&mut tmp)
                .await
                .map_err(|e| SyncError::Remote(format!("read chunk: {e}")))?;
            if n == 0 {
                return Err(SyncError::Remote("eof in chunked body".into()));
            }
            buf.extend_from_slice(&tmp[..n]);
        }
        out.extend_from_slice(&buf[..size]);
        buf.drain(..size + 2); // data + trailing CRLF
    }
    Ok(out)
}

fn find_crlf2(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n").map(|p| p + 4)
}

fn find_crlf(buf: &[u8]) -> Option<usize> {
    buf.windows(2).position(|w| w == b"\r\n")
}

/// Convenience GET returning the body text; errors on non-2xx.
pub async fn http_get_text(url: &str) -> Result<String, SyncError> {
    let resp = http_request("GET", url, None, &[]).await?;
    if !(200..300).contains(&resp.status) {
        return Err(SyncError::Remote(format!(
            "GET {url} -> HTTP {}",
            resp.status
        )));
    }
    Ok(resp.text())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_parsing() {
        let u = parse_url("https://example.com:8443/a/b?x=1").unwrap();
        assert!(u.https && u.host == "example.com" && u.port == 8443 && u.path == "/a/b?x=1");
        let u = parse_url("http://example.com").unwrap();
        assert!(!u.https && u.port == 80 && u.path == "/");
        assert!(parse_url("ftp://example.com").is_err());
        assert!(parse_url("no-scheme").is_err());
    }

    #[test]
    fn crlf_finders() {
        assert_eq!(find_crlf2(b"a\r\n\r\nb"), Some(5));
        assert_eq!(find_crlf(b"a\r\nb"), Some(1));
        assert_eq!(find_crlf2(b"abc"), None);
    }

    #[tokio::test]
    async fn local_http_roundtrip() {
        // Tiny in-test HTTP server speaking Content-Length.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 1024];
            let _ = sock.read(&mut buf).await;
            let body = b"{\"ok\":true}";
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            sock.write_all(resp.as_bytes()).await.unwrap();
            sock.write_all(body).await.unwrap();
        });
        let url = format!("http://{addr}/ping");
        let text = http_get_text(&url).await.unwrap();
        assert_eq!(text, "{\"ok\":true}");
    }

    #[tokio::test]
    async fn local_chunked_roundtrip() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 1024];
            let _ = sock.read(&mut buf).await;
            sock.write_all(
                b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n",
            )
            .await
            .unwrap();
            sock.write_all(b"5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n")
                .await
                .unwrap();
        });
        let url = format!("http://{addr}/chunked");
        let text = http_get_text(&url).await.unwrap();
        assert_eq!(text, "hello world");
    }
}
