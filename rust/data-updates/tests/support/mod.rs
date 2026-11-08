use super::*;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tempfile::tempdir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

enum CancellationOutcome {
    Cancelled,
    Finished(Result<(), UpdateError>),
}

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
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 1024];
            let n = socket.read(&mut buf).await.unwrap();
            assert!(n > 0);
            request_received_tx.send(()).unwrap();
            send_headers_rx.await.unwrap();
            let response = "HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\n";
            socket.write_all(response.as_bytes()).await.unwrap();
            let mut discard = [0u8; 128];
            let _ = socket.read(&mut discard).await;
        });

        let staging_notify = Arc::new(tokio::sync::Notify::new());
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let downloader =
            UpdateDownloader::new(client).with_staging_started_notify(staging_notify.clone());

        let url = format!("http://127.0.0.1:{port}/file");
        let dest_clone = destination.clone();

        let (cancel_tx, mut cancel_rx) = tokio::sync::oneshot::channel::<()>();
        let download_task = tokio::spawn(async move {
            tokio::select! {
                biased;
                _ = &mut cancel_rx => CancellationOutcome::Cancelled,
                res = downloader.download(&url, &dest_clone, |_| {}) => {
                    CancellationOutcome::Finished(res)
                }
            }
        });

        request_received_rx.await.unwrap();

        // Occupy the single blocking worker so the queued staging creation cannot run yet.
        let (worker_occupied_tx, worker_occupied_rx) = std::sync::mpsc::channel();
        let (worker_release_tx, worker_release_rx) = std::sync::mpsc::channel();
        let worker_handle = tokio::task::spawn_blocking(move || {
            worker_occupied_tx.send(()).unwrap();
            let _ = worker_release_rx.recv();
        });
        worker_occupied_rx.recv().unwrap();

        send_headers_tx.send(()).unwrap();

        // The notifier fires before spawn_blocking enqueues the staging creation.
        // It is not proof that a staging file was created.
        staging_notify.notified().await;

        drop(cancel_tx);
        match download_task.await.unwrap() {
            CancellationOutcome::Cancelled => {}
            CancellationOutcome::Finished(result) => {
                panic!("download returned before cancellation: {result:?}")
            }
        }

        // Drop the cancelled future before releasing the worker. The queued
        // spawn_blocking creation then runs detached and its StagedFile result
        // is dropped, which performs the eventual best-effort cleanup.
        worker_release_tx.send(()).unwrap();
        worker_handle.await.unwrap();

        let (drain_tx, drain_rx) = std::sync::mpsc::channel();
        tokio::task::spawn_blocking(move || {
            drain_tx.send(()).unwrap();
        })
        .await
        .unwrap();
        drain_rx.recv().unwrap();

        server_task.abort();
        match server_task.await {
            Err(error) if error.is_cancelled() => {}
            Ok(()) => {}
            Err(error) if error.is_panic() => std::panic::resume_unwind(error.into_panic()),
            Err(error) => panic!("server task failed: {error}"),
        }

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

    let server_task = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut buf = [0u8; 1024];
        let n = socket.read(&mut buf).await.unwrap();
        assert!(n > 0);
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            new_content.len(),
            new_content
        );
        socket.write_all(response.as_bytes()).await.unwrap();
        socket.shutdown().await.unwrap();
    });

    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let downloader = UpdateDownloader::new(client).with_candidate_provider(candidate_provider);

    downloader
        .download(
            &format!("http://127.0.0.1:{port}/file"),
            &destination,
            |_| {},
        )
        .await
        .unwrap();
    server_task.await.unwrap();

    assert_eq!(std::fs::read_to_string(&destination).unwrap(), new_content);
    assert_eq!(
        std::fs::read_to_string(&colliding_path).unwrap(),
        "COLLIDING CONTENT DO NOT MODIFY"
    );
    assert!(
        !clean_candidate.exists(),
        "acquired clean candidate must be removed"
    );

    #[cfg(unix)]
    {
        let metadata = std::fs::symlink_metadata(&symlink_candidate).unwrap();
        assert!(
            metadata.file_type().is_symlink(),
            "unacquired symlink candidate must remain a symlink"
        );
        assert_eq!(
            std::fs::read_to_string(&symlink_target).unwrap(),
            "TARGET CONTENT DO NOT TRUNCATE"
        );
    }

    let mut entries: Vec<String> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    entries.sort();
    #[cfg(unix)]
    let expected_entries = vec![
        ".update.AppImage.colliding.part".to_owned(),
        ".update.AppImage.symlink.part".to_owned(),
        "symlink_target.txt".to_owned(),
        "update.AppImage".to_owned(),
    ];
    #[cfg(not(unix))]
    let expected_entries = vec![
        ".update.AppImage.colliding.part".to_owned(),
        "update.AppImage".to_owned(),
    ];
    assert_eq!(entries, expected_entries);

    // This provider reproduces the injected cfg(test) candidate branch, not the
    // production random-candidate branch.
    let unacquired_candidate = dir.path().join(".update.AppImage.unacquired.part");
    std::fs::write(&unacquired_candidate, "UNACQUIRED DO NOT DELETE").unwrap();

    let u_clone = unacquired_candidate.clone();
    let error_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let error_port = error_listener.local_addr().unwrap().port();

    let error_server_task = tokio::spawn(async move {
        let (mut socket, _) = error_listener.accept().await.unwrap();
        let mut buf = [0u8; 1024];
        let n = socket.read(&mut buf).await.unwrap();
        assert!(n > 0);
        let response = "HTTP/1.1 500 Internal Server Error\r\nConnection: close\r\n\r\nERROR";
        socket.write_all(response.as_bytes()).await.unwrap();
        socket.shutdown().await.unwrap();
    });

    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let error_downloader =
        UpdateDownloader::new(client).with_candidate_provider(move |_, _| vec![u_clone.clone()]);

    let res = error_downloader
        .download(
            &format!("http://127.0.0.1:{error_port}/file"),
            &destination,
            |_| {},
        )
        .await;
    error_server_task.await.unwrap();

    match res {
        Err(UpdateError::Http(message)) => {
            assert!(message.contains("500"), "expected HTTP 500: {message}");
        }
        other => panic!("expected Http error with 500, got: {other:?}"),
    }
    assert_eq!(
        std::fs::read_to_string(&unacquired_candidate).unwrap(),
        "UNACQUIRED DO NOT DELETE"
    );
    assert_eq!(std::fs::read_to_string(&destination).unwrap(), new_content);

    let mut entries: Vec<String> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    entries.sort();
    #[cfg(unix)]
    let expected_entries = vec![
        ".update.AppImage.colliding.part".to_owned(),
        ".update.AppImage.symlink.part".to_owned(),
        ".update.AppImage.unacquired.part".to_owned(),
        "symlink_target.txt".to_owned(),
        "update.AppImage".to_owned(),
    ];
    #[cfg(not(unix))]
    let expected_entries = vec![
        ".update.AppImage.colliding.part".to_owned(),
        ".update.AppImage.unacquired.part".to_owned(),
        "update.AppImage".to_owned(),
    ];
    assert_eq!(entries, expected_entries);
}

