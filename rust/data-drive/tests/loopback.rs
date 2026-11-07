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
