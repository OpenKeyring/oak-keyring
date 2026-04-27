//! Local HTTP callback server for OAuth2 authorization code.
//!
//! Listens on 127.0.0.1:8879 and waits for the OAuth2 callback.

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use url::Url;

use super::error::OAuth2Error;

const CALLBACK_PORT: u16 = 8879;

/// Extract authorization code from callback URL.
pub fn extract_code_from_url(url: &str) -> Option<String> {
    let url = Url::parse(url).ok()?;
    for (key, value) in url.query_pairs() {
        if key == "code" {
            return Some(value.into_owned());
        }
    }
    None
}

const SUCCESS_HTML: &str = r#"<!DOCTYPE html>
<html>
<head><meta charset="utf-8"><title>授权成功</title>
<style>
body{font-family:-apple-system,BlinkMacSystemFont,"Segoe UI",Roboto,sans-serif;text-align:center;padding:50px;background:#f9f9f9}
.card{background:#fff;padding:40px;border-radius:12px;box-shadow:0 4px 15px rgba(0,0,0,.1);display:inline-block;max-width:400px}
h1{color:#28a745;margin-bottom:20px}p{color:#555;line-height:1.6}
.footer{margin-top:30px;font-size:13px;color:#999}
</style></head>
<body><div class="card">
<h1>授权成功！</h1>
<p>授权码已捕获。请返回终端继续操作。</p>
<p>基于安全策略，部分浏览器可能无法自动关闭此页面。如果 3 秒后页面未关闭，请手动关闭。</p>
<div class="footer">OpenKeyring Auth Service</div>
</div>
<script>setTimeout(function(){window.close();},3000);</script>
</body></html>"#;

const ERROR_HTML: &str = r#"<!DOCTYPE html>
<html>
<head><meta charset="utf-8"><title>授权失败</title>
<style>
body{font-family:-apple-system,BlinkMacSystemFont,"Segoe UI",Roboto,sans-serif;text-align:center;padding:50px;background:#f9f9f9}
.card{background:#fff;padding:40px;border-radius:12px;box-shadow:0 4px 15px rgba(0,0,0,.1);display:inline-block;max-width:400px}
h1{color:#dc3545;margin-bottom:20px}p{color:#555}
</style></head>
<body><div class="card">
<h1>授权失败</h1>
<p>未收到有效的授权码。请返回终端重试。</p>
</div></body></html>"#;

/// Start the callback server and wait for an authorization code.
///
/// Returns the code string, or an error.
/// Respects the cancellation token for user-initiated cancel.
pub async fn wait_for_callback(cancel: CancellationToken) -> Result<String, OAuth2Error> {
    let listener = TcpListener::bind(format!("127.0.0.1:{}", CALLBACK_PORT))
        .await
        .map_err(|_| OAuth2Error::PortInUse {
            port: CALLBACK_PORT,
        })?;

    tokio::select! {
        result = accept_callback_request(&listener) => result,
        _ = cancel.cancelled() => Err(OAuth2Error::Cancelled),
    }
}

async fn accept_callback_request(listener: &TcpListener) -> Result<String, OAuth2Error> {
    let (mut socket, _) = listener
        .accept()
        .await
        .map_err(|e| OAuth2Error::TokenExchange {
            message: format!("failed to accept connection: {}", e),
        })?;

    let mut buffer = [0u8; 2048];
    let n = socket
        .read(&mut buffer)
        .await
        .map_err(|e| OAuth2Error::TokenExchange {
            message: format!("failed to read request: {}", e),
        })?;

    let request = String::from_utf8_lossy(&buffer[..n]);
    let first_line = request.lines().next().unwrap_or("");
    let path = first_line.split_whitespace().nth(1).unwrap_or("");

    let full_url = format!("http://localhost:{}{}", CALLBACK_PORT, path);
    let code = extract_code_from_url(&full_url);

    if let Some(ref code) = code {
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            SUCCESS_HTML.len(),
            SUCCESS_HTML
        );
        let _ = socket.write_all(response.as_bytes()).await;
        Ok(code.clone())
    } else {
        let response = format!(
            "HTTP/1.1 400 Bad Request\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            ERROR_HTML.len(),
            ERROR_HTML
        );
        let _ = socket.write_all(response.as_bytes()).await;
        Err(OAuth2Error::MissingCode)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_util::sync::CancellationToken;

    #[tokio::test]
    async fn callback_server_extracts_code_from_url() {
        let url = "http://localhost:8879/?code=4/0AX4XfWgABC123&state=xyz";
        let code = extract_code_from_url(url);
        assert_eq!(code, Some("4/0AX4XfWgABC123".to_string()));
    }

    #[tokio::test]
    async fn callback_server_returns_none_for_missing_code() {
        let url = "http://localhost:8879/?error=access_denied";
        let code = extract_code_from_url(url);
        assert!(code.is_none());
    }

    #[tokio::test]
    async fn callback_server_respects_cancel_token() {
        let cancel = CancellationToken::new();
        cancel.cancel();

        let result = wait_for_callback(cancel).await;
        assert!(result.is_err());
    }
}
