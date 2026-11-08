use emusync_data_updates::{UpdateDownloader, UpdateError};
use std::sync::{Arc, Mutex};
use tempfile::tempdir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::test]
async fn download_without_content_length_preserves_all_bytes_and_reports_completion() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    let content = "COMPLETE UPDATE PAYLOAD";
    tokio::spawn(async move {
        if let Ok((mut socket, _)) = listener.accept().await {
            let mut buf = [0u8; 1024];
            let _ = socket.read(&mut buf).await;
            // No Content-Length header, chunked / close
            let response = format!("HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n{content}");
            let _ = socket.write_all(response.as_bytes()).await;
        }
    });

    let dir = tempdir().unwrap();
    let destination = dir.path().join("update.AppImage");

    let progress_events = Arc::new(Mutex::new(Vec::new()));
    let p_clone = progress_events.clone();

    let client = reqwest::Client::new();
    let downloader = UpdateDownloader::new(client);
    downloader
        .download(
            &format!("http://127.0.0.1:{port}/file"),
            &destination,
            move |p| {
                p_clone.lock().unwrap().push(p);
            },
        )
        .await
        .unwrap();

    assert_eq!(std::fs::read_to_string(&destination).unwrap(), content);
    assert_eq!(*progress_events.lock().unwrap(), vec![1.0]);

    // Verify no temporary .part files remained
    let entries: Vec<String> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    assert_eq!(entries, vec!["update.AppImage"]);
}

#[tokio::test]
async fn download_with_content_length_reports_incremental_progress() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    let content = "SIXSIX";
    tokio::spawn(async move {
        if let Ok((mut socket, _)) = listener.accept().await {
            let mut buf = [0u8; 1024];
            let _ = socket.read(&mut buf).await;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: 6\r\nConnection: close\r\n\r\n{content}"
            );
            let _ = socket.write_all(response.as_bytes()).await;
        }
    });

    let dir = tempdir().unwrap();
    let destination = dir.path().join("update.AppImage");

    let progress_events = Arc::new(Mutex::new(Vec::new()));
    let p_clone = progress_events.clone();

    let client = reqwest::Client::new();
    let downloader = UpdateDownloader::new(client);
    downloader
        .download(
            &format!("http://127.0.0.1:{port}/file"),
            &destination,
            move |p| {
                p_clone.lock().unwrap().push(p);
            },
        )
        .await
        .unwrap();

    assert_eq!(std::fs::read_to_string(&destination).unwrap(), content);
    let events = progress_events.lock().unwrap().clone();
    assert!(!events.is_empty());
    assert_eq!(*events.last().unwrap(), 1.0);
}

#[tokio::test]
async fn empty_download_is_rejected_without_replacing_previous_file() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        if let Ok((mut socket, _)) = listener.accept().await {
            let mut buf = [0u8; 1024];
            let _ = socket.read(&mut buf).await;
            let response = "HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
            let _ = socket.write_all(response.as_bytes()).await;
        }
    });

    let dir = tempdir().unwrap();
    let destination = dir.path().join("update.AppImage");
    std::fs::write(&destination, "PREVIOUS").unwrap();

    let client = reqwest::Client::new();
    let downloader = UpdateDownloader::new(client);
    let result = downloader
        .download(
            &format!("http://127.0.0.1:{port}/file"),
            &destination,
            |_| {},
        )
        .await;

    assert!(result.is_err());
    assert_eq!(std::fs::read_to_string(&destination).unwrap(), "PREVIOUS");

    // No lingering .part files
    let entries: Vec<String> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    assert_eq!(entries, vec!["update.AppImage"]);
}

