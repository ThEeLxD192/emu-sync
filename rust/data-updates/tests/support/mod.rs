use super::*;
use std::path::{Path, PathBuf};
use tempfile::tempdir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[test]
fn cancelled_queued_creation_cleans_temporary_file_without_orphaning() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .max_blocking_threads(1)
        .build()
        .unwrap();

    rt.block_on(async {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        let dir = tempdir().unwrap();
        let destination = dir.path().join("update.AppImage");
        std::fs::write(&destination, "PREVIOUS").unwrap();

        let (request_received_tx, request_received_rx) = tokio::sync::oneshot::channel();
        let (send_headers_tx, send_headers_rx) = tokio::sync::oneshot::channel();

        let server_task = tokio::spawn(async move {
            if let Ok((mut socket, _)) = listener.accept().await {
                let mut buf = [0u8; 1024];
                let n = socket.read(&mut buf).await.unwrap_or(0);
                assert!(n > 0);
                let _ = request_received_tx.send(());
                let _ = send_headers_rx.await;
                let response = "HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\n";
                let _ = socket.write_all(response.as_bytes()).await;
                let mut discard = [0u8; 128];
                let _ = socket.read(&mut discard).await;
            }
        });

        let staging_notify = std::sync::Arc::new(tokio::sync::Notify::new());
        let client = reqwest::Client::new();
        let downloader =
            UpdateDownloader::new(client).with_staging_started_notify(staging_notify.clone());

        let url = format!("http://127.0.0.1:{port}/file");
        let dest_clone = destination.clone();

        let (cancel_tx, mut cancel_rx) = tokio::sync::oneshot::channel::<()>();
        let download_task = tokio::spawn(async move {
            tokio::select! {
                biased;
                _ = &mut cancel_rx => {}
                res = downloader.download(&url, &dest_clone, |_| {}) => {
                    let _ = res;
                }
            }
        });

        // 1. Wait until create_dir_all has completed and HTTP request was received
        request_received_rx.await.unwrap();

        // 2. Occupy the single blocking worker thread
        let (worker_occupied_tx, worker_occupied_rx) = std::sync::mpsc::channel();
        let (worker_release_tx, worker_release_rx) = std::sync::mpsc::channel();

        tokio::task::spawn_blocking(move || {
            worker_occupied_tx.send(()).unwrap();
            worker_release_rx.recv().unwrap();
        });

        worker_occupied_rx.recv().unwrap();

        // 3. Deliver headers to trigger temporary staging file creation
        send_headers_tx.send(()).unwrap();

        // 4. Wait until the download task initiates staging creation (enqueued on blocking pool)
        staging_notify.notified().await;

        // 5. Cancel the download future
        drop(cancel_tx);
        download_task.await.unwrap();

        // 6. Release the blocking worker thread so the enqueued creation runs
        worker_release_tx.send(()).unwrap();

        // 7. Explicitly wait for pending blocking work to finish
        let (drain_tx, drain_rx) = std::sync::mpsc::channel();
        tokio::task::spawn_blocking(move || {
            drain_tx.send(()).unwrap();
        })
        .await
        .unwrap();
        drain_rx.recv().unwrap();

        server_task.abort();

        // 8. Assert destination is preserved and no temporary .part file was orphaned
        assert_eq!(std::fs::read_to_string(&destination).unwrap(), "PREVIOUS");

        let entries: Vec<String> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .collect();
        assert_eq!(entries, vec!["update.AppImage"]);
    });
}

#[tokio::test]
async fn collision_and_symlinks_are_skipped_without_modifying_or_deleting_unacquired_paths() {
    let dir = tempdir().unwrap();
    let destination = dir.path().join("update.AppImage");

    let colliding_path = dir.path().join(".update.AppImage.colliding.part");
    std::fs::write(&colliding_path, "COLLIDING CONTENT DO NOT MODIFY").unwrap();

    #[cfg(unix)]
    let (symlink_candidate, symlink_target) = {
        let target = dir.path().join("symlink_target.txt");
        std::fs::write(&target, "TARGET CONTENT DO NOT TRUNCATE").unwrap();
        let symlink_path = dir.path().join(".update.AppImage.symlink.part");
        std::os::unix::fs::symlink(&target, &symlink_path).unwrap();
        (symlink_path, target)
    };

    let clean_candidate = dir.path().join(".update.AppImage.clean.part");

    let c1 = colliding_path.clone();
    #[cfg(unix)]
    let c2 = symlink_candidate.clone();
    let c3 = clean_candidate.clone();

    let candidate_provider = move |_parent: &Path, _file_name: &str| -> Vec<PathBuf> {
        #[cfg(unix)]
        {
            vec![c1.clone(), c2.clone(), c3.clone()]
        }
        #[cfg(not(unix))]
        {
            vec![c1.clone(), c3.clone()]
        }
    };

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let new_content = "NEW AND VALID UPDATE PAYLOAD";

    tokio::spawn(async move {
        if let Ok((mut socket, _)) = listener.accept().await {
            let mut buf = [0u8; 1024];
            let _ = socket.read(&mut buf).await;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                new_content.len(),
                new_content
            );
            let _ = socket.write_all(response.as_bytes()).await;
        }
    });

    let client = reqwest::Client::new();
    let downloader = UpdateDownloader::new(client).with_candidate_provider(candidate_provider);

    downloader
        .download(
            &format!("http://127.0.0.1:{port}/file"),
            &destination,
            |_| {},
        )
        .await
        .unwrap();

    // Destination has the new content
    assert_eq!(std::fs::read_to_string(&destination).unwrap(), new_content);

    // Pre-existing colliding file remains untouched
    assert_eq!(
        std::fs::read_to_string(&colliding_path).unwrap(),
        "COLLIDING CONTENT DO NOT MODIFY"
    );

    // On Unix, target of symlink is NOT truncated or modified
    #[cfg(unix)]
    assert_eq!(
        std::fs::read_to_string(&symlink_target).unwrap(),
        "TARGET CONTENT DO NOT TRUNCATE"
    );

    // Second check: an HTTP error never removes an unacquired path
    let unacquired_candidate = dir.path().join(".update.AppImage.unacquired.part");
    std::fs::write(&unacquired_candidate, "UNACQUIRED DO NOT DELETE").unwrap();

    let u_clone = unacquired_candidate.clone();
    let error_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let error_port = error_listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        if let Ok((mut socket, _)) = error_listener.accept().await {
            let mut buf = [0u8; 1024];
            let _ = socket.read(&mut buf).await;
            let response = "HTTP/1.1 500 Internal Server Error\r\nConnection: close\r\n\r\nERROR";
            let _ = socket.write_all(response.as_bytes()).await;
        }
    });

    let client = reqwest::Client::new();
    let error_downloader =
        UpdateDownloader::new(client).with_candidate_provider(move |_, _| vec![u_clone.clone()]);

    let res = error_downloader
        .download(
            &format!("http://127.0.0.1:{error_port}/file"),
            &destination,
            |_| {},
        )
        .await;

    assert!(res.is_err());
    assert_eq!(
        std::fs::read_to_string(&unacquired_candidate).unwrap(),
        "UNACQUIRED DO NOT DELETE"
    );
}
