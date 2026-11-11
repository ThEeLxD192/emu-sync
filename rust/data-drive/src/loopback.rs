use crate::OAuthError;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use reqwest::Url;
use std::{collections::HashMap, future::Future, time::Duration};
use subtle::ConstantTimeEq;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

/// Wait on an already-bound loopback listener; dropping the future closes its listener.
/// The caller supplies a browser launcher and should enforce an overall timeout.
pub async fn loopback_code<F, Fut>(
    listener: TcpListener,
    client_id: &str,
    authorization_endpoint: Url,
    redirect_uri: &str,
    open: F,
) -> Result<String, OAuthError>
where
    F: FnOnce(Url) -> Fut,
    Fut: Future<Output = Result<(), OAuthError>>,
{
    if !listener
        .local_addr()
        .map_err(|e| OAuthError::Provider(e.to_string()))?
        .ip()
        .is_loopback()
    {
        return Err(OAuthError::Provider(
            "OAuth listener must be loopback".into(),
        ));
    }
    let mut random = [0u8; 32];
    getrandom::getrandom(&mut random).map_err(|e| OAuthError::Provider(e.to_string()))?;
    let state = URL_SAFE_NO_PAD.encode(random);
    let mut url = authorization_endpoint;
    url.query_pairs_mut().extend_pairs([
        ("client_id", client_id),
        ("redirect_uri", redirect_uri),
        ("response_type", "code"),
        ("scope", "https://www.googleapis.com/auth/drive.file"),
        ("access_type", "offline"),
        ("prompt", "consent"),
        ("state", &state),
    ]);
    open(url).await?;
    loop {
        let (mut stream, _) = listener
            .accept()
            .await
            .map_err(|e| OAuthError::Provider(e.to_string()))?;
        let mut request = Vec::new();
        let read = tokio::time::timeout(Duration::from_secs(5), async {
            let mut buf = [0; 1024];
            let mut has_header_end = false;
            while request.len() < 8192 && !has_header_end {
                let max_read = (8192 - request.len()).min(buf.len());
                if max_read == 0 {
                    break;
                }
                let count = stream.read(&mut buf[..max_read]).await?;
                if count == 0 {
                    break;
                }
                let check_start = request.len().saturating_sub(3);
                request.extend_from_slice(&buf[..count]);
                if request[check_start..].windows(4).any(|w| w == b"\r\n\r\n") {
                    has_header_end = true;
                }
            }
            std::io::Result::Ok(has_header_end)
        })
        .await;
        let has_header_end = match read {
            Ok(Ok(found)) => found,
            _ => continue,
        };
        let first = request.split(|b| *b == b'\n').next().unwrap_or_default();
        let first = std::str::from_utf8(first)
            .unwrap_or_default()
            .trim_end_matches('\r');
        let mut parts = first.split_whitespace();
        let method = parts.next();
        let target = parts.next();
        let version = parts.next();
        let parsed =
            target.and_then(|target| Url::parse("http://localhost/").ok()?.join(target).ok());
        let params: HashMap<String, String> = parsed
            .map(|url| url.query_pairs().into_owned().collect())
            .unwrap_or_default();
        let valid = has_header_end
            && version.is_some_and(|v| v.starts_with("HTTP/1."))
            && method == Some("GET")
            && params
                .get("state")
                .is_some_and(|s| bool::from(s.as_bytes().ct_eq(state.as_bytes())));
        let code = params.get("code").filter(|s| !s.trim().is_empty());
        let error = params.get("error").filter(|s| !s.trim().is_empty());
        let terminal = valid && (code.is_some() || error.is_some());
        let success = terminal && error.is_none() && code.is_some();
        let (status, body) = if success {
            ("200 OK", include_str!("oauth_success.html"))
        } else {
            ("400 Bad Request", include_str!("oauth_error.html"))
        };
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = tokio::time::timeout(Duration::from_secs(5), async {
            stream.write_all(response.as_bytes()).await?;
            stream.shutdown().await
        })
        .await;
        if terminal {
            if let Some(error) = error {
                return Err(OAuthError::Provider(
                    params
                        .get("error_description")
                        .filter(|s| !s.trim().is_empty())
                        .unwrap_or(error)
                        .clone(),
                ));
            }
            return Ok(code.cloned().unwrap_or_default());
        }
    }
}