#[tokio::test]
async fn truncated_download_preserves_previous_file_and_cleans_staging() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        if let Ok((mut socket, _)) = listener.accept().await {
            let mut buf = [0u8; 1024];
            let _ = socket.read(&mut buf).await;
            // Promised 100 bytes, only send 4
            let response =
                "HTTP/1.1 200 OK\r\nContent-Length: 100\r\nConnection: close\r\n\r\nFOUR";
            let _ = socket.write_all(response.as_bytes()).await;
        }
    });

    let dir = tempdir().unwrap();
    let destination = dir.path().join("update.AppImage");
    std::fs::write(&destination, "PREVIOUS").unwrap();

    let client = reqwest::Client::new();
    let downloader = UpdateDownloader::new(client);
    let result = downloader
        .download(
            &format!("http://127.0.0.1:{port}/file"),
            &destination,
            |_| {},
        )
        .await;

    assert!(result.is_err());
    assert_eq!(std::fs::read_to_string(&destination).unwrap(), "PREVIOUS");

    // No lingering .part files
    let entries: Vec<String> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    assert_eq!(entries, vec!["update.AppImage"]);
}

#[tokio::test]
async fn cancelled_download_preserves_destination_and_cleans_staging() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        if let Ok((mut socket, _)) = listener.accept().await {
            let mut buf = [0u8; 1024];
            let _ = socket.read(&mut buf).await;
            let response =
                "HTTP/1.1 200 OK\r\nContent-Length: 1000\r\nConnection: keep-alive\r\n\r\nPARTIAL";
            let _ = socket.write_all(response.as_bytes()).await;
            // Stall without closing
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
        }
    });

    let dir = tempdir().unwrap();
    let destination = dir.path().join("update.AppImage");
    std::fs::write(&destination, "PREVIOUS").unwrap();

    let client = reqwest::Client::new();
    let downloader = UpdateDownloader::new(client);
    let dest_clone = destination.clone();

    let handle = tokio::spawn(async move {
        downloader
            .download(
                &format!("http://127.0.0.1:{port}/file"),
                &dest_clone,
                |_| {},
            )
            .await
    });

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    handle.abort();
    let _ = handle.await;

    assert_eq!(std::fs::read_to_string(&destination).unwrap(), "PREVIOUS");

    // No lingering .part files
    let entries: Vec<String> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    assert_eq!(entries, vec!["update.AppImage"]);
}

// ----------------------------------------------------------------------------
// Public HTTP Regression Tests (Contract-Oriented)
// ----------------------------------------------------------------------------

#[tokio::test]
async fn download_rejects_partial_content_206_and_preserves_destination_and_cleans_staging() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        if let Ok((mut socket, _)) = listener.accept().await {
            let mut buf = [0u8; 1024];
            let _ = socket.read(&mut buf).await;
            let response = "HTTP/1.1 206 Partial Content\r\nContent-Length: 4\r\nContent-Range: bytes 0-3/100\r\nConnection: close\r\n\r\nPART";
            let _ = socket.write_all(response.as_bytes()).await;
        }
    });

    let dir = tempdir().unwrap();
    let destination = dir.path().join("update.AppImage");
    std::fs::write(&destination, "PREVIOUS").unwrap();

    let client = reqwest::Client::new();
    let downloader = UpdateDownloader::new(client);
    let result = downloader
        .download(
            &format!("http://127.0.0.1:{port}/file"),
            &destination,
            |_| {},
        )
        .await;

    match result {
        Err(UpdateError::Http(msg)) => {
            assert!(
                msg.contains("206"),
                "error message should contain 206: {msg}"
            );
        }
        other => panic!("expected Http error with 206 status, got: {other:?}"),
    }

    assert_eq!(std::fs::read_to_string(&destination).unwrap(), "PREVIOUS");

    let entries: Vec<String> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    assert_eq!(entries, vec!["update.AppImage"]);
}

