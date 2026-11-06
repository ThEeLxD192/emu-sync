use emusync_data_updates::UpdateDownloader;
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