#[tokio::test]
async fn staging_creation_io_error_does_not_fall_back_or_delete_unacquired_paths() {
    let dir = tempdir().unwrap();
    let destination = dir.path().join("update.AppImage");
    std::fs::write(&destination, "PREVIOUS").unwrap();

    let blocker = dir.path().join("blocker");
    std::fs::write(&blocker, "BLOCKER").unwrap();
    let blocked_candidate = blocker.join("child.part");
    let clean_candidate = dir.path().join(".update.AppImage.clean.part");

    let c1 = blocked_candidate.clone();
    let c2 = clean_candidate.clone();
    let candidate_provider =
        move |_parent: &Path, _file_name: &str| -> Vec<PathBuf> { vec![c1.clone(), c2.clone()] };

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let content = "UPDATE";

    let server_task = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut buf = [0u8; 1024];
        let n = socket.read(&mut buf).await.unwrap();
        assert!(n > 0);
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            content.len(),
            content
        );
        socket.write_all(response.as_bytes()).await.unwrap();
        socket.shutdown().await.unwrap();
    });

    let progress = Arc::new(Mutex::new(Vec::new()));
    let progress_clone = progress.clone();
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let downloader = UpdateDownloader::new(client).with_candidate_provider(candidate_provider);
    let result = downloader
        .download(
            &format!("http://127.0.0.1:{port}/file"),
            &destination,
            move |value| {
                progress_clone.lock().unwrap().push(value);
            },
        )
        .await;
    server_task.await.unwrap();

    match result {
        Err(UpdateError::Io(_)) => {}
        other => panic!("expected non-AlreadyExists staging I/O error, got: {other:?}"),
    }
    assert!(
        !clean_candidate.exists(),
        "must not fall back to candidate 2"
    );
    assert!(!blocked_candidate.exists());
    assert_eq!(std::fs::read_to_string(&destination).unwrap(), "PREVIOUS");
    assert_eq!(std::fs::read_to_string(&blocker).unwrap(), "BLOCKER");
    assert!(
        progress.lock().unwrap().is_empty(),
        "failure before the body loop must not report progress"
    );

    let mut entries: Vec<String> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    entries.sort();
    assert_eq!(entries, vec!["blocker", "update.AppImage"]);
}