#[tokio::test]
async fn inactivity_timeout_aborts_when_server_does_not_deliver_headers() {
    tokio::time::pause();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    let (request_received_tx, request_received_rx) = tokio::sync::oneshot::channel();

    let server_task = tokio::spawn(async move {
        if let Ok((mut socket, _)) = listener.accept().await {
            let mut buf = [0u8; 1024];
            let n = socket.read(&mut buf).await.unwrap_or(0);
            assert!(n > 0);
            let _ = request_received_tx.send(());
            // Do not send any headers, keep socket open
            let mut discard = [0u8; 128];
            let _ = socket.read(&mut discard).await;
        }
    });

    let dir = tempdir().unwrap();
    let destination = dir.path().join("update.AppImage");
    std::fs::write(&destination, "PREVIOUS").unwrap();

    let client = reqwest::Client::new();
    let downloader = UpdateDownloader::new(client);
    let dest_clone = destination.clone();
    let url = format!("http://127.0.0.1:{port}/file");

    let download_handle =
        tokio::spawn(async move { downloader.download(&url, &dest_clone, |_| {}).await });

    // Wait until the loopback server receives the HTTP request
    request_received_rx.await.unwrap();

    // Advance virtual time past the 60s inactivity watchdog
    tokio::time::advance(std::time::Duration::from_secs(61)).await;

    let result = download_handle.await.unwrap();
    assert!(
        result.is_err(),
        "expected timeout error when headers are not delivered"
    );
    match result {
        Err(UpdateError::Network(msg)) => {
            assert!(
                msg.contains("timed out") || msg.contains("inactivity"),
                "unexpected error message: {msg}"
            );
        }
        other => panic!("expected Network error, got: {other:?}"),
    }

    server_task.abort();

    // Verify destination preserved and no temporary .part files created
    assert_eq!(std::fs::read_to_string(&destination).unwrap(), "PREVIOUS");
    let entries: Vec<String> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    assert_eq!(entries, vec!["update.AppImage"]);
}

#[tokio::test]
async fn inactivity_timeout_aborts_when_server_delivers_headers_then_stalls_body() {
    tokio::time::pause();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    let (headers_sent_tx, headers_sent_rx) = tokio::sync::oneshot::channel();

    let server_task = tokio::spawn(async move {
        if let Ok((mut socket, _)) = listener.accept().await {
            let mut buf = [0u8; 1024];
            let n = socket.read(&mut buf).await.unwrap_or(0);
            assert!(n > 0);
            // Send headers promising 1000 bytes, then stall without sending body
            let response = "HTTP/1.1 200 OK\r\nContent-Length: 1000\r\n\r\n";
            let _ = socket.write_all(response.as_bytes()).await;
            let _ = headers_sent_tx.send(());
            let mut discard = [0u8; 128];
            let _ = socket.read(&mut discard).await;
        }
    });

    let dir = tempdir().unwrap();
    let destination = dir.path().join("update.AppImage");
    std::fs::write(&destination, "PREVIOUS").unwrap();

    let client = reqwest::Client::new();
    let downloader = UpdateDownloader::new(client);
    let dest_clone = destination.clone();
    let url = format!("http://127.0.0.1:{port}/file");

    let download_handle =
        tokio::spawn(async move { downloader.download(&url, &dest_clone, |_| {}).await });

    headers_sent_rx.await.unwrap();

    // Allow downloader to process headers and create the staging file
    while std::fs::read_dir(dir.path()).unwrap().count() < 2 {
        tokio::task::yield_now().await;
    }

    // Advance virtual time past the 60s watchdog
    tokio::time::advance(std::time::Duration::from_secs(61)).await;

    let result = download_handle.await.unwrap();
    assert!(
        result.is_err(),
        "expected timeout error on stalled body stream"
    );
    match result {
        Err(UpdateError::Network(msg)) => {
            assert!(
                msg.contains("stalled") || msg.contains("timed out"),
                "unexpected error message: {msg}"
            );
        }
        other => panic!("expected Network error, got: {other:?}"),
    }

    server_task.abort();

    // Verify destination preserved and staging file was cleaned up
    assert_eq!(std::fs::read_to_string(&destination).unwrap(), "PREVIOUS");
    let entries: Vec<String> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    assert_eq!(entries, vec!["update.AppImage"]);
}

