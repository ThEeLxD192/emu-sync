use emusync_data_drive::loopback_code;
use reqwest::{Client, Url};
use std::{sync::mpsc, time::Duration};
use tokio::net::TcpListener;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn loopback_ignores_wrong_state_and_accepts_matching_code() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (tx, rx) = mpsc::channel();
    let task = tokio::spawn(async move {
        loopback_code(
            listener,
            "synthetic client",
            Url::parse("https://example.invalid/authorize").unwrap(),
            "http://localhost:8089",
            |url| async move {
                tx.send(url).unwrap();
                Ok(())
            },
        )
        .await
    });
    let url = rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let query: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
    assert_eq!(
        query.get("scope").unwrap(),
        "https://www.googleapis.com/auth/drive.file"
    );
    assert_eq!(query.get("redirect_uri").unwrap(), "http://localhost:8089");
    assert_eq!(query.get("access_type").unwrap(), "offline");
    let client = Client::new();
    assert_eq!(
        client
            .get(format!("http://{address}/?state=wrong&code=bad"))
            .send()
            .await
            .unwrap()
            .status(),
        400
    );
    assert!(!task.is_finished());
    assert_eq!(
        client
            .get(format!(
                "http://{address}/?state={}&code=synthetic",
                query["state"]
            ))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    assert_eq!(task.await.unwrap().unwrap(), "synthetic");
    assert!(TcpListener::bind(address).await.is_ok());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancellation_releases_listener() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (tx, rx) = mpsc::channel();
    let task = tokio::spawn(async move {
        loopback_code(
            listener,
            "client",
            Url::parse("https://example.invalid/").unwrap(),
            "http://localhost:8089",
            |_| async move {
                tx.send(()).unwrap();
                Ok(())
            },
        )
        .await
    });
    rx.recv_timeout(Duration::from_secs(5)).unwrap();
    task.abort();
    let _ = task.await;
    assert!(TcpListener::bind(address).await.is_ok());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn loopback_rejects_truncated_http_request_and_continues_listening() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (tx, rx) = mpsc::channel();
    let task = tokio::spawn(async move {
        loopback_code(
            listener,
            "synthetic client",
            Url::parse("https://example.invalid/authorize").unwrap(),
            "http://localhost:8089",
            |url| async move {
                tx.send(url).unwrap();
                Ok(())
            },
        )
        .await
    });
    let url = rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let query: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
    let state = &query["state"];

    let mut stream = tokio::net::TcpStream::connect(address).await.unwrap();
    let truncated = format!("GET /?state={state}&code=partial");
    stream.write_all(truncated.as_bytes()).await.unwrap();
    stream.shutdown().await.unwrap();

    let mut response = Vec::new();
    stream.read_to_end(&mut response).await.unwrap();
    let response_str = String::from_utf8_lossy(&response);
    assert!(
        response_str.starts_with("HTTP/1.1 400 Bad Request"),
        "expected 400 Bad Request but got: {response_str}"
    );

    assert!(!task.is_finished());

    let client = Client::new();
    assert_eq!(
        client
            .get(format!("http://{address}/?state={state}&code=valid_code"))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    assert_eq!(task.await.unwrap().unwrap(), "valid_code");
    assert!(TcpListener::bind(address).await.is_ok());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn oauth_error_callback_with_matching_state_terminates_and_returns_provider_error_description()
 {
    use emusync_data_drive::OAuthError;

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (tx, rx) = mpsc::channel();
    let task = tokio::spawn(async move {
        loopback_code(
            listener,
            "synthetic client",
            Url::parse("https://example.invalid/authorize").unwrap(),
            "http://localhost:8089",
            |url| async move {
                tx.send(url).unwrap();
                Ok(())
            },
        )
        .await
    });
    let url = rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let query: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
    let state = &query["state"];

    let client = Client::new();

    let post_resp = client
        .post(format!("http://{address}/?state={state}&code=abc"))
        .send()
        .await
        .unwrap();
    assert_eq!(post_resp.status(), 400);
    assert!(!task.is_finished());

    let err_resp = client
        .get(format!(
            "http://{address}/?error=access_denied&error_description=User+denied+access&state={state}"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(err_resp.status(), 400);

    let err = task.await.unwrap().unwrap_err();
    assert!(matches!(err, OAuthError::Provider(ref msg) if msg.contains("User denied access")));
    assert!(TcpListener::bind(address).await.is_ok());

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (tx, rx) = mpsc::channel();
    let task = tokio::spawn(async move {
        loopback_code(
            listener,
            "synthetic client",
            Url::parse("https://example.invalid/authorize").unwrap(),
            "http://localhost:8089",
            |url| async move {
                tx.send(url).unwrap();
                Ok(())
            },
        )
        .await
    });
    let url = rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let query: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
    let state = &query["state"];

    let err_resp = client
        .get(format!(
            "http://{address}/?error=access_denied&error_description=%20%20&state={state}"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(err_resp.status(), 400);

    let err = task.await.unwrap().unwrap_err();
    assert!(matches!(err, OAuthError::Provider(ref msg) if msg == "access_denied"));
    assert!(TcpListener::bind(address).await.is_ok());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn oversized_request_headers_and_unspecified_bind_address_are_rejected_safely() {
    use emusync_data_drive::OAuthError;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = TcpListener::bind("0.0.0.0:0").await.unwrap();
    let launched = std::sync::atomic::AtomicBool::new(false);
    let result = loopback_code(
        listener,
        "client",
        Url::parse("https://example.invalid/").unwrap(),
        "http://localhost:8089",
        |_| async {
            launched.store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        },
    )
    .await;
    assert!(matches!(result, Err(OAuthError::Provider(ref msg)) if msg.contains("loopback")));
    assert!(!launched.load(std::sync::atomic::Ordering::SeqCst));

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (tx, rx) = mpsc::channel();
    let task = tokio::spawn(async move {
        loopback_code(
            listener,
            "synthetic client",
            Url::parse("https://example.invalid/authorize").unwrap(),
            "http://localhost:8089",
            |url| async move {
                tx.send(url).unwrap();
                Ok(())
            },
        )
        .await
    });
    let url = rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let query: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
    let state = &query["state"];

    let mut stream = tokio::net::TcpStream::connect(address).await.unwrap();
    let oversized = format!(
        "GET /?state={state}&code=x HTTP/1.1\r\nHost: localhost\r\nX-Oversized: {}\r\n\r\n",
        "a".repeat(10240)
    );
    stream.write_all(oversized.as_bytes()).await.unwrap();
    stream.shutdown().await.unwrap();

    let mut response = Vec::new();
    stream.read_to_end(&mut response).await.unwrap();
    let response_str = String::from_utf8_lossy(&response);
    assert!(
        response_str.starts_with("HTTP/1.1 400 Bad Request"),
        "expected 400 Bad Request but got: {response_str}"
    );
    assert!(!task.is_finished());

    let client = Client::new();
    let resp = client
        .get(format!("http://{address}/?state={state}&code=good-code"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    assert_eq!(task.await.unwrap().unwrap(), "good-code");
    assert!(TcpListener::bind(address).await.is_ok());
}