#[tokio::test]
async fn active_transfer_with_periodic_progress_survives_past_watchdog_duration() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    let chunk1: &'static [u8] = b"0123456789";
    let chunk2: &'static [u8] = b"ABCDEFGHIJ";
    let chunk3: &'static [u8] = b"klmnopqrst";
    let total_expected_len = chunk1.len() + chunk2.len() + chunk3.len();

    let (send_chunk_tx, mut send_chunk_rx) = tokio::sync::mpsc::channel::<Option<&'static [u8]>>(1);

    let server_task = tokio::spawn(async move {
        if let Ok((mut socket, _)) = listener.accept().await {
            let mut buf = [0u8; 1024];
            let _ = socket.read(&mut buf).await;
            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {total_expected_len}\r\nConnection: close\r\n\r\n"
            );
            let _ = socket.write_all(header.as_bytes()).await;

            while let Some(maybe_chunk) = send_chunk_rx.recv().await {
                match maybe_chunk {
                    Some(chunk) => {
                        let _ = socket.write_all(chunk).await;
                        let _ = socket.flush().await;
                    }
                    None => {
                        let _ = socket.shutdown().await;
                        break;
                    }
                }
            }
        }
    });

    let dir = tempdir().unwrap();
    let destination = dir.path().join("update.AppImage");

    let client = reqwest::Client::new();
    let downloader = UpdateDownloader::new(client);
    let dest_clone = destination.clone();
    let url = format!("http://127.0.0.1:{port}/file");

    let (progress_tx, mut progress_rx) = tokio::sync::mpsc::unbounded_channel::<f32>();

    let download_handle = tokio::spawn(async move {
        downloader
            .download(&url, &dest_clone, move |p| {
                let _ = progress_tx.send(p);
            })
            .await
    });

    // 1. Deliver chunk 1 in real time
    send_chunk_tx.send(Some(chunk1)).await.unwrap();

    // Confirm that the client consumed chunk 1 via on_progress before advancing clock
    let p1 = progress_rx.recv().await.unwrap();
    assert!(
        (0.3..=0.4).contains(&p1),
        "first chunk progress expected ~0.33, got {p1}"
    );

    // 2. Start virtual time measurement after confirming first chunk was consumed
    tokio::time::pause();
    let start_virtual = tokio::time::Instant::now();

    // 3. Advance virtual clock by 35s (< 60s watchdog limit) while client waits for chunk 2
    tokio::time::advance(std::time::Duration::from_secs(35)).await;

    // Resume clock during real socket I/O to avoid uncontrolled auto-advancement
    tokio::time::resume();
    send_chunk_tx.send(Some(chunk2)).await.unwrap();

    // Confirm chunk 2 consumed via on_progress before advancing clock again
    let p2 = progress_rx.recv().await.unwrap();
    assert!(
        (0.6..=0.7).contains(&p2),
        "second chunk progress expected ~0.66, got {p2}"
    );

    // 4. Advance virtual clock by another 35s (< 60s watchdog limit; accumulated 70s > 60s)
    tokio::time::pause();
    tokio::time::advance(std::time::Duration::from_secs(35)).await;

    tokio::time::resume();
    send_chunk_tx.send(Some(chunk3)).await.unwrap();

    // Confirm chunk 3 consumed via on_progress
    let p3 = progress_rx.recv().await.unwrap();
    assert!(
        (p3 - 1.0).abs() < f32::EPSILON,
        "third chunk progress expected 1.0, got {p3}"
    );

    // End server stream
    send_chunk_tx.send(None).await.unwrap();

    // 5. Complete download and verify success
    let result = download_handle.await.unwrap();
    result.unwrap();

    tokio::time::pause();
    let elapsed_virtual = start_virtual.elapsed();
    assert!(
        elapsed_virtual >= std::time::Duration::from_secs(70),
        "total virtual time spent during reading ({elapsed_virtual:?}) must exceed 60s watchdog"
    );

    server_task.abort();

    // 6. Verify intact destination content and staging cleanup
    assert_eq!(
        std::fs::read_to_string(&destination).unwrap(),
        "0123456789ABCDEFGHIJklmnopqrst"
    );

    let entries: Vec<String> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    assert_eq!(entries, vec!["update.AppImage"]);
}
